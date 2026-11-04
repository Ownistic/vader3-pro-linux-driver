//! Experimental DualSense Edge USB report encoding and feedback decoding.
//!
//! This module does not create a device or enable a physical motion stream.
//! Report field positions and flags follow the Linux hid-playstation driver and
//! the USB layout described in the UHID implementation handoff.

use std::time::Duration;

use crate::encode::centre_axis;
use crate::input::{button, extra, State};
use crate::motion::RawMotion;

/// A recently observed raw notification and its age at report construction.
#[derive(Clone, Copy, Debug)]
pub struct MotionSample {
    pub raw: RawMotion,
    pub age: Duration,
}

/// Encode the 64-byte numbered USB input report.
///
/// The first and second uncalibrated notification values are provisionally
/// mapped to X and Y. The third component is zero. `scale` is an integer
/// multiplier, with signed 16-bit saturation; a stale sample becomes zero.
/// None of these values is a calibrated physical gyro measurement.
#[must_use]
pub fn input_report(
    state: &State,
    sample: Option<MotionSample>,
    scale: i16,
    stale_after: Duration,
    elapsed: Duration,
    sequence: u8,
) -> [u8; 64] {
    let mut out = [0u8; 64];
    out[0] = 1;
    out[1] = centre_axis(state.left_x);
    out[2] = centre_axis(state.left_y);
    out[3] = centre_axis(state.right_x);
    out[4] = centre_axis(state.right_y);
    out[5] = state.left_trigger;
    out[6] = state.right_trigger;
    out[7] = sequence;

    let v = i8::from(state.button(button::DPAD_DOWN)) - i8::from(state.button(button::DPAD_UP));
    let h = i8::from(state.button(button::DPAD_RIGHT)) - i8::from(state.button(button::DPAD_LEFT));
    out[8] = match (v, h) {
        (-1, 0) => 0,
        (-1, 1) => 1,
        (0, 1) => 2,
        (1, 1) => 3,
        (1, 0) => 4,
        (1, -1) => 5,
        (0, -1) => 6,
        (-1, -1) => 7,
        _ => 8,
    };
    for (index, mask) in [
        (button::X, 0x10),
        (button::A, 0x20),
        (button::B, 0x40),
        (button::Y, 0x80),
    ] {
        if state.button(index) {
            out[8] |= mask;
        }
    }
    for (index, mask) in [
        (button::LB, 1),
        (button::RB, 2),
        (button::SELECT, 0x10),
        (button::START, 0x20),
        (button::L3, 0x40),
        (button::R3, 0x80),
    ] {
        if state.button(index) {
            out[9] |= mask;
        }
    }
    if state.left_trigger != 0 {
        out[9] |= 4;
    }
    if state.right_trigger != 0 {
        out[9] |= 8;
    }
    if state.button(button::HOME) {
        out[10] |= 1;
    }
    for (index, mask) in [
        (extra::C, 0x02),
        (extra::Z, 0x04),
        (extra::M1, 0x10),
        (extra::M2, 0x20),
        (extra::M3, 0x40),
        (extra::M4, 0x80),
    ] {
        if state.extra(index) {
            out[10] |= mask;
        }
    }

    if let Some(sample) = sample.filter(|sample| sample.age <= stale_after) {
        for (offset, value) in [(16, sample.raw.first), (18, sample.raw.second)] {
            let value = (i32::from(value) * i32::from(scale))
                .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
            out[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
    }
    // Fabricated stationary acceleration and inactive contacts; not measurements.
    out[26..28].copy_from_slice(&8192i16.to_le_bytes());
    // hid-playstation's virtual sensor clock uses three ticks per microsecond.
    let ticks = elapsed.as_micros().wrapping_mul(3) as u32;
    out[28..32].copy_from_slice(&ticks.to_le_bytes());
    out[33] = 0x80;
    out[37] = 0x80;
    out[53] = 0x20;
    out
}

/// A recognized feedback command; amplitudes are in the physical 0..=250 range.
#[derive(Debug, PartialEq, Eq)]
pub enum Feedback {
    Levels(u8, u8),
    Stop,
}

/// Decode one numbered USB output report. Unrelated traffic is ignored.
///
/// A packet with either rumble-valid flag carries levels, including zeros.
/// While active, a completely zeroed control payload also stops rumble. Other
/// packets without rumble-valid flags are not stop commands. `active` must
/// reflect the caller's current latch and must be cleared on disconnect/close.
#[must_use]
pub fn feedback(report: &[u8], active: bool) -> Option<Feedback> {
    if !matches!(report.len(), 48 | 63) || report[0] != 2 {
        return None;
    }
    let valid = report[1] & 1 != 0 || report[39] & 4 != 0;
    if valid {
        let scale = |v: u8| ((u16::from(v) * 250 + 127) / 255) as u8;
        let left = scale(report[4]);
        let right = scale(report[3]);
        return if left == 0 && right == 0 {
            Some(Feedback::Stop)
        } else {
            Some(Feedback::Levels(left, right))
        };
    }
    if active && report[1..].iter().all(|&b| b == 0) {
        return Some(Feedback::Stop);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn individual_controls_are_distinct_and_extras_release() {
        let neutral = input_report(
            &State::default(),
            None,
            1,
            Duration::ZERO,
            Duration::ZERO,
            0,
        );
        let mut states = std::collections::HashSet::new();
        for i in 0..button::STANDARD_COUNT {
            let state = State {
                buttons: 1 << i,
                ..State::default()
            };
            let r = input_report(&state, None, 1, Duration::ZERO, Duration::ZERO, 0);
            assert_ne!(&r[8..11], &neutral[8..11]);
            assert!(states.insert([r[8], r[9], r[10]]));
        }
        for i in 0..extra::COUNT {
            let state = State {
                extras: 1 << i,
                ..State::default()
            };
            let r = input_report(&state, None, 1, Duration::ZERO, Duration::ZERO, 0);
            assert!(states.insert([r[8], r[9], r[10]]));
        }
        let all = State {
            extras: 0x3f,
            ..State::default()
        };
        assert_eq!(
            input_report(&all, None, 1, Duration::ZERO, Duration::ZERO, 0)[10],
            0xf6
        );
        assert_eq!(neutral[10], 0);
        assert_eq!(states.len(), 21);
    }

    #[test]
    fn motion_saturates_expires_and_clock_wraps() {
        let sample = MotionSample {
            raw: RawMotion {
                first: 2047,
                second: -2048,
            },
            age: Duration::from_millis(10),
        };
        let r = input_report(
            &State::default(),
            Some(sample),
            100,
            Duration::from_millis(10),
            Duration::from_micros(1_431_655_766),
            0,
        );
        assert_eq!(i16::from_le_bytes([r[16], r[17]]), i16::MAX);
        assert_eq!(i16::from_le_bytes([r[18], r[19]]), i16::MIN);
        assert_eq!(&r[20..22], &[0, 0]);
        assert_eq!(u32::from_le_bytes(r[28..32].try_into().unwrap()), 2);
        let stale = input_report(
            &State::default(),
            Some(sample),
            1,
            Duration::from_millis(9),
            Duration::ZERO,
            0,
        );
        assert_eq!(&stale[16..22], &[0; 6]);
    }

    #[test]
    fn hat_directions_diagonals_and_opposites() {
        for (buttons, hat) in [
            (0, 8),
            (1 << button::DPAD_UP, 0),
            ((1 << button::DPAD_UP) | (1 << button::DPAD_RIGHT), 1),
            (1 << button::DPAD_RIGHT, 2),
            ((1 << button::DPAD_DOWN) | (1 << button::DPAD_RIGHT), 3),
            (1 << button::DPAD_DOWN, 4),
            ((1 << button::DPAD_DOWN) | (1 << button::DPAD_LEFT), 5),
            (1 << button::DPAD_LEFT, 6),
            ((1 << button::DPAD_UP) | (1 << button::DPAD_LEFT), 7),
            ((1 << button::DPAD_UP) | (1 << button::DPAD_DOWN), 8),
        ] {
            let state = State {
                buttons,
                ..State::default()
            };
            assert_eq!(
                input_report(&state, None, 1, Duration::ZERO, Duration::ZERO, 0)[8],
                hat
            );
        }
    }

    #[test]
    fn rumble_flags_zeroes_unrelated_and_malformed() {
        let mut r = [0u8; 48];
        r[0] = 2;
        assert_eq!(feedback(&r, false), None);
        assert_eq!(feedback(&r, true), Some(Feedback::Stop));
        r[1] = 1;
        r[3] = 255;
        r[4] = 128;
        assert_eq!(feedback(&r, false), Some(Feedback::Levels(125, 250)));
        r[1] = 0;
        r[39] = 4;
        assert_eq!(feedback(&r, false), Some(Feedback::Levels(125, 250)));
        r[3] = 0;
        r[4] = 0;
        assert_eq!(feedback(&r, true), Some(Feedback::Stop));
        r[39] = 0;
        r[2] = 4;
        assert_eq!(feedback(&r, true), None);
        assert_eq!(feedback(&r[..47], true), None);
        r[0] = 1;
        assert_eq!(feedback(&r, true), None);
    }
}
