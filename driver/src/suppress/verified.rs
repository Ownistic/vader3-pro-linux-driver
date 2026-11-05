//! Identity-pinned evdev discovery and pre-grab verification.

use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileTypeExt, MetadataExt};

use super::*;
use crate::device::IdentifiedNode;
use crate::identity::MacAddress;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    dev: u64,
    inode: u64,
    rdev: u64,
    character: bool,
}

impl Stamp {
    fn read(metadata: &std::fs::Metadata) -> Self {
        Self {
            dev: metadata.dev(),
            inode: metadata.ino(),
            rdev: metadata.rdev(),
            character: metadata.file_type().is_char_device(),
        }
    }
}

/// Evdev node and HID identity captured from one identified candidate.
#[derive(Clone, Debug)]
pub struct EvdevCandidate {
    pub path: PathBuf,
    hid_sysfs: PathBuf,
    hid_parent: PathBuf,
    event_sysfs: PathBuf,
    event_parent: PathBuf,
    stamp: Stamp,
    address: MacAddress,
    bus: u16,
    vendor: u16,
    product: u16,
}

fn device_number(event_sysfs: &Path, stamp: Stamp) -> Result<()> {
    let text = std::fs::read_to_string(event_sysfs.join("dev"))?;
    let (major, minor) = text
        .trim()
        .split_once(':')
        .context("invalid evdev sysfs device number")?;
    let major: u32 = major.parse()?;
    let minor: u32 = minor.parse()?;
    if libc::major(stamp.rdev) != major || libc::minor(stamp.rdev) != minor {
        anyhow::bail!("evdev device number does not match sysfs");
    }
    Ok(())
}

/// Discover evdev siblings only below the identified HID ancestry.
///
/// A missing or inaccessible input tree returns an error. Callers should warn
/// and continue translation without suppression, rather than guessing a node.
pub fn evdev_candidates_for(hid: &IdentifiedNode) -> Result<Vec<EvdevCandidate>> {
    discover(hid, Path::new("/dev/input"))
}

fn discover(hid: &IdentifiedNode, dev_root: &Path) -> Result<Vec<EvdevCandidate>> {
    if hid.node.sysfs.join("device").canonicalize()? != hid.hid_parent {
        anyhow::bail!("HID ancestry changed before evdev discovery");
    }
    let mut result = Vec::new();
    for input in std::fs::read_dir(hid.node.sysfs.join("device/input"))? {
        let input = input?;
        if !input.file_name().as_bytes().starts_with(b"input") {
            continue;
        }
        for event in std::fs::read_dir(input.path())? {
            let event = event?;
            let name = event.file_name();
            let Some(text) = name.to_str() else {
                anyhow::bail!("non-UTF-8 evdev node name");
            };
            if !text.starts_with("event")
                || !text[5..].bytes().all(|b| b.is_ascii_digit())
                || text.len() == 5
            {
                continue;
            }
            let event_sysfs = event.path();
            let event_parent = event_sysfs.canonicalize()?;
            if !event_parent.starts_with(&hid.hid_parent) {
                anyhow::bail!("evdev ancestry is outside the identified HID device");
            }
            let path = dev_root.join(name);
            let stamp = Stamp::read(&std::fs::symlink_metadata(&path)?);
            if !stamp.character {
                anyhow::bail!("evdev path is not a character device");
            }
            device_number(&event_sysfs, stamp)?;
            result.push(EvdevCandidate {
                path,
                hid_sysfs: hid.node.sysfs.clone(),
                hid_parent: hid.hid_parent.clone(),
                event_sysfs,
                event_parent,
                stamp,
                address: hid.address,
                bus: u16::try_from(hid.node.bus)?,
                vendor: hid.node.vendor,
                product: hid.node.product,
            });
        }
    }
    if hid.node.sysfs.join("device").canonicalize()? != hid.hid_parent {
        anyhow::bail!("HID ancestry changed during evdev discovery");
    }
    result.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
}

#[derive(Clone, Copy)]
struct Identity {
    bus: u16,
    vendor: u16,
    product: u16,
    uniq: MacAddress,
}

struct Observed<'a> {
    before: Stamp,
    opened: Stamp,
    after: Stamp,
    hid_parent: &'a Path,
    event_parent: &'a Path,
    number_matches: bool,
    identity: Identity,
}

