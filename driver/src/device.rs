//! Physical device discovery and report reading.
//!
//! Two transports are relevant and they are genuinely different:
//!
//! - **USB DInput** exposes a hidraw node with real output reports on the
//!   `0xFFA0` vendor interface (`mi_02`). Rumble works there with a plain
//!   `05 0F <L> <R>` hidraw write.
//! - **Bluetooth DInput** exposes a hidraw node for *input only*. Its
//!   descriptor declares zero output reports, so hidraw writes are accepted
//!   unconditionally and do nothing. Rumble over Bluetooth must go through the
//!   Nordic UART GATT characteristic instead.
//!
//! This module only locates and reads the physical device. It deliberately does
//! not write to hidraw, because on Bluetooth that would silently succeed while
//! having no effect.

use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};

use crate::input::{self, State};

/// Vendor ID of Flydigi devices.
pub const VENDOR_ID: u16 = 0xd7d7;

/// Product ID of the Vader 3 Pro.
pub const PRODUCT_ID: u16 = 0x0041;

/// PnP IDs the same firmware can advertise.
///
/// The controller's own PnP ID characteristic reported `04b4:2412` (Cypress)
/// on a connection whose report descriptor was byte-identical to the DInput
/// capture. Discovery therefore cannot rely on VID/PID alone: the descriptor is
/// the reliable identity, and the ID is advisory.
pub const ALT_VENDOR_IDS: [u16; 1] = [0x04b4];

/// SHA-256 of the Bluetooth DInput report descriptor.
///
/// Established from the capture and independently confirmed against the device
/// firmware image at offset `0x23764`. Matching on this digest identifies the
/// DInput personality regardless of what PnP ID the firmware advertises.
pub const DINPUT_DESCRIPTOR_SHA256: &str =
    "227924e94560bea5149d6a75a8bb225143a1e4b0685075dea729a30649bc1f22";

/// Length of the Bluetooth DInput report descriptor in bytes.
pub const DINPUT_DESCRIPTOR_LEN: usize = 163;

/// `HIDIOCGRAWINFO` ioctl request number.
const HIDIOCGRAWINFO: libc::c_ulong = 0x8008_4803;

/// `hidraw_devinfo` from `linux/hidraw.h`: bus u32, vendor i16, product i16.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct HidrawDevinfo {
    bustype: u32,
    vendor: i16,
    product: i16,
}

/// A physical hidraw device matched by vendor and product.
#[derive(Debug)]
pub struct PhysicalDevice {
    pub path: PathBuf,
    pub vendor: u16,
    pub product: u16,
    bus: u32,
    file: File,
}

impl PhysicalDevice {
    /// True when the device is on the Bluetooth bus (`BUS_BLUETOOTH` = 0x05).
    ///
    /// This is **not** sufficient to choose a rumble mechanism on its own. A
    /// 2.4 GHz dongle was observed presenting itself as `HID_ID=0005:04B4:2412`
    /// — Bluetooth bus, Cypress vendor id — because a userspace daemon forwards
    /// it through uhid. Delivery is therefore decided by attempting the GATT
    /// characteristic for the configured address and falling back to a hidraw
    /// write, rather than trusting the bus field alone.
    #[must_use]
    pub fn is_bluetooth(&self) -> bool {
        self.bus == 0x05
    }

    /// True when the device is on USB (`BUS_USB` = 0x03).
    #[must_use]
    pub fn is_usb(&self) -> bool {
        self.bus == 0x03
    }

