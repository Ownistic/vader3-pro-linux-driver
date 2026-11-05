//! Supervises independent controller threads without performing device I/O.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use super::{run_pinned_session, Config, Duration, Instant, Result, SessionExit};
use crate::device::{self, DiscoverySnapshot, IdentifiedNode};
use crate::identity::MacAddress;
use crate::lease::{ControllerLease, LeaseDirectory};

const SCAN_INTERVAL: Duration = Duration::from_secs(1);
const MANAGER_TICK: Duration = Duration::from_millis(25);
const SHUTDOWN_GRACE: Duration = Duration::from_millis(1200);

type Runner = dyn Fn(Config, IdentifiedNode, Arc<ControllerLease>, Arc<AtomicBool>) -> Result<SessionExit>
    + Send
    + Sync;

struct Session {
    node: IdentifiedNode,
    stop: Arc<AtomicBool>,
    thread: JoinHandle<Result<SessionExit>>,
}

struct Retiring {
    address: MacAddress,
    thread: JoinHandle<()>,
}

/// Discovery and lifecycle coordinator; each controller does its I/O elsewhere.
pub struct ControllerFleet {
    cfg: Config,
    filter: Option<MacAddress>,
    leases: LeaseDirectory,
    sessions: BTreeMap<MacAddress, Session>,
    retiring: Vec<Retiring>,
    runner: Arc<Runner>,
}

fn collisions(pairs: impl IntoIterator<Item = (MacAddress, MacAddress)>) -> BTreeSet<MacAddress> {
    let mut by_virtual: BTreeMap<MacAddress, BTreeSet<MacAddress>> = BTreeMap::new();
    for (physical, virtual_mac) in pairs {
        by_virtual.entry(virtual_mac).or_default().insert(physical);
    }
    by_virtual
        .into_values()
        .filter(|group| group.len() > 1)
        .flatten()
        .collect()
}

impl ControllerFleet {
    /// Validate configuration before opening the private per-user lease directory.
    pub fn new(cfg: Config) -> Result<Self> {
        let filter = cfg
            .bluetooth_address
            .as_deref()
            .map(str::parse)
            .transpose()?;
        cfg.validate()?;
        let leases = LeaseDirectory::open_default()?;
        Ok(Self::with_runner(
            cfg,
            filter,
            leases,
            Arc::new(run_pinned_session),
        ))
    }

    fn with_runner(
        cfg: Config,
        filter: Option<MacAddress>,
        leases: LeaseDirectory,
        runner: Arc<Runner>,
    ) -> Self {
        Self {
            cfg,
            filter,
            leases,
            sessions: BTreeMap::new(),
            retiring: Vec::new(),
            runner,
        }
    }

    /// Run scans until requested to stop; all sessions are signalled before the
    /// single global shutdown grace period begins.
    pub fn run<F: Fn() -> bool>(&mut self, should_stop: F) -> Result<()> {
        let mut next_scan = Instant::now();
        while !should_stop() {
            self.reap();
            if Instant::now() >= next_scan {
                self.reconcile(device::discover_identified());
                next_scan = Instant::now() + SCAN_INTERVAL;
            }
            thread::sleep(MANAGER_TICK);
        }
        self.shutdown();
        Ok(())
    }

    fn reap(&mut self) {
        let done: Vec<_> = self
            .sessions
            .iter()
            .filter_map(|(address, session)| session.thread.is_finished().then_some(*address))
            .collect();
        for address in done {
            let session = self
                .sessions
                .remove(&address)
                .expect("finished session exists");
            match session.thread.join() {
                Ok(Ok(exit)) => {
                    if let Err(error) = exit.result {
                        eprintln!("[{address}] session ended: {error:#}");
                    }
                    if let Some(thread) = exit.retiring_worker {
                        self.retiring.push(Retiring { address, thread });
                    }
                }
                Ok(Err(error)) => eprintln!("[{address}] session setup failed: {error:#}"),
                Err(_) => eprintln!("[{address}] session thread panicked"),
            }
        }
        let mut index = 0;
        while index < self.retiring.len() {
            if self.retiring[index].thread.is_finished() {
                let retired = self.retiring.swap_remove(index);
                if retired.thread.join().is_err() {
                    eprintln!("[{}] BLE worker panicked", retired.address);
                }
            } else {
                index += 1;
            }
        }
    }

