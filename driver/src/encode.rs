//! Encoding canonical state into the virtual device's input report.
//!
//! The virtual descriptor declares 21 buttons, which do not align to byte
//! boundaries. Standard buttons occupy HID slots 1-15 and the six extras
//! occupy slots 16-21, so extra 0 (C) lands in the top bit of the second
//! button byte. Getting this mapping wrong is the kind of error that silently
//! reassigns controls, so the packing is done once here and covered by tests.

use crate::descriptor::{INPUT_PAYLOAD_LEN, INPUT_REPORT_ID, INPUT_REPORT_LEN};
use crate::input::{button, extra, State, BUTTON_COUNT};

/// Convert a physical axis byte into a centred 0-255 axis value.
///
/// The device reports sticks as **signed 8-bit** values, not as offsets from a
/// fixed centre, and not as absolute positions. The captured extremes pin this
/// down: a full-left hold reads `0x80` and a full-right hold reads `0x7f`,
/// which are only consistent as `-128` and `+127`. 
///
/// An earlier version of this function subtracted `0xfd` (an observed idle
/// value) instead. That was wrong: it placed `0x7f` to the *left* of `0x80`,
/// contradicting the captures. A test caught it.
///
/// Idle frames sit near `0xfd`/`0x01`, i.e. signed `-3`/`+1`, so an untouched
/// stick maps to about 125-129 rather than exactly 128. That small offset is a
/// property of the hardware's own centre calibration and is passed through
/// rather than hidden; a per-session recentre would need to observe startup
/// samples, which this stateless mapping deliberately does not do.
#[must_use]
pub fn centre_axis(raw: u8) -> u8 {
    let signed = i16::from(raw as i8);
    (128 + signed).clamp(0, 255) as u8
}

/// Pack canonical buttons into the three report bytes.
///
/// Byte 6 holds standard buttons 0-7, byte 7 holds standard 8-14 plus extra 0,
/// and byte 8 holds extras 1-5.
#[must_use]
pub fn pack_buttons(state: &State) -> [u8; 3] {
    let mut b = [0u8; 3];

    for idx in 0..8.min(BUTTON_COUNT) {
        if state.button(idx) {
            b[0] |= 1 << idx;
        }
    }
    for idx in 8..button::STANDARD_COUNT {
        if state.button(idx) {
            b[1] |= 1 << (idx - 8);
        }
    }

    // Standard buttons end at HID slot 15, so extra 0 starts at slot 16, which
    // is bit 7 of the second byte.
    if state.extra(extra::C) {
        b[1] |= 0x80;
    }
    for idx in 1..extra::COUNT {
        if state.extra(idx) {
            b[2] |= 1 << (idx - 1);
        }
    }

    b
}

/// Map canonical D-pad bits to a HID hat value.
///
/// The descriptor declares a null state, so `0` is used for neutral. Diagonals
/// are not representable in the physical stream (the device reports a single
/// direction at a time), but simultaneous physical outputs from Steam are
/// possible, so opposite directions cancel and adjacent ones become diagonals.
#[must_use]
pub fn hat_value(state: &State) -> u8 {
    let up = state.button(button::DPAD_UP);
    let down = state.button(button::DPAD_DOWN);
    let left = state.button(button::DPAD_LEFT);
    let right = state.button(button::DPAD_RIGHT);

    // Opposite directions cancel rather than latching.
    let vertical = match (up, down) {
        (true, false) => Some(0u8),
        (false, true) => Some(4),
        _ => None,
    };
    let horizontal = match (left, right) {
        (true, false) => Some(6u8),
        (false, true) => Some(2),
        _ => None,
    };

    match (vertical, horizontal) {
        (Some(v), None) => v,
        (None, Some(h)) => h,
        // 0=up 1=up-right 2=right 3=down-right 4=down 5=down-left 6=left 7=up-left
        (Some(0), Some(2)) => 1,
        (Some(4), Some(2)) => 3,
        (Some(4), Some(6)) => 5,
        (Some(0), Some(6)) => 7,
        _ => 0,
    }
}

