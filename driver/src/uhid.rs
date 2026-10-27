//! UHID virtual device.
//!
//! Creates a `/dev/uhid` device that the kernel presents as a HID gamepad, and
//! services its control events, including rumble output reports.
//!
//! Offsets below are taken from `/usr/include/linux/uhid.h`. Every structure is
//! `__attribute__((__packed__))`, so the C layout is the byte layout and no
//! Rust-side alignment is needed. Only the events this driver needs are
//! handled; anything else is reported as [`Event::Other`] so a kernel extension
//! cannot desynchronise the reader.

use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::OpenOptionsExt;

use anyhow::{Context, Result};

use crate::descriptor;

/// `UHID_DATA_MAX`.
const UHID_DATA_MAX: usize = 4096;

/// Generous read buffer covering the union of every request struct.
const EVENT_BUFFER: usize = 4 + 2 + UHID_DATA_MAX + 8;

/// `enum uhid_event_type` discriminants, from the kernel header.
mod ev {
    pub const DESTROY: u32 = 1;
    pub const START: u32 = 2;
    pub const STOP: u32 = 3;
    pub const OPEN: u32 = 4;
    pub const CLOSE: u32 = 5;
    pub const OUTPUT: u32 = 6;
    pub const GET_REPORT: u32 = 9;
    pub const GET_REPORT_REPLY: u32 = 10;
    pub const CREATE2: u32 = 11;
    pub const INPUT2: u32 = 12;
}

/// `struct uhid_create2_req` byte offsets. The 4-byte `type` field of
/// `struct uhid_event` precedes this union.
const CREATE2_NAME_OFF: usize = 4;
const CREATE2_PHYS_OFF: usize = CREATE2_NAME_OFF + 128;
const CREATE2_UNIQ_OFF: usize = CREATE2_PHYS_OFF + 64;
const CREATE2_RD_SIZE_OFF: usize = CREATE2_UNIQ_OFF + 64;
const CREATE2_BUS_OFF: usize = CREATE2_RD_SIZE_OFF + 2;
const CREATE2_VENDOR_OFF: usize = CREATE2_BUS_OFF + 2;
const CREATE2_RD_DATA_OFF: usize = CREATE2_VENDOR_OFF + 16;

/// `UHID_DEV_NUMBERED_INPUT_REPORTS`.
#[allow(dead_code)]
const DEV_FLAG_NUMBERED_INPUT: u64 = 1 << 2;
/// `UHID_DEV_NUMBERED_OUTPUT_REPORTS`.
#[allow(dead_code)]
const DEV_FLAG_NUMBERED_OUTPUT: u64 = 1 << 1;

/// `struct uhid_input2_req` payload offset within the event.
const INPUT2_DATA_OFF: usize = 4 + 2;

/// `struct uhid_output_req` field offsets within the event.
const OUTPUT_DATA_OFF: usize = 4;
const OUTPUT_SIZE_OFF: usize = OUTPUT_DATA_OFF + UHID_DATA_MAX;
#[allow(dead_code)]
const OUTPUT_RTYPE_OFF: usize = OUTPUT_SIZE_OFF + 2;

/// An open UHID device.
pub struct UhidDevice {
    file: File,
    name: String,
    started: bool,
}

impl UhidDevice {
    /// Create the virtual device.
    ///
    /// `bus`, `vendor`, `product` and `version` control how the kernel labels
    /// the device. They are deliberately not a real vendor's IDs: impersonating
    /// hardware the device is not would mislead both users and Steam's device
    /// lookup tables.
    ///
    /// Note `dev_flags` lives in `uhid_start_req`, not in CREATE2. Numbered
    /// report handling is therefore not requested here; the descriptor declares
    /// report IDs itself, which is what determines the wire format.
    pub fn create(name: &str) -> Result<Self> {
        Self::create_with_descriptor(name, descriptor::DESCRIPTOR)
    }

    /// Create the virtual device with an explicit descriptor.
    ///
    /// Exposed separately so descriptor experiments can run against the real
    /// kernel without editing the shipped constant.
    pub fn create_with_descriptor(name: &str, rd: &[u8]) -> Result<Self> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open("/dev/uhid")
            .context("opening /dev/uhid (needs the uhid module and access)")?;

