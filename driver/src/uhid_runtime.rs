//! Identity-bound controller sessions and fleet supervision.

use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::ble_worker::BleWorker;
use crate::config::Config;
use crate::device::PhysicalDevice;
use crate::edge::EdgePad;
use crate::input::{self, State};
use crate::suppress::{self, ExclusiveGrab};
use crate::{log_info, log_warn, Logger};

mod session;
pub use session::{run_pinned_session, SessionExit};
mod fleet;
pub use fleet::ControllerFleet;

const REPORT_INTERVAL: Duration = Duration::from_millis(4);
// Limit each drain so a busy device cannot starve host events and report deadlines.
const MAX_PHYSICAL_REPORTS_PER_TICK: usize = 64;

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
        let mut cfg = Config::default();
        cfg.motion.stale_ms = 0;
        assert!(ControllerFleet::new(cfg)
            .err()
            .unwrap()
            .to_string()
            .contains("stale_ms"));
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