fn parse_uniq(data: &[u8], len: usize) -> Result<MacAddress> {
    if len == 0 || len > data.len() {
        anyhow::bail!("missing evdev remote identity");
    }
    let end = data[..len].iter().position(|b| *b == 0).unwrap_or(len);
    std::str::from_utf8(&data[..end])?
        .parse()
        .context("missing or invalid evdev remote identity")
}

// This is the same gate used immediately before the production EVIOCGRAB.
fn checked_grab<T>(
    candidate: &EvdevCandidate,
    observed: &Observed<'_>,
    grab: impl FnOnce() -> Result<T>,
) -> Result<T> {
    if !candidate.stamp.character
        || observed.before != candidate.stamp
        || observed.opened != candidate.stamp
        || observed.after != candidate.stamp
    {
        anyhow::bail!("evdev path or opened descriptor changed");
    }
    if observed.hid_parent != candidate.hid_parent
        || observed.event_parent != candidate.event_parent
        || !observed.event_parent.starts_with(&candidate.hid_parent)
        || !observed.number_matches
    {
        anyhow::bail!("evdev topology or device number changed");
    }
    if (
        observed.identity.bus,
        observed.identity.vendor,
        observed.identity.product,
        observed.identity.uniq,
    ) != (
        candidate.bus,
        candidate.vendor,
        candidate.product,
        candidate.address,
    ) {
        anyhow::bail!("opened evdev identity does not match HID identity");
    }
    grab()
}

