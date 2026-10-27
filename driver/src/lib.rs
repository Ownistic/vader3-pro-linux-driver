//! Userspace translator: Flydigi Vader 3 Pro (Bluetooth) to a Steam-recognizable
//! virtual gamepad.
//!
//! Data path:
//!
//! ```text
//! physical hidraw reports
//!   -> input::parse        canonical State
//!   -> encode::encode      virtual input report
//!   -> pad::send           uinput evdev gamepad
//!
//! host feedback (uinput EV_FF + UI_FF_UPLOAD)
//!   -> pad::poll_host
//!   -> rumble::bt_frame    (Bluetooth)  4-byte NUS frame
//!   -> rumble::usb_frame   (USB)       05 0F vendor command
//! ```
//!
//! See `docs/ARCHITECTURE.md` for the decisions and `docs/PROTOCOL.md` for the
//! protocol evidence.

pub mod config;
pub mod descriptor;
pub mod device;
pub mod encode;
pub mod ff;
pub mod input;
pub mod pad;
pub mod rumble;
pub mod suppress;
pub mod uhid;

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::config::Config;
use crate::input::State;

/// Minimal logger.
///
/// A dependency-free logger keeps the binary small and avoids pulling a
/// logging framework into a daemon that writes a handful of lines per session.
pub struct Logger {
    level: Level,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
    Trace = 4,
}

impl Level {
    fn parse(text: &str) -> Self {
        match text.to_ascii_lowercase().as_str() {
            "error" => Self::Error,
            "warn" => Self::Warn,
            "debug" => Self::Debug,
            "trace" => Self::Trace,
            _ => Self::Info,
        }
    }
}

impl Logger {
    #[must_use]
    pub fn new(level: &str) -> Self {
        Self {
            level: Level::parse(level),
        }
    }

