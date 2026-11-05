//! One physical identity and one independently serviced virtual controller.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use super::*;
use crate::device::IdentifiedNode;
use crate::lease::ControllerLease;

/// Session outcome and the independently retiring BLE thread, if any.
/// Only join the thread once `is_finished()` is true.
pub struct SessionExit {
    pub result: Result<()>,
    pub retiring_worker: Option<JoinHandle<()>>,
}

#[derive(Default)]
struct SessionInput {
    state: State,
    valid: bool,
    nus_active: bool,
}

impl SessionInput {
    fn update(
        &mut self,
        hid: Option<State>,
        nus: Option<(State, Duration)>,
        stale: Duration,
    ) -> bool {
        let (state, _, valid) = choose_input(
            &mut self.nus_active,
            nus,
            stale,
            hid,
            self.state,
            self.valid,
        );
        let changed = self.state != state || self.valid != valid;
        self.state = state;
        self.valid = valid;
        changed
    }

    fn feedback(&self, rumble: bool, update: Option<(u8, u8)>) -> Option<(u8, u8)> {
        if feedback_allowed(true, rumble) {
            update
        } else {
            None
        }
    }
}

fn read_latest(mut read: impl FnMut() -> Result<Option<Vec<u8>>>) -> Result<Option<State>> {
    let mut latest = None;
    for _ in 0..MAX_PHYSICAL_REPORTS_PER_TICK {
        let Some(raw) = read()? else { break };
        if let Ok(state) = input::parse(&raw) {
            latest = Some(state);
        }
    }
    Ok(latest)
}

trait SessionIo {
    fn wait_readable(&mut self, timeout: Duration) -> Result<bool>;
    fn read(&mut self) -> Result<Option<Vec<u8>>>;
    fn input_sample(&self) -> Option<(State, Duration)>;
    fn motion_sample(&self) -> Option<crate::edge_report::MotionSample>;
    fn feedback(&mut self, left: u8, right: u8);
    fn color(&mut self, color: [u8; 3]);
    fn clear_color(&mut self);
}

struct PhysicalIo<'a> {
    physical: &'a mut PhysicalDevice,
    worker: Option<&'a BleWorker>,
}

impl SessionIo for PhysicalIo<'_> {
    fn wait_readable(&mut self, timeout: Duration) -> Result<bool> {
        self.physical.wait_readable_checked(timeout)
    }
    fn read(&mut self) -> Result<Option<Vec<u8>>> {
        self.physical.try_read()
    }
    fn input_sample(&self) -> Option<(State, Duration)> {
        self.worker.and_then(BleWorker::input_sample)
    }
    fn motion_sample(&self) -> Option<crate::edge_report::MotionSample> {
        self.worker.and_then(BleWorker::motion_sample)
    }
    fn feedback(&mut self, left: u8, right: u8) {
        if let Some(worker) = self.worker {
            worker.set_rumble(left, right);
        }
    }
    fn color(&mut self, color: [u8; 3]) {
        if let Some(worker) = self.worker {
            worker.set_color(color);
        }
    }
    fn clear_color(&mut self) {
        if let Some(worker) = self.worker {
            worker.clear_color();
        }
    }
}