        let mut event = vec![0u8; EVENT_BUFFER];
        event[0..4].copy_from_slice(&ev::CREATE2.to_le_bytes());

        let name_bytes = name.as_bytes();
        let n = name_bytes.len().min(127);
        event[CREATE2_NAME_OFF..CREATE2_NAME_OFF + n].copy_from_slice(&name_bytes[..n]);

        event[CREATE2_RD_SIZE_OFF..CREATE2_RD_SIZE_OFF + 2]
            .copy_from_slice(&(rd.len() as u16).to_le_bytes());
        // BUS_USB
        event[CREATE2_BUS_OFF..CREATE2_BUS_OFF + 2].copy_from_slice(&0x03u16.to_le_bytes());
        let mut off = CREATE2_VENDOR_OFF;
        for value in [0x1d6bu32, 0x0001, 0x0100, 0] {
            event[off..off + 4].copy_from_slice(&value.to_le_bytes());
            off += 4;
        }
        // `uhid_create2_req` has no dev_flags field; its rd_data starts here.
        debug_assert_eq!(off, CREATE2_RD_DATA_OFF);
        event[CREATE2_RD_DATA_OFF..CREATE2_RD_DATA_OFF + rd.len()].copy_from_slice(rd);

        let len = CREATE2_RD_DATA_OFF + rd.len();
        file.write_all(&event[..len])
            .context("writing UHID CREATE2 event")?;

        Ok(Self {
            file,
            name: name.to_string(),
            started: false,
        })
    }

    /// Device name as registered with the kernel.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// True once the kernel has started the device.
    #[must_use]
    pub fn started(&self) -> bool {
        self.started
    }

    /// Read and handle one pending event.
    ///
    /// `Ok(None)` means no event is pending, which is the normal case and must
    /// not be treated as an error.
    pub fn poll_event(&mut self) -> Result<Option<Event>> {
        let mut buf = [0u8; EVENT_BUFFER];
        let n = match self.file.read(&mut buf) {
            Ok(n) => n,
            Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(None),
            Err(e) if e.kind() == ErrorKind::Interrupted => return Ok(None),
            Err(e) => return Err(e).context("reading UHID event"),
        };
        if n < 4 {
            return Ok(None);
        }

        let kind = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let event = match kind {
            ev::START => {
                self.started = true;
                Event::Start
            }
            ev::STOP => {
                self.started = false;
                Event::Stop
            }
            ev::OPEN => Event::Open,
            ev::CLOSE => Event::Close,
            ev::DESTROY => Event::Destroy,
            ev::OUTPUT => {
                if n >= OUTPUT_SIZE_OFF + 2 {
                    let size = u16::from_le_bytes([buf[OUTPUT_SIZE_OFF], buf[OUTPUT_SIZE_OFF + 1]])
                        as usize;
                    let available = n.saturating_sub(OUTPUT_DATA_OFF);
                    let size = size.min(UHID_DATA_MAX).min(available);
                    Event::Output(buf[OUTPUT_DATA_OFF..OUTPUT_DATA_OFF + size].to_vec())
                } else {
                    Event::Output(Vec::new())
                }
            }
            ev::GET_REPORT => Event::GetReport,
            ev::GET_REPORT_REPLY => Event::Other(kind),
            other => Event::Other(other),
        };
        Ok(Some(event))
    }

    /// Send an input report to the kernel.
    pub fn send_input(&mut self, report: &[u8]) -> Result<()> {
        let report = &report[..report.len().min(UHID_DATA_MAX)];
        let mut event = vec![0u8; EVENT_BUFFER];
        event[0..4].copy_from_slice(&ev::INPUT2.to_le_bytes());
        event[4..6].copy_from_slice(&(report.len() as u16).to_le_bytes());
        event[INPUT2_DATA_OFF..INPUT2_DATA_OFF + report.len()].copy_from_slice(report);
        let len = INPUT2_DATA_OFF + report.len();
        match self.file.write_all(&event[..len]) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(()),
            Err(e) => Err(e).context("writing UHID input event"),
        }
    }

    /// Destroy the device, best effort.
    pub fn destroy(&mut self) {
        let event = ev::DESTROY.to_le_bytes();
        let _ = self.file.write_all(&event);
        self.started = false;
    }
}

