//! Background BlueZ transport for optional experimental motion and rumble.
//!
//! Motion values are uncalibrated; their physical meaning, axes and scale are
//! not established. A successful selector write is not an observed response.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use dbus::arg::{prop_cast, PropMap};
use dbus::blocking::stdintf::org_freedesktop_dbus::PropertiesPropertiesChanged;
use dbus::blocking::{Connection, Proxy};
use dbus::channel::MatchingReceiver;
use dbus::message::MatchRule;

use crate::edge_report::MotionSample;
use crate::motion::{self, RawMotion};
use crate::rumble;

const CALL_TIMEOUT: Duration = Duration::from_millis(500);
const POLL: Duration = Duration::from_millis(20);
const RETRY: Duration = Duration::from_millis(500);
const CHECK_LINK: Duration = Duration::from_secs(1);
const TX_UUID: &str = "6e400003-b5a3-f393-e0a9-e50e24dcca9e";
const RX_UUID: &str = rumble::bluez::NUS_RX_UUID;
const CHARACTERISTIC: &str = "org.bluez.GattCharacteristic1";
type Objects = HashMap<dbus::Path<'static>, HashMap<String, PropMap>>;

/// Snapshot of observed notifications and subscription state, not device guarantees.
#[derive(Clone, Copy, Debug, Default)]
pub struct Diagnostics {
    pub notifications: u64,
    pub motion_notifications: u64,
    pub selector_status: Option<u8>,
    pub subscribed: bool,
}

struct State {
    connected: bool,
    shutdown: bool,
    generation: u64,
    desired: (u8, u8),
    revision: u64,
    sample: Option<(u64, RawMotion, Instant)>,
    diagnostics: Diagnostics,
}

impl State {
    fn new() -> Self {
        Self {
            connected: true,
            shutdown: false,
            generation: 0,
            desired: (0, 0),
            revision: 0,
            sample: None,
            diagnostics: Diagnostics::default(),
        }
    }

    fn set_connected(&mut self, connected: bool) {
        if self.connected != connected {
            self.connected = connected;
            self.generation = self.generation.wrapping_add(1);
            self.desired = (0, 0);
            self.revision = self.revision.wrapping_add(1);
            self.sample = None;
            self.diagnostics.subscribed = false;
            self.diagnostics.selector_status = None;
        }
    }

    fn set_rumble(&mut self, left: u8, right: u8) {
        if self.connected && !self.shutdown {
            self.desired = (left, right);
            self.revision = self.revision.wrapping_add(1);
        }
    }

    fn current(&self, generation: u64) -> bool {
        self.connected && !self.shutdown && self.generation == generation
    }

    fn observe(&mut self, generation: u64, data: &[u8]) {
        if !self.current(generation) {
            return;
        }
        self.diagnostics.notifications = self.diagnostics.notifications.saturating_add(1);
        // Status format observed in docs/HANDOFF.md, UHID spike checkpoint.
        if data.len() == 20 && data.starts_with(&[0xac, 0xc0]) {
            self.diagnostics.selector_status = Some(data[10]);
        } else if let Some(raw) = motion::parse_notification(data) {
            self.diagnostics.motion_notifications =
                self.diagnostics.motion_notifications.saturating_add(1);
            self.sample = Some((generation, raw, Instant::now()));
        }
    }
}

/// A nonblocking handle to a single serialized BLE worker.
pub struct BleWorker {
    state: Arc<Mutex<State>>,
    thread: Option<JoinHandle<()>>,
}

impl BleWorker {
    /// Start with the physical link presumed connected; discovery verifies it.
    #[must_use]
    pub fn start(address: String, motion_enabled: bool, rumble_enabled: bool) -> Self {
        let state = Arc::new(Mutex::new(State::new()));
        let shared = Arc::clone(&state);
        let thread = thread::spawn(move || run(shared, address, motion_enabled, rumble_enabled));
        Self {
            state,
            thread: Some(thread),
        }
    }

    /// Invalidate samples and feedback whenever physical connectivity changes.
    pub fn set_connected(&self, connected: bool) {
        self.state.lock().unwrap().set_connected(connected);
    }

    /// Replace the desired levels; feedback while disconnected is discarded.
    pub fn set_rumble(&self, left: u8, right: u8) {
        self.state.lock().unwrap().set_rumble(left, right);
    }

