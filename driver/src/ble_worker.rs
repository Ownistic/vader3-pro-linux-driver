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
use crate::{input, nus_input};

const CALL_TIMEOUT: Duration = Duration::from_millis(500);
const POLL: Duration = Duration::from_millis(20);
const RETRY: Duration = Duration::from_millis(500);
const CHECK_LINK: Duration = Duration::from_secs(1);
const PASSIVE_WAIT: Duration = Duration::from_millis(250);
const ACTIVATION_WAIT: Duration = Duration::from_secs(1);
const TX_UUID: &str = "6e400003-b5a3-f393-e0a9-e50e24dcca9e";
const RX_UUID: &str = rumble::bluez::NUS_RX_UUID;
const CHARACTERISTIC: &str = "org.bluez.GattCharacteristic1";
const BLUEZ: &str = "org.bluez";
const BUS: &str = "org.freedesktop.DBus";
const BUS_PATH: &str = "/org/freedesktop/DBus";
type Objects = HashMap<dbus::Path<'static>, HashMap<String, PropMap>>;

/// Current subscription's observed activation assessment, not a transport result.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ActivationState {
    #[default]
    Idle,
    Awaiting,
    ObservedData,
    ObservedSelector,
    Unverified,
}

/// Snapshot of observed notifications and subscription state, not device guarantees.
#[derive(Clone, Copy, Debug, Default)]
pub struct Diagnostics {
    pub notifications: u64,
    pub motion_notifications: u64,
    pub selector_write_failures: u64,
    pub selector_status: Option<u8>,
    pub subscribed: bool,
    pub activation: ActivationState,
}

