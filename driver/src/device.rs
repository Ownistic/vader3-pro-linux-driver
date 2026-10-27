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

use anyhow::{Context, Result};

use crate::input::{self, State};

/// Vendor ID of Flydigi devices.
pub const VENDOR_ID: u16 = 0xd7d7;

/// Product ID of the Vader 3 Pro.
pub const PRODUCT_ID: u16 = 0x0041;

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
        let rc = unsafe { libc::ioctl(std::os::unix::io::AsRawFd::as_raw_fd(&file), HIDIOCGRAWINFO, &mut info) };
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

    /// Read and parse one report, ignoring unrecognised frames.
    ///
    /// A malformed frame is dropped rather than ending the session: the device
    /// occasionally emits short or unexpected reports around mode changes, and
    /// terminating on those would make the daemon fragile.
    pub fn read_state(&mut self) -> Result<Option<State>> {
        while let Some(raw) = self.try_read()? {
            match input::parse(&raw) {
                Ok(state) => return Ok(Some(state)),
                Err(_) => continue,
            }
        }
        Ok(None)
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
        let path = Path::new("/dev/hidraw").join(&name);
        let interface = entry
            .path()
            .join("device")
            .canonicalize()
            .ok()
            .and_then(|p| interface_from_path(&p));

        found.push(HidrawNode {
            path,
            bus: bus.unwrap_or(0),
            interface,
        });
    }

    found.sort_by(|a, b| a.path.cmp(&b.path));
    found
}

/// A discovered hidraw node with transport detail.
#[derive(Debug, Clone)]
pub struct HidrawNode {
    pub path: PathBuf,
    pub bus: u32,
    /// USB interface number, when the sysfs path exposes one.
    pub interface: Option<u8>,
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
            found.push(Path::new("/dev/hidraw").join(name));
        }
    }

    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_constants_are_the_documented_pair() {
        // Bluetooth DInput identity established from real hardware.
        assert_eq!(VENDOR_ID, 0xd7d7);
        assert_eq!(PRODUCT_ID, 0x0041);
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
            interface_from_path(Path::new("/sys/devices/virtual/misc/uhid/0005:04B4:2412.002F")),
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
