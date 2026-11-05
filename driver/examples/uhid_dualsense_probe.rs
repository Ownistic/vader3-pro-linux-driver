//! Disposable synthetic DualSense Edge UHID experiment; see docs/evidence/uhid-dualsense-probe.md.
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

// UHID ABI: https://docs.kernel.org/hid/uhid.html and linux/uhid.h.
const MAX: usize = 4096;
const EVENT_SIZE: usize = 4380; // CREATE2 includes a 4096-byte descriptor.
const CREATE2: u32 = 11;
const INPUT2: u32 = 12;
const GET_REPLY: u32 = 10;
const SET_REPLY: u32 = 14;
const DESTROY: u32 = 1;
static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn stop(_: libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
}

// Minimal independently authored descriptor: six byte axes and vendor-defined
// remainder; numbered output and feature reports with USB wire lengths.
// HID item semantics: https://www.usb.org/document-library/device-class-definition-hid-111
const DESCRIPTOR: &[u8] = &[
    0x05, 0x01, 0x09, 0x05, 0xa1, 0x01, 0x85, 0x01, 0x15, 0x00, 0x26, 0xff, 0x00, 0x75, 0x08, 0x95,
    0x06, 0x09, 0x30, 0x09, 0x31, 0x09, 0x33, 0x09, 0x34, 0x09, 0x32, 0x09, 0x35, 0x81, 0x02, 0x06,
    0x00, 0xff, 0x09, 0x01, 0x95, 0x39, 0x81, 0x02, 0x85, 0x02, 0x09, 0x02, 0x95, 0x3e, 0x91, 0x02,
    0x85, 0x05, 0x09, 0x05, 0x95, 0x28, 0xb1, 0x02, 0x85, 0x09, 0x09, 0x09, 0x95, 0x13, 0xb1, 0x02,
    0x85, 0x20, 0x09, 0x20, 0x95, 0x3f, 0xb1, 0x02, 0xc0,
];

fn put16(b: &mut [u8], at: usize, n: u16) {
    b[at..at + 2].copy_from_slice(&n.to_ne_bytes());
}
fn put32(b: &mut [u8], at: usize, n: u32) {
    b[at..at + 4].copy_from_slice(&n.to_ne_bytes());
}
fn get16(b: &[u8], at: usize) -> u16 {
    u16::from_ne_bytes([b[at], b[at + 1]])
}
fn get32(b: &[u8], at: usize) -> u32 {
    u32::from_ne_bytes(b[at..at + 4].try_into().unwrap())
}

fn event(kind: u32, len: usize) -> Vec<u8> {
    let mut b = vec![0; len];
    put32(&mut b, 0, kind);
    b
}

fn create_event() -> Vec<u8> {
    let mut b = event(CREATE2, 280 + DESCRIPTOR.len());
    let name = b"Virtual DualSense Edge probe";
    b[4..4 + name.len()].copy_from_slice(name);
    put16(&mut b, 260, DESCRIPTOR.len() as u16);
    put16(&mut b, 262, 0x03); // BUS_USB (linux/input.h)
                              // Linux hid-ids.h: Sony and DualSense Edge USB IDs.
    put32(&mut b, 264, 0x054c);
    put32(&mut b, 268, 0x0df2);
    put32(&mut b, 272, 0x0100);
    b[280..].copy_from_slice(DESCRIPTOR);
    b
}

fn calibration() -> Vec<u8> {
    let mut b = vec![0; 41];
    b[0] = 5;
    // Fabricated symmetric calibration: nonzero denominators, unity scaling in
    // Linux hid-playstation units. These are not measured device calibration.
    for offset in [7, 11, 15] {
        b[offset..offset + 2].copy_from_slice(&1024i16.to_le_bytes());
        b[offset + 2..offset + 4].copy_from_slice(&(-1024i16).to_le_bytes());
    }
    for offset in [19, 21] {
        b[offset..offset + 2].copy_from_slice(&1i16.to_le_bytes());
    }
    for offset in [23, 27, 31] {
        b[offset..offset + 2].copy_from_slice(&8192i16.to_le_bytes());
        b[offset + 2..offset + 4].copy_from_slice(&(-8192i16).to_le_bytes());
    }
    b
}