    /// Human-readable transport label for logging.
    #[must_use]
    pub fn bus_label(&self) -> &'static str {
        match self.bus {
            0x03 => "USB",
            0x05 => "bus 5 (Bluetooth, or a device presented on bus 5)",
            _ => "unknown bus",
        }
    }

    /// Open a hidraw node and confirm its identity.
    pub fn open(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
            .with_context(|| format!("opening {}", path.display()))?;

        let mut info = HidrawDevinfo::default();
        // SAFETY: the fd is open for the lifetime of the call and `info` is a
        // correctly sized `hidraw_devinfo` out-parameter.
        let rc = unsafe {
            libc::ioctl(
                std::os::unix::io::AsRawFd::as_raw_fd(&file),
                HIDIOCGRAWINFO,
                &mut info,
            )
        };
        if rc < 0 {
            return Err(std::io::Error::last_os_error())
                .with_context(|| format!("HIDIOCGRAWINFO on {}", path.display()));
        }

        Ok(Self {
            path: path.to_path_buf(),
            vendor: info.vendor as u16,
            product: info.product as u16,
            bus: info.bustype,
            file,
        })
    }

    /// Read one report.
    ///
    /// The controller is edge-triggered: it sends nothing while untouched, so
    /// `Ok(None)` is the normal idle result and not an error. Callers must not
    /// treat silence as a disconnect.
    pub fn try_read(&mut self) -> Result<Option<Vec<u8>>> {
        let mut buf = [0u8; 256];
        match self.file.read(&mut buf) {
            Ok(0) => Ok(None),
            Ok(n) => Ok(Some(buf[..n].to_vec())),
            Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(None),
            Err(e) if e.kind() == ErrorKind::Interrupted => Ok(None),
            Err(e) => Err(e).with_context(|| format!("reading {}", self.path.display())),
        }
    }

    /// Read every queued report and return the **newest** valid state.
    ///
    /// Draining matters for latency. Returning after the first parsed report
    /// leaves the rest in the kernel buffer, so with the loop's sleep between
    /// iterations a burst of reports is consumed one per iteration and the
    /// device's position falls steadily further behind — the pad shows where the
    /// stick *was*, not where it is. For an input translator the newest state is
    /// the only one worth showing, so older queued states are discarded.
    ///
    /// A malformed frame is dropped rather than ending the session: the device
    /// occasionally emits short or unexpected reports around mode changes, and
    /// terminating on those would make the daemon fragile.
    /// Sleep until a report is available, or until `timeout` elapses.
    ///
    /// The fd is `O_NONBLOCK`, so a plain read returns `WouldBlock` instead of
    /// waiting. Sleeping for a fixed interval to compensate adds that interval to
    /// every input as latency — measured at exactly 4 ms p95 when the poll was
    /// 4 ms, against a 0.16 ms minimum. `poll` blocks in the kernel instead and
    /// wakes the moment a report arrives, so the timeout only bounds how often
    /// the caller re-checks its own housekeeping.
    pub fn wait_readable(&self, timeout: Duration) -> Result<bool> {
        let mut fd = libc::pollfd {
            fd: std::os::unix::io::AsRawFd::as_raw_fd(&self.file),
            events: libc::POLLIN,
            revents: 0,
        };
        let millis = timeout.as_millis().min(i32::MAX as u128) as i32;
        // SAFETY: `fd` is a single initialised pollfd describing an open fd.
        let rc = unsafe { libc::poll(&mut fd, 1, millis) };
        if rc < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == ErrorKind::Interrupted {
                return Ok(false);
            }
            return Err(err).with_context(|| format!("poll on {}", self.path.display()));
        }
        Ok(rc > 0)
    }

    pub fn read_state(&mut self) -> Result<Option<State>> {
        let mut newest = None;
        while let Some(raw) = self.try_read()? {
            if let Ok(state) = input::parse(&raw) {
                newest = Some(state);
            }
        }
        Ok(newest)
    }
}

