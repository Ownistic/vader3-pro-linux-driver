//! Advisory identity fencing for cooperating processes running as the same UID.
//!
//! Noncooperating processes and other users are not globally fenced. Lock files
//! are never unlinked: removing a locked inode would allow a second lock domain.

use std::ffi::{CString, OsStr};
use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::identity::MacAddress;

const APP: &str = "vader3p-steam";

fn cstring(name: &OsStr) -> Result<CString> {
    CString::new(name.as_bytes()).context("path contains NUL")
}

fn stat_fd(fd: RawFd) -> Result<libc::stat> {
    // SAFETY: zeroed stat is a valid writable buffer for fstat.
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd, &mut stat) } != 0 {
        return Err(io::Error::last_os_error()).context("fstat lease path");
    }
    Ok(stat)
}

fn stat_at(dir: RawFd, name: &CString) -> Result<libc::stat> {
    // SAFETY: name is NUL-terminated and stat points to writable storage.
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstatat(dir, name.as_ptr(), &mut stat, libc::AT_SYMLINK_NOFOLLOW) } != 0 {
        return Err(io::Error::last_os_error()).context("fstatat lease path");
    }
    Ok(stat)
}

fn same_inode(a: &libc::stat, b: &libc::stat) -> bool {
    a.st_dev == b.st_dev && a.st_ino == b.st_ino
}

fn validate_dir(stat: &libc::stat) -> Result<()> {
    // SAFETY: geteuid has no preconditions.
    if stat.st_uid != unsafe { libc::geteuid() }
        || stat.st_mode & libc::S_IFMT != libc::S_IFDIR
        || stat.st_mode & 0o077 != 0
    {
        bail!("lease directory must be private and owned by the effective UID");
    }
    Ok(())
}

fn validate_file(stat: &libc::stat) -> Result<()> {
    // SAFETY: geteuid has no preconditions.
    if stat.st_uid != unsafe { libc::geteuid() }
        || stat.st_mode & libc::S_IFMT != libc::S_IFREG
        || stat.st_mode & 0o7777 != 0o600
        || stat.st_nlink != 1
    {
        bail!("lease file must be UID-owned, regular, mode 0600 and have one link");
    }
    Ok(())
}