fn session_loop(
    cfg: &Config,
    pad: &mut EdgePad,
    io: &mut impl SessionIo,
    stop: &AtomicBool,
) -> Result<()> {
    let mut input = SessionInput::default();
    let stale = Duration::from_millis(cfg.motion.stale_ms);
    let mut next_report = Instant::now();
    while !stop.load(Ordering::Acquire) {
        let host = pad.poll_host()?;
        if pad.take_color_cancellation() {
            io.clear_color();
        }
        if let Some(color) = pad.take_color().filter(|_| cfg.led) {
            io.color(color);
        }
        if let Some((left, right)) = input.feedback(cfg.rumble, host) {
            io.feedback(left, right);
        }
        if !cfg.rumble {
            pad.stop_feedback();
        }
        let sample = if cfg.motion.enabled {
            io.input_sample()
        } else {
            None
        };
        let changed = input.update(None, sample, stale);
        if !input.valid {
            pad.cancel_quick_access();
        }
        let now = Instant::now();
        if pad.started() && report_due(now, next_report, changed) {
            let motion = if cfg.motion.enabled {
                io.motion_sample()
            } else {
                None
            };
            pad.send(
                &input.state,
                input.valid,
                motion,
                cfg.motion.axis_scales(),
                cfg.motion.swap_xy,
                stale,
            )?;
            next_report = Instant::now() + REPORT_INTERVAL;
        } else if !pad.started() && now >= next_report {
            next_report = now + REPORT_INTERVAL;
        }
        let timeout = next_report
            .saturating_duration_since(Instant::now())
            .min(REPORT_INTERVAL);
        if !io.wait_readable(timeout)? {
            continue;
        }
        let latest = read_latest(|| io.read())?;
        if let Some(hid) = latest {
            let sample = if cfg.motion.enabled {
                io.input_sample()
            } else {
                None
            };
            let changed = input.update(Some(hid), sample, stale);
            if !input.valid {
                pad.cancel_quick_access();
            }
            if changed && pad.started() {
                let motion = if cfg.motion.enabled {
                    io.motion_sample()
                } else {
                    None
                };
                pad.send(
                    &input.state,
                    input.valid,
                    motion,
                    cfg.motion.axis_scales(),
                    cfg.motion.swap_xy,
                    stale,
                )?;
                next_report = Instant::now() + REPORT_INTERVAL;
            }
        }
    }
    Ok(())
}

fn prepare_session<P, G: Default>(
    open: impl FnOnce() -> Result<P>,
    grab: impl FnOnce(&P) -> Result<G>,
    create: impl FnOnce() -> Result<EdgePad>,
    warn: impl FnOnce(&anyhow::Error),
) -> Result<(P, G, EdgePad)> {
    let physical = open()?;
    let grabs = match grab(&physical) {
        Ok(grabs) => grabs,
        Err(error) => {
            warn(&error);
            G::default()
        }
    };
    let pad = create()?;
    Ok((physical, grabs, pad))
}

fn run_with_worker(
    worker: Result<Option<BleWorker>>,
    run: impl FnOnce(Option<&BleWorker>) -> Result<()>,
) -> (Option<BleWorker>, Result<()>) {
    match worker {
        Ok(worker) => {
            let result = run(worker.as_ref());
            (worker, result)
        }
        Err(error) => (None, Err(error)),
    }
}

// The session thread owns this entire sequence, including a potentially
// blocking final virtual-device close. Worker cleanup retains its own lease.
pub(super) fn retire_resources(
    worker: Option<BleWorker>,
    lease: Arc<ControllerLease>,
    close_virtual: impl FnOnce() -> Result<()>,
    close_physical: impl FnOnce(),
) -> (Result<()>, Option<JoinHandle<()>>) {
    if let Some(worker) = &worker {
        worker.request_shutdown();
    }
    let result = close_virtual();
    close_physical();
    let retiring = worker.and_then(BleWorker::retire);
    drop(lease);
    (result, retiring)
}