/// Find hidraw nodes for a vendor/product pair, annotated with the USB
/// interface number when the sysfs path reveals one.
///
/// Rumble over USB must target the vendor interface (`mi_02`, usage page
/// `0xFFA0`). Writing the same frame to a different interface reports success
/// and does nothing, which is indistinguishable from a silent device.
///
/// The interface number is taken from the USB interface directory name, e.g.
/// `1-3:1.2` means interface 2. A device forwarded through uhid has no such
/// suffix, so its interface is `None`.
#[must_use]
pub fn discover_detailed(vendor: u16, product: u16) -> Vec<HidrawNode> {
    let mut found = Vec::new();
    let dir = match std::fs::read_dir("/sys/class/hidraw") {
        Ok(d) => d,
        Err(_) => return found,
    };

    for entry in dir.flatten() {
        let uevent = entry.path().join("device/uevent");
        let Ok(text) = std::fs::read_to_string(&uevent) else {
            continue;
        };

        let mut id = None;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("HID_ID=") {
                id = Some(rest.to_string());
            }
        }
        let Some(id) = id else { continue };
        let mut parts = id.split(':');
        let bus = parts.next().and_then(|s| u32::from_str_radix(s, 16).ok());
        let vid = parts.next().and_then(|s| u32::from_str_radix(s, 16).ok());
        let pid = parts.next().and_then(|s| u32::from_str_radix(s, 16).ok());
        if vid.map(|v| v as u16) != Some(vendor) || pid.map(|p| p as u16) != Some(product) {
            continue;
        }

        let name = entry.file_name();
        let path = Path::new("/dev").join(&name);
        let interface = entry
            .path()
            .join("device")
            .canonicalize()
            .ok()
            .and_then(|p| interface_from_path(&p));

        found.push(HidrawNode {
            path,
            sysfs: entry.path(),
            bus: bus.unwrap_or(0),
            vendor,
            product,
            interface,
            descriptor_match: false,
        });
    }

    found.sort_by(|a, b| a.path.cmp(&b.path));
    found
}

/// Discover every hidraw node that looks like a Vader 3 Pro.
///
/// A node is a candidate when **either** its vendor/product pair matches a
/// known Flydigi or alternate ID, **or** its report descriptor digest matches the
/// recorded DInput descriptor. The second condition is what actually matters:
/// a real connection was observed advertising Cypress IDs (`04b4:2412`) while
/// serving the genuine DInput descriptor, which a VID/PID match alone would
/// reject.
#[must_use]
pub fn discover_candidates() -> Vec<HidrawNode> {
    let mut found = Vec::new();
    let dir = match std::fs::read_dir("/sys/class/hidraw") {
        Ok(d) => d,
        Err(_) => return found,
    };

    for entry in dir.flatten() {
        let uevent = entry.path().join("device/uevent");
        let Ok(text) = std::fs::read_to_string(&uevent) else {
            continue;
        };

        let mut hid_id = None;
        let mut name = None;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("HID_ID=") {
                hid_id = Some(rest.to_string());
            } else if let Some(rest) = line.strip_prefix("HID_NAME=") {
                name = Some(rest.to_string());
            }
        }
        let Some(hid_id) = hid_id else { continue };

        let mut parts = hid_id.split(':');
        let bus = parts
            .next()
            .and_then(|s| u32::from_str_radix(s, 16).ok())
            .unwrap_or(0);
        let vid = parts
            .next()
            .and_then(|s| u32::from_str_radix(s, 16).ok())
            .unwrap_or(0) as u16;
        let pid = parts
            .next()
            .and_then(|s| u32::from_str_radix(s, 16).ok())
            .unwrap_or(0) as u16;

        let id_match = (vid == VENDOR_ID && pid == PRODUCT_ID) || ALT_VENDOR_IDS.contains(&vid);
        let name_match = name
            .as_deref()
            .is_some_and(|n| n.to_ascii_lowercase().contains("vader3"));

        // Read the descriptor only when the cheap checks did not already match,
        // to avoid opening descriptors for every mouse on the system.
        let descriptor_match = if id_match || name_match {
            false
        } else {
            descriptor_digest_matches(&entry.path())
        };

        if !(id_match || name_match || descriptor_match) {
            continue;
        }

        let hidraw_name = entry.file_name();
        let path = Path::new("/dev").join(&hidraw_name);
        let interface = entry
            .path()
            .join("device")
            .canonicalize()
            .ok()
            .and_then(|p| interface_from_path(&p));

        found.push(HidrawNode {
            path,
            sysfs: entry.path(),
            bus,
            vendor: vid,
            product: pid,
            interface,
            descriptor_match,
        });
    }

    // Prefer a genuine descriptor match over an ID match, then take a stable
    // order so repeated runs pick the same node.
    found.sort_by(|a, b| {
        b.descriptor_match
            .cmp(&a.descriptor_match)
            .then_with(|| a.path.cmp(&b.path))
    });
    found
}

