//! Experimental event-driven Edge backend. The virtual device survives link loss.

use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::ble_worker::BleWorker;
use crate::config::Config;
use crate::device::{self, PhysicalDevice};
use crate::edge::EdgePad;
use crate::input::{self, State};
use crate::suppress::{self, ExclusiveGrab};
use crate::{describe, log_debug, log_info, log_warn, Logger};

const REPORT_INTERVAL: Duration = Duration::from_millis(4);
const REATTACH_INTERVAL: Duration = Duration::from_secs(1);
const METRICS_INTERVAL: Duration = Duration::from_secs(5);

fn worker_needed(cfg: &Config) -> bool {
    cfg.motion.enabled || cfg.rumble
}

fn feedback_allowed(connected: bool, rumble: bool) -> bool {
    connected && rumble
}

fn report_due(now: Instant, deadline: Instant, changed: bool) -> bool {
    changed || now >= deadline
}

// Once NUS has supplied input on this physical connection, never fall back to
// queued HID reports: session loss or stale data must neutralize controls.
fn choose_input(
    active: &mut bool,
    sample: Option<(State, Duration)>,
    stale: Duration,
    hid: Option<State>,
    previous: State,
    previous_valid: bool,
) -> (State, bool, bool) {
    if let Some((state, age)) = sample {
        *active = true;
        return (
            if age <= stale {
                state
            } else {
                State::default()
            },
            true,
            age <= stale,
        );
    }
    if *active {
        (State::default(), true, false)
    } else {
        (
            hid.unwrap_or(previous),
            false,
            hid.is_some() || previous_valid,
        )
    }
}

fn attach(cfg: &Config, log: &Logger) -> Result<(PhysicalDevice, Vec<ExclusiveGrab>)> {
    let node = device::discover_candidates()
        .into_iter()
        .next()
        .with_context(|| {
            format!(
                "no hidraw device with {:04x}:{:04x}; is the controller connected?",
                cfg.vendor_id, cfg.product_id
            )
        })?;
    let physical = PhysicalDevice::open(&node.path)?;
    log_info!(
        log,
        "physical device: {} ({:04x}:{:04x}, bus {:#x}, interface {:?}, {})",
        node.path.display(),
        node.vendor,
        node.product,
        node.bus,
        node.interface,
        if node.descriptor_match {
            "matched by descriptor"
        } else {
            "matched by id"
        }
    );
    log_info!(log, "transport: {}", physical.bus_label());
    let mut grabs = Vec::new();
    if cfg.suppress_physical {
        let nodes = suppress::evdev_nodes_for(&node.sysfs);
        if nodes.is_empty() {
            log_warn!(
                log,
                "no evdev nodes found for the controller; not suppressing"
            );
        }
        for path in nodes {
            match ExclusiveGrab::acquire(&path) {
                Ok(grab) => {
                    log_info!(log, "suppressed physical node {}", grab.path().display());
                    grabs.push(grab);
                }
                Err(e) => log_warn!(log, "could not suppress {}: {e:#}", path.display()),
            }
        }
    }
    Ok((physical, grabs))
}

#[derive(Default)]
struct Counters {
    physical: u64,
    changes: u64,
    nus_changes: u64,
    virtual_reports: u64,
    max_generation_us: u128,
    debug_changes: u64,
}

/// Owns all handles required to keep the virtual device and physical link alive.
pub struct EdgeRuntime {
    cfg: Config,
    log: Logger,
    pad: EdgePad,
    worker: Option<BleWorker>,
    physical: Option<PhysicalDevice>,
    grabs: Vec<ExclusiveGrab>,
    state: State,
    controls_valid: bool,
    nus_active: bool,
    next_report: Instant,
    next_attach: Instant,
    next_metrics: Instant,
    counters: Counters,
}

impl EdgeRuntime {
    pub fn start(cfg: Config) -> Result<Self> {
        cfg.validate()?;
        let log = Logger::new(&cfg.log_level);
        let (physical, grabs) = attach(&cfg, &log)?;
        let pad = EdgePad::create(&cfg.virtual_name)?;
        log_info!(log, "experimental UHID virtual pad created: {}", pad.name());
        let worker = worker_needed(&cfg).then(|| {
            BleWorker::start(
                cfg.bluetooth_address.clone().expect("validated address"),
                cfg.motion.enabled,
                cfg.rumble,
            )
        });
        let now = Instant::now();
        Ok(Self {
            cfg,
            log,
            pad,
            worker,
            physical: Some(physical),
            grabs,
            state: State::default(),
            controls_valid: false,
            nus_active: false,
            next_report: now,
            next_attach: now + REATTACH_INTERVAL,
            next_metrics: now + METRICS_INTERVAL,
            counters: Counters::default(),
        })
    }