struct State {
    connected: bool,
    shutdown: bool,
    generation: u64,
    desired: (u8, u8),
    revision: u64,
    sample: Option<(u64, RawMotion, input::State, Instant)>,
    diagnostics: Diagnostics,
    activation_write_attempted: bool,
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
            activation_write_attempted: false,
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
            self.diagnostics.activation = ActivationState::Idle;
            self.activation_write_attempted = false;
        }
    }

    fn set_rumble(&mut self, left: u8, right: u8) {
        if self.connected && !self.shutdown {
            self.desired = (left, right);
            self.revision = self.revision.wrapping_add(1);
        }
    }

    fn record_selector_write_failure(&mut self) {
        self.diagnostics.selector_write_failures =
            self.diagnostics.selector_write_failures.saturating_add(1);
    }

    fn current(&self, generation: u64) -> bool {
        self.connected && !self.shutdown && self.generation == generation
    }

    fn input_sample(&self) -> Option<(input::State, Duration)> {
        let (generation, _, input, at) = self.sample?;
        self.current(generation).then(|| (input, at.elapsed()))
    }

    fn begin_activation(&mut self) {
        self.sample = None;
        self.diagnostics.selector_status = None;
        self.diagnostics.activation = ActivationState::Awaiting;
        self.activation_write_attempted = false;
    }

    // Called after the passive window; this is the only write budget grant.
    fn activation_write_due(&mut self, enabled: bool, generation: u64) -> bool {
        if select_motion(
            enabled,
            self.diagnostics.subscribed,
            self.current(generation),
        ) && self.diagnostics.activation == ActivationState::Awaiting
            && !self.activation_write_attempted
        {
            self.activation_write_attempted = true;
            true
        } else {
            false
        }
    }

    fn finish_activation_wait(&mut self, generation: u64) {
        if self.current(generation) && self.diagnostics.activation == ActivationState::Awaiting {
            self.diagnostics.activation = ActivationState::Unverified;
        }
    }

    fn clear_session(&mut self, generation: u64) {
        if self.generation == generation {
            self.generation = self.generation.wrapping_add(1);
            self.sample = None;
            self.desired = (0, 0);
            self.revision = self.revision.wrapping_add(1);
            self.diagnostics.subscribed = false;
            self.diagnostics.selector_status = None;
            self.diagnostics.activation = ActivationState::Idle;
            self.activation_write_attempted = false;
        }
    }

    fn observe(&mut self, generation: u64, data: &[u8]) {
        if !self.current(generation) {
            return;
        }
        self.diagnostics.notifications = self.diagnostics.notifications.saturating_add(1);
        if let (Some(raw), Some(input)) = (motion::parse_notification(data), nus_input::parse(data))
        {
            self.diagnostics.motion_notifications =
                self.diagnostics.motion_notifications.saturating_add(1);
            self.sample = Some((generation, raw, input, Instant::now()));
            self.diagnostics.activation = ActivationState::ObservedData;
        } else if data.len() == 20 && data.starts_with(&[0xac, 0xc0]) {
            // Status format observed in docs/HANDOFF.md, UHID spike checkpoint.
            self.diagnostics.selector_status = Some(data[10]);
            if data[10] == 1 && self.activation_write_attempted {
                self.diagnostics.activation = ActivationState::ObservedSelector;
            }
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
        let (generation, raw, _, at) = state.sample?;
        state.current(generation).then(|| MotionSample {
            raw,
            age: at.elapsed(),
        })
    }

    /// Latest ordinary controls and their age, from the current connection generation.
    #[must_use]
    pub fn input_sample(&self) -> Option<(input::State, Duration)> {
        self.state.lock().unwrap().input_sample()
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

fn managed(conn: &Connection, owner: &str) -> Result<Objects, dbus::Error> {
    let (objects,) = Proxy::new(owner, "/", CALL_TIMEOUT, conn).method_call(
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

struct Binding {
    owner: String,
    paths: Paths,
}

// The bus assigns unique names. Never accept a well-known or absent sender.
fn authentic_sender(message: &dbus::Message, owner: &str) -> bool {
    owner.starts_with(':')
        && dbus::strings::BusName::new(owner).is_ok()
        && message.sender().as_deref() == Some(owner)
}

fn owner_change(message: &dbus::Message) -> bool {
    if message.msg_type() != dbus::MessageType::Signal
        || message.sender().as_deref() != Some(BUS)
        || message.path().as_deref() != Some(BUS_PATH)
        || message.interface().as_deref() != Some(BUS)
        || message.member().as_deref() != Some("NameOwnerChanged")
    {
        return false;
    }
    message
        .read3::<String, String, String>()
        .is_ok_and(|(name, _, _)| name == BLUEZ)
}

fn handle_owner_change(message: &dbus::Message, state: &Mutex<State>, generation: u64) {
    if owner_change(message) {
        state.lock().unwrap().clear_session(generation);
    }
}

fn handle_notification(
    message: &dbus::Message,
    owner: &str,
    path: &dbus::Path<'_>,
    state: &Mutex<State>,
    generation: u64,
) {
    if !authentic_sender(message, owner)
        || message.msg_type() != dbus::MessageType::Signal
        || message.path().as_ref() != Some(path)
        || message.interface().as_deref() != Some("org.freedesktop.DBus.Properties")
        || message.member().as_deref() != Some("PropertiesChanged")
    {
        return;
    }
    if let Ok(signal) = message.read_all::<PropertiesPropertiesChanged>() {
        if signal.interface_name == CHARACTERISTIC {
            if let Some(value) = prop_cast::<Vec<u8>>(&signal.changed_properties, "Value") {
                state.lock().unwrap().observe(generation, value);
            }
        }
    }
}

fn resolve_owner(conn: &Connection) -> Option<String> {
    let result: Result<(String,), dbus::Error> =
        Proxy::new(BUS, BUS_PATH, CALL_TIMEOUT, conn).method_call(BUS, "GetNameOwner", (BLUEZ,));
    let owner = result.ok()?.0;
    (owner.starts_with(':') && dbus::strings::BusName::new(&owner).is_ok()).then_some(owner)
}

// Register before resolving, so a change during setup is queued and invalidates
// the generation. The bus match filters broadcasts; the callback checks headers
// and arg0 again because directly addressed signals bypass broker match rules.
fn watch_owner(conn: &Connection, state: &Arc<Mutex<State>>, generation: u64) -> bool {
    let rule = MatchRule::new_signal(BUS, "NameOwnerChanged")
        .with_path(BUS_PATH)
        .with_strict_sender(BUS);
    let match_text = format!("{},arg0='org.bluez'", rule.match_str());
    if conn.add_match_no_cb(&match_text).is_err() {
        return false;
    }
    let shared = Arc::clone(state);
    conn.start_receive(
        rule,
        Box::new(move |message, _| {
            handle_owner_change(&message, &shared, generation);
            true
        }),
    );
    true
}

// Drain signals queued by blocking method calls before using a resolved owner.
// A saturated queue fails closed rather than running an unbounded drain.
fn drain_pending(conn: &Connection, state: &Mutex<State>, generation: u64) -> bool {
    for _ in 0..1024 {
        if !current(state, generation) {
            return false;
        }
        match conn.process(Duration::ZERO) {
            Ok(false) => return current(state, generation),
            Ok(true) => {}
            Err(_) => return false,
        }
    }
    false
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

fn write(conn: &Connection, binding: &Binding, bytes: &[u8]) -> Result<(), dbus::Error> {
    let options: PropMap = HashMap::new();
    Proxy::new(
        binding.owner.as_str(),
        binding.paths.rx.clone(),
        CALL_TIMEOUT,
        conn,
    )
    .method_call(CHARACTERISTIC, "WriteValue", (bytes, options))
}

fn current(state: &Mutex<State>, generation: u64) -> bool {
    state.lock().unwrap().current(generation)
}

fn select_motion(motion_enabled: bool, subscribed: bool, current: bool) -> bool {
    motion_enabled && subscribed && current
}

fn redact_address(message: &str, address: &str) -> String {
    let underscore_address = address.replace(':', "_");
    [address, underscore_address.as_str()]
        .into_iter()
        .filter(|candidate| !candidate.is_empty())
        .fold(message.to_string(), |message, candidate| {
            replace_ascii_case_insensitive(&message, candidate, "[redacted]")
        })
}

fn selector_error_details(error_name: &str, message: &str, address: &str) -> String {
    format!("{error_name}: {}", redact_address(message, address))
}

fn replace_ascii_case_insensitive(text: &str, needle: &str, replacement: &str) -> String {
    let lower_text = text.to_ascii_lowercase();
    let lower_needle = needle.to_ascii_lowercase();
    let mut result = String::with_capacity(text.len());
    let mut cursor = 0;
    while let Some(relative) = lower_text[cursor..].find(&lower_needle) {
        let start = cursor + relative;
        let end = start + needle.len();
        result.push_str(&text[cursor..start]);
        result.push_str(replacement);
        cursor = end;
    }
    result.push_str(&text[cursor..]);
    result
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
            if watch_owner(&conn, &state, generation) {
                if let Some(owner) = resolve_owner(&conn) {
                    if let Ok(objects) = managed(&conn, &owner) {
                        if let Some(paths) = discover(&objects, &address) {
                            let binding = Binding { owner, paths };
                            if resolve_owner(&conn).as_deref() == Some(&binding.owner)
                                && drain_pending(&conn, &state, generation)
                            {
                                session(
                                    &conn,
                                    &binding,
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
    binding: &Binding,
    address: &str,
    state: &Arc<Mutex<State>>,
    generation: u64,
    motion_enabled: bool,
    rumble_enabled: bool,
) {
    let paths = &binding.paths;
    let mut subscribed = false;
    let mut token = None;
    if motion_enabled {
        let shared = Arc::clone(state);
        let mut rule =
            MatchRule::new_signal("org.freedesktop.DBus.Properties", "PropertiesChanged")
                .with_strict_sender(binding.owner.clone());
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
            let owner = binding.owner.clone();
            let tx = paths.tx.clone();
            let t = conn.start_receive(
                rule,
                Box::new(move |message, _| {
                    handle_notification(&message, &owner, &tx, &shared, generation);
                    true
                }),
            );
            token = Some(t);
            if resolve_owner(conn).as_deref() == Some(&binding.owner)
                && drain_pending(conn, state, generation)
            {
                let result: Result<(), dbus::Error> =
                    Proxy::new(binding.owner.as_str(), paths.tx.clone(), CALL_TIMEOUT, conn)
                        .method_call(CHARACTERISTIC, "StartNotify", ());
                subscribed = result.is_ok();
                if select_motion(motion_enabled, subscribed, current(state, generation)) {
                    let mut s = state.lock().unwrap();
                    if s.current(generation) {
                        s.diagnostics.subscribed = true;
                        s.begin_activation();
                    }
                }
            }
        }
        if !subscribed || !drain_pending(conn, state, generation) {
            cleanup(
                conn,
                binding,
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
    let mut activation_deadline = motion_enabled.then(|| Instant::now() + PASSIVE_WAIT);
    loop {
        if !drain_pending(conn, state, generation) {
            break;
        }
        let (current, desired, revision) = {
            let s = state.lock().unwrap();
            (s.current(generation), s.desired, s.revision)
        };
        if !current {
            break;
        }
        if activation_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            let write_due = state
                .lock()
                .unwrap()
                .activation_write_due(motion_enabled, generation);
            if write_due {
                // A method result is transport evidence, not proof of device activation.
                if let Err(error) = write(conn, binding, &[0xac, 0x01]) {
                    state.lock().unwrap().record_selector_write_failure();
                    let details = selector_error_details(
                        error.name().unwrap_or("unknown D-Bus error"),
                        error.message().unwrap_or("no error message"),
                        address,
                    );
                    eprintln!(
                        "BLE worker: selector method error ({details}); awaiting observations"
                    );
                } else {
                    eprintln!("BLE worker: selector write accepted; activation not yet observed");
                }
                activation_deadline = Some(Instant::now() + ACTIVATION_WAIT);
            } else {
                state.lock().unwrap().finish_activation_wait(generation);
                activation_deadline = None;
            }
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
                if write(conn, binding, &frame).is_err() {
                    break;
                }
                sent_revision = Some(revision);
                last_write = Instant::now();
            }
        }
        if last_check.elapsed() >= CHECK_LINK {
            match managed(conn, &binding.owner) {
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
    cleanup(
        conn,
        binding,
        state,
        generation,
        rumble_enabled,
        subscribed,
        token,
    );
}

fn cleanup(
    conn: &Connection,
    binding: &Binding,
    state: &Arc<Mutex<State>>,
    generation: u64,
    rumble_enabled: bool,
    subscribed: bool,
    token: Option<dbus::channel::Token>,
) {
    // A new connected generation owns its own output; never send old cleanup to it.
    let can_stop = {
        let mut s = state.lock().unwrap();
        let can_stop = s.generation == generation || !s.connected;
        s.clear_session(generation);
        can_stop
    };
    if rumble_enabled && can_stop {
        let _ = write(conn, binding, &rumble::bt_stop_frame());
    }
    if subscribed {
        let _: Result<(), dbus::Error> = Proxy::new(
            binding.owner.as_str(),
            binding.paths.tx.clone(),
            CALL_TIMEOUT,
            conn,
        )
        .method_call(CHARACTERISTIC, "StopNotify", ());
    }
    // Closing this private bus connection removes the bus-side match as well.
    if let Some(token) = token {
        let _ = conn.stop_receive(token);
    }
    let diagnostics = state.lock().unwrap().diagnostics;
    eprintln!(
        "BLE worker: cleanup; notifications={}, motion={}, selector={:?}; selector not restored",
        diagnostics.notifications, diagnostics.motion_notifications, diagnostics.selector_status
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notification(sender: Option<&str>, destination: Option<&str>) -> dbus::Message {
        let mut props = PropMap::new();
        props.insert(
            "Value".into(),
            dbus::arg::Variant(Box::new(data_frame().to_vec())),
        );
        let mut message = dbus::Message::new_signal(
            "/test/tx",
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
        )
        .unwrap()
        .append3(CHARACTERISTIC, props, Vec::<String>::new());
        message.set_sender(sender.map(|s| s.into()));
        message.set_destination(destination.map(|s| s.into()));
        message
    }

    fn ownership_signal(sender: Option<&str>, name: &str) -> dbus::Message {
        let mut message = dbus::Message::new_signal(BUS_PATH, BUS, "NameOwnerChanged")
            .unwrap()
            .append3(name, ":1.42", ":1.43");
        message.set_sender(sender.map(|s| s.into()));
        message
    }

    #[test]
    fn notification_requires_exact_unique_owner_even_for_unicast() {
        let path: dbus::Path<'static> = "/test/tx".into();
        let state = Mutex::new(ready_state());
        for (sender, destination) in [
            (Some(":1.99"), None),
            (Some(":1.99"), Some(":1.77")),
            (Some("org.bluez"), Some(":1.77")),
            (None, Some(":1.77")),
        ] {
            handle_notification(
                &notification(sender, destination),
                ":1.42",
                &path,
                &state,
                0,
            );
            assert!(state.lock().unwrap().sample.is_none());
        }
        let message = notification(Some(":1.42"), None);
        handle_notification(&message, "org.bluez", &path, &state, 0);
        assert!(state.lock().unwrap().sample.is_none());
        handle_notification(&message, "", &path, &state, 0);
        assert!(state.lock().unwrap().sample.is_none());
        handle_notification(&message, ":1.42", &"/other".into(), &state, 0);
        assert!(state.lock().unwrap().sample.is_none());
        handle_notification(&message, ":1.42", &path, &state, 0);
        assert!(state.lock().unwrap().input_sample().is_some());
    }

    #[test]
    fn authenticated_owner_change_invalidates_samples_and_generation() {
        let path: dbus::Path<'static> = "/test/tx".into();
        let state = Mutex::new(ready_state());
        let message = notification(Some(":1.42"), Some(":1.77"));
        handle_notification(&message, ":1.42", &path, &state, 0);
        {
            let mut s = state.lock().unwrap();
            assert!(s.sample.is_some());
            s.observe(0, &selector_frame());
            assert_eq!(s.diagnostics.selector_status, Some(1));
        }
        for event in [
            ownership_signal(Some(":1.99"), BLUEZ),
            ownership_signal(None, BLUEZ),
            ownership_signal(Some(BUS), "org.example.Other"),
        ] {
            handle_owner_change(&event, &state, 0);
            assert!(state.lock().unwrap().sample.is_some());
        }
        handle_owner_change(&ownership_signal(Some(BUS), BLUEZ), &state, 0);
        {
            let s = state.lock().unwrap();
            assert_eq!(s.generation, 1);
            assert!(s.input_sample().is_none());
            assert!(s.sample.is_none());
            assert_eq!(s.diagnostics.selector_status, None);
            assert!(!s.diagnostics.subscribed);
            assert_eq!(s.diagnostics.activation, ActivationState::Idle);
        }
        handle_notification(&message, ":1.42", &path, &state, 0);
        assert!(state.lock().unwrap().sample.is_none());
    }

    #[test]
    fn signal_rules_are_strict_and_device_properties_do_not_invalidate_owner() {
        let state = Mutex::new(ready_state());
        let path: dbus::Path<'static> = "/test/tx".into();
        let rule = MatchRule::new_signal("org.freedesktop.DBus.Properties", "PropertiesChanged")
            .with_path(path.clone())
            .with_strict_sender(":1.42");
        assert!(rule.matches(&notification(Some(":1.42"), None)));
        assert!(!rule.matches(&notification(Some("org.bluez"), None)));
        assert!(!rule.matches(&notification(Some(":1.99"), Some(":1.77"))));
        assert!(!rule.matches(&notification(None, None)));

        let watcher = MatchRule::new_signal(BUS, "NameOwnerChanged")
            .with_path(BUS_PATH)
            .with_strict_sender(BUS);
        assert!(watcher.matches(&ownership_signal(Some(BUS), BLUEZ)));
        assert!(!watcher.matches(&ownership_signal(Some(":1.99"), BLUEZ)));
        handle_owner_change(&notification(Some(":1.42"), None), &state, 0);
        assert_eq!(state.lock().unwrap().generation, 0);
    }

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
        s.observe(old, &selector_frame());
        assert_eq!(s.diagnostics.selector_status, Some(1));
        s.set_rumble(7, 8);
        s.set_connected(false);
        s.observe(old, &frame);
        s.set_rumble(9, 9);
        assert!(s.sample.is_none());
        assert_eq!(s.diagnostics.selector_status, None);
        assert_eq!(s.diagnostics.activation, ActivationState::Idle);
        assert_eq!(s.desired, (0, 0));
        assert_eq!(s.diagnostics.notifications, 2);
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

    fn ready_state() -> State {
        let mut s = State::new();
        s.diagnostics.subscribed = true;
        s.begin_activation();
        s
    }

    fn data_frame() -> [u8; 20] {
        let mut frame = [0; 20];
        frame[18] = 0xfe;
        frame
    }

    fn selector_frame() -> [u8; 20] {
        let mut frame = [0; 20];
        frame[..2].copy_from_slice(&[0xac, 0xc0]);
        frame[10] = 1;
        frame
    }

    #[test]
    fn passive_data_skips_write_and_disabled_never_writes() {
        let mut s = ready_state();
        assert!(!s.activation_write_due(false, 0));
        s.observe(0, &data_frame());
        assert_eq!(s.diagnostics.activation, ActivationState::ObservedData);
        assert!(!s.activation_write_due(true, 0));
    }

    #[test]
    fn empty_passive_window_grants_only_one_write_even_without_response() {
        let mut s = ready_state();
        assert!(s.activation_write_due(true, 0));
        assert!(!s.activation_write_due(true, 0));
        s.finish_activation_wait(0);
        assert_eq!(s.diagnostics.activation, ActivationState::Unverified);
        assert!(!s.activation_write_due(true, 0));
    }

    #[test]
    fn method_error_does_not_prevent_observed_selector_or_data() {
        for (frame, expected) in [
            (selector_frame(), ActivationState::ObservedSelector),
            (data_frame(), ActivationState::ObservedData),
        ] {
            let mut s = ready_state();
            assert!(s.activation_write_due(true, 0));
            s.record_selector_write_failure();
            s.observe(0, &frame);
            s.finish_activation_wait(0);
            assert_eq!(s.diagnostics.activation, expected);
            assert_eq!(s.diagnostics.selector_write_failures, 1);
            assert!(!s.activation_write_due(true, 0));
        }
    }

    #[test]
    fn stale_generation_and_passive_status_cannot_confirm_activation() {
        let mut s = ready_state();
        s.observe(0, &data_frame());
        s.observe(0, &selector_frame());
        assert_eq!(s.diagnostics.activation, ActivationState::ObservedData);
        s.clear_session(0);
        assert!(s.sample.is_none());
        assert_eq!(s.diagnostics.selector_status, None);
        assert_eq!(s.diagnostics.activation, ActivationState::Idle);
        s.diagnostics.subscribed = true;
        s.begin_activation();
        s.observe(1, &selector_frame());
        assert_eq!(s.diagnostics.activation, ActivationState::Awaiting);
        s.observe(0, &data_frame());
        s.observe(0, &selector_frame());
        assert!(s.sample.is_none());
        assert_eq!(s.diagnostics.activation, ActivationState::Awaiting);
        assert!(!s.activation_write_due(true, 0));
        assert!(s.activation_write_due(true, 1));
    }

    #[test]
    fn selector_write_failure_is_counted_and_address_forms_are_redacted() {
        let address = "Aa:Bb:Cc:Dd:Ee:Ff";
        let message = "NotAuthorized for AA:BB:CC:DD:EE:FF (aa_bb_cc_dd_ee_ff)";
        let details = selector_error_details("org.bluez.Error.NotAuthorized", message, address);
        assert_eq!(
            details,
            "org.bluez.Error.NotAuthorized: NotAuthorized for [redacted] ([redacted])"
        );
        let sanitized = redact_address(message, address);
        assert!(!sanitized.contains("AA:BB:CC:DD:EE:FF"));
        assert!(!sanitized.contains("aa_bb_cc_dd_ee_ff"));

        let mut state = State::new();
        state.record_selector_write_failure();
        assert_eq!(state.diagnostics.selector_write_failures, 1);
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

    #[test]
    fn input_samples_coalesce_and_are_invalidated_by_generation() {
        let mut s = State::new();
        let mut f = data_frame();
        f[4] = 0x10;
        s.observe(0, &f);
        assert!(s.input_sample().unwrap().0.button(input::button::A));
        f[4] = 0x20;
        s.observe(0, &f);
        let current = s.input_sample().unwrap().0;
        assert!(current.button(input::button::B));
        assert!(!current.button(input::button::A));
        s.clear_session(0);
        assert!(s.input_sample().is_none());
        s.observe(0, &f);
        assert!(s.input_sample().is_none());
        s.observe(1, &f);
        assert!(s.input_sample().is_some());
        s.set_connected(false);
        assert!(s.input_sample().is_none());
    }

    #[test]
    fn axis_prefix_is_data_and_selector_status_does_not_replace_it() {
        let mut s = ready_state();
        assert!(s.activation_write_due(true, 0));
        let mut f = data_frame();
        f[..2].copy_from_slice(&[0xac, 0xc0]);
        f[4] = 0x10;
        s.observe(0, &f);
        let sample = s.sample.unwrap();
        assert_eq!((sample.2.left_x, sample.2.left_y), (0x2c, 0x40));
        assert!(sample.2.button(input::button::A));
        assert_eq!(s.diagnostics.selector_status, None);
        assert_eq!(s.diagnostics.activation, ActivationState::ObservedData);

        for status in [selector_frame(), {
            let mut framed = selector_frame();
            framed[18] = 0xfe;
            framed
        }] {
            s.observe(0, &status);
            assert_eq!(s.diagnostics.selector_status, Some(1));
            assert_eq!(s.diagnostics.activation, ActivationState::ObservedSelector);
            assert_eq!(s.sample.unwrap(), sample);
        }
        assert_eq!(s.diagnostics.motion_notifications, 1);
        s.observe(0, &[0; 19]);
        s.observe(0, &[0; 20]);
        assert_eq!(s.sample.unwrap(), sample);
        assert_eq!(s.diagnostics.motion_notifications, 1);
    }
}
