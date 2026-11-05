//! Keep process creation outside the parallel library unit-test process: a
//! forked child temporarily inherits that process's flock FDs until exec.

use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use vader3p_steam::identity::MacAddress;
use vader3p_steam::lease::{ControllerLease, LeaseDirectory};

fn addresses() -> (MacAddress, MacAddress) {
    let physical: MacAddress = "12:34:56:78:9A:BC".parse().unwrap();
    (physical, physical.virtual_address())
}

#[test]
fn lease_child_helper() {
    let Some(path) = std::env::var_os("VADER_TEST_LEASE_CHILD") else {
        return;
    };
    let dir = LeaseDirectory::open_at(Path::new(&path)).unwrap();
    let (physical, virtual_mac) = addresses();
    let _lease = ControllerLease::acquire(&dir, physical, virtual_mac).unwrap();
    println!("LEASE_CHILD_READY");
    std::thread::sleep(Duration::from_secs(30));
}

#[test]
fn lease_child_process_death_releases_lock() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let dir = LeaseDirectory::open_at(temp.path()).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "lease_child_helper", "--nocapture"])
        .env("VADER_TEST_LEASE_CHILD", temp.path())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if line.unwrap_or_default().contains("LEASE_CHILD_READY") {
                let _ = tx.send(());
                break;
            }
        }
    });
    let ready = rx.recv_timeout(Duration::from_secs(5)).is_ok();
    let (physical, virtual_mac) = addresses();
    let contended = ready && ControllerLease::acquire(&dir, physical, virtual_mac).is_err();
    let _ = child.kill();
    child.wait().unwrap();
    reader.join().unwrap();
    assert!(ready, "child did not acquire lease");
    assert!(contended, "child did not exclude parent");
    assert!(ControllerLease::acquire(&dir, physical, virtual_mac).is_ok());
}
