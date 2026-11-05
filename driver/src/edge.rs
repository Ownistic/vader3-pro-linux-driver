//! Virtual Bluetooth DualSense Edge control plane over Linux UHID.

use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use crate::edge_report::{self, Feedback, MotionSample};
use crate::identity::MacAddress;
use crate::input::State;
use crate::quick_access::QuickAccess;

// Packed native-endian ABI described by linux/uhid.h. HID fields are LE.
const DATA_MAX: usize = 4096;
// Accommodate CREATE2 with a maximal descriptor, including spare ABI room.
const EVENT_SIZE: usize = 4380;
const DESCRIPTOR: &[u8] = &[
    0x05, 0x01, 0x09, 0x05, 0xa1, 0x01, // Gamepad application
    0x06, 0x00, 0xff, 0x15, 0x00, 0x26, 0xff, 0x00, 0x75, 0x08, 0x85, 0x31, 0x09, 0x31, 0x95, 0x4d,
    0x81, 0x02, 0x09, 0x31, 0x95, 0x4d, 0x91, 0x02, 0x85, 0x05, 0x09, 0x05, 0x95, 0x28, 0xb1, 0x02,
    0x85, 0x09, 0x09, 0x09, 0x95, 0x13, 0xb1, 0x02, 0x85, 0x20, 0x09, 0x20, 0x95, 0x3f, 0xb1, 0x02,
    0xc0,
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

fn create_message(name: &str, mac: MacAddress) -> Result<Vec<u8>> {
    if name.is_empty() || name.len() > 127 || name.as_bytes().contains(&0) {
        bail!("UHID name must be 1..=127 bytes without NUL");
    }
    let mut out = event(11, 280 + DESCRIPTOR.len());
    out[4..4 + name.len()].copy_from_slice(name.as_bytes());
    let phys = format!("vader3p-steam/{}", mac.compact());
    let uniq = mac.to_string();
    out[132..132 + phys.len()].copy_from_slice(phys.as_bytes());
    out[196..196 + uniq.len()].copy_from_slice(uniq.as_bytes());
    put16(&mut out, 260, DESCRIPTOR.len() as u16);
    put16(&mut out, 262, 5); // BUS_BLUETOOTH, linux/input.h
                             // Linux hid-ids.h: Sony DualSense Edge identity.
    put32(&mut out, 264, 0x054c);
    put32(&mut out, 268, 0x0df2);
    put32(&mut out, 272, 0x0100);
    out[280..].copy_from_slice(DESCRIPTOR);
    Ok(out)
}

fn feature(number: u8, mac: MacAddress) -> Option<Vec<u8>> {
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
        data[1..7].copy_from_slice(&mac.reversed_pairing_bytes());
    }
    edge_report::bluetooth_append_crc(0xa3, &mut data);
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
    mac: MacAddress,
    created: bool,
    started: bool,
    opened: bool,
    quick_access: QuickAccess,
    origin: Instant,
    sequence: u8,
    levels: Option<(u8, u8)>,
    pending_color: Option<[u8; 3]>,
    color_cancelled: bool,
    output_count: u64,
}