/// Run one pinned session on the caller's dedicated controller thread.
///
/// The caller validates shared configuration and obtains both leases first.
/// Runtime errors are returned in `SessionExit` so the BLE thread handle is
/// retained even when I/O fails. Setup failures occur before worker creation.
pub fn run_pinned_session(
    cfg: Config,
    node: IdentifiedNode,
    lease: Arc<ControllerLease>,
    stop: Arc<AtomicBool>,
) -> Result<SessionExit> {
    if lease.physical_address() != node.address
        || lease.virtual_address() != node.address.virtual_address()
    {
        anyhow::bail!("session lease identity does not match discovered controller");
    }
    let log = Logger::new(&cfg.log_level);
    let (mut physical, grabs, mut pad) = prepare_session(
        || {
            let physical = PhysicalDevice::open_identified(&node)?;
            log_info!(
                log,
                "[{}] verified hidraw {} ({})",
                node.address,
                node.node.path.display(),
                if node.node.descriptor_match {
                    "descriptor match"
                } else {
                    "ID match"
                }
            );
            Ok(physical)
        },
        |_| {
            let mut grabs = Vec::new();
            if cfg.suppress_physical {
                let nodes = suppress::evdev_candidates_for(&node)?;
                if nodes.is_empty() {
                    log_warn!(
                        log,
                        "[{}] no evdev nodes available for suppression",
                        node.address
                    );
                }
                for candidate in nodes {
                    match ExclusiveGrab::acquire_verified(&candidate) {
                        Ok(grab) => grabs.push(grab),
                        Err(error) => log_warn!(
                            log,
                            "[{}] suppression unavailable for {}: {error:#}",
                            node.address,
                            candidate.path.display()
                        ),
                    }
                }
            }
            Ok(grabs)
        },
        || EdgePad::create(&cfg.virtual_name, node.address),
        |error| {
            log_warn!(
                log,
                "[{}] evdev discovery unavailable: {error:#}",
                node.address
            )
        },
    )?;
    log_info!(
        log,
        "[{}] UHID create accepted; virtual identity {}",
        node.address,
        pad.virtual_address()
    );
    let started = if worker_needed(&cfg) {
        BleWorker::start_scoped_with_led(
            Arc::clone(&lease),
            cfg.motion.enabled,
            cfg.rumble,
            cfg.led,
        )
        .map(Some)
    } else {
        Ok(None)
    };
    let (worker, result) = run_with_worker(started, |worker| {
        session_loop(
            &cfg,
            &mut pad,
            &mut PhysicalIo {
                physical: &mut physical,
                worker,
            },
            &stop,
        )
    });
    let (destroyed, retiring_worker) = retire_resources(
        worker,
        lease,
        || {
            pad.stop_feedback();
            pad.cancel_quick_access();
            let result = pad.destroy().context("destroying session UHID device");
            drop(pad);
            result
        },
        || {
            drop(grabs);
            drop(physical);
        },
    );
    let result = match result {
        Err(error) => {
            if let Err(cleanup) = destroyed {
                log_warn!(log, "[{}] UHID cleanup failed: {cleanup:#}", node.address);
            }
            Err(error)
        }
        Ok(()) => destroyed,
    };
    Ok(SessionExit {
        result,
        retiring_worker,
    })
}

#[cfg(test)]
pub(super) mod test_support {
    use super::*;
    use crate::identity::MacAddress;
    use std::os::fd::{FromRawFd, IntoRawFd};
    use std::os::unix::net::UnixDatagram;
    use std::sync::{mpsc, Mutex};

    pub(crate) struct FakeIo {
        pub(crate) rx: mpsc::Receiver<Result<Vec<u8>>>,
        pub(crate) pending: Option<Result<Vec<u8>>>,
        pub(crate) feedback: mpsc::Sender<(u8, u8)>,
        pub(crate) colors: mpsc::Sender<[u8; 3]>,
        pub(crate) nus: Arc<Mutex<Option<(State, Duration)>>>,
        pub(crate) motion: Arc<Mutex<Option<crate::edge_report::MotionSample>>>,
        pub(crate) hup: Arc<AtomicBool>,
        pub(crate) worker_state: Option<crate::ble_worker::TestWorkerState>,
    }

    pub(crate) struct Fixture {
        pub(crate) host: UnixDatagram,
        pub(crate) virtual_identity: MacAddress,
        pub(crate) physical: mpsc::Sender<Result<Vec<u8>>>,
        pub(crate) feedback: mpsc::Receiver<(u8, u8)>,
        pub(crate) colors: mpsc::Receiver<[u8; 3]>,
        pub(crate) nus: Arc<Mutex<Option<(State, Duration)>>>,
        pub(crate) motion: Arc<Mutex<Option<crate::edge_report::MotionSample>>>,
        pub(crate) hup: Arc<AtomicBool>,
    }