    /// Return the latest current-generation raw sample with its current age.
    #[must_use]
    pub fn motion_sample(&self) -> Option<MotionSample> {
        let state = self.state.lock().unwrap();
        let (generation, raw, at) = state.sample?;
        state.current(generation).then(|| MotionSample {
            raw,
            age: at.elapsed(),
        })
    }

    #[must_use]
    pub fn diagnostics(&self) -> Diagnostics {
        self.state.lock().unwrap().diagnostics
    }
}

impl Drop for BleWorker {
    fn drop(&mut self) {
        {
            let mut state = self.state.lock().unwrap();
            state.shutdown = true;
            state.desired = (0, 0);
            state.sample = None;
        }
        // D-Bus connection establishment itself is not guaranteed to have a timeout.
        // Detach rather than let shutdown indefinitely block the caller.
        if let Some(handle) = self.thread.take() {
            let deadline = Instant::now() + Duration::from_millis(1200);
            while !handle.is_finished() && Instant::now() < deadline {
                thread::sleep(POLL);
            }
            if handle.is_finished() {
                let _ = handle.join();
            }
        }
    }
}

fn managed(conn: &Connection) -> Result<Objects, dbus::Error> {
    let (objects,) = Proxy::new("org.bluez", "/", CALL_TIMEOUT, conn).method_call(
        "org.freedesktop.DBus.ObjectManager",
        "GetManagedObjects",
        (),
    )?;
    Ok(objects)
}

fn device_ready(objects: &Objects, device: &dbus::Path<'static>, address: &str) -> bool {
    objects
        .get(device)
        .and_then(|i| i.get("org.bluez.Device1"))
        .is_some_and(|p| {
            prop_cast::<String>(p, "Address").is_some_and(|v| v.eq_ignore_ascii_case(address))
                && prop_cast::<bool>(p, "Connected") == Some(&true)
                && prop_cast::<bool>(p, "ServicesResolved") == Some(&true)
        })
}

struct Paths {
    device: dbus::Path<'static>,
    tx: dbus::Path<'static>,
    rx: dbus::Path<'static>,
}

fn discover(objects: &Objects, address: &str) -> Option<Paths> {
    let devices: Vec<_> = objects
        .keys()
        .filter(|p| device_ready(objects, p, address))
        .collect();
    if devices.len() != 1 {
        return None;
    }
    let device = devices[0];
    let services: Vec<_> = objects
        .iter()
        .filter_map(|(path, interfaces)| {
            let props = interfaces.get("org.bluez.GattService1")?;
            (prop_cast::<dbus::Path<'static>>(props, "Device") == Some(device)).then_some(path)
        })
        .collect();
    let mut tx = Vec::new();
    let mut rx = Vec::new();
    for (path, interfaces) in objects {
        let Some(props) = interfaces.get(CHARACTERISTIC) else {
            continue;
        };
        let Some(service) = prop_cast::<dbus::Path<'static>>(props, "Service") else {
            continue;
        };
        if !services.contains(&service) {
            continue;
        }
        match prop_cast::<String>(props, "UUID").map(String::as_str) {
            Some(TX_UUID) => tx.push(path.clone()),
            Some(RX_UUID) => rx.push(path.clone()),
            _ => {}
        }
    }
    if tx.len() != 1 || rx.len() != 1 {
        return None;
    }
    Some(Paths {
        device: device.clone(),
        tx: tx.remove(0),
        rx: rx.remove(0),
    })
}

fn write(conn: &Connection, path: &dbus::Path<'static>, bytes: &[u8]) -> Result<(), dbus::Error> {
    let options: PropMap = HashMap::new();
    Proxy::new("org.bluez", path.clone(), CALL_TIMEOUT, conn).method_call(
        CHARACTERISTIC,
        "WriteValue",
        (bytes, options),
    )
}

fn current(state: &Mutex<State>, generation: u64) -> bool {
    state.lock().unwrap().current(generation)
}

fn select_motion(motion_enabled: bool, subscribed: bool, current: bool) -> bool {
    motion_enabled && subscribed && current
}

