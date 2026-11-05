//! Identity-bound discovery and opened-hidraw validation.

use std::collections::{BTreeMap, BTreeSet};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt};

use anyhow::{bail, Context, Result};

use super::*;
use crate::identity::MacAddress;

/// Discovery metadata pinned for subsequent opened-FD verification.
#[derive(Clone, Debug)]
pub struct IdentifiedNode {
    pub node: HidrawNode,
    pub address: MacAddress,
    pub hid_parent: PathBuf,
    stamp: NodeStamp,
}

impl IdentifiedNode {
    /// True only for the same discovered node and pinned metadata.
    #[must_use]
    pub fn same_binding(&self, other: &Self) -> bool {
        self.address == other.address
            && self.node.path == other.node.path
            && self.node.sysfs == other.node.sysfs
            && self.hid_parent == other.hid_parent
            && self.stamp == other.stamp
            && self.node.bus == other.node.bus
            && self.node.vendor == other.node.vendor
            && self.node.product == other.node.product
            && self.node.interface == other.node.interface
            && self.node.descriptor_match == other.node.descriptor_match
    }

    #[cfg(test)]
    pub(crate) fn fixture(address: MacAddress, suffix: &str) -> Self {
        Self {
            node: HidrawNode {
                path: format!("/dev/{suffix}").into(),
                sysfs: format!("/sys/{suffix}").into(),
                bus: 5,
                vendor: VENDOR_ID,
                product: PRODUCT_ID,
                interface: None,
                descriptor_match: false,
            },
            address,
            hid_parent: format!("/sys/parent-{suffix}").into(),
            stamp: NodeStamp {
                dev: 1,
                inode: address
                    .bytes()
                    .into_iter()
                    .chain(suffix.bytes())
                    .fold(0u64, |n, b| n.wrapping_mul(131).wrapping_add(u64::from(b))),
                rdev: 1,
                character: true,
            },
        }
    }