impl EdgePad {
    #[cfg(test)]
    pub(crate) fn from_test_file(file: File, physical: MacAddress) -> Self {
        Self {
            file,
            name: "test".into(),
            mac: physical.virtual_address(),
            created: true,
            started: false,
            opened: false,
            quick_access: QuickAccess::default(),
            origin: Instant::now(),
            sequence: 0,
            levels: None,
            pending_color: None,
            color_cancelled: false,
            output_count: 0,
        }
    }
    pub fn create(name: &str, physical: MacAddress) -> Result<Self> {
        let mac = physical.virtual_address();
        let message = create_message(name, mac)?;
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
            mac,
            created: true,
            started: false,
            opened: false,
            quick_access: QuickAccess::default(),
            origin: Instant::now(),
            sequence: 0,
            levels: None,
            pending_color: None,
            color_cancelled: false,
            output_count: 0,
        })
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn virtual_address(&self) -> MacAddress {
        self.mac
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
        controls_valid: bool,
        sample: Option<MotionSample>,
        scale: [i32; 2],
        swap_xy: bool,
        stale_after: Duration,
    ) -> Result<()> {
        self.send_at(
            state,
            controls_valid,
            sample,
            scale,
            swap_xy,
            stale_after,
            self.origin.elapsed(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn send_at(
        &mut self,
        state: &State,
        controls_valid: bool,
        sample: Option<MotionSample>,
        scale: [i32; 2],
        swap_xy: bool,
        stale_after: Duration,
        elapsed: Duration,
    ) -> Result<()> {
        if !self.created || !self.started {
            bail!("Edge UHID device has not started");
        }
        let state = self
            .quick_access
            .compose(state, controls_valid, self.opened, elapsed);
        let report = edge_report::bluetooth_input_report(
            &state,
            sample,
            scale,
            swap_xy,
            stale_after,
            elapsed,
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

    /// Take the latest color received since the last call.
    pub fn take_color(&mut self) -> Option<[u8; 3]> {
        self.pending_color.take()
    }

    pub fn take_color_cancellation(&mut self) -> bool {
        std::mem::take(&mut self.color_cancelled)
    }

    pub(crate) fn cancel_quick_access(&mut self) {
        self.quick_access.cancel();
    }

    /// Destroy once. A failed write is returned; dropping the file still closes
    /// the UHID device, and repeated calls do not send another destroy.
    pub fn destroy(&mut self) -> Result<()> {
        if !self.created {
            return Ok(());
        }
        self.created = false;
        self.started = false;
        self.opened = false;
        self.cancel_quick_access();
        self.stop_feedback();
        self.pending_color = None;
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
            4 => {
                self.opened = true;
                self.cancel_quick_access();
                Ok(None)
            }
            3 | 5 => {
                if get32(message, 0) == 3 {
                    self.started = false;
                }
                if get32(message, 0) == 5 {
                    self.opened = false;
                }
                self.cancel_quick_access();
                self.stop_feedback();
                self.pending_color = None;
                self.color_cancelled = true;
                Ok(Some((0, 0)))
            }
            6 => {
                let Some(report) = output_report(message)? else {
                    return Ok(None);
                };
                self.output_count = self.output_count.saturating_add(1);
                let update = edge_report::bluetooth_feedback(report, self.levels.is_some());
                if let Some(color) = edge_report::bluetooth_lightbar(report) {
                    self.pending_color = Some(color);
                }
                let next = match update {
                    Some(Feedback::Levels(l, r)) => Some((l, r)),
                    Some(Feedback::Stop) => Some((0, 0)),
                    None => None,
                };
                let changed = next.is_some_and(|v| Some(v).filter(|v| *v != (0, 0)) != self.levels);
                if self.output_count <= 16 || changed {
                    eprintln!(
                        "UHID output #{} len={} id={:02x} header={:02x?}",
                        self.output_count,
                        report.len(),
                        report[0],
                        &report[1..3]
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

fn output_report(message: &[u8]) -> Result<Option<&[u8]>> {
    if message.len() < 4103 {
        bail!("short UHID OUTPUT event");
    }
    let size = usize::from(get16(message, 4100));
    if size > DATA_MAX {
        bail!("oversized UHID OUTPUT report: {size}");
    }
    if size != 78 || message[4102] != 1 || message[4] != 0x31 {
        return Ok(None);
    }
    Ok(Some(&message[4..4 + size]))
}

fn control_reply(message: &[u8], mac: MacAddress) -> Result<Vec<u8>> {
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
    use crate::input::button;
    use std::os::fd::{FromRawFd, IntoRawFd};
    use std::os::unix::net::UnixDatagram;

    fn test_mac() -> MacAddress {
        "12:34:56:78:9A:BC"
            .parse::<MacAddress>()
            .unwrap()
            .virtual_address()
    }

    fn test_pad() -> (EdgePad, UnixDatagram) {
        let (device, host) = UnixDatagram::pair().unwrap();
        let file = unsafe { File::from_raw_fd(device.into_raw_fd()) };
        (
            EdgePad {
                file,
                name: "test".into(),
                mac: test_mac(),
                created: true,
                started: false,
                opened: false,
                quick_access: QuickAccess::default(),
                origin: Instant::now(),
                sequence: 0,
                levels: None,
                pending_color: None,
                color_cancelled: false,
                output_count: 0,
            },
            host,
        )
    }

    fn wire(
        pad: &mut EdgePad,
        host: &UnixDatagram,
        state: State,
        valid: bool,
        ms: u64,
    ) -> [u8; 78] {
        pad.send_at(
            &state,
            valid,
            None,
            [1, 1],
            false,
            Duration::ZERO,
            Duration::from_millis(ms),
        )
        .unwrap();
        let mut packet = [0; 84];
        assert_eq!(host.recv(&mut packet).unwrap(), 84);
        assert_eq!(get16(&packet, 4), 78);
        assert_eq!(packet[6], 0x31);
        assert!(edge_report::bluetooth_verify_crc(0xa1, &packet[6..]));
        packet[6..].try_into().unwrap()
    }

    fn state(buttons: u16) -> State {
        State {
            buttons,
            extras: 0x3f,
            ..State::default()
        }
    }

    fn chord(report: [u8; 78]) -> (u8, u8) {
        (report[9] & 0x20, report[11] & 1)
    }

    #[test]
    fn emitted_short_tap_boundaries_and_long_hold_do_not_repeat() {
        let (mut pad, host) = test_pad();
        let up = state(0);
        let down = state(1 << button::FN);
        assert!(pad
            .send_at(
                &up,
                true,
                None,
                [1, 1],
                false,
                Duration::ZERO,
                Duration::ZERO
            )
            .is_err());
        pad.handle(&event(4, 4)).unwrap();
        pad.handle(&event(2, 4)).unwrap();
        assert_eq!(chord(wire(&mut pad, &host, down, true, 0)), (0, 0));
        assert_eq!(chord(wire(&mut pad, &host, up, true, 1)), (0, 0));
        assert_eq!(chord(wire(&mut pad, &host, down, true, 2)), (0, 1));
        assert_eq!(chord(wire(&mut pad, &host, up, true, 101)), (0, 1));
        assert_eq!(chord(wire(&mut pad, &host, up, true, 102)), (0x20, 1));
        assert_eq!(chord(wire(&mut pad, &host, down, true, 161)), (0x20, 1));
        assert_eq!(chord(wire(&mut pad, &host, down, true, 162)), (0, 1));
        assert_eq!(chord(wire(&mut pad, &host, down, true, 201)), (0, 1));
        assert_eq!(chord(wire(&mut pad, &host, down, true, 202)), (0, 0));
        assert_eq!(chord(wire(&mut pad, &host, down, true, 1000)), (0, 0));
        assert_eq!(chord(wire(&mut pad, &host, up, true, 1001)), (0, 0));
        assert_eq!(chord(wire(&mut pad, &host, down, true, 1002)), (0, 1));
    }

    #[test]
    fn before_start_and_after_destroy_never_emit_a_pulse() {
        let (mut pad, host) = test_pad();
        let down = state(1 << button::FN);
        pad.handle(&event(4, 4)).unwrap();
        assert!(pad
            .send_at(
                &down,
                true,
                None,
                [1, 1],
                false,
                Duration::ZERO,
                Duration::from_secs(10)
            )
            .is_err());
        pad.handle(&event(2, 4)).unwrap();
        assert_eq!(chord(wire(&mut pad, &host, down, true, 20_000)), (0, 0));
        wire(&mut pad, &host, state(0), true, 20_001);
        assert_eq!(chord(wire(&mut pad, &host, down, true, 20_002)), (0, 1));
        pad.destroy().unwrap();
        let mut event = [0; 4];
        assert_eq!(host.recv(&mut event).unwrap(), 4);
        assert!(pad
            .send_at(
                &down,
                true,
                None,
                [1, 1],
                false,
                Duration::ZERO,
                Duration::from_secs(30)
            )
            .is_err());
    }

    #[test]
    fn emitted_phases_never_skip_on_large_jumps_and_ignore_represses() {
        let (mut pad, host) = test_pad();
        pad.handle(&event(2, 4)).unwrap();
        pad.handle(&event(4, 4)).unwrap();
        let up = state(0);
        let down = state(1 << button::FN);
        wire(&mut pad, &host, up, true, 0);
        assert_eq!(chord(wire(&mut pad, &host, down, true, 1)), (0, 1));
        assert_eq!(chord(wire(&mut pad, &host, up, true, 10000)), (0x20, 1));
        assert_eq!(chord(wire(&mut pad, &host, down, true, 20000)), (0, 1));
        assert_eq!(chord(wire(&mut pad, &host, down, true, 30000)), (0, 0));
        assert_eq!(chord(wire(&mut pad, &host, down, true, 40000)), (0, 0));
    }

    #[test]
    fn invalid_and_host_lifecycle_cancel_and_require_fresh_release() {
        for phase in [0, 100, 160] {
            for action in [0, 3, 5, 9] {
                let (mut pad, host) = test_pad();
                pad.handle(&event(2, 4)).unwrap();
                pad.handle(&event(4, 4)).unwrap();
                let up = state(0);
                let down = state(1 << button::FN);
                wire(&mut pad, &host, up, true, 0);
                wire(&mut pad, &host, down, true, 1);
                if phase >= 100 {
                    wire(&mut pad, &host, down, true, 101);
                }
                if phase >= 160 {
                    wire(&mut pad, &host, down, true, 161);
                }
                match action {
                    0 => {
                        assert_eq!(chord(wire(&mut pad, &host, up, false, 200)), (0, 0));
                    }
                    3 => {
                        pad.handle(&event(3, 4)).unwrap();
                        pad.handle(&event(2, 4)).unwrap();
                    }
                    5 => {
                        pad.handle(&event(5, 4)).unwrap();
                        assert_eq!(chord(wire(&mut pad, &host, down, true, 200)), (0, 0));
                        pad.handle(&event(4, 4)).unwrap();
                    }
                    _ => pad.cancel_quick_access(),
                }
                assert_eq!(chord(wire(&mut pad, &host, down, true, 300)), (0, 0));
                assert_eq!(chord(wire(&mut pad, &host, up, true, 301)), (0, 0));
                assert_eq!(chord(wire(&mut pad, &host, down, true, 302)), (0, 1));
            }
        }
    }

    #[test]
    fn physical_controls_are_preserved_through_every_phase() {
        for physical in [
            0,
            1 << button::A,
            1 << button::HOME,
            (1 << button::A) | (1 << button::HOME),
        ] {
            let (mut pad, host) = test_pad();
            pad.handle(&event(2, 4)).unwrap();
            pad.handle(&event(4, 4)).unwrap();
            let base = physical | (1 << button::B) | (1 << button::LB);
            wire(&mut pad, &host, state(base), true, 0);
            for (ms, synthetic) in [(1, (0, 1)), (101, (0x20, 1)), (161, (0, 1)), (201, (0, 0))] {
                let report = wire(&mut pad, &host, state(base | (1 << button::FN)), true, ms);
                assert_eq!(
                    chord(report),
                    (
                        synthetic.0
                            | if physical & (1 << button::A) != 0 {
                                0x20
                            } else {
                                0
                            },
                        synthetic.1
                            | if physical & (1 << button::HOME) != 0 {
                                1
                            } else {
                                0
                            }
                    )
                );
                assert_eq!(report[9] & 0x40, 0x40);
                assert_eq!(report[10] & 1, 1);
                assert_eq!(report[11] & 0xf6, 0xf6);
            }
            let report = wire(&mut pad, &host, state(0), true, 202);
            assert_eq!(chord(report), (0, 0));
        }
    }

    #[test]
    fn physical_a_and_home_can_change_during_pulse() {
        let (mut pad, host) = test_pad();
        pad.handle(&event(2, 4)).unwrap();
        pad.handle(&event(4, 4)).unwrap();
        let fn_bit = 1 << button::FN;
        wire(&mut pad, &host, state(0), true, 0);
        assert_eq!(
            chord(wire(
                &mut pad,
                &host,
                state(fn_bit | (1 << button::A)),
                true,
                1
            )),
            (0x20, 1)
        );
        assert_eq!(
            chord(wire(
                &mut pad,
                &host,
                state(fn_bit | (1 << button::HOME)),
                true,
                101
            )),
            (0x20, 1)
        );
        assert_eq!(
            chord(wire(
                &mut pad,
                &host,
                state(fn_bit | (1 << button::A)),
                true,
                161
            )),
            (0x20, 1)
        );
        assert_eq!(
            chord(wire(
                &mut pad,
                &host,
                state(fn_bit | (1 << button::HOME)),
                true,
                201
            )),
            (0, 1)
        );
        assert_eq!(
            chord(wire(&mut pad, &host, state(fn_bit), true, 202)),
            (0, 0)
        );
    }

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
            let reply = control_reply(&request(9, number, 0), test_mac()).unwrap();
            assert_eq!(get32(&reply, 0), 10);
            assert_eq!(get32(&reply, 4), 0x12345678);
            assert_eq!(get16(&reply, 8), 0);
            assert_eq!(get16(&reply, 10) as usize, size);
            assert_eq!(reply[12], number);
        }
        for (number, ty) in [(5, 1), (0xff, 0)] {
            let reply = control_reply(&request(9, number, ty), test_mac()).unwrap();
            assert_eq!(get32(&reply, 4), 0x12345678);
            assert_eq!(get16(&reply, 8), libc::EOPNOTSUPP as u16);
            assert_eq!(get16(&reply, 10), 0);
        }
    }

    #[test]
    fn set_and_malformed_control_requests() {
        assert!(control_reply(&[0; 3], test_mac()).is_err());
        assert!(control_reply(&event(13, 10), test_mac()).is_err());
        let mut req = request(13, 5, 0);
        let reply = control_reply(&req, test_mac()).unwrap();
        assert_eq!(
            (get32(&reply, 0), get32(&reply, 4), get16(&reply, 8)),
            (14, 0x12345678, libc::EOPNOTSUPP as u16)
        );
        put16(&mut req, 10, 1);
        assert_eq!(
            get16(&control_reply(&req, test_mac()).unwrap(), 8),
            libc::EINVAL as u16
        );
        put16(&mut req, 10, 4097);
        assert_eq!(
            get16(&control_reply(&req, test_mac()).unwrap(), 8),
            libc::EINVAL as u16
        );
    }

    #[test]
    fn calibration_and_unique_local_addresses() {
        let c = feature(5, test_mac()).unwrap();
        let val = |at| i16::from_le_bytes([c[at], c[at + 1]]) as i32;
        for at in [7, 11, 15] {
            let span = val(at) - val(at + 2);
            assert_eq!(span, 65534);
            assert_eq!((val(19) + val(21)) * 32767 / span, 2000);
        }
        for at in [23, 27, 31] {
            assert_eq!(val(at) - val(at + 2), 16384);
        }
        let a = test_mac();
        let b = "01:23:45:67:89:AB"
            .parse::<MacAddress>()
            .unwrap()
            .virtual_address();
        assert_ne!(a, b);
        assert_eq!(a.bytes()[0] & 3, 2);
        assert_eq!(&feature(9, a).unwrap()[1..7], &a.reversed_pairing_bytes());
        // Independent Python zlib CRC values for this fabricated feature data.
        for (number, expected) in [
            (5, [0x7d, 0x56, 0xdc, 0x78]),
            (9, [0xef, 0x1a, 0x51, 0xf5]),
            (0x20, [0xcf, 0xd5, 0x4c, 0x5d]),
        ] {
            let feature = feature(number, a).unwrap();
            assert_eq!(&feature[feature.len() - 4..], &expected);
        }
    }

    #[test]
    fn output_bounds_and_types() {
        let mut b = event(6, EVENT_SIZE);
        assert!(output_report(&b[..4102]).is_err());
        b[4] = 0x31;
        b[4102] = 1;
        put16(&mut b, 4100, 78);
        assert_eq!(output_report(&b).unwrap().unwrap().len(), 78);
        for size in [0, 2, 48, 63, 77, 79] {
            put16(&mut b, 4100, size);
            assert!(output_report(&b).unwrap().is_none());
        }
        put16(&mut b, 4100, 4097);
        assert!(output_report(&b).is_err());
        put16(&mut b, 4100, 78);
        b[4102] = 0;
        assert!(output_report(&b).unwrap().is_none());
        b[4102] = 1;
        b[4] = 3;
        assert!(output_report(&b).unwrap().is_none());
    }

    #[test]
    fn start_feedback_stop_and_destroy_are_latched() {
        let file = tempfile::tempfile().unwrap();
        let mut pad = EdgePad {
            file,
            name: "test".into(),
            mac: test_mac(),
            created: true,
            started: false,
            opened: false,
            quick_access: QuickAccess::default(),
            origin: Instant::now(),
            sequence: 0,
            levels: None,
            pending_color: None,
            color_cancelled: false,
            output_count: 0,
        };
        assert!(pad
            .send(&State::default(), true, None, [1, 1], true, Duration::ZERO)
            .is_err());
        pad.handle(&event(2, 4)).unwrap();
        pad.send(&State::default(), true, None, [1, 1], true, Duration::ZERO)
            .unwrap();
        let mut b = event(6, EVENT_SIZE);
        put16(&mut b, 4100, 78);
        b[4102] = 1;
        b[4] = 0x31;
        b[5] = 0;
        b[6] = 0x10;
        b[7] = 1;
        b[10] = 255;
        edge_report::bluetooth_append_crc(0xa2, &mut b[4..82]);
        assert_eq!(pad.handle(&b).unwrap(), Some((250, 0)));
        for (size, kind, id) in [(2, 1, 0x31), (48, 1, 0x02), (78, 1, 0x02), (78, 0, 0x31)] {
            let mut unrelated = b.clone();
            put16(&mut unrelated, 4100, size);
            unrelated[4102] = kind;
            unrelated[4] = id;
            assert_eq!(pad.handle(&unrelated).unwrap(), None);
            assert_eq!(pad.levels, Some((250, 0)));
        }
        b[10] ^= 1;
        assert_eq!(pad.handle(&b).unwrap(), None);
        assert_eq!(pad.levels, Some((250, 0)));
        b[10] ^= 1;
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
    fn lightbar_is_independent_of_rumble_and_cancelled_on_close() {
        let (mut pad, _) = test_pad();
        let mut event = event(6, EVENT_SIZE);
        put16(&mut event, 4100, 78);
        event[4102] = 1;
        event[4] = 0x31;
        event[6] = 0x10;
        event[8] = 4;
        event[51..54].copy_from_slice(&[1, 2, 3]);
        edge_report::bluetooth_append_crc(0xa2, &mut event[4..82]);
        assert_eq!(pad.handle(&event).unwrap(), None);
        assert_eq!(pad.take_color(), Some([1, 2, 3]));
        event[51..54].copy_from_slice(&[4, 5, 6]);
        edge_report::bluetooth_append_crc(0xa2, &mut event[4..82]);
        pad.handle(&event).unwrap();
        let mut rumble = event.clone();
        rumble[8] = 0;
        rumble[7] = 1;
        rumble[10] = 255;
        edge_report::bluetooth_append_crc(0xa2, &mut rumble[4..82]);
        assert_eq!(pad.handle(&rumble).unwrap(), Some((250, 0)));
        assert_eq!(pad.take_color(), Some([4, 5, 6]));
        pad.handle(&event).unwrap();
        pad.handle(&super::event(5, 4)).unwrap();
        assert_eq!(pad.take_color(), None);
        assert!(pad.take_color_cancellation());
        assert!(!pad.take_color_cancellation());
    }

    #[test]
    fn create_buffer_and_short_writes() {
        assert_eq!(EVENT_SIZE, 4380);
        let c = create_message("Edge", test_mac()).unwrap();
        assert_eq!(
            (get32(&c, 0), get32(&c, 264), get32(&c, 268)),
            (11, 0x054c, 0x0df2)
        );
        assert_eq!(&c[280..], DESCRIPTOR);
        assert_eq!(get16(&c, 262), 5);
        assert_eq!(&c[132..158], b"vader3p-steam/D20076FC8378");
        assert_eq!(&c[196..213], b"D2:00:76:FC:83:78");
        assert_eq!(get16(&c, 260) as usize, DESCRIPTOR.len());
        assert!(create_message(&"x".repeat(128), test_mac()).is_err());
        assert!(create_message("", test_mac()).is_err());
        assert!(create_message("bad\0name", test_mac()).is_err());
        assert!(write_event(&mut &mut [0u8; 2][..], &[1, 2, 3]).is_err());
    }

    #[test]
    fn bluetooth_descriptor_and_identity_are_stable_and_consistent() {
        let mut id = 0;
        let mut count = 0;
        let mut reports = Vec::new();
        let mut pos = 0;
        while pos < DESCRIPTOR.len() {
            let prefix = DESCRIPTOR[pos];
            let size = match prefix & 3 {
                3 => 4,
                n => usize::from(n),
            };
            if size == 1 {
                let value = DESCRIPTOR[pos + 1];
                match prefix {
                    0x85 => id = value,
                    0x95 => count = value,
                    0x81 | 0x91 | 0xb1 => reports.push((prefix, id, count)),
                    _ => {}
                }
            }
            pos += 1 + size;
        }
        assert_eq!(
            reports,
            [
                (0x81, 0x31, 77),
                (0x91, 0x31, 77),
                (0xb1, 5, 40),
                (0xb1, 9, 19),
                (0xb1, 0x20, 63)
            ]
        );
        let first = test_mac();
        let second = "01:23:45:67:89:AB"
            .parse::<MacAddress>()
            .unwrap()
            .virtual_address();
        let c1 = create_message("Edge", first).unwrap();
        let c1_again = create_message("Edge", test_mac()).unwrap();
        let c2 = create_message("Edge", second).unwrap();
        assert_eq!(c1, c1_again);
        assert_ne!(&c1[196..260], &c2[196..260]);
        assert_eq!(
            &feature(9, first).unwrap()[1..7],
            &first.reversed_pairing_bytes()
        );
        assert_eq!(
            &feature(9, second).unwrap()[1..7],
            &second.reversed_pairing_bytes()
        );
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
            mac: test_mac(),
            created: true,
            started: false,
            opened: false,
            quick_access: QuickAccess::default(),
            origin: Instant::now(),
            sequence: 0,
            levels: None,
            pending_color: None,
            color_cancelled: false,
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
