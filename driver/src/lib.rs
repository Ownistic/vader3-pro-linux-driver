//! Bluetooth Vader 3 Pro to DualSense Edge UHID translator.

pub mod ble_worker;
pub mod config;
pub mod device;
pub mod edge;
pub mod edge_report;
pub mod identity;
pub mod input;
pub mod lease;
pub mod motion;
pub mod nus_input;
mod quick_access;
pub mod rumble;
pub mod suppress;
pub mod uhid_runtime;

use crate::input::State;
use anyhow::Result;
use std::path::Path;

pub struct Logger {
    level: Level,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl Logger {
    #[must_use]
    pub fn new(level: &str) -> Self {
        let level = match level.to_ascii_lowercase().as_str() {
            "error" => Level::Error,
            "warn" => Level::Warn,
            "debug" => Level::Debug,
            "trace" => Level::Trace,
            _ => Level::Info,
        };
        Self { level }
    }
    pub fn log(&self, level: Level, message: &str) {
        if level <= self.level {
            let tag = match level {
                Level::Error => "error",
                Level::Warn => "warn",
                Level::Info => "info",
                Level::Debug => "debug",
                Level::Trace => "trace",
            };
            eprintln!("[{tag}] {message}");
        }
    }
    #[must_use]
    pub fn level(&self) -> Level {
        self.level
    }
}

macro_rules! log_warn { ($l:expr, $($arg:tt)*) => { if $l.level() >= $crate::Level::Warn { $l.log($crate::Level::Warn, &format!($($arg)*)); } }; }
macro_rules! log_info { ($l:expr, $($arg:tt)*) => { if $l.level() >= $crate::Level::Info { $l.log($crate::Level::Info, &format!($($arg)*)); } }; }
pub(crate) use {log_info, log_warn};

pub fn run(config_path: Option<&Path>) -> Result<()> {
    let cfg = config::Config::load_or_default(config_path)?;
    RUNNING.store(true, std::sync::atomic::Ordering::SeqCst);
    // SAFETY: the handler only stores to an atomic.
    unsafe {
        libc::signal(
            libc::SIGINT,
            handle_signal as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGTERM,
            handle_signal as *const () as libc::sighandler_t,
        );
    }
    uhid_runtime::ControllerFleet::new(cfg)?
        .run(|| !RUNNING.load(std::sync::atomic::Ordering::SeqCst))
}

#[must_use]
pub fn describe(state: &State) -> String {
    let mut parts = Vec::new();
    for (idx, name) in input::STANDARD_NAMES.iter().enumerate() {
        if state.button(idx) {
            parts.push(*name);
        }
    }
    for (idx, name) in input::EXTRA_NAMES.iter().enumerate() {
        if state.extra(idx) {
            parts.push(*name);
        }
    }
    let sticks = format!(
        "lx={:#04x} ly={:#04x} rx={:#04x} ry={:#04x} lt={:#04x} rt={:#04x}",
        state.left_x,
        state.left_y,
        state.right_x,
        state.right_y,
        state.left_trigger,
        state.right_trigger
    );
    if parts.is_empty() {
        format!("none | {sticks}")
    } else {
        format!("{} | {sticks}", parts.join("+"))
    }
}

static RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
extern "C" fn handle_signal(_sig: libc::c_int) {
    RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
}
