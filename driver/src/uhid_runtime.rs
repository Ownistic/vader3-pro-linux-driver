//! Experimental event-driven Edge backend. The virtual device survives link loss.

use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::ble_worker::BleWorker;
use crate::config::{Backend, Config};
use crate::device::{self, PhysicalDevice};
use crate::edge::EdgePad;
use crate::input::{self, State};
use crate::suppress::{self, ExclusiveGrab};
use crate::{describe, log_debug, log_info, log_warn, Logger};

const REPORT_INTERVAL: Duration = Duration::from_millis(4);
const REATTACH_INTERVAL: Duration = Duration::from_secs(1);
const METRICS_INTERVAL: Duration = Duration::from_secs(5);

fn worker_needed(cfg: &Config) -> bool {
    cfg.pad.backend == Backend::Uhid && (cfg.motion.enabled || cfg.rumble)
}

fn feedback_allowed(connected: bool, rumble: bool) -> bool {
    connected && rumble
}

fn report_due(now: Instant, deadline: Instant, changed: bool) -> bool {
    changed || now >= deadline
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
    next_report: Instant,
    next_attach: Instant,
    next_metrics: Instant,
    counters: Counters,
}

impl EdgeRuntime {
    pub fn start(cfg: Config) -> Result<Self> {
        cfg.validate()?;
        anyhow::ensure!(
            cfg.pad.backend == Backend::Uhid,
            "Edge runtime requires uhid backend"
        );
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
                log_info!(self.log, "UHID counters (5s): physical_reports={} state_updates={} virtual_reports={} motion_notifications_total={} selector_status={:?} subscribed={} max_report_generation_us={}", self.counters.physical, self.counters.changes, self.counters.virtual_reports, diag.motion_notifications, diag.selector_status, diag.subscribed, self.counters.max_generation_us);
                self.counters = Counters::default();
                self.next_metrics = now + METRICS_INTERVAL;
            }
            if self.pad.started() && report_due(now, self.next_report, false) {
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
                Ok(Some(state)) if state != self.state => {
                    self.state = state;
                    self.counters.changes += 1;
                    if self.counters.debug_changes < 20 {
                        log_debug!(self.log, "state: {}", describe(&state));
                        self.counters.debug_changes += 1;
                    }
                    if self.pad.started() && report_due(Instant::now(), self.next_report, true) {
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

    fn send(&mut self) -> Result<()> {
        let start = Instant::now();
        let sample = if self.cfg.motion.enabled && self.physical.is_some() {
            self.worker.as_ref().and_then(BleWorker::motion_sample)
        } else {
            None
        };
        self.pad.send(
            &self.state,
            sample,
            self.cfg.motion.scale,
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
        self.state = State::default();
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
        self.state = State::default();
        if self.pad.started() {
            let _ = self.pad.send(
                &self.state,
                None,
                self.cfg.motion.scale,
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
        assert!(!worker_needed(&cfg));
        cfg.pad.backend = Backend::Uhid;
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
        let mut cfg = Config::default();
        cfg.pad.backend = Backend::Uhid;
        assert!(EdgeRuntime::start(cfg)
            .err()
            .unwrap()
            .to_string()
            .contains("bluetooth_address"));
    }
}