    pub(crate) fn fixture(address: MacAddress) -> (Fixture, FakeIo, EdgePad) {
        let (device, host) = UnixDatagram::pair().unwrap();
        device.set_nonblocking(true).unwrap();
        host.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        // SAFETY: ownership moves from this socket to exactly one File.
        let file = unsafe { std::fs::File::from_raw_fd(device.into_raw_fd()) };
        let pad = EdgePad::from_test_file(file, address);
        let virtual_identity = pad.virtual_address();
        let (physical, rx) = mpsc::channel();
        let (feedback, received) = mpsc::channel();
        let (colors, received_colors) = mpsc::channel();
        let nus = Arc::new(Mutex::new(None));
        let motion = Arc::new(Mutex::new(None));
        let hup = Arc::new(AtomicBool::new(false));
        (
            Fixture {
                host,
                virtual_identity,
                physical,
                feedback: received,
                colors: received_colors,
                nus: Arc::clone(&nus),
                motion: Arc::clone(&motion),
                hup: Arc::clone(&hup),
            },
            FakeIo {
                rx,
                pending: None,
                feedback,
                colors,
                nus,
                motion,
                hup,
                worker_state: None,
            },
            pad,
        )
    }

    #[derive(Default)]
    pub(crate) struct SetupFaults {
        pub(crate) open: bool,
        pub(crate) grab: bool,
        pub(crate) grab_warning: Arc<AtomicBool>,
        pub(crate) before_close: Option<Box<dyn FnOnce() + Send>>,
        pub(crate) worker_spawn: bool,
    }