impl Drop for UhidDevice {
    fn drop(&mut self) {
        self.destroy();
    }
}

/// A parsed UHID event of interest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Start,
    Stop,
    Open,
    Close,
    Destroy,
    /// An output report from the host. The first byte is the report ID when the
    /// descriptor uses numbered reports, which this one does.
    Output(Vec<u8>),
    GetReport,
    Other(u32),
}

/// Report ID for the rumble output report declared by the descriptor.
pub const RUMBLE_REPORT_ID: u8 = descriptor::OUTPUT_REPORT_ID;

/// Extract left/right rumble levels from an output report.
///
/// Returns `None` for a report that is not the rumble report or is too short,
/// so a malformed host report produces no feedback rather than random motion.
#[must_use]
pub fn decode_rumble(report: &[u8]) -> Option<(u8, u8)> {
    if report.len() >= 3 && report[0] == RUMBLE_REPORT_ID {
        Some((report[1], report[2]))
    } else if report.len() == 2 {
        // Defensive: some stacks strip the report ID before delivery.
        Some((report[0], report[1]))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struct_offsets_match_the_kernel_header() {
        // uhid_create2_req: name[128], phys[64], uniq[64], rd_size u16,
        // bus u16, vendor/product/version/country u32 x4, then rd_data.
        assert_eq!(CREATE2_NAME_OFF, 4);
        assert_eq!(CREATE2_PHYS_OFF, 132);
        assert_eq!(CREATE2_UNIQ_OFF, 196);
        assert_eq!(CREATE2_RD_SIZE_OFF, 260);
        assert_eq!(CREATE2_BUS_OFF, 262);
        assert_eq!(CREATE2_VENDOR_OFF, 264);
        assert_eq!(CREATE2_RD_DATA_OFF, 280);
    }

    #[test]
    fn input2_layout_matches_header() {
        // struct uhid_input2_req { u16 size; u8 data[4096]; }
        assert_eq!(INPUT2_DATA_OFF, 6);
    }

    #[test]
    fn output_layout_matches_header() {
        // struct uhid_output_req { u8 data[4096]; u16 size; u8 rtype; }
        assert_eq!(OUTPUT_DATA_OFF, 4);
        assert_eq!(OUTPUT_SIZE_OFF, 4100);
        assert_eq!(OUTPUT_RTYPE_OFF, 4102);
    }

    #[test]
    fn event_buffer_covers_output_report() {
        // Must be able to hold the largest event we read.
        const _: () = assert!(EVENT_BUFFER > OUTPUT_RTYPE_OFF);
        assert!(EVENT_BUFFER >= CREATE2_RD_DATA_OFF + descriptor::DESCRIPTOR.len());
    }

    #[test]
    fn decode_rumble_reads_numbered_report() {
        assert_eq!(decode_rumble(&[RUMBLE_REPORT_ID, 0x40, 0x80]), Some((0x40, 0x80)));
    }

    #[test]
    fn decode_rumble_reads_unnumbered_report() {
        assert_eq!(decode_rumble(&[0x40, 0x80]), Some((0x40, 0x80)));
    }

    #[test]
    fn decode_rumble_stop_is_zero() {
        assert_eq!(decode_rumble(&[RUMBLE_REPORT_ID, 0, 0]), Some((0, 0)));
    }

    #[test]
    fn decode_rumble_rejects_wrong_report_id() {
        assert_eq!(decode_rumble(&[0x01, 0x40, 0x80, 0x00]), None);
    }

    #[test]
    fn decode_rumble_rejects_short_reports() {
        assert_eq!(decode_rumble(&[]), None);
        assert_eq!(decode_rumble(&[RUMBLE_REPORT_ID]), None);
    }

    #[test]
    fn dev_flag_constants_match_header() {
        const _: () = assert!(DEV_FLAG_NUMBERED_OUTPUT == 2);
        const _: () = assert!(DEV_FLAG_NUMBERED_INPUT == 4);
    }
}