    fn reconcile(&mut self, snapshot: DiscoverySnapshot) {
        for error in &snapshot.errors {
            eprintln!("discovery: {error}");
        }
        let blocked = collisions(
            snapshot
                .nodes
                .keys()
                .copied()
                .chain(snapshot.ambiguous.iter().copied())
                .chain(self.sessions.keys().copied())
                .chain(self.retiring.iter().map(|worker| worker.address))
                .map(|address| (address, address.virtual_address())),
        );
        for (address, session) in &self.sessions {
            let changed = snapshot
                .nodes
                .get(address)
                .is_some_and(|node| !node.same_binding(&session.node));
            let absent = snapshot.complete && !snapshot.nodes.contains_key(address);
            if self.filter.is_some_and(|filter| filter != *address)
                || blocked.contains(address)
                || snapshot.ambiguous.contains(address)
                || snapshot.rejected_sysfs.contains(&session.node.node.sysfs)
                || snapshot.rejected_parents.contains(&session.node.hid_parent)
                || changed
                || absent
            {
                session.stop.store(true, Ordering::Release);
            }
        }
        // Incomplete scans cannot establish absence or admit new sessions.
        if !snapshot.complete {
            return;
        }
        for (address, node) in snapshot.nodes {
            if self.filter.is_some_and(|filter| filter != address)
                || blocked.contains(&address)
                || self.sessions.contains_key(&address)
                || self.retiring.iter().any(|worker| worker.address == address)
            {
                continue;
            }
            let lease =
                match ControllerLease::acquire(&self.leases, address, address.virtual_address()) {
                    Ok(lease) => Arc::new(lease),
                    Err(error) => {
                        eprintln!("[{address}] lease unavailable: {error:#}");
                        continue;
                    }
                };
            let stop = Arc::new(AtomicBool::new(false));
            let thread_stop = Arc::clone(&stop);
            let runner = Arc::clone(&self.runner);
            let cfg = self.cfg.clone();
            let thread_node = node.clone();
            match thread::Builder::new()
                .name(format!("vader-{}", address.compact()))
                .spawn(move || runner(cfg, thread_node, lease, thread_stop))
            {
                Ok(thread) => {
                    self.sessions
                        .insert(address, Session { node, stop, thread });
                }
                Err(error) => eprintln!("[{address}] spawning session failed: {error}"),
            }
        }
    }

    fn shutdown(&mut self) {
        for session in self.sessions.values() {
            session.stop.store(true, Ordering::Release);
        }
        let deadline = Instant::now() + SHUTDOWN_GRACE;
        while Instant::now() < deadline {
            self.reap();
            if self.sessions.is_empty() && self.retiring.is_empty() {
                return;
            }
            thread::sleep(MANAGER_TICK.min(deadline.saturating_duration_since(Instant::now())));
        }
        // Dropping unfinished JoinHandles detaches; actual owners keep their leases.
        self.sessions.clear();
        self.retiring.clear();
    }
}