fn feature(report: u8) -> Option<Vec<u8>> {
    match report {
        5 => Some(calibration()),
        9 => {
            let mut b = vec![0; 20];
            b[0] = 9;
            // Locally administered, fabricated address, not a hardware address.
            b[1..7].copy_from_slice(&[0x02, 0x46, 0x44, 0x50, 0x00, 0x01]);
            Some(b)
        }
        0x20 => {
            let mut b = vec![0; 64];
            b[0] = 0x20;
            Some(b)
        }
        _ => None,
    }
}

fn report(elapsed: Duration, sequence: u8, synthetic_gyro: bool) -> [u8; 64] {
    let mut b = [0; 64];
    b[0] = 1;
    b[1..5].fill(128);
    b[7] = sequence;
    b[8] = 8; // hat neutral
    if synthetic_gyro {
        // Alternating fabricated angular velocity, not a physical sensor reading.
        let value: i16 = if elapsed.as_secs() % 2 == 0 {
            1024
        } else {
            -1024
        };
        b[16..18].copy_from_slice(&value.to_le_bytes());
    }
    b[26..28].copy_from_slice(&8192i16.to_le_bytes()); // stationary +1g on Z
                                                       // hid-playstation describes the sensor clock as three ticks per microsecond.
    let ticks = (elapsed.as_micros().wrapping_mul(3) & u32::MAX as u128) as u32;
    b[28..32].copy_from_slice(&ticks.to_le_bytes());
    b[33] = 0x80;
    b[37] = 0x80; // inactive contacts
    b[53] = 0x20; // full battery
    b
}

fn send(file: &mut File, bytes: &[u8]) -> Result<()> {
    // UHID writes are messages: never retry a partial write as another event.
    let n = file.write(bytes).context("writing UHID event")?;
    if n != bytes.len() {
        bail!("short UHID write: {n}/{}", bytes.len());
    }
    Ok(())
}

struct Device(File);
impl Drop for Device {
    fn drop(&mut self) {
        if let Err(e) = send(&mut self.0, &event(DESTROY, 4)) {
            eprintln!("UHID destroy: {e:#}");
        }
    }
}

fn handle(file: &mut File, b: &[u8]) -> Result<()> {
    let kind = get32(b, 0);
    match kind {
        2 => println!(
            "UHID START flags={:#x}",
            u64::from_ne_bytes(b[4..12].try_into().unwrap())
        ),
        3 => println!("UHID STOP"),
        4 => println!("UHID OPEN"),
        5 => println!("UHID CLOSE"),
        6 => {
            let size = usize::from(get16(b, 4 + MAX));
            if size > MAX {
                bail!("invalid OUTPUT size {size}");
            }
            println!(
                "UHID OUTPUT type={} size={size} data={:02x?}",
                b[4 + MAX + 2],
                &b[4..4 + size]
            );
        }
        9 => {
            let id = get32(b, 4);
            let number = b[8];
            let data = if b[9] == 0 { feature(number) } else { None };
            println!(
                "UHID GET_REPORT id={id} report={number:#04x} type={} supported={}",
                b[9],
                data.is_some()
            );
            let mut reply = event(GET_REPLY, 12 + data.as_ref().map_or(0, Vec::len));
            put32(&mut reply, 4, id);
            if let Some(data) = data {
                put16(&mut reply, 10, data.len() as u16);
                reply[12..].copy_from_slice(&data);
            } else {
                put16(&mut reply, 8, libc::EOPNOTSUPP as u16);
            }
            send(file, &reply)?;
        }
        13 => {
            let id = get32(b, 4);
            let size = usize::from(get16(b, 10));
            println!(
                "UHID SET_REPORT id={id} report={:#04x} type={} size={size}",
                b[8], b[9]
            );
            let mut reply = event(SET_REPLY, 10);
            put32(&mut reply, 4, id);
            // No persistent writable features are emulated.
            put16(
                &mut reply,
                8,
                if size > MAX {
                    libc::EINVAL
                } else {
                    libc::EOPNOTSUPP
                } as u16,
            );
            send(file, &reply)?;
        }
        _ => println!("UHID event type={kind}"),
    }
    Ok(())
}