    /// Emit a message at `level` if enabled.
    ///
    /// Takes a pre-formatted string rather than a `format_args!` so call sites
    /// can use ordinary `format!` inline captures without macro plumbing.
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

/// Log at error level when enabled.
#[allow(unused_macros)]
macro_rules! log_error {
    ($l:expr, $($arg:tt)*) => {
        if $l.level() >= $crate::Level::Error {
            $l.log($crate::Level::Error, &format!($($arg)*));
        }
    };
}

/// Log at warn level when enabled.
macro_rules! log_warn {
    ($l:expr, $($arg:tt)*) => {
        if $l.level() >= $crate::Level::Warn {
            $l.log($crate::Level::Warn, &format!($($arg)*));
        }
    };
}

/// Log at info level when enabled.
macro_rules! log_info {
    ($l:expr, $($arg:tt)*) => {
        if $l.level() >= $crate::Level::Info {
            $l.log($crate::Level::Info, &format!($($arg)*));
        }
    };
}

/// Log at debug level when enabled.
macro_rules! log_debug {
    ($l:expr, $($arg:tt)*) => {
        if $l.level() >= $crate::Level::Debug {
            $l.log($crate::Level::Debug, &format!($($arg)*));
        }
    };
}

#[allow(unused_imports)]
pub(crate) use {log_debug, log_info, log_warn};

/// Where physical rumble goes.
enum RumbleSink {
    /// Bluetooth: 4-byte frames over the Nordic UART characteristic.
    Bluetooth(rumble::bluez::NusWriter),
    /// USB: `05 0F` hidraw writes to the vendor interface.
    Usb { path: std::path::PathBuf },
    /// Rumble disabled or unavailable.
    Disabled(&'static str),
}

/// The translator.
pub struct Translator {
    cfg: Config,
    log: Logger,
    physical: device::PhysicalDevice,
    virtual_pad: pad::VirtualPad,
    /// Exclusive grabs on the physical controller's evdev nodes, held for the
    /// process lifetime so the controller does not appear twice.
    _grabs: Vec<suppress::ExclusiveGrab>,
    rumble: RumbleSink,
    last_state: State,
    /// True while host feedback is active, so frames are re-sent before the
    /// firmware timeout expires.
    rumble_levels: Option<(u8, u8)>,
    last_rumble_send: Instant,
}

impl Translator {
    /// Attach to the physical controller and create the virtual pad.
    pub fn start(cfg: Config) -> Result<Self> {
        let log = Logger::new(&cfg.log_level);

        let candidates = device::discover_candidates();
        let path = candidates
            .first()
            .with_context(|| {
                format!(
                    "no hidraw device with {:04x}:{:04x}; is the controller connected?",
                    cfg.vendor_id, cfg.product_id
                )
            })?
            .clone();

        log_info!(
            log,
            "physical device: {} ({:04x}:{:04x}, bus {:#x}, interface {:?}, {})",
            path.path.display(),
            path.vendor,
            path.product,
            path.bus,
            path.interface,
            if path.descriptor_match {
                "matched by descriptor"
            } else {
                "matched by id"
            }
        );
        let sysfs = path.sysfs.clone();
        let path = path.path;
        let physical = device::PhysicalDevice::open(&path)?;
        log_info!(log, "transport: {}", physical.bus_label());

        // Suppress the physical node *before* the virtual pad exists, so there
        // is no window in which both appear. The grab is reversible on process
        // exit by design; see `suppress` for why this is not a driver unbind.
        let mut grabs = Vec::new();
        if cfg.suppress_physical {
            let nodes = suppress::evdev_nodes_for(&sysfs);
            if nodes.is_empty() {
                log_warn!(
                    log,
                    "no evdev nodes found for the controller; not suppressing"
                );
            }
            for node in nodes {
                match suppress::ExclusiveGrab::acquire(&node) {
                    Ok(grab) => {
                        log_info!(log, "suppressed physical node {}", grab.path().display());
                        grabs.push(grab);
                    }
                    Err(e) => {
                        log_warn!(log, "could not suppress {}: {e:#}", node.display());
                    }
                }
            }
        }

        // Rumble delivery is chosen by *attempting* the GATT path when an
        // address is configured, because the bus field is not trustworthy: a
        // 2.4 GHz dongle was observed presenting itself on bus 5 with a Cypress
        // vendor id. Falling back to a hidraw write covers USB and any case
        // where BlueZ has no matching characteristic.
        let hidraw_path = path.clone();
        let rumble = if !cfg.rumble {
            RumbleSink::Disabled("disabled by configuration")
        } else if let Some(addr) = cfg.bluetooth_address.as_deref() {
            match rumble::bluez::NusWriter::connect(addr) {
                Ok(writer) => {
                    log_info!(log, "rumble via Nordic UART characteristic");
                    RumbleSink::Bluetooth(writer)
                }
                Err(e) => {
                    log_warn!(
                        log,
                        "no Nordic UART characteristic for {addr}: {e:#}; falling back to hidraw"
                    );
                    RumbleSink::Usb { path: hidraw_path }
                }
            }
        } else {
            log_info!(log, "no bluetooth_address set; rumble via hidraw");
            RumbleSink::Usb { path: hidraw_path }
        };

        let virtual_pad = pad::VirtualPad::create(&cfg.virtual_name)?;
        log_info!(log, "virtual pad created: {}", cfg.virtual_name);

        Ok(Self {
            cfg,
            log,
            physical,
            virtual_pad,
            _grabs: grabs,
            rumble,
            last_state: State::default(),
            rumble_levels: None,
            last_rumble_send: Instant::now(),
        })
    }