    #[cfg(test)]
    pub(crate) fn fixture_inode(&self) -> u64 {
        self.stamp.inode
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NodeStamp {
    dev: u64,
    inode: u64,
    rdev: u64,
    character: bool,
}

impl NodeStamp {
    fn from_metadata(metadata: &std::fs::Metadata) -> Self {
        Self {
            dev: metadata.dev(),
            inode: metadata.ino(),
            rdev: metadata.rdev(),
            character: metadata.file_type().is_char_device(),
        }
    }
}

/// An incomplete scan must not be interpreted as authoritative removal.
#[derive(Debug, Default)]
pub struct DiscoverySnapshot {
    pub nodes: BTreeMap<MacAddress, IdentifiedNode>,
    pub ambiguous: BTreeSet<MacAddress>,
    /// Exact class paths whose current metadata could not be verified.
    pub rejected_sysfs: BTreeSet<PathBuf>,
    /// Canonical HID parents of rejected entries, when independently resolvable.
    pub rejected_parents: BTreeSet<PathBuf>,
    pub errors: Vec<String>,
    pub complete: bool,
    seen: Vec<(MacAddress, PathBuf, NodeStamp)>,
}

impl DiscoverySnapshot {
    #[cfg(test)]
    pub(crate) fn fixture(nodes: Vec<IdentifiedNode>, complete: bool) -> Self {
        let mut snapshot = Self {
            complete,
            ..Self::default()
        };
        for node in nodes {
            snapshot.insert(node);
        }
        snapshot
    }
    fn insert(&mut self, candidate: IdentifiedNode) {
        let address = candidate.address;
        let conflicts: Vec<_> = self
            .seen
            .iter()
            .filter_map(|(other, parent, stamp)| {
                (*other != address
                    && (*parent == candidate.hid_parent || *stamp == candidate.stamp))
                    .then_some(*other)
            })
            .collect();
        self.seen
            .push((address, candidate.hid_parent.clone(), candidate.stamp));
        if !conflicts.is_empty() {
            self.nodes.remove(&address);
            self.ambiguous.insert(address);
            for other in conflicts {
                self.nodes.remove(&other);
                self.ambiguous.insert(other);
            }
            return;
        }
        if self.ambiguous.contains(&address) {
            return;
        }
        if let Some(previous) = self.nodes.get(&address) {
            let same = previous.hid_parent == candidate.hid_parent
                && previous.stamp == candidate.stamp
                && previous.node.bus == candidate.node.bus
                && previous.node.vendor == candidate.node.vendor
                && previous.node.product == candidate.node.product
                && previous.node.interface == candidate.node.interface;
            if !same {
                self.nodes.remove(&address);
                self.ambiguous.insert(address);
            }
        } else {
            self.nodes.insert(address, candidate);
        }
    }

    fn error(&mut self, message: String) {
        self.complete = false;
        self.errors.push(message);
    }

    fn reject(&mut self, sysfs: &Path, message: String) {
        self.rejected_sysfs.insert(sysfs.to_path_buf());
        if let Ok(parent) = sysfs.join("device").canonicalize() {
            self.rejected_parents.insert(parent);
        }
        self.error(message);
    }
}

fn parse_uevent(text: &str) -> Result<(u32, u16, u16, MacAddress)> {
    let mut id = None;
    let mut uniq = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("HID_ID=") {
            if id.replace(value).is_some() {
                bail!("duplicate HID_ID");
            }
        }
        if let Some(value) = line.strip_prefix("HID_UNIQ=") {
            if uniq.replace(value).is_some() {
                bail!("duplicate HID_UNIQ");
            }
        }
    }
    let mut parts = id.context("missing HID_ID")?.split(':');
    let bus = u32::from_str_radix(parts.next().context("missing bus")?, 16)?;
    let vendor = u16::try_from(u32::from_str_radix(
        parts.next().context("missing vendor")?,
        16,
    )?)?;
    let product = u16::try_from(u32::from_str_radix(
        parts.next().context("missing product")?,
        16,
    )?)?;
    if parts.next().is_some() {
        bail!("extra HID_ID field");
    }
    let address = uniq
        .context("missing HID_UNIQ")?
        .parse()
        .context("invalid HID_UNIQ")?;
    Ok((bus, vendor, product, address))
}

fn verify_sysfs_dev(sysfs: &Path, stamp: NodeStamp) -> Result<()> {
    let text = std::fs::read_to_string(sysfs.join("dev"))?;
    let (major, minor) = text
        .trim()
        .split_once(':')
        .context("invalid sysfs device number")?;
    let major: u32 = major.parse()?;
    let minor: u32 = minor.parse()?;
    if libc::major(stamp.rdev) != major || libc::minor(stamp.rdev) != minor {
        bail!("hidraw device number does not match sysfs");
    }
    Ok(())
}

/// Scan hidraw metadata; candidates never derive their address from HID_PHYS.
#[must_use]
pub fn discover_identified() -> DiscoverySnapshot {
    scan(Path::new("/sys/class/hidraw"), Path::new("/dev"))
}

fn scan(root: &Path, dev_root: &Path) -> DiscoverySnapshot {
    let mut snapshot = DiscoverySnapshot {
        complete: true,
        ..DiscoverySnapshot::default()
    };
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(e) => {
            snapshot.error(format!("reading {}: {e}", root.display()));
            return snapshot;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                snapshot.error(format!("reading hidraw entry: {e}"));
                continue;
            }
        };
        let sysfs = entry.path();
        let text = match std::fs::read_to_string(sysfs.join("device/uevent")) {
            Ok(text) => text,
            Err(e) => {
                snapshot.reject(&sysfs, format!("reading {} uevent: {e}", sysfs.display()));
                continue;
            }
        };
        // Parse ID separately so unrelated devices need not supply an address.
        let id_line = text.lines().find_map(|line| line.strip_prefix("HID_ID="));
        let id_parts: Vec<_> = id_line.unwrap_or("").split(':').collect();
        if id_parts.len() != 3 {
            snapshot.reject(&sysfs, format!("invalid HID_ID at {}", sysfs.display()));
            continue;
        }
        let parsed = (
            u32::from_str_radix(id_parts[0], 16),
            u32::from_str_radix(id_parts[1], 16),
            u32::from_str_radix(id_parts[2], 16),
        );
        let (Ok(bus), Ok(vid), Ok(pid)) = parsed else {
            snapshot.reject(&sysfs, format!("invalid HID_ID at {}", sysfs.display()));
            continue;
        };
        if bus != 5 {
            continue;
        }
        let id_match = vid == u32::from(VENDOR_ID) && pid == u32::from(PRODUCT_ID);
        let descriptor_match = if id_match {
            false
        } else {
            match std::fs::read(sysfs.join("device/report_descriptor")) {
                Ok(bytes) => {
                    bytes.len() == DINPUT_DESCRIPTOR_LEN
                        && sha256_hex(&bytes) == DINPUT_DESCRIPTOR_SHA256
                }
                Err(e) => {
                    snapshot.reject(
                        &sysfs,
                        format!("reading descriptor at {}: {e}", sysfs.display()),
                    );
                    continue;
                }
            }
        };
        if !candidate_matches(bus, id_match, descriptor_match) {
            continue;
        }
        let (bus, vendor, product, address) = match parse_uevent(&text) {
            Ok(info) => info,
            Err(e) => {
                snapshot.reject(
                    &sysfs,
                    format!("invalid identity at {}: {e:#}", sysfs.display()),
                );
                continue;
            }
        };
        let parent = match sysfs.join("device").canonicalize() {
            Ok(parent) => parent,
            Err(e) => {
                snapshot.reject(&sysfs, format!("resolving {}: {e}", sysfs.display()));
                continue;
            }
        };
        let path = dev_root.join(entry.file_name());
        let stamp = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => NodeStamp::from_metadata(&metadata),
            Err(e) => {
                snapshot.reject(&sysfs, format!("stat {}: {e}", path.display()));
                continue;
            }
        };
        if !stamp.character {
            snapshot.reject(
                &sysfs,
                format!("{} is not a character device", path.display()),
            );
            continue;
        }
        if let Err(e) = verify_sysfs_dev(&sysfs, stamp) {
            snapshot.reject(
                &sysfs,
                format!("device number at {}: {e:#}", sysfs.display()),
            );
            continue;
        }
        let interface = interface_from_path(&parent);
        snapshot.insert(IdentifiedNode {
            node: HidrawNode {
                path,
                sysfs,
                bus,
                vendor,
                product,
                interface,
                descriptor_match,
            },
            address,
            hid_parent: parent,
            stamp,
        });
    }
    snapshot
}