fn run(state: Arc<Mutex<State>>, address: String, motion_enabled: bool, rumble_enabled: bool) {
    loop {
        let (shutdown, connected, generation) = {
            let s = state.lock().unwrap();
            (s.shutdown, s.connected, s.generation)
        };
        if shutdown {
            break;
        }
        if !connected || (!motion_enabled && !rumble_enabled) {
            thread::sleep(POLL);
            continue;
        }
        if let Ok(conn) = Connection::new_system() {
            if let Ok(objects) = managed(&conn) {
                if let Some(paths) = discover(&objects, &address) {
                    if current(&state, generation) {
                        session(
                            &conn,
                            &paths,
                            &address,
                            &state,
                            generation,
                            motion_enabled,
                            rumble_enabled,
                        );
                    }
                }
            }
        }
        // Interruptible backoff, including while waiting for an explicit reconnect.
        let until = Instant::now() + RETRY;
        while Instant::now() < until {
            let s = state.lock().unwrap();
            if s.shutdown {
                break;
            }
            drop(s);
            thread::sleep(POLL);
        }
    }
}

fn session(
    conn: &Connection,
    paths: &Paths,
    address: &str,
    state: &Arc<Mutex<State>>,
    generation: u64,
    motion_enabled: bool,
    rumble_enabled: bool,
) {
    let mut subscribed = false;
    let mut activation_failed = false;
    let mut token = None;
    if motion_enabled {
        let shared = Arc::clone(state);
        let mut rule =
            MatchRule::new_signal("org.freedesktop.DBus.Properties", "PropertiesChanged");
        rule.path = Some(paths.tx.clone());
        let match_text = rule.match_str();
        let added: Result<(), dbus::Error> = Proxy::new(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            CALL_TIMEOUT,
            conn,
        )
        .method_call("org.freedesktop.DBus", "AddMatch", (match_text,));
        if added.is_ok() {
            let t = conn.start_receive(
                rule,
                Box::new(move |message, _| {
                    if let Ok(signal) = message.read_all::<PropertiesPropertiesChanged>() {
                        if signal.interface_name == CHARACTERISTIC {
                            if let Some(value) =
                                prop_cast::<Vec<u8>>(&signal.changed_properties, "Value")
                            {
                                shared.lock().unwrap().observe(generation, value);
                            }
                        }
                    }
                    true
                }),
            );
            token = Some(t);
            if current(state, generation) {
                let result: Result<(), dbus::Error> =
                    Proxy::new("org.bluez", paths.tx.clone(), CALL_TIMEOUT, conn).method_call(
                        CHARACTERISTIC,
                        "StartNotify",
                        (),
                    );
                subscribed = result.is_ok();
                if select_motion(motion_enabled, subscribed, current(state, generation)) {
                    state.lock().unwrap().diagnostics.subscribed = true;
                    // AC 01 is the explicitly authorized selector from docs/HANDOFF.md.
                    if write(conn, &paths.rx, &[0xac, 0x01]).is_err() {
                        eprintln!("BLE worker: selector write failed; retrying discovery");
                        activation_failed = true;
                    }
                }
            }
        }
        if !subscribed || activation_failed || !current(state, generation) {
            cleanup(
                conn,
                paths,
                state,
                generation,
                rumble_enabled,
                subscribed,
                token,
            );
            return;
        }
    }
    eprintln!("BLE worker: service connected (motion={motion_enabled}, rumble={rumble_enabled})");
    let mut last_check = Instant::now();
    let mut last_write = Instant::now() - Duration::from_millis(rumble::BT_RESEND_MS);
    let mut sent_revision = None;
    loop {
        let (current, desired, revision) = {
            let s = state.lock().unwrap();
            (s.current(generation), s.desired, s.revision)
        };
        if !current {
            break;
        }
        if rumble_enabled
            && (sent_revision != Some(revision)
                || (desired != (0, 0)
                    && last_write.elapsed() >= Duration::from_millis(rumble::BT_RESEND_MS)))
        {
            // Recheck immediately before each blocking operation. Its completion is
            // never used to restore an obsolete desired value.
            let valid = {
                let s = state.lock().unwrap();
                s.current(generation) && s.revision == revision
            };
            if valid {
                let frame = if desired == (0, 0) {
                    rumble::bt_stop_frame()
                } else {
                    rumble::bt_frame(desired.0, desired.1)
                };
                if write(conn, &paths.rx, &frame).is_err() {
                    break;
                }
                sent_revision = Some(revision);
                last_write = Instant::now();
            }
        }
        if last_check.elapsed() >= CHECK_LINK {
            match managed(conn) {
                Ok(objects)
                    if device_ready(&objects, &paths.device, address)
                        && discover(&objects, address)
                            .is_some_and(|p| p.tx == paths.tx && p.rx == paths.rx) => {}
                _ => break,
            }
            last_check = Instant::now();
        }
        if conn.process(POLL).is_err() {
            break;
        }
    }
    {
        let mut s = state.lock().unwrap();
        if s.generation == generation {
            s.sample = None;
            s.desired = (0, 0);
            s.revision = s.revision.wrapping_add(1);
            s.diagnostics.subscribed = false;
        }
    }
    cleanup(
        conn,
        paths,
        state,
        generation,
        rumble_enabled,
        subscribed,
        token,
    );
}