fn run() -> Result<()> {
    let mut synthetic = false;
    let mut seconds = 600u64;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("Usage: uhid_dualsense_probe [--synthetic-gyro] [--seconds N]\n\nCreates a temporary virtual DualSense Edge; no physical controller is accessed.\nDefault duration: 600 seconds. --synthetic-gyro produces fabricated motion.");
                return Ok(());
            }
            "--synthetic-gyro" => synthetic = true,
            "--seconds" => {
                seconds = args
                    .next()
                    .context("--seconds requires a value")?
                    .parse()
                    .context("invalid --seconds")?;
                if !(1..=86400).contains(&seconds) {
                    bail!("--seconds must be between 1 and 86400");
                }
            }
            _ => bail!("unknown argument {arg}; use --help"),
        }
    }
    // signal() handler only updates a lock-free atomic flag.
    unsafe {
        if libc::signal(libc::SIGINT, stop as *const () as libc::sighandler_t) == libc::SIG_ERR
            || libc::signal(libc::SIGTERM, stop as *const () as libc::sighandler_t) == libc::SIG_ERR
        {
            return Err(io::Error::last_os_error()).context("installing signal handlers");
        }
    }
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open("/dev/uhid")
        .context("opening /dev/uhid; owner must grant access if denied")?;
    send(&mut file, &create_event()).context("creating virtual device")?;
    let mut device = Device(file);
    println!(
        "CREATE2 sent; duration={seconds}s synthetic_gyro={synthetic}; waiting for kernel events"
    );
    let start = Instant::now();
    let mut next = start;
    let mut seq = 0u8;
    while !STOP.load(Ordering::Relaxed) && start.elapsed() < Duration::from_secs(seconds) {
        // Drain events first, including feature requests during driver probe.
        for _ in 0..64 {
            let mut b = [0u8; EVENT_SIZE];
            match device.0.read(&mut b) {
                Ok(0) => bail!("UHID returned EOF"),
                Ok(n) if n >= 4 => handle(&mut device.0, &b)?,
                Ok(n) => bail!("short UHID event header: {n}"),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e).context("reading UHID event"),
            }
        }
        let now = Instant::now();
        if now >= next {
            let input = report(start.elapsed(), seq, synthetic);
            let mut msg = event(INPUT2, 6 + input.len());
            put16(&mut msg, 4, input.len() as u16);
            msg[6..].copy_from_slice(&input);
            send(&mut device.0, &msg).context("sending synthetic input")?;
            seq = seq.wrapping_add(1);
            next = now + Duration::from_millis(4);
        }
        let mut pfd = libc::pollfd {
            fd: device.0.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let result = unsafe { libc::poll(&mut pfd, 1, 4) };
        if result < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            return Err(io::Error::last_os_error()).context("polling UHID");
        }
        if pfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            bail!("UHID poll error flags {:#x}", pfd.revents);
        }
    }
    println!("Stopping virtual probe");
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("probe failed: {e:#}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn feature_lengths_ids_and_calibration_are_valid() {
        for (id, len) in [(5, 41), (9, 20), (0x20, 64)] {
            let b = feature(id).unwrap();
            assert_eq!(b.len(), len);
            assert_eq!(b[0], id);
        }
        assert!(feature(0xff).is_none());
        let c = calibration();
        for at in [7, 11, 15, 23, 27, 31] {
            assert_ne!(
                i16::from_le_bytes([c[at], c[at + 1]]),
                i16::from_le_bytes([c[at + 2], c[at + 3]])
            );
        }
    }
    #[test]
    fn input_is_neutral_and_clock_tracks_elapsed_time() {
        let b = report(Duration::from_millis(4), 7, false);
        assert_eq!(&b[..9], &[1, 128, 128, 128, 128, 0, 0, 7, 8]);
        assert_eq!(&b[16..22], &[0; 6]);
        assert_eq!(i16::from_le_bytes([b[26], b[27]]), 8192);
        assert_eq!(u32::from_le_bytes(b[28..32].try_into().unwrap()), 12000);
        assert_eq!((b[33], b[37]), (128, 128));
        assert_ne!(&report(Duration::ZERO, 0, true)[16..18], &[0, 0]);
    }
    #[test]
    fn create_has_edge_identity_and_descriptor() {
        let b = create_event();
        assert_eq!(get32(&b, 0), CREATE2);
        assert_eq!(get32(&b, 264), 0x054c);
        assert_eq!(get32(&b, 268), 0x0df2);
        assert_eq!(usize::from(get16(&b, 260)), DESCRIPTOR.len());
        assert_eq!(&b[280..], DESCRIPTOR);
    }
}