/// Build the full input report, including the report ID.
#[must_use]
pub fn encode(state: &State) -> [u8; INPUT_REPORT_LEN] {
    let buttons = pack_buttons(state);
    let mut report = [0u8; INPUT_REPORT_LEN];
    report[0] = INPUT_REPORT_ID;
    report[1] = centre_axis(state.left_x);
    report[2] = centre_axis(state.left_y);
    report[3] = centre_axis(state.right_x);
    report[4] = centre_axis(state.right_y);
    report[5] = hat_value(state);
    report[6] = buttons[0];
    report[7] = buttons[1];
    report[8] = buttons[2];
    debug_assert_eq!(INPUT_PAYLOAD_LEN + 1, report.len());
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with_standard(idx: usize) -> State {
        State {
            buttons: 1 << idx,
            ..State::default()
        }
    }

    fn state_with_extra(idx: usize) -> State {
        State {
            extras: 1 << idx,
            ..State::default()
        }
    }

    #[test]
    fn full_left_and_right_match_the_captures() {
        // Captured: full-left plateau 0x80, full-right plateau 0x7f.
        assert_eq!(centre_axis(0x80), 0, "0x80 is full left");
        assert_eq!(centre_axis(0x7f), 255, "0x7f is full right");
    }

    #[test]
    fn idle_reads_near_centre() {
        // Idle values seen in captured frames: 0xfd and 0x01.
        let a = centre_axis(0xfd);
        let b = centre_axis(0x01);
        assert!((120..=136).contains(&a), "0xfd idle should be near centre, got {a}");
        assert!((120..=136).contains(&b), "0x01 idle should be near centre, got {b}");
    }

    #[test]
    fn axis_follows_signed_order_not_raw_byte_order() {
        // Raw byte order wraps at the sign boundary: 0x7f is +127 (full right)
        // and 0x80 is -128 (full left). Sweeping raw 0x00..0xff is therefore
        // *not* monotonic, and asserting that it were would be a wrong test.
        // What must hold is monotonicity in signed order: -128..=127.
        let mut prev = centre_axis(0x80); // -128, the minimum
        for v in -127..=127i8 {
            let cur = centre_axis(v as u8);
            assert!(cur >= prev, "axis went backwards at signed {v}");
            prev = cur;
        }
    }

    #[test]
    fn axis_endpoints_saturate() {
        assert_eq!(centre_axis(0x00), 128);
        assert_eq!(centre_axis(0x81), 1);
        assert_eq!(centre_axis(0xff), 127);
    }

    #[test]
    fn every_standard_button_lands_in_a_unique_bit() {
        for idx in 0..button::STANDARD_COUNT {
            let b = pack_buttons(&state_with_standard(idx));
            let total = u32::from(b[0]) | (u32::from(b[1]) << 8) | (u32::from(b[2]) << 16);
            assert_eq!(total.count_ones(), 1, "standard {idx} must set one bit");
            for other in 0..button::STANDARD_COUNT {
                if other != idx {
                    let o = pack_buttons(&state_with_standard(other));
                    let ot = u32::from(o[0]) | (u32::from(o[1]) << 8) | (u32::from(o[2]) << 16);
                    assert_ne!(total, ot, "standard {idx} aliases {other}");
                }
            }
        }
    }

    #[test]
    fn every_extra_lands_in_a_unique_bit() {
        for idx in 0..extra::COUNT {
            let b = pack_buttons(&state_with_extra(idx));
            let total = u32::from(b[0]) | (u32::from(b[1]) << 8) | (u32::from(b[2]) << 16);
            assert_eq!(total.count_ones(), 1, "extra {idx} must set one bit");
            for other in 0..extra::COUNT {
                if other != idx {
                    let o = pack_buttons(&state_with_extra(other));
                    let ot = u32::from(o[0]) | (u32::from(o[1]) << 8) | (u32::from(o[2]) << 16);
                    assert_ne!(total, ot, "extra {idx} aliases {other}");
                }
            }
        }
    }

    #[test]
    fn extras_never_alias_standard_buttons() {
        for idx in 0..extra::COUNT {
            let e = pack_buttons(&state_with_extra(idx));
            let et = u32::from(e[0]) | (u32::from(e[1]) << 8) | (u32::from(e[2]) << 16);
            for s in 0..button::STANDARD_COUNT {
                let sb = pack_buttons(&state_with_standard(s));
                let st = u32::from(sb[0]) | (u32::from(sb[1]) << 8) | (u32::from(sb[2]) << 16);
                assert_ne!(et, st, "extra {idx} collides with standard {s}");
            }
        }
    }

    #[test]
    fn all_six_extras_pack_simultaneously() {
        let state = State {
            extras: 0x3f,
            ..State::default()
        };
        let b = pack_buttons(&state);
        assert_eq!(b[1] & 0x80, 0x80, "C must occupy bit 7 of byte 7");
        assert_eq!(b[2] & 0x1f, 0x1f, "Z..M4 must fill the low five bits of byte 8");
    }

    #[test]
    fn all_buttons_pack_without_overflow() {
        let state = State {
            buttons: (1 << button::STANDARD_COUNT) - 1,
            extras: 0x3f,
            ..State::default()
        };
        let b = pack_buttons(&state);
        let total = u32::from(b[0]) | (u32::from(b[1]) << 8) | (u32::from(b[2]) << 16);
        // 15 standard + 6 extras = 21 bits set.
        assert_eq!(total.count_ones(), 21);
    }

    #[test]
    fn hat_covers_cardinals() {
        assert_eq!(hat_value(&state_with_standard(button::DPAD_UP)), 0);
        assert_eq!(hat_value(&state_with_standard(button::DPAD_RIGHT)), 2);
        assert_eq!(hat_value(&state_with_standard(button::DPAD_DOWN)), 4);
        assert_eq!(hat_value(&state_with_standard(button::DPAD_LEFT)), 6);
        assert_eq!(hat_value(&State::default()), 0);
    }

    #[test]
    fn hat_produces_diagonals() {
        let up_right = State {
            buttons: (1 << button::DPAD_UP) | (1 << button::DPAD_RIGHT),
            ..State::default()
        };
        assert_eq!(hat_value(&up_right), 1);

        let down_left = State {
            buttons: (1 << button::DPAD_DOWN) | (1 << button::DPAD_LEFT),
            ..State::default()
        };
        assert_eq!(hat_value(&down_left), 5);
    }

    #[test]
    fn opposite_directions_cancel() {
        let up_down = State {
            buttons: (1 << button::DPAD_UP) | (1 << button::DPAD_DOWN),
            ..State::default()
        };
        assert_eq!(hat_value(&up_down), 0);

        let left_right = State {
            buttons: (1 << button::DPAD_LEFT) | (1 << button::DPAD_RIGHT),
            ..State::default()
        };
        assert_eq!(hat_value(&left_right), 0);
    }

    #[test]
    fn report_id_is_present_once() {
        let r = encode(&State::default());
        assert_eq!(r[0], INPUT_REPORT_ID);
        assert_eq!(r.len(), INPUT_REPORT_LEN);
    }

    #[test]
    fn neutral_state_encodes_to_centred_rest() {
        // 0x00 is the signed zero the description calls neutral on the wire.
        let state = State::default();
        let r = encode(&state);
        assert_eq!(&r[1..5], &[128, 128, 128, 128]);
        assert_eq!(r[5], 0);
        assert_eq!(&r[6..9], &[0, 0, 0]);
    }
}
