//! Virtual DualSense Edge USB control plane over Linux UHID.
//!
//! The descriptor is the independently authored descriptor used by the Steam
//! compatibility spike in `docs/evidence/uhid-dualsense-probe.md`.

use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};

use crate::edge_report::{self, Feedback, MotionSample};
use crate::input::State;

// Packed native-endian ABI described by linux/uhid.h. HID fields are LE.
const DATA_MAX: usize = 4096;
// Accommodate CREATE2 with a maximal descriptor, including spare ABI room.
const EVENT_SIZE: usize = 4380;
const DESCRIPTOR: &[u8] = &[
    0x05, 0x01, 0x09, 0x05, 0xa1, 0x01, 0x85, 0x01, 0x15, 0x00, 0x26, 0xff, 0x00, 0x75, 0x08, 0x95,
    0x06, 0x09, 0x30, 0x09, 0x31, 0x09, 0x33, 0x09, 0x34, 0x09, 0x32, 0x09, 0x35, 0x81, 0x02, 0x06,
    0x00, 0xff, 0x09, 0x01, 0x95, 0x39, 0x81, 0x02, 0x85, 0x02, 0x09, 0x02, 0x95, 0x3e, 0x91, 0x02,
    0x85, 0x05, 0x09, 0x05, 0x95, 0x28, 0xb1, 0x02, 0x85, 0x09, 0x09, 0x09, 0x95, 0x13, 0xb1, 0x02,
    0x85, 0x20, 0x09, 0x20, 0x95, 0x3f, 0xb1, 0x02, 0xc0,
];

fn put16(b: &mut [u8], at: usize, value: u16) {
    b[at..at + 2].copy_from_slice(&value.to_ne_bytes());
}
fn put32(b: &mut [u8], at: usize, value: u32) {
    b[at..at + 4].copy_from_slice(&value.to_ne_bytes());
}
fn get16(b: &[u8], at: usize) -> u16 {
    u16::from_ne_bytes([b[at], b[at + 1]])
}
fn get32(b: &[u8], at: usize) -> u32 {
    u32::from_ne_bytes(b[at..at + 4].try_into().unwrap())
}
fn event(kind: u32, len: usize) -> Vec<u8> {
    let mut out = vec![0; len];
    put32(&mut out, 0, kind);
    out
}

fn create_message(name: &str) -> Result<Vec<u8>> {
    if name.is_empty() || name.len() > 127 || name.as_bytes().contains(&0) {
        bail!("UHID name must be 1..=127 bytes without NUL");
    }
    let mut out = event(11, 280 + DESCRIPTOR.len());
    out[4..4 + name.len()].copy_from_slice(name.as_bytes());
    put16(&mut out, 260, DESCRIPTOR.len() as u16);
    put16(&mut out, 262, 3); // BUS_USB, linux/input.h
                             // Linux hid-ids.h: Sony DualSense Edge USB identity.
    put32(&mut out, 264, 0x054c);
    put32(&mut out, 268, 0x0df2);
    put32(&mut out, 272, 0x0100);
    out[280..].copy_from_slice(DESCRIPTOR);
    Ok(out)
}

fn address() -> [u8; 6] {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    static SEED: OnceLock<u64> = OnceLock::new();
    let seed = *SEED.get_or_init(|| {
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        time ^ (u64::from(std::process::id()) << 17)
    });
    let value = seed.wrapping_add(SERIAL.fetch_add(1, Ordering::Relaxed));
    let mut bytes = value.to_le_bytes();
    // Pairing address is stored in reverse order; set local/unicast bits on
    // its most significant octet. This is an identifier, not a credential.
    bytes[5] = (bytes[5] & 0xfc) | 0x02;
    bytes[..6].try_into().unwrap()
}

fn feature(number: u8, mac: [u8; 6]) -> Option<Vec<u8>> {
    let mut data = vec![
        0;
        match number {
            5 => 41,
            9 => 20,
            0x20 => 64,
            _ => return None,
        }
    ];
    data[0] = number;
    if number == 5 {
        // Fabricated calibration, not a physical measurement. The public
        // Linux hid-playstation calibration layout uses speed / count span;
        // +/-32767 counts at +/-2000 deg/s gives 2000/32767 deg/s per count.
        // https://github.com/torvalds/linux/blob/master/drivers/hid/hid-playstation.c
        for at in [7, 11, 15] {
            data[at..at + 2].copy_from_slice(&32767i16.to_le_bytes());
            data[at + 2..at + 4].copy_from_slice(&(-32767i16).to_le_bytes());
        }
        for at in [19, 21] {
            data[at..at + 2].copy_from_slice(&2000i16.to_le_bytes());
        }
        for at in [23, 27, 31] {
            data[at..at + 2].copy_from_slice(&8192i16.to_le_bytes());
            data[at + 2..at + 4].copy_from_slice(&(-8192i16).to_le_bytes());
        }
    } else if number == 9 {
        data[1..7].copy_from_slice(&mac);
    }
    Some(data)
}