impl Drop for ControllerFleet {
    fn drop(&mut self) {
        for session in self.sessions.values() {
            session.stop.store(true, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ble_worker::BleWorker;
    use crate::input::{button, State};
    use crate::uhid_runtime::session::test_support::{fixture, run_fake, Fixture, SetupFaults};
    use crate::uhid_runtime::session::tests::FakeSession;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::AtomicUsize;
    use std::sync::mpsc;

    fn report_matching(fixture: &Fixture, predicate: impl Fn(&[u8]) -> bool) -> [u8; 78] {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut packet = [0u8; 128];
        loop {
            assert!(Instant::now() < deadline, "timed out waiting for report");
            let len = fixture.host.recv(&mut packet).unwrap();
            if len == 84
                && u32::from_ne_bytes(packet[..4].try_into().unwrap()) == 12
                && predicate(&packet[6..84])
            {
                return packet[6..84].try_into().unwrap();
            }
        }
    }

    fn send_feedback(fixture: &Fixture, sdl: bool, left: u8, right: u8) {
        let mut event = vec![0u8; 4103];
        event[..4].copy_from_slice(&6u32.to_ne_bytes());
        event[4] = 0x31;
        let base = if sdl {
            event[5] = 2;
            2
        } else {
            event[6] = 0x10;
            3
        };
        event[4 + base] = 1;
        event[4 + base + 2] = right;
        event[4 + base + 3] = left;
        crate::edge_report::bluetooth_append_crc(0xa2, &mut event[4..82]);
        event[4100..4102].copy_from_slice(&78u16.to_ne_bytes());
        event[4102] = 1;
        fixture.host.send(&event).unwrap();
    }

    fn drain_reports(fixture: &Fixture) {
        fixture.host.set_nonblocking(true).unwrap();
        let mut packet = [0u8; 128];
        for _ in 0..1024 {
            match fixture.host.recv(&mut packet) {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    fixture.host.set_nonblocking(false).unwrap();
                    return;
                }
                Err(error) => panic!("draining fake host: {error}"),
            }
        }
        panic!("fake host did not drain");
    }

    fn raw_hid(buttons: u8, extras: u8) -> Vec<u8> {
        let mut frame = vec![0u8; crate::input::REPORT_LEN];
        frame[0] = crate::input::REPORT_ID;
        frame[9] = buttons;
        frame[11] = extras;
        frame
    }

    fn mac(s: &str) -> MacAddress {
        s.parse().unwrap()
    }
    fn a() -> MacAddress {
        mac("12:34:56:78:9A:BC")
    }
    fn b() -> MacAddress {
        mac("01:23:45:67:89:AB")
    }
    fn node(address: MacAddress, path: &str) -> IdentifiedNode {
        IdentifiedNode::fixture(address, path)
    }
    fn snapshot(nodes: Vec<IdentifiedNode>, complete: bool) -> DiscoverySnapshot {
        DiscoverySnapshot::fixture(nodes, complete)
    }
    fn wait_until(mut predicate: impl FnMut() -> bool) {
        let end = Instant::now() + Duration::from_secs(3);
        while !predicate() {
            assert!(Instant::now() < end, "timed out waiting for test worker");
            thread::sleep(Duration::from_millis(2));
        }
    }
    fn dir() -> (tempfile::TempDir, LeaseDirectory) {
        let temp = tempfile::tempdir().unwrap();
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let directory = LeaseDirectory::open_at(temp.path()).unwrap();
        (temp, directory)
    }

    #[test]
    fn fleet_collision_policy_rejects_all_conflicting_physical_ids() {
        let v = a().virtual_address();
        let blocked = collisions([(a(), v), (b(), v), (a(), v)]);
        assert_eq!(blocked, BTreeSet::from([a(), b()]));
        assert!(collisions([(a(), v), (a(), v)]).is_empty());
    }

    #[test]
    fn fleet_rejects_invalid_filter_before_opening_runtime_directory() {
        let cfg = Config {
            rumble: false,
            bluetooth_address: Some("not-a-mac".into()),
            ..Config::default()
        };
        assert!(ControllerFleet::new(cfg).is_err());
    }

    #[test]
    fn fleet_filter_admits_only_matching_identity() {
        let (_temp, directory) = dir();
        let runner: Arc<Runner> = Arc::new(|_, _, _lease, stop| {
            while !stop.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(2));
            }
            Ok(SessionExit {
                result: Ok(()),
                retiring_worker: None,
            })
        });
        let mut fleet =
            ControllerFleet::with_runner(Config::default(), Some(a()), directory, runner);
        fleet.reconcile(snapshot(vec![node(b(), "b"), node(a(), "a")], true));
        assert_eq!(fleet.sessions.keys().copied().collect::<Vec<_>>(), [a()]);
        fleet.shutdown();
    }

