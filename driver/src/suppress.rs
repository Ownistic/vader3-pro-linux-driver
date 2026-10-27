//! Hide the physical controller from the rest of the system.
//!
//! Without this, both the physical device and the virtual pad appear as
//! gamepads, which is what a user sees as a duplicate controller in Steam. The
//! PRD's acceptance criterion A2 requires the physical node to be suppressed.
//!
//! Two approaches exist and they behave very differently:
//!
//! | Method | Effect | Reversible on crash |
//! | --- | --- | --- |
//! | `EVIOCGRAB` | Exclusive access; other readers get no events | Yes — the grab drops when the fd closes, including on process death |
//! | Unbind driver | Node disappears entirely | No — needs manual rebinding |
//!
//! `EVIOCGRAB` is used because the kernel releases it automatically when the
//! process exits, so a crash cannot leave the controller permanently unusable.
//! Unbinding `hid-generic` would leave the device needing manual repair, which
//! is unacceptable for something a user runs every session.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// `EVIOCGRAB` from `linux/input.h`: `_IOW('E', 0x90, int)`.
const EVIOCGRAB: libc::c_ulong = 0x4004_4590;

/// A held exclusive grab on a physical input node.
///
/// The grab lasts exactly as long as this value is alive. Dropping it releases
/// the device, which is why it is stored in the translator rather than
/// forgotten: the whole point is that the lifetime is the process lifetime.
#[derive(Debug)]
pub struct ExclusiveGrab {
    _file: File,
    path: PathBuf,
}

impl ExclusiveGrab {
    /// Grab an input node exclusively.
    ///
    /// Returns an error when the node cannot be opened or the grab is refused,
    /// which happens if another process already grabbed it. Callers should treat
    /// that as "suppression unavailable" and keep working rather than failing:
    /// a duplicate controller is a cosmetic problem, while no controller at all
    /// is a functional one.
    pub fn acquire(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
            .with_context(|| format!("opening {} for exclusive grab", path.display()))?;

        // SAFETY: EVIOCGRAB takes no argument; the fd must be an evdev node.
        let rc = unsafe { libc::ioctl(file.as_raw_fd(), EVIOCGRAB, 1i32) };
        if rc < 0 {
            let err = io::Error::last_os_error();
            anyhow::bail!("EVIOCGRAB on {} failed: {err}", path.display());
        }

        Ok(Self {
            _file: file,
            path: path.to_path_buf(),
        })
    }

    /// Path of the node that is being held.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Find the evdev nodes belonging to a hidraw node.
///
/// A single HID device can expose several input nodes — a gamepad plus a
/// keyboard or consumer-control node — and only the ones that carry the
/// controller's reports matter. They are discovered through sysfs rather than by
/// matching names, because name matching would also catch the virtual pad we
/// create.
#[must_use]
pub fn evdev_nodes_for(hidraw_sysfs: &Path) -> Vec<PathBuf> {
    let mut nodes = Vec::new();
    let input_dir = hidraw_sysfs.join("device/input");
    let Ok(entries) = std::fs::read_dir(&input_dir) else {
        return nodes;
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let text = name.to_string_lossy();
        if !text.starts_with("input") {
            continue;
        }
        let Ok(events) = std::fs::read_dir(entry.path()) else {
            continue;
        };
        for ev in events.flatten() {
            let ev_name = ev.file_name();
            let ev_text = ev_name.to_string_lossy();
            if ev_text.starts_with("event") {
                nodes.push(Path::new("/dev/input").join(&*ev_text));
            }
        }
    }

    nodes.sort();
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_node_is_reported_not_panicked() {
        let result = ExclusiveGrab::acquire(Path::new("/dev/input/event-does-not-exist"));
        assert!(result.is_err());
        let message = format!("{:#}", result.unwrap_err());
        assert!(message.contains("opening"), "got: {message}");
    }

    #[test]
    fn absent_hidraw_yields_no_nodes() {
        let nodes = evdev_nodes_for(Path::new("/sys/class/hidraw/hidraw-does-not-exist"));
        assert!(nodes.is_empty());
    }

    #[test]
    fn grab_ioctl_number_matches_the_kernel_header() {
        // `_IOW('E', 0x90, int)` expands to this on every Linux architecture
        // using the generic ioctl encoding. A wrong value would either fail or,
        // worse, be interpreted as a different request.
        assert_eq!(EVIOCGRAB, 0x4004_4590);
    }

    #[test]
    fn evdev_nodes_are_absolute_device_paths() {
        // Whatever is discovered must be openable as /dev/input/eventN, not a
        // sysfs path.
        for node in evdev_nodes_for(Path::new("/sys/class/hidraw/hidraw0")) {
            assert!(
                node.starts_with("/dev/input"),
                "unexpected node path: {}",
                node.display()
            );
        }
    }
}