    /// Run until `should_stop` returns true.
    pub fn run<F: Fn() -> bool>(&mut self, should_stop: F) -> Result<()> {
        let poll = Duration::from_millis(self.cfg.poll_ms.max(1));
        let mut connected = true;

        while !should_stop() {
            self.service_virtual_pad()?;

            // Block in the kernel until a report arrives, instead of sleeping for
            // a fixed interval. A fixed sleep adds its full duration to every
            // input: measured p95 and max latency were both 4.0 ms against a
            // 0.16 ms minimum, which is the sleep quantised and nothing else.
            // The timeout keeps the rumble service tick and the stop check
            // responsive when the controller is idle.
            self.physical.wait_readable(poll)?;

            match self.physical.read_state() {
                Ok(Some(state)) => {
                    if state != self.last_state {
                        self.last_state = state;
                        log_debug!(self.log, "state: {}", describe(&state));
                        self.virtual_pad.send(&state)?;
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    if connected {
                        log_warn!(self.log, "physical device error: {e:#}");
                        connected = false;
                        self.neutralise()?;
                    }
                }
            }

            self.service_rumble();
        }

        self.neutralise()?;
        Ok(())
    }

    /// Drive the host feedback loop.
    ///
    /// Also services the uinput force-feedback upload handshake. Skipping it
    /// would leave the kernel's `upload_effect()` blocked, so Steam could not
    /// create a rumble effect at all.
    fn service_virtual_pad(&mut self) -> Result<()> {
        if let Some((left, right)) = self.virtual_pad.poll_host()? {
            log_debug!(self.log, "host rumble {} / {}", left, right);
            self.set_rumble(left, right);
        }
        Ok(())
    }

    fn set_rumble(&mut self, left: u8, right: u8) {
        self.rumble_levels = if left == 0 && right == 0 {
            self.send_rumble(0, 0);
            None
        } else {
            self.send_rumble(left, right);
            Some((left, right))
        };
        self.last_rumble_send = Instant::now();
    }

    fn send_rumble(&mut self, left: u8, right: u8) {
        match &self.rumble {
            RumbleSink::Bluetooth(writer) => {
                let frame = if left == 0 && right == 0 {
                    rumble::bt_stop_frame()
                } else {
                    rumble::bt_frame(left, right)
                };
                if let Err(e) = writer.write(&frame) {
                    log_debug!(self.log, "bluetooth rumble write failed: {e:#}");
                }
            }
            RumbleSink::Usb { path } => {
                let frame = rumble::usb_frame(left, right);
                match std::fs::OpenOptions::new().write(true).open(path) {
                    Ok(mut f) => {
                        use std::io::Write;
                        if let Err(e) = f.write_all(&frame) {
                            log_debug!(self.log, "usb rumble write failed: {e:#}");
                        }
                    }
                    Err(e) => log_debug!(self.log, "usb rumble open failed: {e:#}"),
                }
            }
            RumbleSink::Disabled(reason) => {
                log_debug!(self.log, "rumble skipped: {reason}");
            }
        }
    }

    /// Keep vibration alive while the host holds a level.
    ///
    /// The firmware duration is a hard timeout with no latch, so a continuous
    /// effect must be re-sent. Skipping this would make Steam feedback stutter.
    fn service_rumble(&mut self) {
        let Some((left, right)) = self.rumble_levels else {
            return;
        };
        if self.last_rumble_send.elapsed() >= Duration::from_millis(rumble::BT_RESEND_MS) {
            self.send_rumble(left, right);
            self.last_rumble_send = Instant::now();
        }
    }

    /// Force the virtual pad to a neutral state.
    ///
    /// Called on physical disconnect and at shutdown so losing the controller
    /// never leaves a stick held or a button latched in Steam.
    fn neutralise(&mut self) -> Result<()> {
        self.set_rumble(0, 0);
        self.last_state = State::default();
        let _ = self.virtual_pad.send(&State::default());
        Ok(())
    }
}

/// Entry point used by `main`.
pub fn run(config_path: Option<&Path>) -> Result<()> {
    let cfg = Config::load_or_default(config_path)?;
    let mut translator = Translator::start(cfg)?;

    RUNNING.store(true, std::sync::atomic::Ordering::SeqCst);
    // SAFETY: the handler only stores to an atomic, which is async-signal-safe.
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

    // The loop runs `while !should_stop()`, so the closure must report
    // "stop" by returning true.
    translator.run(|| !RUNNING.load(std::sync::atomic::Ordering::SeqCst))
}

/// Render a state as pressed control names, for debugging and acceptance
/// checks. Only pressed controls are listed, so a change is readable at a glance.
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