/// True when the node's report descriptor is byte-identical to the recorded
/// Bluetooth DInput descriptor.
///
/// Only the length is checked before hashing, so a wrong-sized descriptor is
/// rejected without reading its contents into a hash.
fn descriptor_digest_matches(hidraw_sysfs: &Path) -> bool {
    let path = hidraw_sysfs.join("device/report_descriptor");
    let Ok(bytes) = std::fs::read(&path) else {
        return false;
    };
    if bytes.len() != DINPUT_DESCRIPTOR_LEN {
        return false;
    }
    sha256_hex(&bytes) == DINPUT_DESCRIPTOR_SHA256
}

/// Minimal SHA-256, so descriptor identity needs no dependency.
///
/// A full crypto library is not justified for one fixed comparison, and this
/// keeps the release binary small. The implementation follows FIPS 180-4.
#[must_use]
pub fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let mut message = data.to_vec();
    let bit_len = (data.len() as u64) * 8;
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in message.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = String::with_capacity(64);
    for word in h {
        for byte in word.to_be_bytes() {
            use core::fmt::Write as _;
            let _ = write!(out, "{byte:02x}");
        }
    }
    out
}

/// A discovered hidraw node with transport detail.
#[derive(Debug, Clone)]
pub struct HidrawNode {
    pub path: PathBuf,
    /// Sysfs directory for this node, e.g. `/sys/class/hidraw/hidraw9`.
    ///
    /// Kept because related nodes (its evdev siblings, its descriptor) must be
    /// found through sysfs, and rebuilding that path from `/dev/hidraw9` is not
    /// possible — a mistake that silently disabled physical-node suppression.
    pub sysfs: PathBuf,
    pub bus: u32,
    pub vendor: u16,
    pub product: u16,
    /// USB interface number, when the sysfs path exposes one.
    pub interface: Option<u8>,
    /// True when identified by report-descriptor digest rather than by ID.
    pub descriptor_match: bool,
}

impl HidrawNode {
    #[must_use]
    pub fn is_bluetooth(&self) -> bool {
        self.bus == 0x05
    }
}

/// Extract the interface number from a USB sysfs path such as
/// `/sys/devices/pci.../usb1/1-3/1-3:1.2/0003:...`.
fn interface_from_path(path: &Path) -> Option<u8> {
    for component in path.components() {
        let text = component.as_os_str().to_string_lossy();
        // Interface directories look like `1-3:1.2` or `3-7:1.0`.
        if let Some(pos) = text.find(":1.") {
            let rest = &text[pos + 3..];
            if let Ok(n) = rest.parse::<u8>() {
                return Some(n);
            }
        }
    }
    None
}

/// Find candidate hidraw nodes without opening every device for write.
///
/// Reads `/sys/class/hidraw/*/device/uevent` so discovery needs no privileges;
/// only the matched node is later opened.
#[must_use]
pub fn discover(vendor: u16, product: u16) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let dir = match std::fs::read_dir("/sys/class/hidraw") {
        Ok(d) => d,
        Err(_) => return found,
    };

    for entry in dir.flatten() {
        let uevent = entry.path().join("device/uevent");
        let text = match std::fs::read_to_string(&uevent) {
            Ok(t) => t,
            Err(_) => continue,
        };

        let mut hid_id = None;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("HID_ID=") {
                hid_id = Some(rest.to_string());
            }
        }
        let Some(id) = hid_id else { continue };

        // HID_ID=0005:0000D7D7:00000041
        let mut parts = id.split(':');
        let _bus = parts.next();
        let vid = parts.next().and_then(|s| u32::from_str_radix(s, 16).ok());
        let pid = parts.next().and_then(|s| u32::from_str_radix(s, 16).ok());
        if vid.map(|v| v as u16) == Some(vendor) && pid.map(|p| p as u16) == Some(product) {
            let name = entry.file_name();
            found.push(Path::new("/dev").join(name));
        }
    }

    found.sort();
    found
}