fn cleanup(
    conn: &Connection,
    paths: &Paths,
    state: &Arc<Mutex<State>>,
    generation: u64,
    rumble_enabled: bool,
    subscribed: bool,
    token: Option<dbus::channel::Token>,
) {
    // A new connected generation owns its own output; never send old cleanup to it.
    let can_stop = {
        let s = state.lock().unwrap();
        s.generation == generation || !s.connected
    };
    if rumble_enabled && can_stop {
        let _ = write(conn, &paths.rx, &rumble::bt_stop_frame());
    }
    if subscribed {
        let _: Result<(), dbus::Error> =
            Proxy::new("org.bluez", paths.tx.clone(), CALL_TIMEOUT, conn).method_call(
                CHARACTERISTIC,
                "StopNotify",
                (),
            );
    }
    // Closing this private bus connection removes the bus-side match as well.
    if let Some(token) = token {
        let _ = conn.stop_receive(token);
    }
    let diagnostics = {
        let mut s = state.lock().unwrap();
        if s.generation == generation {
            // Retry discovery without requiring a new physical transition.
            s.generation = s.generation.wrapping_add(1);
            s.sample = None;
            s.desired = (0, 0);
            s.revision = s.revision.wrapping_add(1);
            s.diagnostics.subscribed = false;
        }
        s.diagnostics
    };
    eprintln!(
        "BLE worker: cleanup; notifications={}, motion={}, selector={:?}; selector not restored",
        diagnostics.notifications, diagnostics.motion_notifications, diagnostics.selector_status
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feedback_coalesces_and_stop_wins() {
        let mut s = State::new();
        s.set_rumble(1, 2);
        s.set_rumble(3, 4);
        s.set_rumble(0, 0);
        assert_eq!(s.desired, (0, 0));
        assert_eq!(s.revision, 3);
    }

    #[test]
    fn disconnect_discards_old_sample_and_feedback() {
        let mut s = State::new();
        let old = s.generation;
        let mut frame = [0; 20];
        frame[18] = 0xfe;
        s.observe(old, &frame);
        assert!(s.sample.is_some());
        s.set_rumble(7, 8);
        s.set_connected(false);
        s.observe(old, &frame);
        s.set_rumble(9, 9);
        assert!(s.sample.is_none());
        assert_eq!(s.desired, (0, 0));
        assert_eq!(s.diagnostics.notifications, 1);
        s.set_connected(true);
        s.observe(old, &frame);
        assert!(s.sample.is_none());
        assert_eq!(s.desired, (0, 0));
    }

    #[test]
    fn selector_requires_enabled_subscription_and_current_generation() {
        assert!(!select_motion(false, true, true));
        assert!(!select_motion(true, false, true));
        assert!(!select_motion(true, true, false));
        assert!(select_motion(true, true, true));
    }

    #[test]
    fn notifications_replace_one_sample_without_queueing() {
        let mut s = State::new();
        let mut frame = [0; 20];
        frame[18] = 0xfe;
        for i in 0..10_000 {
            frame[11] = i as u8;
            s.observe(0, &frame);
        }
        assert_eq!(s.diagnostics.motion_notifications, 10_000);
        assert_eq!(s.sample.unwrap().1.first, 15);
    }
}