    #[test]
    fn fleet_partial_unverifiable_binding_stops_only_affected_session() {
        let (_temp, directory) = dir();
        let runner: Arc<Runner> = Arc::new(|_, _, _lease, stop| {
            while !stop.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(2));
            }
            Ok(SessionExit {
                result: Ok(()),
                retiring_worker: None,
            })
        });
        let mut fleet = ControllerFleet::with_runner(Config::default(), None, directory, runner);
        let first = node(a(), "a");
        let second = node(b(), "b");
        fleet.reconcile(snapshot(vec![first.clone(), second.clone()], true));
        assert_eq!(fleet.sessions.len(), 2);
        fleet.reconcile(snapshot(vec![], false));
        assert!(!fleet.sessions[&a()].stop.load(Ordering::Acquire));
        assert!(!fleet.sessions[&b()].stop.load(Ordering::Acquire));
        let mut unrelated = snapshot(vec![second.clone()], false);
        unrelated.rejected_sysfs.insert("/sys/unrelated".into());
        fleet.reconcile(unrelated);
        assert!(!fleet.sessions[&a()].stop.load(Ordering::Acquire));
        assert!(!fleet.sessions[&b()].stop.load(Ordering::Acquire));
        let mut rejected = snapshot(vec![second.clone()], false);
        rejected.rejected_sysfs.insert(first.node.sysfs.clone());
        fleet.reconcile(rejected);
        assert!(fleet.sessions[&a()].stop.load(Ordering::Acquire));
        assert!(!fleet.sessions[&b()].stop.load(Ordering::Acquire));
        let mut rejected_alias = snapshot(vec![], false);
        rejected_alias
            .rejected_parents
            .insert(second.hid_parent.clone());
        fleet.reconcile(rejected_alias);
        assert!(fleet.sessions[&b()].stop.load(Ordering::Acquire));
        fleet.shutdown();
    }

    #[test]
    fn fleet_real_loops_isolate_feedback_and_survive_delayed_peer_retirement() {
        let (_temp, directory) = dir();
        let pad_release = Arc::new(AtomicBool::new(false));
        let ble_release = Arc::new(AtomicBool::new(false));
        let (entered_tx, entered_rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::channel();
        let runner: Arc<Runner> = {
            let pad_release = Arc::clone(&pad_release);
            let ble_release = Arc::clone(&ble_release);
            Arc::new(move |cfg, node, lease, stop| {
                let address = node.address;
                let (fixture, mut io, pad) = fixture(address);
                fixture.host.send(&2u32.to_ne_bytes()).unwrap();
                fixture.host.send(&4u32.to_ne_bytes()).unwrap();
                let worker = if address == a() {
                    let release = Arc::clone(&ble_release);
                    Some(
                        BleWorker::spawn_scoped_test(Arc::clone(&lease), move || {
                            let end = Instant::now() + Duration::from_secs(5);
                            while !release.load(Ordering::Acquire) && Instant::now() < end {
                                thread::sleep(Duration::from_millis(2));
                            }
                        })
                        .unwrap(),
                    )
                } else {
                    None
                };
                let worker_state = worker.as_ref().map(BleWorker::test_state);
                io.worker_state = worker_state.clone();
                let mut faults = SetupFaults::default();
                if address == a() {
                    let release = Arc::clone(&pad_release);
                    let entered = entered_tx.clone();
                    faults.before_close = Some(Box::new(move || {
                        let _ = entered.send(());
                        let end = Instant::now() + Duration::from_secs(5);
                        while !release.load(Ordering::Acquire) && Instant::now() < end {
                            thread::sleep(Duration::from_millis(2));
                        }
                    }));
                }
                started_tx
                    .send((address, node.node.path.clone(), fixture, worker_state))
                    .unwrap();
                run_fake(cfg, lease, stop, io, || Ok(pad), faults, worker)
            })
        };
        let mut cfg = Config::default();
        cfg.motion.enabled = true;
        let mut fleet = ControllerFleet::with_runner(cfg, None, directory, runner);
        let old_a = node(a(), "a-old-real-loop");
        let b_node = node(b(), "b-real-loop");
        fleet.reconcile(snapshot(vec![old_a.clone(), b_node.clone()], true));
        let first = started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let second = started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let (a_fixture, b_fixture, a_worker) = if first.0 == a() {
            (first.2, second.2, first.3.unwrap())
        } else {
            (second.2, first.2, second.3.unwrap())
        };
        a_fixture.physical.send(Ok(raw_hid(0, 0))).unwrap();
        b_fixture.physical.send(Ok(raw_hid(0, 0))).unwrap();
        report_matching(&a_fixture, |r| r[9] & 0xf0 == 0 && r[11] & 1 == 0);
        report_matching(&b_fixture, |r| r[9] & 0xf0 == 0 && r[11] & 1 == 0);
        send_feedback(&a_fixture, false, 255, 128);
        send_feedback(&b_fixture, true, 64, 255);
        assert_eq!(
            a_fixture
                .feedback
                .recv_timeout(Duration::from_secs(2))
                .unwrap(),
            (250, 125)
        );
        assert_eq!(
            b_fixture
                .feedback
                .recv_timeout(Duration::from_secs(2))
                .unwrap(),
            (63, 250)
        );
        let (shutdown, connected, desired, old_generation) = a_worker.snapshot();
        assert!(!shutdown && connected);
        assert_eq!(desired, (250, 125));
        a_fixture.physical.send(Ok(raw_hid(0x10, 0x04))).unwrap();
        let mut nus = [0u8; 20];
        nus[..4].fill(0x80);
        nus[4] = 0x20;
        nus[9] = 1;
        nus[18] = 0xfe;
        *b_fixture.nus.lock().unwrap() =
            Some((crate::nus_input::parse(&nus).unwrap(), Duration::ZERO));
        *b_fixture.motion.lock().unwrap() = Some(crate::edge_report::MotionSample {
            raw: crate::motion::RawMotion {
                first: 3,
                second: 5,
            },
            age: Duration::ZERO,
        });
        report_matching(&a_fixture, |r| r[9] & 0x20 != 0 && r[11] & 0x10 != 0);
        let report = report_matching(&b_fixture, |r| r[9] & 0x40 != 0 && r[11] & 2 != 0);
        assert_eq!(i16::from_le_bytes([report[17], report[18]]), -5);
        assert_eq!(i16::from_le_bytes([report[19], report[20]]), -3);
        fleet.reconcile(snapshot(vec![b_node.clone(), old_a.clone()], true));
        assert!(started_rx.try_recv().is_err());
        a_fixture.physical.send(Ok(raw_hid(0, 0))).unwrap();
        report_matching(&a_fixture, |r| r[9] & 0x20 == 0 && r[11] & 1 == 0);
        let mut a_nus = [0u8; 20];
        a_nus[..4].fill(0x80);
        a_nus[9] = 2;
        a_nus[18] = 0xfe;
        *a_fixture.nus.lock().unwrap() =
            Some((crate::nus_input::parse(&a_nus).unwrap(), Duration::ZERO));
        report_matching(&a_fixture, |r| r[11] & 0x04 != 0 && r[11] & 1 == 0);
        a_nus[8] = 1;
        *a_fixture.nus.lock().unwrap() =
            Some((crate::nus_input::parse(&a_nus).unwrap(), Duration::ZERO));
        report_matching(&a_fixture, |r| r[11] & 1 != 0 && r[9] & 0x20 == 0);
        report_matching(&a_fixture, |r| r[11] & 1 != 0 && r[9] & 0x20 != 0);
        a_fixture.hup.store(true, Ordering::Release);
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let (shutdown, connected, desired, generation) = a_worker.snapshot();
        assert!(shutdown && !connected && generation != old_generation);
        assert_eq!(desired, (0, 0));
        let mut old_notification = [0u8; 20];
        old_notification[18] = 0xfe;
        assert!(!a_worker.observe(old_generation, &old_notification));
        drain_reports(&b_fixture);
        for _ in 0..4 {
            report_matching(&b_fixture, |r| r[9] & 0x40 != 0);
        }
        assert!(ControllerLease::acquire(&fleet.leases, a(), a().virtual_address()).is_err());
        pad_release.store(true, Ordering::Release);
        wait_until(|| {
            fleet.reap();
            !fleet.sessions.contains_key(&a())
        });
        let mut packet = [0u8; 128];
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            assert!(Instant::now() < deadline, "timed out waiting for A destroy");
            let n = a_fixture.host.recv(&mut packet).unwrap();
            if n == 4 && u32::from_ne_bytes(packet[..4].try_into().unwrap()) == 1 {
                break;
            }
        }
        assert!(fleet.retiring.iter().any(|worker| worker.address == a()));
        assert!(ControllerLease::acquire(&fleet.leases, a(), a().virtual_address()).is_err());
        drain_reports(&b_fixture);
        for _ in 0..4 {
            report_matching(&b_fixture, |r| r[9] & 0x40 != 0);
        }
        ble_release.store(true, Ordering::Release);
        wait_until(|| {
            fleet.reap();
            fleet.retiring.is_empty()
        });
        assert!(a_fixture.physical.send(Ok(raw_hid(0, 0))).is_err());
        let new_a = node(a(), "a-new-real-loop");
        assert_ne!(new_a.fixture_inode(), old_a.fixture_inode());
        fleet.reconcile(snapshot(vec![new_a, b_node], true));
        let (address, path, replacement, replacement_worker) =
            started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(address, a());
        assert_eq!(path, std::path::Path::new("/dev/a-new-real-loop"));
        assert_eq!(replacement.virtual_identity, a_fixture.virtual_identity);
        assert_eq!(replacement.virtual_identity, a().virtual_address());
        assert_eq!(replacement_worker.unwrap().snapshot().2, (0, 0));
        assert!(a_fixture.host.send(&4u32.to_ne_bytes()).is_err());
        replacement.physical.send(Ok(raw_hid(0x20, 0x80))).unwrap();
        for _ in 0..3 {
            report_matching(&replacement, |r| r[9] & 0x40 != 0 && r[11] & 0x05 == 0);
        }
        assert!(replacement.feedback.try_recv().is_err());
        replacement.physical.send(Ok(raw_hid(0, 0))).unwrap();
        report_matching(&replacement, |r| r[9] & 0x40 == 0 && r[11] & 1 == 0);
        replacement.physical.send(Ok(raw_hid(0, 0x80))).unwrap();
        report_matching(&replacement, |r| r[11] & 1 != 0);
        replacement.physical.send(Ok(raw_hid(0x20, 0))).unwrap();
        report_matching(&replacement, |r| r[9] & 0x40 != 0);
        send_feedback(&b_fixture, true, 128, 64);
        assert_eq!(
            b_fixture
                .feedback
                .recv_timeout(Duration::from_secs(2))
                .unwrap(),
            (125, 63)
        );
        fleet.shutdown();
    }

    #[test]
    fn fleet_real_loop_setup_and_io_failures_are_isolated() {
        for case in ["open", "create", "grab", "read", "send"] {
            let (_temp, directory) = dir();
            let (started_tx, started_rx) = mpsc::channel();
            let (finished_tx, finished_rx) = mpsc::channel();
            let warned = Arc::new(AtomicBool::new(false));
            let runner: Arc<Runner> = {
                let warned = Arc::clone(&warned);
                Arc::new(move |cfg, node, lease, stop| {
                    let address = node.address;
                    let (fixture, io, pad) = fixture(address);
                    fixture.host.send(&2u32.to_ne_bytes()).unwrap();
                    fixture.host.send(&4u32.to_ne_bytes()).unwrap();
                    started_tx.send((address, fixture)).unwrap();
                    let is_a = address == a();
                    let faults = SetupFaults {
                        open: is_a && case == "open",
                        grab: is_a && case == "grab",
                        grab_warning: Arc::clone(&warned),
                        ..SetupFaults::default()
                    };
                    let result = run_fake(
                        cfg,
                        lease,
                        stop,
                        io,
                        || {
                            if is_a && case == "create" {
                                anyhow::bail!("injected UHID create failure");
                            }
                            Ok(pad)
                        },
                        faults,
                        None,
                    );
                    if is_a {
                        let error = match &result {
                            Ok(exit) => exit.result.as_ref().err().map(|e| format!("{e:#}")),
                            Err(e) => Some(format!("{e:#}")),
                        };
                        let _ = finished_tx.send(error);
                    }
                    result
                })
            };
            let mut fleet =
                ControllerFleet::with_runner(Config::default(), None, directory, runner);
            fleet.reconcile(snapshot(
                vec![node(a(), "failure-a"), node(b(), "failure-b")],
                true,
            ));
            let first = started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            let second = started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            let (a_fixture, b_fixture) = if first.0 == a() {
                (first.1, second.1)
            } else {
                (second.1, first.1)
            };
            b_fixture.physical.send(Ok(raw_hid(0x20, 0))).unwrap();
            report_matching(&b_fixture, |r| r[9] & 0x40 != 0);
            if matches!(case, "grab" | "read" | "send") {
                a_fixture.physical.send(Ok(raw_hid(0, 0))).unwrap();
                report_matching(&a_fixture, |r| r[11] & 1 == 0);
                send_feedback(&a_fixture, false, 255, 128);
                assert_eq!(
                    a_fixture
                        .feedback
                        .recv_timeout(Duration::from_secs(2))
                        .unwrap(),
                    (250, 125)
                );
                a_fixture.physical.send(Ok(raw_hid(0, 0x80))).unwrap();
                report_matching(&a_fixture, |r| r[11] & 1 != 0 && r[9] & 0x20 == 0);
            }
            match case {
                "grab" => {
                    a_fixture.physical.send(Ok(raw_hid(0x10, 0))).unwrap();
                    report_matching(&a_fixture, |r| r[9] & 0x20 != 0);
                    assert!(warned.load(Ordering::Acquire));
                    a_fixture.hup.store(true, Ordering::Release);
                }
                "read" => {
                    a_fixture
                        .physical
                        .send(Err(anyhow::anyhow!("injected physical EOF")))
                        .unwrap();
                }
                "send" => {
                    drop(a_fixture.host);
                }
                _ => {}
            }
            let error = finished_rx
                .recv_timeout(Duration::from_secs(3))
                .unwrap()
                .expect("A must fail");
            match case {
                "open" => assert!(error.contains("physical open failure")),
                "create" => assert!(error.contains("UHID create failure")),
                "grab" => assert!(error.contains("physical HUP")),
                "read" => assert!(error.contains("physical EOF")),
                "send" => assert!(error.contains("UHID")),
                _ => unreachable!(),
            }
            wait_until(|| {
                fleet.reap();
                !fleet.sessions.contains_key(&a())
            });
            assert!(ControllerLease::acquire(&fleet.leases, a(), a().virtual_address()).is_ok());
            for _ in 0..3 {
                report_matching(&b_fixture, |r| r[9] & 0x40 != 0);
            }
            assert!(fleet.sessions.contains_key(&b()));
            fleet.shutdown();
        }
    }

    #[test]
    fn fleet_ble_spawn_eagain_retires_only_failed_session_after_pad_close() {
        let (_temp, directory) = dir();
        let release = Arc::new(AtomicBool::new(false));
        let (created_tx, created_rx) = mpsc::channel();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::channel();
        let (finished_tx, finished_rx) = mpsc::channel();
        let runner: Arc<Runner> = {
            let release = Arc::clone(&release);
            Arc::new(move |cfg, node, lease, stop| {
                let address = node.address;
                let (fixture, io, pad) = fixture(address);
                fixture.host.send(&2u32.to_ne_bytes()).unwrap();
                fixture.host.send(&4u32.to_ne_bytes()).unwrap();
                started_tx.send((address, fixture)).unwrap();
                let mut faults = SetupFaults::default();
                if address == a() {
                    faults.worker_spawn = true;
                    let entered = entered_tx.clone();
                    let release = Arc::clone(&release);
                    faults.before_close = Some(Box::new(move || {
                        let _ = entered.send(());
                        let deadline = Instant::now() + Duration::from_secs(5);
                        while !release.load(Ordering::Acquire) && Instant::now() < deadline {
                            thread::sleep(Duration::from_millis(2));
                        }
                    }));
                }
                let result = run_fake(
                    cfg,
                    lease,
                    stop,
                    io,
                    || {
                        if address == a() {
                            created_tx.send(()).unwrap();
                        }
                        Ok(pad)
                    },
                    faults,
                    None,
                );
                if address == a() {
                    let error = result.as_ref().unwrap().result.as_ref().unwrap_err();
                    let errno = error
                        .downcast_ref::<std::io::Error>()
                        .and_then(std::io::Error::raw_os_error);
                    let _ = finished_tx.send((format!("{error:#}"), errno));
                }
                result
            })
        };
        let mut fleet = ControllerFleet::with_runner(Config::default(), None, directory, runner);
        fleet.reconcile(snapshot(
            vec![node(a(), "spawn-fail-a"), node(b(), "spawn-fail-b")],
            true,
        ));
        let first = started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let second = started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let (a_fixture, b_fixture) = if first.0 == a() {
            (first.1, second.1)
        } else {
            (second.1, first.1)
        };
        created_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let other = mac("22:33:44:55:66:77");
        assert!(ControllerLease::acquire(&fleet.leases, a(), other.virtual_address()).is_err());
        assert!(ControllerLease::acquire(&fleet.leases, other, a().virtual_address()).is_err());
        b_fixture.physical.send(Ok(raw_hid(0x20, 0))).unwrap();
        report_matching(&b_fixture, |r| r[9] & 0x40 != 0);
        drain_reports(&b_fixture);
        for _ in 0..4 {
            report_matching(&b_fixture, |r| r[9] & 0x40 != 0);
        }
        send_feedback(&b_fixture, true, 128, 255);
        assert_eq!(
            b_fixture
                .feedback
                .recv_timeout(Duration::from_secs(2))
                .unwrap(),
            (125, 250)
        );
        assert!(!fleet.sessions[&b()].stop.load(Ordering::Acquire));
        release.store(true, Ordering::Release);
        let (error, errno) = finished_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(error.contains("spawning BLE worker thread"));
        assert_eq!(errno, Some(libc::EAGAIN));
        wait_until(|| {
            fleet.reap();
            !fleet.sessions.contains_key(&a())
        });
        let mut event = [0u8; 128];
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            assert!(Instant::now() < deadline, "timed out waiting for A destroy");
            let len = a_fixture.host.recv(&mut event).unwrap();
            if len == 4 && u32::from_ne_bytes(event[..4].try_into().unwrap()) == 1 {
                break;
            }
        }
        assert!(a_fixture.host.send(&4u32.to_ne_bytes()).is_err());
        assert!(ControllerLease::acquire(&fleet.leases, a(), other.virtual_address()).is_ok());
        assert!(ControllerLease::acquire(&fleet.leases, other, a().virtual_address()).is_ok());
        drain_reports(&b_fixture);
        for _ in 0..4 {
            report_matching(&b_fixture, |r| r[9] & 0x40 != 0);
        }
        assert!(fleet.sessions.contains_key(&b()));
        fleet.shutdown();
    }

    #[test]
    fn fleet_two_threads_reconnect_and_fenced_delayed_teardown() {
        let (_temp, directory) = dir();
        let pad_release = Arc::new(AtomicBool::new(false));
        let ble_release = Arc::new(AtomicBool::new(false));
        let b_ticks = Arc::new(AtomicUsize::new(0));
        let (starts_tx, starts_rx) = mpsc::channel();
        let runner: Arc<Runner> = {
            let pad_release = Arc::clone(&pad_release);
            let ble_release = Arc::clone(&ble_release);
            let b_ticks = Arc::clone(&b_ticks);
            Arc::new(move |_, node, lease, stop| {
                let address = node.address;
                let worker = if address == a() {
                    let release = Arc::clone(&ble_release);
                    Some(
                        BleWorker::spawn_scoped_test(Arc::clone(&lease), move || {
                            let end = Instant::now() + Duration::from_secs(5);
                            while !release.load(Ordering::Acquire) && Instant::now() < end {
                                thread::sleep(Duration::from_millis(2));
                            }
                        })
                        .unwrap(),
                    )
                } else {
                    None
                };
                starts_tx
                    .send((address, node.node.path.clone(), lease.virtual_address()))
                    .unwrap();
                let mut fake = FakeSession::default();
                let mut tick = 0u64;
                while !stop.load(Ordering::Acquire) {
                    let state = if address == a() && tick > 0 {
                        State {
                            buttons: 1 << button::FN,
                            ..State::default()
                        }
                    } else if address == b() {
                        State {
                            buttons: 1 << button::B,
                            ..State::default()
                        }
                    } else {
                        State::default()
                    };
                    let nus = (address == b()).then_some((state, Duration::ZERO));
                    let report = fake.tick(Some(state), nus, Some((2, 3)), tick);
                    if address == a() && tick == 0 {
                        assert_eq!(report[11] & 1, 0);
                    }
                    if address == a() && tick == 1 {
                        assert_eq!(report[11] & 1, 1);
                    }
                    if address == b() {
                        assert_eq!(report[9] & 0x40, 0x40);
                        b_ticks.fetch_add(1, Ordering::Relaxed);
                    }
                    tick += 1;
                    thread::sleep(Duration::from_millis(2));
                }
                let (result, retiring_worker) = super::super::session::retire_resources(
                    worker,
                    lease,
                    || {
                        if address == a() {
                            let end = Instant::now() + Duration::from_secs(5);
                            while !pad_release.load(Ordering::Acquire) && Instant::now() < end {
                                thread::sleep(Duration::from_millis(2));
                            }
                        }
                        Ok(())
                    },
                    || {},
                );
                Ok(SessionExit {
                    result,
                    retiring_worker,
                })
            })
        };
        let mut fleet = ControllerFleet::with_runner(Config::default(), None, directory, runner);
        let old_a = node(a(), "a-old");
        let b_node = node(b(), "b");
        fleet.reconcile(snapshot(vec![old_a.clone(), b_node.clone()], true));
        let first = starts_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        let second = starts_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        let initial_a = if first.0 == a() { first } else { second };
        assert_eq!(initial_a.2, a().virtual_address());
        fleet.reconcile(snapshot(vec![b_node.clone(), old_a.clone()], true));
        fleet.reconcile(snapshot(vec![], false));
        assert_eq!(fleet.sessions.len(), 2);
        fleet.reconcile(snapshot(
            vec![old_a.clone(), node(a(), "a-conflict"), b_node.clone()],
            false,
        ));
        assert!(fleet.sessions[&a()].stop.load(Ordering::Acquire));
        assert!(!fleet.sessions[&b()].stop.load(Ordering::Acquire));
        fleet.reconcile(snapshot(vec![b_node.clone()], true));
        assert!(fleet.sessions[&a()].stop.load(Ordering::Acquire));
        assert!(!fleet.sessions[&b()].stop.load(Ordering::Acquire));
        let before = b_ticks.load(Ordering::Relaxed);
        wait_until(|| b_ticks.load(Ordering::Relaxed) > before + 2);
        assert!(ControllerLease::acquire(&fleet.leases, a(), a().virtual_address()).is_err());
        pad_release.store(true, Ordering::Release);
        wait_until(|| {
            fleet.reap();
            !fleet.sessions.contains_key(&a())
        });
        assert!(fleet
            .retiring
            .iter()
            .any(|retiring| retiring.address == a()));
        assert!(ControllerLease::acquire(&fleet.leases, a(), a().virtual_address()).is_err());
        let before = b_ticks.load(Ordering::Relaxed);
        wait_until(|| b_ticks.load(Ordering::Relaxed) > before + 2);
        ble_release.store(true, Ordering::Release);
        wait_until(|| {
            fleet.reap();
            fleet.retiring.is_empty()
        });
        let probe = ControllerLease::acquire(&fleet.leases, a(), a().virtual_address()).unwrap();
        drop(probe);
        fleet.reconcile(snapshot(vec![b_node, node(a(), "a-new")], true));
        let restarted = starts_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(restarted.0, a());
        assert_eq!(restarted.1, std::path::Path::new("/dev/a-new"));
        assert_eq!(restarted.2, initial_a.2);
        fleet.shutdown();
        assert!(fleet.sessions.is_empty());
    }
}