/// Reduce a batch of raw reports to the newest parseable state.
///
/// Extracted from [`PhysicalDevice::read_state`] so the drain semantics can be
/// tested without a device. Returning the *first* state instead of the last is a
/// latency bug that is invisible on a lightly loaded system: the pad then shows
/// an older position than the controller is actually at.
#[must_use]
pub fn newest_state(reports: &[Vec<u8>]) -> Option<crate::input::State> {
    reports
        .iter()
        .filter_map(|raw| crate::input::parse(raw).ok())
        .next_back()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report_with_left_x(value: u8) -> Vec<u8> {
        let mut r = vec![0u8; crate::input::REPORT_LEN];
        r[0] = crate::input::REPORT_ID;
        r[1] = value;
        r[2] = 0xfd;
        r[3] = 0xfd;
        r[4] = 0xfd;
        r
    }

    #[test]
    fn newest_state_returns_the_last_report_not_the_first() {
        // Three queued reports; the pad must show the third, which is the most
        // recent physical position.
        let reports = vec![
            report_with_left_x(0x10),
            report_with_left_x(0x20),
            report_with_left_x(0x30),
        ];
        let state = newest_state(&reports).expect("a parseable state");
        assert_eq!(state.left_x, 0x30, "must take the newest, not the oldest");
    }

    #[test]
    fn newest_state_skips_unparseable_frames() {
        let mut bad = vec![0u8; 4];
        bad[0] = crate::input::REPORT_ID;
        let reports = vec![bad, report_with_left_x(0x40)];
        let state = newest_state(&reports).expect("the good frame survives");
        assert_eq!(state.left_x, 0x40);
    }

    #[test]
    fn newest_state_of_nothing_is_none() {
        assert!(newest_state(&[]).is_none());
    }

    #[test]
    fn identity_constants_are_the_documented_pair() {
        // Bluetooth DInput identity established from real hardware.
        assert_eq!(VENDOR_ID, 0xd7d7);
        assert_eq!(PRODUCT_ID, 0x0041);
    }

    #[test]
    fn alternate_vendor_id_is_accepted() {
        // Observed on a real connection: the same firmware advertised Cypress
        // IDs while serving the DInput descriptor.
        assert!(ALT_VENDOR_IDS.contains(&0x04b4));
    }

    #[test]
    fn descriptor_digest_is_a_full_sha256() {
        assert_eq!(DINPUT_DESCRIPTOR_SHA256.len(), 64);
        assert!(DINPUT_DESCRIPTOR_SHA256
            .chars()
            .all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn descriptor_length_matches_the_capture() {
        // The wrapper's descriptor is 163 bytes including the report ID item.
        assert_eq!(DINPUT_DESCRIPTOR_LEN, 163);
    }

    #[test]
    fn devinfo_layout_matches_kernel_header() {
        // struct hidraw_devinfo { __u32 bustype; __s16 vendor, product; }
        assert_eq!(std::mem::size_of::<HidrawDevinfo>(), 8);
    }

    #[test]
    fn interface_number_parses_from_usb_paths() {
        // Real sysfs shapes for a USB device's HID interface.
        assert_eq!(
            interface_from_path(Path::new(
                "/sys/devices/pci0000:00/0000:00:14.0/usb1/1-3/1-3:1.2/0003:04B4:2412.0009"
            )),
            Some(2)
        );
        assert_eq!(
            interface_from_path(Path::new(
                "/sys/devices/pci0000:00/0000:00:08.1/usb3/3-7/3-7:1.0/0003:045E:02E0.0010"
            )),
            Some(0)
        );
    }

    #[test]
    fn interface_number_is_absent_for_uhid_forwarded_devices() {
        // A device forwarded through uhid has a virtual path with no interface
        // component. This is the shape actually observed for a Flydigi dongle,
        // which reported bus 5 with a Cypress vendor id.
        assert_eq!(
            interface_from_path(Path::new(
                "/sys/devices/virtual/misc/uhid/0005:04B4:2412.002F"
            )),
            None
        );
    }

    #[test]
    fn bus_label_does_not_claim_bluetooth_for_bus_5_alone() {
        // The label is deliberately hedged, because bus 5 was observed for a
        // dongle forwarded through uhid.
        let label = "bus 5 (Bluetooth, or a device presented on bus 5)";
        assert!(label.contains("bus 5"));
        assert!(label.contains("or a device presented"));
    }
}