    impl SessionIo for FakeIo {
        fn wait_readable(&mut self, timeout: Duration) -> Result<bool> {
            if self.hup.load(Ordering::Acquire) {
                anyhow::bail!("injected physical HUP");
            }
            if self.pending.is_some() {
                return Ok(true);
            }
            match self.rx.recv_timeout(timeout) {
                Ok(value) => {
                    self.pending = Some(value);
                    Ok(true)
                }
                Err(mpsc::RecvTimeoutError::Timeout) => Ok(false),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    anyhow::bail!("fake physical source closed")
                }
            }
        }
        fn read(&mut self) -> Result<Option<Vec<u8>>> {
            match self.pending.take().or_else(|| self.rx.try_recv().ok()) {
                Some(result) => result.map(Some),
                None => Ok(None),
            }
        }
        fn input_sample(&self) -> Option<(State, Duration)> {
            *self.nus.lock().unwrap()
        }
        fn motion_sample(&self) -> Option<crate::edge_report::MotionSample> {
            *self.motion.lock().unwrap()
        }
        fn feedback(&mut self, left: u8, right: u8) {
            if let Some(worker) = &self.worker_state {
                worker.feedback(left, right);
            }
            let _ = self.feedback.send((left, right));
        }
        fn color(&mut self, color: [u8; 3]) {
            let _ = self.colors.send(color);
        }
        fn clear_color(&mut self) {}
    }

    pub(crate) fn run_fake(
        cfg: Config,
        lease: Arc<ControllerLease>,
        stop: Arc<AtomicBool>,
        io: FakeIo,
        create: impl FnOnce() -> Result<EdgePad>,
        faults: SetupFaults,
        worker: Option<BleWorker>,
    ) -> Result<SessionExit> {
        let (mut io, (), mut pad) = prepare_session(
            || {
                if faults.open {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "injected physical open failure",
                    )
                    .into())
                } else {
                    Ok(io)
                }
            },
            |_| {
                if faults.grab {
                    anyhow::bail!("injected grab failure")
                } else {
                    Ok(())
                }
            },
            create,
            |_| faults.grab_warning.store(true, Ordering::Release),
        )?;
        let started = if faults.worker_spawn {
            BleWorker::fail_spawn_scoped_test(Arc::clone(&lease)).map(Some)
        } else {
            Ok(worker)
        };
        let (worker, result) =
            run_with_worker(started, |_| session_loop(&cfg, &mut pad, &mut io, &stop));
        let (destroyed, retiring_worker) = retire_resources(
            worker,
            lease,
            || {
                pad.stop_feedback();
                pad.cancel_quick_access();
                if let Some(before_close) = faults.before_close {
                    before_close();
                }
                let result = pad.destroy();
                drop(pad);
                result
            },
            || drop(io),
        );
        Ok(SessionExit {
            result: result.and(destroyed),
            retiring_worker,
        })
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::edge_report;
    use crate::input::button;
    use crate::quick_access::QuickAccess;

    #[test]
    fn session_colors_are_per_controller_and_can_be_disabled() {
        use super::test_support::fixture;
        use crate::identity::MacAddress;
        use std::sync::mpsc::RecvTimeoutError;

        let a: MacAddress = "12:34:56:78:9A:BC".parse().unwrap();
        let b: MacAddress = "12:34:56:78:9A:BD".parse().unwrap();
        let (first, mut io_a, mut pad_a) = fixture(a);
        let (second, mut io_b, mut pad_b) = fixture(b);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_a = Arc::clone(&stop);
        let stop_b = Arc::clone(&stop);
        let cfg_a = Config {
            rumble: false,
            ..Config::default()
        };
        let mut cfg_b = cfg_a.clone();
        cfg_b.led = false;
        let thread_a =
            std::thread::spawn(move || session_loop(&cfg_a, &mut pad_a, &mut io_a, &stop_a));
        let thread_b =
            std::thread::spawn(move || session_loop(&cfg_b, &mut pad_b, &mut io_b, &stop_b));
        let mut event = vec![0u8; 4103];
        event[..4].copy_from_slice(&6u32.to_ne_bytes());
        event[4] = 0x31;
        event[6] = 0x10;
        event[8] = 4;
        event[51..54].copy_from_slice(&[3, 4, 5]);
        edge_report::bluetooth_append_crc(0xa2, &mut event[4..82]);
        event[4100..4102].copy_from_slice(&78u16.to_ne_bytes());
        event[4102] = 1;
        first.host.send(&event).unwrap();
        second.host.send(&event).unwrap();
        assert_eq!(
            first.colors.recv_timeout(Duration::from_secs(2)).unwrap(),
            [3, 4, 5]
        );
        assert_eq!(
            second.colors.recv_timeout(Duration::from_millis(100)),
            Err(RecvTimeoutError::Timeout)
        );
        stop.store(true, Ordering::Release);
        thread_a.join().unwrap().unwrap();
        thread_b.join().unwrap().unwrap();
    }

    #[test]
    fn session_shared_loop_uses_real_edge_hid_feedback_and_cleanup() {
        use super::test_support::{run_fake, FakeIo, SetupFaults};
        use crate::edge_report::bluetooth_append_crc;
        use crate::identity::MacAddress;
        use crate::lease::LeaseDirectory;
        use std::os::fd::{FromRawFd, IntoRawFd};
        use std::os::unix::fs::PermissionsExt;
        use std::os::unix::net::UnixDatagram;
        use std::sync::{mpsc, Mutex};

        let temp = tempfile::tempdir().unwrap();
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let directory = LeaseDirectory::open_at(temp.path()).unwrap();
        let address: MacAddress = "12:34:56:78:9A:BC".parse().unwrap();
        let lease = Arc::new(
            ControllerLease::acquire(&directory, address, address.virtual_address()).unwrap(),
        );
        let (device, host) = UnixDatagram::pair().unwrap();
        device.set_nonblocking(true).unwrap();
        host.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        // SAFETY: ownership transfers from the socket into File exactly once.
        let file = unsafe { std::fs::File::from_raw_fd(device.into_raw_fd()) };
        let pad = EdgePad::from_test_file(file, address);
        let (physical_tx, rx) = mpsc::channel();
        let (feedback, feedback_rx) = mpsc::channel();
        let (colors, color_rx) = mpsc::channel();
        let io = FakeIo {
            rx,
            pending: None,
            feedback,
            colors,
            nus: Arc::new(Mutex::new(None)),
            motion: Arc::new(Mutex::new(None)),
            hup: Arc::new(AtomicBool::new(false)),
            worker_state: None,
        };
        let nus = Arc::clone(&io.nus);
        let motion = Arc::clone(&io.motion);
        let stop = Arc::new(AtomicBool::new(false));
        let mut cfg = Config::default();
        cfg.motion.enabled = true;
        let worker = std::thread::spawn(move || {
            run_fake(
                cfg,
                lease,
                stop,
                io,
                || Ok(pad),
                SetupFaults::default(),
                None,
            )
            .unwrap()
        });
        host.send(&2u32.to_ne_bytes()).unwrap();
        host.send(&4u32.to_ne_bytes()).unwrap();
        let mut frame = vec![0; input::REPORT_LEN];
        frame[0] = input::REPORT_ID;
        frame[9] = 0x20;
        physical_tx.send(Ok(frame)).unwrap();
        let mut packet = [0u8; 128];
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for HID report"
            );
            let size = host.recv(&mut packet).unwrap();
            if size == 84 && packet[6] == 0x31 && packet[15] & 0x40 != 0 {
                assert!(edge_report::bluetooth_verify_crc(0xa1, &packet[6..84]));
                break;
            }
        }
        let mut event = vec![0u8; 4103];
        event[..4].copy_from_slice(&6u32.to_ne_bytes());
        event[4] = 0x31;
        event[6] = 0x10;
        event[7] = 1;
        event[9] = 255;
        event[10] = 128;
        event[8] = 4;
        event[51..54].copy_from_slice(&[12, 34, 56]);
        bluetooth_append_crc(0xa2, &mut event[4..82]);
        event[4100..4102].copy_from_slice(&78u16.to_ne_bytes());
        event[4102] = 1;
        host.send(&event).unwrap();
        assert_eq!(
            feedback_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            (125, 250)
        );
        assert_eq!(
            color_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            [12, 34, 56]
        );
        let mut notification = [0u8; 20];
        notification[..4].fill(0x80);
        notification[4] = 0x10;
        notification[18] = 0xfe;
        *nus.lock().unwrap() = Some((
            crate::nus_input::parse(&notification).unwrap(),
            Duration::ZERO,
        ));
        *motion.lock().unwrap() = Some(crate::edge_report::MotionSample {
            raw: crate::motion::RawMotion {
                first: 3,
                second: 5,
            },
            age: Duration::ZERO,
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for NUS report"
            );
            let size = host.recv(&mut packet).unwrap();
            if size == 84 && packet[15] & 0x20 != 0 {
                assert_eq!(i16::from_le_bytes([packet[23], packet[24]]), -5);
                assert_eq!(i16::from_le_bytes([packet[25], packet[26]]), -3);
                break;
            }
        }
        let mut sdl = vec![0u8; 4103];
        sdl[..4].copy_from_slice(&6u32.to_ne_bytes());
        sdl[4] = 0x31;
        sdl[5] = 2;
        sdl[6] = 1;
        sdl[8] = 128;
        sdl[9] = 255;
        bluetooth_append_crc(0xa2, &mut sdl[4..82]);
        sdl[4100..4102].copy_from_slice(&78u16.to_ne_bytes());
        sdl[4102] = 1;
        host.send(&sdl).unwrap();
        assert_eq!(
            feedback_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            (250, 125)
        );
        physical_tx
            .send(Err(anyhow::anyhow!("injected physical EOF")))
            .unwrap();
        let exit = worker.join().unwrap();
        assert!(format!("{:#}", exit.result.unwrap_err()).contains("injected physical EOF"));
        assert!(ControllerLease::acquire(&directory, address, address.virtual_address()).is_ok());
        let mut destroyed = false;
        for _ in 0..64 {
            let size = host.recv(&mut packet).unwrap();
            if size == 4 && u32::from_ne_bytes(packet[..4].try_into().unwrap()) == 1 {
                destroyed = true;
                break;
            }
        }
        assert!(destroyed);
    }

    #[derive(Default)]
    pub(crate) struct FakeSession {
        input: SessionInput,
        quick: QuickAccess,
        feedback: Vec<(u8, u8)>,
    }

    impl FakeSession {
        pub(crate) fn tick(
            &mut self,
            hid: Option<State>,
            nus: Option<(State, Duration)>,
            host: Option<(u8, u8)>,
            ms: u64,
        ) -> [u8; 78] {
            self.input.update(hid, nus, Duration::from_millis(100));
            if !self.input.valid {
                self.quick.cancel();
            }
            if let Some(value) = self.input.feedback(true, host) {
                self.feedback.push(value);
            }
            let elapsed = Duration::from_millis(ms);
            let state = self
                .quick
                .compose(&self.input.state, self.input.valid, true, elapsed);
            edge_report::bluetooth_input_report(
                &state,
                None,
                [1, 1],
                false,
                Duration::from_millis(100),
                elapsed,
                0,
            )
        }
    }

    #[test]
    fn session_two_independent_inputs_nus_fn_and_feedback() {
        let mut a = FakeSession::default();
        let mut b = FakeSession::default();
        let neutral = State::default();
        let down = State {
            buttons: 1 << button::FN,
            ..neutral
        };
        let b_pressed = State {
            buttons: 1 << button::B,
            ..neutral
        };
        a.tick(Some(neutral), None, None, 0);
        b.tick(Some(b_pressed), None, Some((2, 3)), 0);
        let a_lead = a.tick(Some(down), None, Some((8, 9)), 1);
        assert_eq!((a_lead[9] & 0x20, a_lead[11] & 1), (0, 1));
        let b_nus = b.tick(None, Some((b_pressed, Duration::ZERO)), None, 101);
        let chord = a.tick(None, None, None, 101);
        assert_eq!((chord[9] & 0x20, chord[11] & 1), (0x20, 1));
        assert_eq!(b_nus[9] & 0x40, 0x40);
        assert_eq!(a.feedback, [(8, 9)]);
        assert_eq!(b.feedback, [(2, 3)]);
        let stale = b.tick(
            Some(b_pressed),
            Some((b_pressed, Duration::from_millis(101))),
            None,
            102,
        );
        assert_eq!(stale[9] & 0x40, 0);
        let no_fallback = b.tick(Some(b_pressed), None, None, 103);
        assert_eq!(no_fallback[9] & 0x40, 0);
        assert!(!b.input.valid);
        assert!(edge_report::bluetooth_verify_crc(0xa1, &chord));
    }

    #[test]
    fn session_read_error_is_local_and_drain_is_bounded() {
        let mut a = FakeSession::default();
        let mut b = FakeSession::default();
        let mut raw = vec![0; input::REPORT_LEN];
        raw[0] = input::REPORT_ID;
        let mut count = 0;
        let hid = read_latest(|| {
            count += 1;
            Ok(Some(raw.clone()))
        })
        .unwrap();
        assert_eq!(count, MAX_PHYSICAL_REPORTS_PER_TICK);
        assert!(hid.is_some());
        a.tick(hid, None, Some((1, 2)), 0);
        assert!(read_latest(|| anyhow::bail!("simulated physical EOF")).is_err());
        let b_state = State {
            buttons: 1 << button::B,
            ..State::default()
        };
        let report = b.tick(Some(b_state), None, Some((3, 4)), 1);
        assert_eq!(report[9] & 0x40, 0x40);
        assert_eq!(b.feedback, [(3, 4)]);
        assert_eq!(a.feedback, [(1, 2)]);
    }
}