fn write_event(writer: &mut impl Write, data: &[u8]) -> Result<()> {
    let n = writer.write(data).context("writing UHID event")?;
    if n != data.len() {
        bail!("short UHID write: {n}/{}", data.len());
    }
    Ok(())
}

/// An independently created UHID device. Call `poll_host` every service tick,
/// including immediately after creation, so kernel probe requests are answered.
pub struct EdgePad {
    file: File,
    name: String,
    mac: [u8; 6],
    created: bool,
    started: bool,
    origin: Instant,
    sequence: u8,
    levels: Option<(u8, u8)>,
    output_count: u64,
}

impl EdgePad {
    pub fn create(name: &str) -> Result<Self> {
        let message = create_message(name)?;
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open("/dev/uhid")
            .context("opening /dev/uhid")?;
        write_event(&mut file, &message).context("creating Edge UHID device")?;
        Ok(Self {
            file,
            name: name.to_owned(),
            mac: address(),
            created: true,
            started: false,
            origin: Instant::now(),
            sequence: 0,
            levels: None,
            output_count: 0,
        })
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether the kernel has delivered START without a subsequent STOP.
    #[must_use]
    pub fn started(&self) -> bool {
        self.started
    }

    /// Send one report after START; before START this returns an error.
    pub fn send(
        &mut self,
        state: &State,
        sample: Option<MotionSample>,
        scale: i16,
        stale_after: Duration,
    ) -> Result<()> {
        if !self.created || !self.started {
            bail!("Edge UHID device has not started");
        }
        let report = edge_report::input_report(
            state,
            sample,
            scale,
            stale_after,
            self.origin.elapsed(),
            self.sequence,
        );
        let mut message = event(12, 6 + report.len());
        put16(&mut message, 4, report.len() as u16);
        message[6..].copy_from_slice(&report);
        write_event(&mut self.file, &message)?;
        self.sequence = self.sequence.wrapping_add(1);
        Ok(())
    }

    /// Drain at most 64 events without blocking. Returns the latest feedback
    /// update, including `(0, 0)` on CLOSE or STOP. Propagate errors to the
    /// caller so it can stop physical feedback.
    pub fn poll_host(&mut self) -> Result<Option<(u8, u8)>> {
        if !self.created {
            bail!("Edge UHID device is destroyed");
        }
        let mut latest = None;
        for _ in 0..64 {
            let mut buf = [0u8; EVENT_SIZE];
            let n = match self.file.read(&mut buf) {
                Ok(0) => bail!("UHID returned EOF"),
                Ok(n) => n,
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(e).context("reading UHID event"),
            };
            if let Some(update) = self.handle(&buf[..n])? {
                latest = Some(update);
            }
        }
        Ok(latest)
    }

    /// Clear the feedback latch; caller is responsible for stopping its sink.
    pub fn stop_feedback(&mut self) {
        self.levels = None;
    }

    /// Destroy once. A failed write is returned; dropping the file still closes
    /// the UHID device, and repeated calls do not send another destroy.
    pub fn destroy(&mut self) -> Result<()> {
        if !self.created {
            return Ok(());
        }
        self.created = false;
        self.started = false;
        self.stop_feedback();
        write_event(&mut self.file, &event(1, 4))
    }

    fn handle(&mut self, message: &[u8]) -> Result<Option<(u8, u8)>> {
        if message.len() < 4 {
            bail!("short UHID event header");
        }
        match get32(message, 0) {
            2 => {
                self.started = true;
                Ok(None)
            }
            3 | 5 => {
                if get32(message, 0) == 3 {
                    self.started = false;
                }
                self.stop_feedback();
                Ok(Some((0, 0)))
            }
            6 => {
                let report = output_report(message)?;
                self.output_count = self.output_count.saturating_add(1);
                let update = edge_report::feedback(report, self.levels.is_some());
                let next = match update {
                    Some(Feedback::Levels(l, r)) => Some((l, r)),
                    Some(Feedback::Stop) => Some((0, 0)),
                    None => None,
                };
                let changed = next.is_some_and(|v| Some(v).filter(|v| *v != (0, 0)) != self.levels);
                if self.output_count <= 16 || changed {
                    eprintln!(
                        "UHID output #{} len={} id={:02x} bytes[1..=5]={:02x?} byte[39]={:02x}",
                        self.output_count,
                        report.len(),
                        report[0],
                        &report[1..6],
                        report[39]
                    );
                }
                if let Some(v) = next {
                    self.levels = (v != (0, 0)).then_some(v);
                }
                Ok(next)
            }
            9 | 13 => {
                let reply = control_reply(message, self.mac)?;
                write_event(&mut self.file, &reply)?;
                Ok(None)
            }
            _ => Ok(None),
        }
    }
}

impl Drop for EdgePad {
    fn drop(&mut self) {
        if let Err(e) = self.destroy() {
            eprintln!("UHID destroy: {e:#}");
        }
    }
}

fn output_report(message: &[u8]) -> Result<&[u8]> {
    if message.len() < 4103 {
        bail!("short UHID OUTPUT event");
    }
    let size = usize::from(get16(message, 4100));
    if size > DATA_MAX || !matches!(size, 48 | 63) || message[4102] != 1 || message[4] != 2 {
        bail!(
            "unsupported UHID OUTPUT report type={} size={size}",
            message[4102]
        );
    }
    Ok(&message[4..4 + size])
}

fn control_reply(message: &[u8], mac: [u8; 6]) -> Result<Vec<u8>> {
    if message.len() < 10 {
        bail!("short UHID control request");
    }
    let id = get32(message, 4);
    match get32(message, 0) {
        9 => {
            let data = if message[9] == 0 {
                feature(message[8], mac)
            } else {
                None
            };
            let mut reply = event(10, 12 + data.as_ref().map_or(0, Vec::len));
            put32(&mut reply, 4, id);
            if let Some(data) = data {
                put16(&mut reply, 10, data.len() as u16);
                reply[12..].copy_from_slice(&data);
            } else {
                put16(&mut reply, 8, libc::EOPNOTSUPP as u16);
            }
            Ok(reply)
        }
        13 => {
            if message.len() < 12 {
                bail!("short UHID SET_REPORT request");
            }
            let size = usize::from(get16(message, 10));
            let mut reply = event(14, 10);
            put32(&mut reply, 4, id);
            let err = if size > DATA_MAX || message.len() < 12 + size {
                libc::EINVAL
            } else {
                libc::EOPNOTSUPP
            };
            put16(&mut reply, 8, err as u16);
            Ok(reply)
        }
        _ => bail!("not a UHID control request"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(kind: u32, number: u8, report_type: u8) -> Vec<u8> {
        let mut b = event(kind, 12);
        put32(&mut b, 4, 0x12345678);
        b[8] = number;
        b[9] = report_type;
        b
    }

    #[test]
    fn feature_control_preserves_request_and_report_ids() {
        for (number, size) in [(5, 41), (9, 20), (0x20, 64)] {
            let reply = control_reply(&request(9, number, 0), [1, 2, 3, 4, 5, 6]).unwrap();
            assert_eq!(get32(&reply, 0), 10);
            assert_eq!(get32(&reply, 4), 0x12345678);
            assert_eq!(get16(&reply, 8), 0);
            assert_eq!(get16(&reply, 10) as usize, size);
            assert_eq!(reply[12], number);
        }
        for (number, ty) in [(5, 1), (0xff, 0)] {
            let reply = control_reply(&request(9, number, ty), [0; 6]).unwrap();
            assert_eq!(get32(&reply, 4), 0x12345678);
            assert_eq!(get16(&reply, 8), libc::EOPNOTSUPP as u16);
            assert_eq!(get16(&reply, 10), 0);
        }
    }

    #[test]
    fn set_and_malformed_control_requests() {
        assert!(control_reply(&[0; 3], [0; 6]).is_err());
        assert!(control_reply(&event(13, 10), [0; 6]).is_err());
        let mut req = request(13, 5, 0);
        let reply = control_reply(&req, [0; 6]).unwrap();
        assert_eq!(
            (get32(&reply, 0), get32(&reply, 4), get16(&reply, 8)),
            (14, 0x12345678, libc::EOPNOTSUPP as u16)
        );
        put16(&mut req, 10, 1);
        assert_eq!(
            get16(&control_reply(&req, [0; 6]).unwrap(), 8),
            libc::EINVAL as u16
        );
        put16(&mut req, 10, 4097);
        assert_eq!(
            get16(&control_reply(&req, [0; 6]).unwrap(), 8),
            libc::EINVAL as u16
        );
    }

    #[test]
    fn calibration_and_unique_local_addresses() {
        let c = feature(5, [0; 6]).unwrap();
        let val = |at| i16::from_le_bytes([c[at], c[at + 1]]) as i32;
        for at in [7, 11, 15] {
            let span = val(at) - val(at + 2);
            assert_eq!(span, 65534);
            assert_eq!((val(19) + val(21)) * 32767 / span, 2000);
        }
        for at in [23, 27, 31] {
            assert_eq!(val(at) - val(at + 2), 16384);
        }
        let a = address();
        let b = address();
        assert_ne!(a, b);
        assert_eq!(a[5] & 3, 2);
        assert_eq!(&feature(9, a).unwrap()[1..7], &a);
    }

    #[test]
    fn output_bounds_and_types() {
        let mut b = event(6, EVENT_SIZE);
        assert!(output_report(&b[..4102]).is_err());
        b[4] = 2;
        b[4102] = 1;
        for size in [48, 63] {
            put16(&mut b, 4100, size);
            assert_eq!(output_report(&b).unwrap().len(), size as usize);
        }
        for size in [0, 47, 64, 4097] {
            put16(&mut b, 4100, size);
            assert!(output_report(&b).is_err());
        }
        put16(&mut b, 4100, 48);
        b[4102] = 0;
        assert!(output_report(&b).is_err());
        b[4102] = 1;
        b[4] = 3;
        assert!(output_report(&b).is_err());
    }

    #[test]
    fn start_feedback_stop_and_destroy_are_latched() {
        let file = tempfile::tempfile().unwrap();
        let mut pad = EdgePad {
            file,
            name: "test".into(),
            mac: [0; 6],
            created: true,
            started: false,
            origin: Instant::now(),
            sequence: 0,
            levels: None,
            output_count: 0,
        };
        assert!(pad
            .send(&State::default(), None, 1, Duration::ZERO)
            .is_err());
        pad.handle(&event(2, 4)).unwrap();
        pad.send(&State::default(), None, 1, Duration::ZERO)
            .unwrap();
        let mut b = event(6, EVENT_SIZE);
        put16(&mut b, 4100, 48);
        b[4102] = 1;
        b[4] = 2;
        b[5] = 1;
        b[8] = 255;
        assert_eq!(pad.handle(&b).unwrap(), Some((250, 0)));
        pad.stop_feedback();
        assert_eq!(pad.levels, None);
        pad.handle(&b).unwrap();
        assert_eq!(pad.handle(&event(5, 4)).unwrap(), Some((0, 0)));
        assert_eq!(pad.levels, None);
        assert_eq!(pad.handle(&event(3, 4)).unwrap(), Some((0, 0)));
        assert!(!pad.started);
        pad.destroy().unwrap();
        pad.destroy().unwrap();
    }

    #[test]
    fn create_buffer_and_short_writes() {
        assert_eq!(EVENT_SIZE, 4380);
        let c = create_message("Edge").unwrap();
        assert_eq!(
            (get32(&c, 0), get32(&c, 264), get32(&c, 268)),
            (11, 0x054c, 0x0df2)
        );
        assert_eq!(&c[280..], DESCRIPTOR);
        assert_eq!(get16(&c, 260) as usize, DESCRIPTOR.len());
        assert!(create_message(&"x".repeat(128)).is_err());
        assert!(write_event(&mut &mut [0u8; 2][..], &[1, 2, 3]).is_err());
    }

    #[test]
    fn poll_services_probe_and_limits_event_drain() {
        use std::os::fd::{FromRawFd, IntoRawFd};
        use std::os::unix::net::UnixDatagram;

        let (device, host) = UnixDatagram::pair().unwrap();
        device.set_nonblocking(true).unwrap();
        host.set_nonblocking(true).unwrap();
        // SAFETY: ownership of the socket descriptor moves into File once.
        let file = unsafe { File::from_raw_fd(device.into_raw_fd()) };
        let mut pad = EdgePad {
            file,
            name: "test".into(),
            mac: [1; 6],
            created: true,
            started: false,
            origin: Instant::now(),
            sequence: 0,
            levels: None,
            output_count: 0,
        };
        host.send(&request(9, 5, 0)).unwrap();
        for _ in 0..63 {
            host.send(&event(4, 4)).unwrap();
        }
        host.send(&event(2, 4)).unwrap();
        assert_eq!(pad.poll_host().unwrap(), None);
        assert!(!pad.started());
        let mut reply = [0; 128];
        let size = host.recv(&mut reply).unwrap();
        assert_eq!(size, 53);
        assert_eq!(get32(&reply, 4), 0x12345678);
        assert_eq!(pad.poll_host().unwrap(), None);
        assert!(pad.started());
        assert_eq!(pad.poll_host().unwrap(), None);
    }
}
