//! Experimental DualSense Edge report encoding and feedback decoding.
//!
//! This module does not create a device or enable a physical motion stream.
//! Bluetooth field positions follow the documented Linux and SDL report layouts.

use std::time::Duration;

// Physical stick bytes are signed, while Edge stick bytes are unsigned.
pub(crate) fn centre_axis(raw: u8) -> u8 {
    (i16::from(raw as i8) + 128) as u8
}
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
/// mapped to Y and X by default when `swap_xy` is true. The third component is zero.
/// `scale` contains X and Y multipliers applied after selection, with signed
/// 16-bit saturation; a stale sample becomes zero.
/// None of these values is a calibrated physical gyro measurement.
#[must_use]
pub fn input_report(
    state: &State,
    sample: Option<MotionSample>,
    scale: [i32; 2],
    swap_xy: bool,
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
        let (x, y) = if swap_xy {
            (sample.raw.second, sample.raw.first)
        } else {
            (sample.raw.first, sample.raw.second)
        };
        for (offset, value, multiplier) in [(16, x, scale[0]), (18, y, scale[1])] {
            let value = (i64::from(value) * i64::from(multiplier))
                .clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16;
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

/// CRC-32/ISO-HDLC of a report preceded by its Bluetooth domain byte.
/// The reflected polynomial and initialization are specified by ISO-HDLC;
/// the Bluetooth domains and their coverage are documented by Linux
/// hid-playstation's report protocol.
#[must_use]
pub fn bluetooth_crc32(domain: u8, data: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in std::iter::once(&domain).chain(data.iter()) {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

/// Write a Bluetooth CRC in the final four bytes. Returns false for short data.
pub fn bluetooth_append_crc(domain: u8, report: &mut [u8]) -> bool {
    let Some(end) = report.len().checked_sub(4) else {
        return false;
    };
    let checksum = bluetooth_crc32(domain, &report[..end]);
    report[end..].copy_from_slice(&checksum.to_le_bytes());
    true
}

/// Check the final four bytes against the domain-prefixed report CRC.
#[must_use]
pub fn bluetooth_verify_crc(domain: u8, report: &[u8]) -> bool {
    let Some(end) = report.len().checked_sub(4) else {
        return false;
    };
    report[end..] == bluetooth_crc32(domain, &report[..end]).to_le_bytes()
}

/// Build a complete 78-byte Bluetooth input report from the common controls.
///
/// The zero Bluetooth header is provisional for full mode; it is not a claim
/// about authentic hardware sequencing. The common sequence is at byte 8.
#[must_use]
pub fn bluetooth_input_report(
    state: &State,
    sample: Option<MotionSample>,
    scale: [i32; 2],
    swap_xy: bool,
    stale_after: Duration,
    elapsed: Duration,
    sequence: u8,
) -> [u8; 78] {
    let common = input_report(
        state,
        sample,
        scale,
        swap_xy,
        stale_after,
        elapsed,
        sequence,
    );
    let mut report = [0u8; 78];
    report[0] = 0x31;
    report[2..65].copy_from_slice(&common[1..64]);
    bluetooth_append_crc(0xa1, &mut report);
    report
}

/// Decode CRC-valid Linux or SDL Bluetooth output, preserving the active latch
/// for unrelated, malformed, or unknown host traffic.
#[must_use]
pub fn bluetooth_feedback(report: &[u8], active: bool) -> Option<Feedback> {
    if report.len() != 78 || report[0] != 0x31 || !bluetooth_verify_crc(0xa2, report) {
        return None;
    }
    let base = if report[1] == 0x02 {
        2
    } else if report[1] & 0x0f == 0 && report[2] == 0x10 {
        3
    } else {
        return None;
    };
    let common = &report[base..base + 47];
    if common[0] & 1 != 0 || common[38] & 4 != 0 {
        let scale = |v: u8| ((u16::from(v) * 250 + 127) / 255) as u8;
        let left = scale(common[3]);
        let right = scale(common[2]);
        return if left == 0 && right == 0 {
            Some(Feedback::Stop)
        } else {
            Some(Feedback::Levels(left, right))
        };
    }
    if active && common.iter().all(|&b| b == 0) {
        return Some(Feedback::Stop);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_report_bluetooth_crc_independent_vectors() {
        for (domain, first, count, expected) in [
            (0xa1, &[0x31][..], 73, 0x1c9c519d),
            (0xa2, &[0x31, 0x00, 0x10][..], 71, 0x231501b5),
            (0xa3, &[0x05][..], 36, 0x2666b240),
            (0xa3, &[0x09][..], 15, 0xd3215df5),
            (0xa3, &[0x20][..], 59, 0x5d4cd5cf),
        ] {
            let mut data = first.to_vec();
            data.resize(data.len() + count, 0);
            assert_eq!(bluetooth_crc32(domain, &data), expected);
            data.extend_from_slice(&expected.to_le_bytes());
            assert!(bluetooth_verify_crc(domain, &data));
            data[0] ^= 1;
            assert!(!bluetooth_verify_crc(domain, &data));
        }
        assert!(!bluetooth_append_crc(0xa1, &mut [0; 3]));
        assert!(!bluetooth_verify_crc(0xa1, &[0; 3]));
    }

    #[test]
    fn edge_report_bluetooth_neutral_literal_vector() {
        // Calculated separately with Python zlib.crc32 over A1 plus bytes 0..74.
        let expected: [u8; 78] = [
            0x31, 0, 0x80, 0x80, 0x80, 0x80, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0x20, 0, 0, 0, 0, 0, 0x80, 0, 0, 0, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0x20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x57, 0xca,
            0xe6, 0x7a,
        ];
        assert_eq!(
            bluetooth_input_report(
                &State::default(),
                None,
                [1, 1],
                false,
                Duration::ZERO,
                Duration::ZERO,
                0
            ),
            expected
        );
    }

    #[test]
    fn edge_report_bluetooth_controls_extras_and_motion_offsets() {
        let encode = |state: &State, sample, swap, stale, sequence| {
            bluetooth_input_report(
                state,
                sample,
                [-2, 3],
                swap,
                stale,
                Duration::from_micros(7),
                sequence,
            )
        };
        for index in 0..crate::input::STANDARD_NAMES.len() {
            let state = State {
                buttons: 1 << index,
                ..State::default()
            };
            let bt = encode(&state, None, false, Duration::ZERO, 37);
            let common = input_report(
                &state,
                None,
                [-2, 3],
                false,
                Duration::ZERO,
                Duration::from_micros(7),
                37,
            );
            assert_eq!(&bt[2..65], &common[1..64]);
            assert!(bluetooth_verify_crc(0xa1, &bt));
        }
        for (index, bit) in [0x02, 0x04, 0x10, 0x20, 0x40, 0x80].into_iter().enumerate() {
            let state = State {
                extras: 1 << index,
                ..State::default()
            };
            assert_eq!(encode(&state, None, false, Duration::ZERO, 0)[11], bit);
        }
        let sample = MotionSample {
            raw: RawMotion {
                first: 11,
                second: -22,
            },
            age: Duration::from_millis(2),
        };
        let bt = encode(
            &State::default(),
            Some(sample),
            true,
            Duration::from_millis(2),
            0xff,
        );
        let value = |at| i16::from_le_bytes([bt[at], bt[at + 1]]);
        assert_eq!(
            (value(17), value(19), value(21), value(27)),
            (44, 33, 0, 8192)
        );
        assert_eq!(&bt[29..33], &21u32.to_le_bytes());
        assert_eq!(bt[8], 0xff);
        let stale = encode(
            &State::default(),
            Some(sample),
            false,
            Duration::from_millis(1),
            0,
        );
        assert_eq!(&stale[17..23], &[0; 6]);
        let nonswap = encode(
            &State::default(),
            Some(sample),
            false,
            Duration::from_millis(2),
            0,
        );
        assert_eq!(i16::from_le_bytes([nonswap[17], nonswap[18]]), -22);
        assert_eq!(i16::from_le_bytes([nonswap[19], nonswap[20]]), -66);
    }

    #[test]
    fn edge_report_bluetooth_feedback_layouts_and_latch_safety() {
        for (header, base) in [([0x31, 0x20, 0x10], 3), ([0x31, 0x02, 0x00], 2)] {
            let mut report = [0u8; 78];
            report[..3].copy_from_slice(&header);
            report[base] = 1;
            report[base + 2] = 255;
            report[base + 3] = 128;
            bluetooth_append_crc(0xa2, &mut report);
            assert_eq!(
                bluetooth_feedback(&report, false),
                Some(Feedback::Levels(125, 250))
            );
            let mut tampered = report;
            tampered[base + 3] ^= 1;
            assert_eq!(bluetooth_feedback(&tampered, true), None);
            assert_eq!(bluetooth_feedback(&report[..77], true), None);
            let mut wrong_id = report;
            wrong_id[0] = 0x30;
            bluetooth_append_crc(0xa2, &mut wrong_id);
            assert_eq!(bluetooth_feedback(&wrong_id, true), None);
            let mut unknown = report;
            unknown[1] = 3;
            bluetooth_append_crc(0xa2, &mut unknown);
            assert_eq!(bluetooth_feedback(&unknown, true), None);
            report[base] = 0;
            report[base + 38] = 4;
            bluetooth_append_crc(0xa2, &mut report);
            assert_eq!(
                bluetooth_feedback(&report, false),
                Some(Feedback::Levels(125, 250))
            );
            report[base + 2] = 0;
            report[base + 3] = 0;
            bluetooth_append_crc(0xa2, &mut report);
            assert_eq!(bluetooth_feedback(&report, false), Some(Feedback::Stop));
            report[base + 38] = 0;
            bluetooth_append_crc(0xa2, &mut report);
            assert_eq!(bluetooth_feedback(&report, true), Some(Feedback::Stop));
            assert_eq!(bluetooth_feedback(&report, false), None);
            report[base + 40] = 1; // LED-only, not a stop.
            bluetooth_append_crc(0xa2, &mut report);
            assert_eq!(bluetooth_feedback(&report, true), None);
        }
        let mut tickle = [0u8; 78];
        tickle[0] = 0x31;
        tickle[1] = 2;
        assert_eq!(bluetooth_feedback(&tickle, true), None);
    }

    #[test]
    fn bare_fn_does_not_affect_encoded_report() {
        let encode = |state: &State| {
            input_report(
                state,
                None,
                [1, 1],
                false,
                Duration::ZERO,
                Duration::ZERO,
                0,
            )
        };
        for buttons in [
            0,
            1 << button::A,
            1 << button::HOME,
            (1 << button::A) | (1 << button::HOME),
            (1 << button::B) | (1 << button::LB) | (1 << button::DPAD_UP),
        ] {
            for extras in [0, 1, 2, 4, 8, 16, 32, 0x3f] {
                let released = State {
                    buttons,
                    extras,
                    ..State::default()
                };
                let held = State {
                    buttons: buttons | (1 << button::FN),
                    ..released
                };
                let before = encode(&released);
                let during = encode(&held);
                assert_eq!(during[8], before[8]);
                assert_eq!(during[9], before[9]);
                assert_eq!(during[10], before[10]);
                assert_eq!(encode(&released), before);
            }
        }
    }

    #[test]
    fn individual_controls_are_distinct_and_extras_release() {
        let neutral = input_report(
            &State::default(),
            None,
            [1, 1],
            false,
            Duration::ZERO,
            Duration::ZERO,
            0,
        );
        let mut states = std::collections::HashSet::new();
        for i in 0..button::FN {
            let state = State {
                buttons: 1 << i,
                ..State::default()
            };
            let r = input_report(
                &state,
                None,
                [1, 1],
                false,
                Duration::ZERO,
                Duration::ZERO,
                0,
            );
            assert_ne!(&r[8..11], &neutral[8..11]);
            assert!(states.insert([r[8], r[9], r[10]]));
        }
        for i in 0..extra::COUNT {
            let state = State {
                extras: 1 << i,
                ..State::default()
            };
            let r = input_report(
                &state,
                None,
                [1, 1],
                false,
                Duration::ZERO,
                Duration::ZERO,
                0,
            );
            assert!(states.insert([r[8], r[9], r[10]]));
        }
        let all = State {
            extras: 0x3f,
            ..State::default()
        };
        assert_eq!(
            input_report(&all, None, [1, 1], false, Duration::ZERO, Duration::ZERO, 0)[10],
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
            [100, 100],
            false,
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
            [1, 1],
            false,
            Duration::from_millis(9),
            Duration::ZERO,
            0,
        );
        assert_eq!(&stale[16..22], &[0; 6]);
    }

    #[test]
    fn motion_axis_swap_is_configurable() {
        let sample = Some(MotionSample {
            raw: RawMotion {
                first: 123,
                second: -456,
            },
            age: Duration::ZERO,
        });
        for (swap, x, y) in [(true, -456i16, 123i16), (false, 123, -456)] {
            let r = input_report(
                &State::default(),
                sample,
                [1, 1],
                swap,
                Duration::ZERO,
                Duration::ZERO,
                0,
            );
            assert_eq!(i16::from_le_bytes([r[16], r[17]]), x);
            assert_eq!(i16::from_le_bytes([r[18], r[19]]), y);
            assert_eq!(&r[20..22], &[0, 0]);
        }
        let extreme = Some(MotionSample {
            raw: RawMotion {
                first: 2047,
                second: -2048,
            },
            age: Duration::from_millis(1),
        });
        let swapped = input_report(
            &State::default(),
            extreme,
            [100, 100],
            true,
            Duration::from_millis(1),
            Duration::ZERO,
            0,
        );
        assert_eq!(i16::from_le_bytes([swapped[16], swapped[17]]), i16::MIN);
        assert_eq!(i16::from_le_bytes([swapped[18], swapped[19]]), i16::MAX);
        let stale = input_report(
            &State::default(),
            extreme,
            [100, 100],
            true,
            Duration::ZERO,
            Duration::ZERO,
            0,
        );
        assert_eq!(&stale[16..22], &[0; 6]);
    }

    #[test]
    fn configured_axis_inversions_apply_after_swap_and_before_saturation() {
        use crate::config::MotionConfig;
        let sample = Some(MotionSample {
            raw: RawMotion {
                first: 123,
                second: -456,
            },
            age: Duration::ZERO,
        });
        let mut cfg = MotionConfig::default();
        for (swap, invert_x, invert_y, x, y) in [
            (true, true, true, 456i16, -123i16),
            (true, false, true, -456, -123),
            (true, true, false, 456, 123),
            (false, true, true, -123, 456),
        ] {
            cfg.swap_xy = swap;
            cfg.invert_x = invert_x;
            cfg.invert_y = invert_y;
            let r = input_report(
                &State::default(),
                sample,
                cfg.axis_scales(),
                cfg.swap_xy,
                Duration::ZERO,
                Duration::ZERO,
                0,
            );
            assert_eq!(i16::from_le_bytes([r[16], r[17]]), x);
            assert_eq!(i16::from_le_bytes([r[18], r[19]]), y);
            assert_eq!(&r[20..22], &[0, 0]);
        }
        cfg.scale = i16::MIN;
        cfg.invert_x = true;
        cfg.invert_y = true;
        cfg.swap_xy = true;
        let r = input_report(
            &State::default(),
            sample,
            cfg.axis_scales(),
            true,
            Duration::ZERO,
            Duration::ZERO,
            0,
        );
        assert_eq!(i16::from_le_bytes([r[16], r[17]]), i16::MIN);
        assert_eq!(i16::from_le_bytes([r[18], r[19]]), i16::MAX);
        cfg.invert_x = false;
        let r = input_report(
            &State::default(),
            sample,
            cfg.axis_scales(),
            true,
            Duration::ZERO,
            Duration::ZERO,
            0,
        );
        assert_eq!(i16::from_le_bytes([r[16], r[17]]), i16::MAX);
        assert_eq!(i16::from_le_bytes([r[18], r[19]]), i16::MAX);
        let r = input_report(
            &State::default(),
            sample,
            [i32::MIN, i32::MAX],
            true,
            Duration::ZERO,
            Duration::ZERO,
            0,
        );
        assert_eq!(i16::from_le_bytes([r[16], r[17]]), i16::MAX);
        assert_eq!(i16::from_le_bytes([r[18], r[19]]), i16::MAX);
        let stale = Some(MotionSample {
            age: Duration::from_millis(1),
            raw: RawMotion {
                first: 123,
                second: -456,
            },
        });
        let r = input_report(
            &State::default(),
            stale,
            cfg.axis_scales(),
            true,
            Duration::ZERO,
            Duration::ZERO,
            0,
        );
        assert_eq!(&r[16..22], &[0; 6]);
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
                input_report(
                    &state,
                    None,
                    [1, 1],
                    false,
                    Duration::ZERO,
                    Duration::ZERO,
                    0
                )[8],
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