    pub fn run<F: Fn() -> bool>(&mut self, should_stop: F) -> Result<()> {
        while !should_stop() {
            let feedback = self.pad.poll_host()?;
            if feedback_allowed(self.physical.is_some(), self.cfg.rumble) {
                if let (Some(worker), Some((left, right))) = (&self.worker, feedback) {
                    worker.set_rumble(left, right);
                }
            } else {
                self.pad.stop_feedback();
            }

            let now = Instant::now();
            if self.physical.is_none() && now >= self.next_attach {
                self.next_attach = now + REATTACH_INTERVAL;
                if let Ok((physical, grabs)) = attach(&self.cfg, &self.log) {
                    log_info!(
                        self.log,
                        "physical device reconnected: {}",
                        physical.path.display()
                    );
                    self.grabs = grabs;
                    self.physical = Some(physical);
                    if let Some(worker) = &self.worker {
                        worker.set_connected(true);
                    }
                }
            }
            if now >= self.next_metrics {
                let diag = self
                    .worker
                    .as_ref()
                    .map(BleWorker::diagnostics)
                    .unwrap_or_default();
                log_info!(self.log, "UHID counters (5s): physical_reports={} state_updates={} nus_state_updates={} virtual_reports={} motion_notifications_total={} selector_status={:?} subscribed={} max_report_generation_us={}", self.counters.physical, self.counters.changes, self.counters.nus_changes, self.counters.virtual_reports, diag.motion_notifications, diag.selector_status, diag.subscribed, self.counters.max_generation_us);
                self.counters = Counters::default();
                self.next_metrics = now + METRICS_INTERVAL;
            }
            let changed = self.update_input(None);
            if self.pad.started() && report_due(now, self.next_report, changed) {
                self.send()?;
            } else if !self.pad.started() && now >= self.next_report {
                self.next_report = now + REPORT_INTERVAL;
            }

            let timeout = self.next_report.saturating_duration_since(Instant::now());
            let Some(physical) = &mut self.physical else {
                std::thread::sleep(timeout);
                continue;
            };
            let outcome = (|| -> Result<Option<State>> {
                if !physical.wait_readable_checked(timeout)? {
                    return Ok(None);
                }
                let mut latest = None;
                while let Some(raw) = physical.try_read()? {
                    self.counters.physical += 1;
                    if let Ok(state) = input::parse(&raw) {
                        latest = Some(state);
                    }
                }
                Ok(latest)
            })();
            match outcome {
                Ok(Some(state)) => {
                    if self.update_input(Some(state)) && self.pad.started() {
                        self.send()?;
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    log_warn!(self.log, "physical device lost: {e:#}");
                    self.disconnect();
                    if self.pad.started() {
                        self.send()?;
                    }
                }
            }
        }
        Ok(())
    }

    fn update_input(&mut self, hid: Option<State>) -> bool {
        let sample = if self.cfg.motion.enabled && self.physical.is_some() {
            self.worker.as_ref().and_then(BleWorker::input_sample)
        } else {
            None
        };
        let (state, nus, valid) = choose_input(
            &mut self.nus_active,
            sample,
            Duration::from_millis(self.cfg.motion.stale_ms),
            hid,
            self.state,
            self.controls_valid,
        );
        let valid = valid && self.physical.is_some();
        let validity_changed = valid != self.controls_valid;
        self.controls_valid = valid;
        if !valid {
            self.pad.cancel_quick_access();
        }
        if state == self.state {
            return validity_changed;
        }
        self.state = state;
        self.counters.changes += 1;
        if nus {
            self.counters.nus_changes += 1;
        }
        if self.counters.debug_changes < 20 {
            log_debug!(
                self.log,
                "{} state: {}",
                if nus { "NUS" } else { "HID" },
                describe(&state)
            );
            self.counters.debug_changes += 1;
        }
        true
    }

    fn send(&mut self) -> Result<()> {
        let start = Instant::now();
        let sample = if self.cfg.motion.enabled && self.physical.is_some() {
            self.worker.as_ref().and_then(BleWorker::motion_sample)
        } else {
            None
        };
        self.pad.send(
            &self.state,
            self.controls_valid,
            sample,
            self.cfg.motion.axis_scales(),
            self.cfg.motion.swap_xy,
            Duration::from_millis(self.cfg.motion.stale_ms),
        )?;
        self.counters.virtual_reports += 1;
        self.counters.max_generation_us = self
            .counters
            .max_generation_us
            .max(start.elapsed().as_micros());
        self.next_report = Instant::now() + REPORT_INTERVAL;
        Ok(())
    }

    fn disconnect(&mut self) {
        if let Some(worker) = &self.worker {
            worker.set_connected(false);
        }
        self.pad.stop_feedback();
        self.pad.cancel_quick_access();
        self.state = State::default();
        self.controls_valid = false;
        self.nus_active = false;
        self.physical = None;
        self.grabs.clear();
        self.next_attach = Instant::now() + REATTACH_INTERVAL;
    }
}

impl Drop for EdgeRuntime {
    fn drop(&mut self) {
        if let Some(worker) = &self.worker {
            worker.set_rumble(0, 0);
        }
        self.pad.stop_feedback();
        self.pad.cancel_quick_access();
        self.state = State::default();
        self.controls_valid = false;
        if self.pad.started() {
            let _ = self.pad.send(
                &self.state,
                false,
                None,
                self.cfg.motion.axis_scales(),
                self.cfg.motion.swap_xy,
                Duration::from_millis(self.cfg.motion.stale_ms),
            );
        }
        self.grabs.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_plan_is_disabled_without_features() {
        let mut cfg = Config::default();
        assert!(worker_needed(&cfg));
        cfg.rumble = false;
        assert!(!worker_needed(&cfg));
        cfg.motion.enabled = true;
        assert!(worker_needed(&cfg));
    }

    #[test]
    fn changed_states_are_immediate_and_feedback_requires_link() {
        let now = Instant::now();
        let deadline = now + REPORT_INTERVAL;
        assert!(!report_due(now, deadline, false));
        assert!(report_due(now, deadline, true));
        assert!(report_due(deadline, deadline, false));
        assert!(!feedback_allowed(false, true));
        assert!(!feedback_allowed(true, false));
        assert!(feedback_allowed(true, true));
    }

    #[test]
    fn direct_constructor_rejects_invalid_configuration_before_device_access() {
        let cfg = Config::default();
        assert!(EdgeRuntime::start(cfg)
            .err()
            .unwrap()
            .to_string()
            .contains("bluetooth_address"));
    }

    #[test]
    fn nus_priority_stale_and_disconnect_reset() {
        let mut active = false;
        let hid = State {
            buttons: 1,
            ..State::default()
        };
        let nus = State {
            buttons: 2,
            left_x: 42,
            left_trigger: 5,
            ..State::default()
        };
        let limit = Duration::from_millis(100);
        assert_eq!(
            choose_input(&mut active, None, limit, Some(hid), State::default(), false).0,
            hid
        );
        assert_eq!(
            choose_input(&mut active, Some((nus, limit)), limit, Some(hid), hid, true).0,
            nus
        );
        assert_eq!(
            choose_input(
                &mut active,
                Some((nus, limit + Duration::from_nanos(1))),
                limit,
                Some(hid),
                nus,
                true,
            )
            .0,
            State::default()
        );
        assert_eq!(
            choose_input(&mut active, None, limit, Some(hid), nus, true).0,
            State::default()
        );
        active = false;
        assert_eq!(
            choose_input(&mut active, None, limit, Some(hid), State::default(), false).0,
            hid
        );
    }

    #[test]
    fn neutral_input_validity_changes_even_when_state_does_not() {
        let mut active = false;
        let neutral = State::default();
        let limit = Duration::from_millis(100);
        let fresh = choose_input(
            &mut active,
            Some((neutral, limit)),
            limit,
            None,
            neutral,
            false,
        );
        let stale = choose_input(
            &mut active,
            Some((neutral, limit + Duration::from_nanos(1))),
            limit,
            None,
            neutral,
            true,
        );
        let missing = choose_input(&mut active, None, limit, Some(neutral), neutral, false);
        assert_eq!(fresh, (neutral, true, true));
        assert_eq!(stale, (neutral, true, false));
        assert_eq!(missing, (neutral, true, false));
        active = false;
        assert_eq!(
            choose_input(&mut active, None, limit, None, neutral, true),
            (neutral, false, true)
        );
    }

    #[test]
    fn first_observed_fn_requires_release_on_initial_attach_and_reconnect() {
        use crate::input::button;
        use crate::quick_access::QuickAccess;

        let limit = Duration::from_millis(100);
        let mut sequence = QuickAccess::default();
        let mut active = false;
        let mut previous = State::default();
        let mut valid = false;
        let mut hid_frame = [0u8; input::REPORT_LEN];
        hid_frame[0] = input::REPORT_ID;
        hid_frame[11] = 0x80;
        let down = input::parse(&hid_frame).unwrap();
        hid_frame[11] = 0;
        let up = input::parse(&hid_frame).unwrap();
        assert_eq!(up, State::default());

        for connection in 0..2 {
            if connection != 0 {
                // Mirror disconnect's invalidation and sequencer cancellation.
                sequence.cancel();
                active = false;
                previous = State::default();
                valid = false;
            }
            let mut emit = |hid: Option<State>, ms: u64| {
                let (state, _, next_valid) =
                    choose_input(&mut active, None, limit, hid, previous, valid);
                let changed = state != previous || next_valid != valid;
                previous = state;
                valid = next_valid;
                let composed = sequence.compose(&state, valid, true, Duration::from_millis(ms));
                let report = crate::edge_report::input_report(
                    &composed,
                    None,
                    [1, 1],
                    false,
                    Duration::ZERO,
                    Duration::ZERO,
                    0,
                );
                (valid, changed, report[8] & 0x20, report[10] & 1)
            };
            assert_eq!(emit(None, 0), (false, false, 0, 0));
            assert_eq!(emit(Some(down), 1), (true, true, 0, 0));
            assert_eq!(emit(None, 101), (true, false, 0, 0));
            assert_eq!(emit(Some(up), 102), (true, true, 0, 0));
            assert_eq!(emit(Some(down), 103), (true, true, 0, 1));
            assert_eq!(emit(None, 202), (true, false, 0, 1));
            assert_eq!(emit(None, 203), (true, false, 0x20, 1));
            assert!(down.button(button::FN));
        }
    }

    #[test]
    fn real_neutral_report_changes_validity_and_arms_even_if_state_unchanged() {
        let mut active = false;
        let mut sequence = crate::quick_access::QuickAccess::default();
        let neutral = State::default();
        let limit = Duration::from_millis(100);
        let selected = choose_input(&mut active, None, limit, None, neutral, false);
        assert_eq!(selected, (neutral, false, false));
        let selected = choose_input(&mut active, None, limit, Some(neutral), neutral, false);
        assert_eq!(selected, (neutral, false, true));
        sequence.compose(&selected.0, selected.2, true, Duration::ZERO);
        let down = State {
            buttons: 1 << input::button::FN,
            ..neutral
        };
        let selected = choose_input(&mut active, None, limit, Some(down), neutral, true);
        let staged = sequence.compose(&selected.0, selected.2, true, Duration::from_millis(1));
        let wire = crate::edge_report::input_report(
            &staged,
            None,
            [1, 1],
            false,
            Duration::ZERO,
            Duration::ZERO,
            0,
        );
        assert_eq!((wire[8] & 0x20, wire[10] & 1), (0, 1));
    }

    #[test]
    fn first_fresh_nus_fn_is_not_an_arming_release() {
        let mut active = false;
        let mut sequence = crate::quick_access::QuickAccess::default();
        let neutral = State::default();
        let down = State {
            buttons: 1 << input::button::FN,
            ..neutral
        };
        let (selected, _, valid) = choose_input(
            &mut active,
            Some((down, Duration::ZERO)),
            Duration::from_millis(100),
            None,
            neutral,
            false,
        );
        assert!(valid);
        let staged = sequence.compose(&selected, valid, true, Duration::from_secs(1));
        let wire = crate::edge_report::input_report(
            &staged,
            None,
            [1, 1],
            false,
            Duration::ZERO,
            Duration::ZERO,
            0,
        );
        assert_eq!((wire[8] & 0x20, wire[10] & 1), (0, 0));
    }

    #[test]
    fn fn_shortcut_is_neutral_after_stale_or_disconnect() {
        let mut frame = [0u8; 20];
        frame[18] = 0xfe;
        frame[8] = 1;
        let pressed = crate::nus_input::parse(&frame).unwrap();
        let limit = Duration::from_millis(100);
        let mut active = false;
        let encode = |state: &State| {
            crate::edge_report::input_report(
                state,
                None,
                [1, 1],
                false,
                Duration::ZERO,
                Duration::ZERO,
                0,
            )
        };
        let fresh = choose_input(
            &mut active,
            Some((pressed, Duration::ZERO)),
            limit,
            None,
            State::default(),
            false,
        );
        assert!(fresh.2);
        assert_eq!((encode(&fresh.0)[8], encode(&fresh.0)[10]), (8, 0));
        for sample in [Some((pressed, limit + Duration::from_nanos(1))), None] {
            let (neutral, _, valid) =
                choose_input(&mut active, sample, limit, Some(pressed), pressed, true);
            assert!(!valid);
            assert_eq!(neutral, State::default());
            assert_eq!((encode(&neutral)[8], encode(&neutral)[10]), (8, 0));
        }
        // disconnect() resets both the retained state and NUS selection.
        active = false;
        let neutral = choose_input(&mut active, None, limit, None, State::default(), false).0;
        assert_eq!((encode(&neutral)[8], encode(&neutral)[10]), (8, 0));
    }
}