fn open_at(dir: RawFd, name: &CString, flags: libc::c_int, mode: libc::mode_t) -> Result<File> {
    // SAFETY: dir is live, name is NUL-terminated, and returned fd is owned here.
    let fd = unsafe { libc::openat(dir, name.as_ptr(), flags, mode) };
    if fd < 0 {
        return Err(io::Error::last_os_error()).context("openat lease path");
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

/// Verified private runtime directory and its private application subdirectory.
pub struct LeaseDirectory {
    runtime: File,
    app: File,
    runtime_path: PathBuf,
}

impl LeaseDirectory {
    /// Open XDG_RUNTIME_DIR, or /run/user/<effective UID> if it is unset.
    pub fn open_default() -> Result<Self> {
        // SAFETY: geteuid has no preconditions.
        let fallback = || PathBuf::from(format!("/run/user/{}", unsafe { libc::geteuid() }));
        let path = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(fallback);
        Self::open_at(&path)
    }

    /// Open a private UID-owned runtime directory and create its app directory.
    pub fn open_at(path: &Path) -> Result<Self> {
        let runtime = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
            .with_context(|| format!("opening runtime directory {}", path.display()))?;
        validate_dir(&stat_fd(runtime.as_raw_fd())?)?;
        let name = cstring(OsStr::new(APP))?;
        // SAFETY: runtime fd and the NUL-terminated name are valid.
        if unsafe { libc::mkdirat(runtime.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::AlreadyExists {
                return Err(error).context("creating lease directory");
            }
        }
        let app = open_at(
            runtime.as_raw_fd(),
            &name,
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0,
        )?;
        let directory = Self {
            runtime,
            app,
            runtime_path: path.to_path_buf(),
        };
        directory.validate()?;
        Ok(directory)
    }

    fn validate(&self) -> Result<()> {
        let runtime = stat_fd(self.runtime.as_raw_fd())?;
        let app = stat_fd(self.app.as_raw_fd())?;
        let linked = stat_at(self.runtime.as_raw_fd(), &cstring(OsStr::new(APP))?)?;
        validate_dir(&runtime)?;
        validate_dir(&app)?;
        validate_dir(&linked)?;
        let runtime_link = std::fs::symlink_metadata(&self.runtime_path)
            .context("checking runtime directory path")?;
        if runtime_link.dev() != runtime.st_dev
            || runtime_link.ino() != runtime.st_ino
            || !runtime_link.file_type().is_dir()
        {
            bail!("runtime directory was replaced");
        }
        if app.st_mode & 0o7777 != 0o700 {
            bail!("application lease directory must have mode 0700");
        }
        if !same_inode(&app, &linked) {
            bail!("lease directory was replaced");
        }
        Ok(())
    }

    fn lock(&self, name: &str) -> Result<File> {
        self.validate()?;
        let name = cstring(OsStr::new(name))?;
        let file = open_at(
            self.app.as_raw_fd(),
            &name,
            libc::O_RDWR | libc::O_CREAT | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
            0o600,
        )?;
        self.validate_lock(&file, &name)?;
        // SAFETY: flock operates on the open lock file descriptor.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(io::Error::last_os_error())
                .context("identity lease already held or unavailable");
        }
        self.validate_lock(&file, &name)?;
        Ok(file)
    }

    fn validate_lock(&self, file: &File, name: &CString) -> Result<()> {
        self.validate()?;
        let opened = stat_fd(file.as_raw_fd())?;
        let linked = stat_at(self.app.as_raw_fd(), name)?;
        validate_file(&opened)?;
        validate_file(&linked)?;
        if !same_inode(&opened, &linked) {
            bail!("lease file was replaced");
        }
        Ok(())
    }
}

/// Physical and virtual locks held until this bundle and all Arc owners drop it.
pub struct ControllerLease {
    _physical: File,
    _virtual: File,
    physical_address: MacAddress,
    virtual_address: MacAddress,
}

impl ControllerLease {
    /// Acquire both identities without waiting. A failed second lock releases the first.
    pub fn acquire(
        directory: &LeaseDirectory,
        physical: MacAddress,
        virtual_mac: MacAddress,
    ) -> Result<Self> {
        let physical_lock = directory.lock(&format!("physical-{}", physical.compact()))?;
        let virtual_lock = directory.lock(&format!("virtual-{}", virtual_mac.compact()))?;
        Ok(Self {
            _physical: physical_lock,
            _virtual: virtual_lock,
            physical_address: physical,
            virtual_address: virtual_mac,
        })
    }

    #[must_use]
    pub fn physical_address(&self) -> MacAddress {
        self.physical_address
    }

    #[must_use]
    pub fn virtual_address(&self) -> MacAddress {
        self.virtual_address
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::sync::{mpsc, Arc};

    fn address(s: &str) -> MacAddress {
        s.parse().unwrap()
    }
    fn pair_a() -> (MacAddress, MacAddress) {
        (address("12:34:56:78:9A:BC"), address("D2:00:76:FC:83:78"))
    }
    fn pair_b() -> (MacAddress, MacAddress) {
        (address("01:23:45:67:89:AB"), address("DA:21:3B:7F:A1:04"))
    }
    fn directory() -> (tempfile::TempDir, LeaseDirectory) {
        let temp = tempfile::tempdir().unwrap();
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let directory = LeaseDirectory::open_at(temp.path()).unwrap();
        (temp, directory)
    }

    #[test]
    fn lease_contention_collision_and_inode_persistence() {
        let (temp, dir) = directory();
        let (a, av) = pair_a();
        let (b, bv) = pair_b();
        let lock = ControllerLease::acquire(&dir, a, av).unwrap();
        let other = ControllerLease::acquire(&dir, b, bv).unwrap();
        assert!(ControllerLease::acquire(&dir, a, bv).is_err());
        drop(other);
        assert!(ControllerLease::acquire(&dir, b, av).is_err());
        // A failed virtual acquisition must release B's physical lock.
        let other = ControllerLease::acquire(&dir, b, bv).unwrap();
        let path = temp
            .path()
            .join(APP)
            .join(format!("physical-{}", a.compact()));
        let inode = std::fs::metadata(&path).unwrap().ino();
        drop(lock);
        let again = ControllerLease::acquire(&dir, a, av).unwrap();
        assert_eq!(std::fs::metadata(path).unwrap().ino(), inode);
        drop((other, again));
    }

    #[test]
    fn lease_rejects_unsafe_paths_and_replacement() {
        use std::os::unix::fs::symlink;
        let (temp, dir) = directory();
        let (a, av) = pair_a();
        let app = temp.path().join(APP);
        let lock = app.join(format!("physical-{}", a.compact()));
        symlink("missing", &lock).unwrap();
        assert!(ControllerLease::acquire(&dir, a, av).is_err());
        std::fs::remove_file(&lock).unwrap();
        std::fs::write(&lock, b"").unwrap();
        std::fs::set_permissions(&lock, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(ControllerLease::acquire(&dir, a, av).is_err());
        std::fs::set_permissions(&lock, std::fs::Permissions::from_mode(0o600)).unwrap();
        let alias = app.join("alias");
        std::fs::hard_link(&lock, &alias).unwrap();
        assert!(ControllerLease::acquire(&dir, a, av).is_err());
        std::fs::remove_file(alias).unwrap();
        std::fs::rename(&app, temp.path().join("old-app")).unwrap();
        std::fs::create_dir(&app).unwrap();
        std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(ControllerLease::acquire(&dir, a, av).is_err());
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(LeaseDirectory::open_at(temp.path()).is_err());
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let link = temp.path().join("runtime-link");
        symlink(&app, &link).unwrap();
        assert!(LeaseDirectory::open_at(&link).is_err());
    }

    #[test]
    fn lease_arc_owner_fences_until_thread_exit() {
        let (_temp, dir) = directory();
        let (a, av) = pair_a();
        let lease = Arc::new(ControllerLease::acquire(&dir, a, av).unwrap());
        let worker = Arc::clone(&lease);
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let _keep = worker;
            rx.recv().unwrap();
        });
        drop(lease);
        assert!(ControllerLease::acquire(&dir, a, av).is_err());
        tx.send(()).unwrap();
        thread.join().unwrap();
        assert!(ControllerLease::acquire(&dir, a, av).is_ok());
    }
}