fn verify(
    expected: &IdentifiedNode,
    before: NodeStamp,
    opened: NodeStamp,
    after: NodeStamp,
    parent: &Path,
    info: HidrawDevinfo,
    uniq: MacAddress,
) -> Result<()> {
    if !expected.stamp.character
        || before != expected.stamp
        || opened != expected.stamp
        || after != expected.stamp
    {
        bail!("hidraw node changed or is not a character device");
    }
    if parent != expected.hid_parent
        || info.bustype != expected.node.bus
        || info.vendor as u16 != expected.node.vendor
        || info.product as u16 != expected.node.product
        || uniq != expected.address
    {
        bail!("opened hidraw identity or topology does not match discovery");
    }
    Ok(())
}

impl PhysicalDevice {
    /// Open and verify the candidate before any grab, virtual device or BLE operation.
    pub fn open_identified(expected: &IdentifiedNode) -> Result<Self> {
        let before = NodeStamp::from_metadata(&std::fs::symlink_metadata(&expected.node.path)?);
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC | libc::O_NOFOLLOW)
            .open(&expected.node.path)?;
        let opened = NodeStamp::from_metadata(&file.metadata()?);
        let mut info = HidrawDevinfo::default();
        // HIDIOCGRAWINFO and HIDIOCGRAWUNIQ(64), from linux/hidraw.h.
        // SAFETY: ioctl receives valid writable buffers and an open descriptor.
        if unsafe { libc::ioctl(file.as_raw_fd(), HIDIOCGRAWINFO, &mut info) } < 0 {
            return Err(std::io::Error::last_os_error()).context("HIDIOCGRAWINFO");
        }
        let mut uniq = [0u8; 64];
        let len = unsafe {
            libc::ioctl(
                file.as_raw_fd(),
                0x8040_4808u64 as libc::c_ulong,
                uniq.as_mut_ptr(),
            )
        };
        if len < 0 {
            return Err(std::io::Error::last_os_error()).context("HIDIOCGRAWUNIQ");
        }
        let len = usize::try_from(len)?;
        if len == 0 || len > uniq.len() {
            bail!("missing or invalid HIDIOCGRAWUNIQ length");
        }
        let end = uniq[..len].iter().position(|b| *b == 0).unwrap_or(len);
        let address: MacAddress = std::str::from_utf8(&uniq[..end])?
            .parse()
            .context("invalid opened HID_UNIQ")?;
        let parent = expected.node.sysfs.join("device").canonicalize()?;
        let text = std::fs::read_to_string(expected.node.sysfs.join("device/uevent"))?;
        let (bus, vendor, product, current_address) = parse_uevent(&text)?;
        if (bus, vendor, product, current_address)
            != (
                expected.node.bus,
                expected.node.vendor,
                expected.node.product,
                expected.address,
            )
        {
            bail!("hidraw sysfs identity changed");
        }
        let after = NodeStamp::from_metadata(&std::fs::symlink_metadata(&expected.node.path)?);
        verify(expected, before, opened, after, &parent, info, address)?;
        verify_sysfs_dev(&expected.node.sysfs, opened)?;
        Ok(Self {
            path: expected.node.path.clone(),
            vendor: info.vendor as u16,
            product: info.product as u16,
            bus: info.bustype,
            file,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(address: &str, path: &str, parent: &str, inode: u64) -> IdentifiedNode {
        IdentifiedNode {
            node: HidrawNode {
                path: path.into(),
                sysfs: "/sys/test".into(),
                bus: 5,
                vendor: VENDOR_ID,
                product: PRODUCT_ID,
                interface: None,
                descriptor_match: false,
            },
            address: address.parse().unwrap(),
            hid_parent: parent.into(),
            stamp: NodeStamp {
                dev: 1,
                inode,
                rdev: 9,
                character: true,
            },
        }
    }

    #[test]
    fn device_identity_groups_exact_aliases_and_rejects_conflicts() {
        let a = node("12:34:56:78:9a:bc", "/dev/a", "/sys/a", 1);
        let alias = node("12:34:56:78:9A:BC", "/dev/alias", "/sys/a", 1);
        let b = node("01:23:45:67:89:ab", "/dev/b", "/sys/b", 2);
        for nodes in [
            vec![a.clone(), alias.clone(), b.clone()],
            vec![b.clone(), alias.clone(), a.clone()],
        ] {
            let mut snapshot = DiscoverySnapshot::default();
            for item in nodes {
                snapshot.insert(item);
            }
            assert_eq!(snapshot.nodes.len(), 2);
            assert!(snapshot.ambiguous.is_empty());
        }
        let mut snapshot = DiscoverySnapshot::default();
        snapshot.insert(a.clone());
        snapshot.insert(node("12:34:56:78:9A:BC", "/dev/c", "/sys/c", 3));
        snapshot.insert(alias);
        assert!(!snapshot.nodes.contains_key(&a.address));
        assert!(snapshot.ambiguous.contains(&a.address));
        let mut snapshot = DiscoverySnapshot::default();
        snapshot.insert(a.clone());
        let mut conflicting = a.clone();
        conflicting.node.interface = Some(1);
        snapshot.insert(conflicting);
        assert!(snapshot.ambiguous.contains(&a.address));

        let mut snapshot = DiscoverySnapshot::default();
        snapshot.insert(a.clone());
        let other = node("01:23:45:67:89:AB", "/dev/other", "/sys/a", 7);
        snapshot.insert(other.clone());
        assert!(snapshot.nodes.is_empty());
        assert!(snapshot.ambiguous.contains(&a.address));
        assert!(snapshot.ambiguous.contains(&other.address));
    }

    #[test]
    fn device_identity_uevent_requires_one_valid_remote_address() {
        let prefix = "HID_ID=0005:0000D7D7:00000041\nHID_PHYS=01:23:45:67:89:AB\n";
        for suffix in [
            "",
            "HID_UNIQ=\n",
            "HID_UNIQ=00:00:00:00:00:00\n",
            "HID_UNIQ=FF:FF:FF:FF:FF:FF\n",
            "HID_UNIQ=bad\n",
            "HID_UNIQ=12:34:56:78:9A:BC\nHID_UNIQ=12:34:56:78:9A:BC\n",
        ] {
            assert!(parse_uevent(&format!("{prefix}{suffix}")).is_err());
        }
        assert_eq!(
            parse_uevent(&format!("{prefix}HID_UNIQ=12:34:56:78:9a:bc\n"))
                .unwrap()
                .3
                .to_string(),
            "12:34:56:78:9A:BC"
        );
    }

    #[test]
    fn device_identity_open_comparison_rejects_reuse_and_mismatch() {
        let a = node("12:34:56:78:9a:bc", "/dev/a", "/sys/a", 1);
        let info = HidrawDevinfo {
            bustype: 5,
            vendor: VENDOR_ID as i16,
            product: PRODUCT_ID as i16,
        };
        let s = a.stamp;
        let bad = NodeStamp { inode: 2, ..s };
        let check = |before, opened, after, parent: &str, info, addr| {
            verify(&a, before, opened, after, Path::new(parent), info, addr).is_ok()
        };
        assert!(check(s, s, s, "/sys/a", info, a.address));
        assert!(!check(bad, s, s, "/sys/a", info, a.address));
        assert!(!check(s, bad, s, "/sys/a", info, a.address));
        assert!(!check(s, s, bad, "/sys/a", info, a.address));
        assert!(!check(s, s, s, "/sys/b", info, a.address));
        assert!(!check(
            s,
            s,
            s,
            "/sys/a",
            HidrawDevinfo { bustype: 3, ..info },
            a.address
        ));
        assert!(!check(
            s,
            s,
            s,
            "/sys/a",
            HidrawDevinfo { vendor: 1, ..info },
            a.address
        ));
        assert!(!check(
            s,
            s,
            s,
            "/sys/a",
            HidrawDevinfo { product: 1, ..info },
            a.address
        ));
        assert!(!check(
            s,
            s,
            s,
            "/sys/a",
            info,
            "01:23:45:67:89:AB".parse().unwrap()
        ));
        assert!(!check(
            s,
            NodeStamp {
                character: false,
                ..s
            },
            s,
            "/sys/a",
            info,
            a.address
        ));
    }

    #[test]
    fn device_identity_scan_failure_is_not_authoritative_empty() {
        let temp = tempfile::tempdir().unwrap();
        let missing = scan(&temp.path().join("missing"), temp.path());
        assert!(!missing.complete);
        assert!(!missing.errors.is_empty());
        let empty = scan(temp.path(), temp.path());
        assert!(empty.complete);
        assert!(empty.nodes.is_empty());
        std::fs::create_dir(temp.path().join("hidraw0")).unwrap();
        let partial = scan(temp.path(), temp.path());
        assert!(!partial.complete);
        assert!(!partial.errors.is_empty());
    }

    #[test]
    fn device_identity_unverifiable_entry_has_scoped_rejection() {
        let temp = tempfile::tempdir().unwrap();
        let sysfs = temp.path().join("hidraw0");
        let device = sysfs.join("device");
        std::fs::create_dir_all(&device).unwrap();
        for uniq in ["", "HID_UNIQ=invalid\n", "HID_UNIQ=00:00:00:00:00:00\n"] {
            std::fs::write(
                device.join("uevent"),
                format!("HID_ID=0005:0000D7D7:00000041\n{uniq}"),
            )
            .unwrap();
            let result = scan(temp.path(), temp.path());
            assert!(!result.complete);
            assert!(result.nodes.is_empty());
            assert!(result.rejected_sysfs.contains(&sysfs));
            assert!(result
                .rejected_parents
                .contains(&device.canonicalize().unwrap()));
        }
        std::fs::remove_file(device.join("uevent")).unwrap();
        let result = scan(temp.path(), temp.path());
        assert!(result.rejected_sysfs.contains(&sysfs));
        let missing_root = scan(&temp.path().join("absent"), temp.path());
        assert!(!missing_root.complete);
        assert!(missing_root.rejected_sysfs.is_empty());
        assert!(missing_root.rejected_parents.is_empty());
    }

    #[test]
    fn device_identity_checks_sysfs_device_number() {
        let temp = tempfile::tempdir().unwrap();
        let stamp = NodeStamp {
            dev: 1,
            inode: 1,
            rdev: libc::makedev(10, 27),
            character: true,
        };
        std::fs::write(temp.path().join("dev"), "10:27\n").unwrap();
        assert!(verify_sysfs_dev(temp.path(), stamp).is_ok());
        std::fs::write(temp.path().join("dev"), "10:28\n").unwrap();
        assert!(verify_sysfs_dev(temp.path(), stamp).is_err());
    }
}