impl ExclusiveGrab {
    /// Verify the opened FD and its remote identity before issuing EVIOCGRAB.
    /// Missing EVIOCGUNIQ makes suppression unavailable; it is never guessed.
    pub fn acquire_verified(candidate: &EvdevCandidate) -> Result<Self> {
        let before = Stamp::read(&std::fs::symlink_metadata(&candidate.path)?);
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&candidate.path)
            .with_context(|| format!("opening {} for verified grab", candidate.path.display()))?;
        let opened = Stamp::read(&file.metadata()?);
        let mut id = [0u16; 4];
        // linux/input.h EVIOCGID and EVIOCGUNIQ(64); both buffers are writable.
        if unsafe {
            libc::ioctl(
                file.as_raw_fd(),
                0x8008_4502u64 as libc::c_ulong,
                id.as_mut_ptr(),
            )
        } < 0
        {
            return Err(io::Error::last_os_error()).context("EVIOCGID");
        }
        let mut uniq = [0u8; 64];
        let len = unsafe {
            libc::ioctl(
                file.as_raw_fd(),
                0x8040_4508u64 as libc::c_ulong,
                uniq.as_mut_ptr(),
            )
        };
        if len < 0 {
            return Err(io::Error::last_os_error()).context("EVIOCGUNIQ");
        }
        let len = usize::try_from(len)?;
        let address = parse_uniq(&uniq, len)?;
        let hid_parent = candidate.hid_sysfs.join("device").canonicalize()?;
        let event_parent = candidate.event_sysfs.canonicalize()?;
        device_number(&candidate.event_sysfs, opened)?;
        let after = Stamp::read(&std::fs::symlink_metadata(&candidate.path)?);
        let observed = Observed {
            before,
            opened,
            after,
            hid_parent: &hid_parent,
            event_parent: &event_parent,
            number_matches: true,
            identity: Identity {
                bus: id[0],
                vendor: id[1],
                product: id[2],
                uniq: address,
            },
        };
        checked_grab(candidate, &observed, || {
            // SAFETY: validation above pins this very FD; EVIOCGRAB takes an integer.
            if unsafe { libc::ioctl(file.as_raw_fd(), EVIOCGRAB, 1i32) } < 0 {
                return Err(io::Error::last_os_error()).context("EVIOCGRAB");
            }
            Ok(Self {
                _file: file,
                path: candidate.path.clone(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn fixture() -> (EvdevCandidate, Identity) {
        let address = "12:34:56:78:9A:BC".parse().unwrap();
        let stamp = Stamp {
            dev: 1,
            inode: 2,
            rdev: 3,
            character: true,
        };
        (
            EvdevCandidate {
                path: "/dev/input/event0".into(),
                hid_sysfs: "/sys/hidraw0".into(),
                hid_parent: "/sys/hid/a".into(),
                event_sysfs: "/sys/hid/a/input/event0".into(),
                event_parent: "/sys/hid/a/input/event0".into(),
                stamp,
                address,
                bus: 5,
                vendor: 0xd7d7,
                product: 0x41,
            },
            Identity {
                bus: 5,
                vendor: 0xd7d7,
                product: 0x41,
                uniq: address,
            },
        )
    }

    #[test]
    fn suppress_verified_gate_never_grabs_mismatched_identity_or_path() {
        let (candidate, identity) = fixture();
        let s = candidate.stamp;
        let mut obs = Observed {
            before: s,
            opened: s,
            after: s,
            hid_parent: &candidate.hid_parent,
            event_parent: &candidate.event_parent,
            number_matches: true,
            identity,
        };
        let calls = Cell::new(0);
        let attempt = |obs: &Observed<'_>| {
            checked_grab(&candidate, obs, || {
                calls.set(calls.get() + 1);
                Ok(())
            })
        };
        let bad = Stamp { inode: 9, ..s };
        obs.before = bad;
        assert!(attempt(&obs).is_err());
        obs.before = s;
        obs.opened = bad;
        assert!(attempt(&obs).is_err());
        obs.opened = s;
        obs.after = bad;
        assert!(attempt(&obs).is_err());
        obs.after = s;
        obs.hid_parent = Path::new("/sys/hid/b");
        assert!(attempt(&obs).is_err());
        obs.hid_parent = &candidate.hid_parent;
        obs.event_parent = Path::new("/sys/hid/b/input/event0");
        assert!(attempt(&obs).is_err());
        obs.event_parent = &candidate.event_parent;
        obs.number_matches = false;
        assert!(attempt(&obs).is_err());
        obs.number_matches = true;
        obs.identity.bus = 3;
        assert!(attempt(&obs).is_err());
        obs.identity = identity;
        obs.identity.vendor = 1;
        assert!(attempt(&obs).is_err());
        obs.identity = identity;
        obs.identity.product = 1;
        assert!(attempt(&obs).is_err());
        obs.identity = identity;
        obs.identity.uniq = "01:23:45:67:89:AB".parse().unwrap();
        assert!(attempt(&obs).is_err());
        obs.identity = identity;
        assert_eq!(calls.get(), 0);
        assert!(attempt(&obs).is_ok());
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn suppress_sysfs_device_number_must_match() {
        let temp = tempfile::tempdir().unwrap();
        let stamp = Stamp {
            rdev: libc::makedev(13, 64),
            ..fixture().0.stamp
        };
        std::fs::write(temp.path().join("dev"), "13:64\n").unwrap();
        assert!(device_number(temp.path(), stamp).is_ok());
        std::fs::write(temp.path().join("dev"), "13:65\n").unwrap();
        assert!(device_number(temp.path(), stamp).is_err());
        std::fs::write(temp.path().join("dev"), "garbage\n").unwrap();
        assert!(device_number(temp.path(), stamp).is_err());
    }

    #[test]
    fn suppress_reused_event_path_never_reaches_grab() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("event0");
        std::fs::write(&path, b"old").unwrap();
        let old = File::open(&path).unwrap();
        let mut candidate = fixture().0;
        candidate.stamp = Stamp {
            character: true,
            ..Stamp::read(&old.metadata().unwrap())
        };
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, b"replacement").unwrap();
        let replacement = Stamp {
            character: true,
            ..Stamp::read(&std::fs::metadata(&path).unwrap())
        };
        let observed = Observed {
            before: replacement,
            opened: replacement,
            after: replacement,
            hid_parent: &candidate.hid_parent,
            event_parent: &candidate.event_parent,
            number_matches: true,
            identity: fixture().1,
        };
        let called = Cell::new(false);
        assert!(checked_grab(&candidate, &observed, || {
            called.set(true);
            Ok(())
        })
        .is_err());
        assert!(!called.get());
    }

    #[test]
    fn suppress_missing_or_malformed_remote_identity_rejected() {
        for value in ["", "00:00:00:00:00:00", "FF:FF:FF:FF:FF:FF", "bad"] {
            assert!(parse_uniq(value.as_bytes(), value.len()).is_err());
        }
        assert!(parse_uniq(b"12:34:56:78:9a:bc\0", 18).is_ok());
        assert!(parse_uniq(b"a", 2).is_err());
    }
}
