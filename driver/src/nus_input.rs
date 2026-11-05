//! Ordinary controls in a NUS data notification.
//! Physical X and Y labels follow live input observations.
use crate::input::{button, State};
use crate::motion;

/// Decode a complete data notification; status and malformed frames are rejected.
#[must_use]
pub fn parse(data: &[u8]) -> Option<State> {
    motion::parse_notification(data)?;
    if data[10] != 0 {
        return None;
    }
    let mut buttons = 0;
    for (byte, mask, index) in [
        (4, 1, button::DPAD_UP),
        (4, 2, button::DPAD_RIGHT),
        (4, 4, button::DPAD_DOWN),
        (4, 8, button::DPAD_LEFT),
        (4, 16, button::A),
        (4, 32, button::B),
        (4, 64, button::SELECT),
        (4, 128, button::X),
        (5, 1, button::Y),
        (5, 2, button::START),
        (5, 4, button::LB),
        (5, 8, button::RB),
        (5, 64, button::L3),
        (5, 128, button::R3),
        (8, 8, button::HOME),
        (8, 1, button::FN),
    ] {
        if data[byte] & mask != 0 {
            buttons |= 1 << index;
        }
    }
    // Digital trigger bits have no separate canonical field; analog levels drive them.
    // A zero prefix may be calibration or a release; never retain old controls.
    Some(State {
        left_x: data[0].wrapping_sub(0x80),
        left_y: data[1].wrapping_sub(0x80),
        right_x: data[2].wrapping_sub(0x80),
        right_y: data[3].wrapping_sub(0x80),
        left_trigger: data[6],
        right_trigger: data[7],
        buttons,
        extras: data[9] & 0x3f,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame() -> [u8; 20] {
        let mut f = [0; 20];
        f[..4].fill(0x80);
        f[18] = 0xfe;
        f
    }
    #[test]
    fn individual_and_simultaneous_controls() {
        let mappings = [
            (4, 1, button::DPAD_UP),
            (4, 2, button::DPAD_RIGHT),
            (4, 4, button::DPAD_DOWN),
            (4, 8, button::DPAD_LEFT),
            (4, 16, button::A),
            (4, 32, button::B),
            (4, 64, button::SELECT),
            (4, 128, button::X),
            (5, 1, button::Y),
            (5, 2, button::START),
            (5, 4, button::LB),
            (5, 8, button::RB),
            (5, 64, button::L3),
            (5, 128, button::R3),
            (8, 8, button::HOME),
            (8, 1, button::FN),
        ];
        let mut all = frame();
        for (byte, mask, index) in mappings {
            let mut f = frame();
            f[byte] = mask;
            let s = parse(&f).unwrap();
            assert_eq!(s.buttons, 1 << index);
            assert_eq!(s.extras, 0);
            all[byte] |= mask;
        }
        for index in 0..6 {
            let mut f = frame();
            f[9] = 1 << index;
            let s = parse(&f).unwrap();
            assert_eq!(s.extras, 1 << index);
            assert_eq!(s.buttons, 0);
        }
        all[9] = 0x3f;
        let s = parse(&all).unwrap();
        assert_eq!(s.buttons.count_ones(), 16);
        assert_eq!(s.extras, 0x3f);
        for bits in [3, 6, 12, 9] {
            let mut f = frame();
            f[4] = bits;
            assert_eq!(parse(&f).unwrap().buttons.count_ones(), 2);
        }
    }

    #[test]
    fn physical_x_and_y_reach_square_and_triangle() {
        for (byte, mask, index, wire) in [(4, 0x80, button::X, 0x10), (5, 0x01, button::Y, 0x80)] {
            let mut f = frame();
            f[byte] = mask;
            let state = parse(&f).unwrap();
            assert_eq!(state.buttons, 1 << index);
            let report = crate::edge_report::input_report(
                &state,
                None,
                [1, 1],
                true,
                std::time::Duration::ZERO,
                std::time::Duration::ZERO,
                0,
            );
            assert_eq!(report[8], 8 | wire);
        }
    }

    #[test]
    fn fn_combines_with_each_extra_and_home_and_releases() {
        for mask in [1, 2, 4, 8, 16, 32, 0x3f] {
            let mut f = frame();
            f[8] = 0x09;
            f[9] = mask;
            let state = parse(&f).unwrap();
            assert_eq!(state.buttons, (1 << button::FN) | (1 << button::HOME));
            assert_eq!(state.extras, mask);
            f[8] = 0x08;
            let released = parse(&f).unwrap();
            assert_eq!(released.buttons, 1 << button::HOME);
            assert_eq!(released.extras, mask);
        }
        let mut f = frame();
        f[9] = 0x40;
        let state = parse(&f).unwrap();
        assert_eq!((state.buttons, state.extras), (0, 0));
    }
    #[test]
    fn axes_triggers_unknowns_and_validation() {
        let mut f = frame();
        f[..4].copy_from_slice(&[0, 0x80, 0xff, 0x7f]);
        f[6] = 1;
        f[7] = 255;
        f[5] = 0x30;
        f[8] = 0xf6;
        f[9] = 0xc0;
        f[14..18].fill(0xff);
        let s = parse(&f).unwrap();
        assert_eq!(
            (s.left_x, s.left_y, s.right_x, s.right_y),
            (128, 0, 127, 255)
        );
        assert_eq!(
            [s.left_x, s.left_y, s.right_x, s.right_y].map(crate::edge_report::centre_axis),
            [0, 128, 255, 127]
        );
        assert_eq!(
            (s.left_trigger, s.right_trigger, s.buttons, s.extras),
            (1, 255, 0, 0)
        );
        f[4..10].fill(0);
        let s = parse(&f).unwrap();
        assert_eq!(
            (s.buttons, s.extras, s.left_trigger, s.right_trigger),
            (0, 0, 0, 0)
        );
        assert!(parse(&f[..19]).is_none());
        f[10] = 1;
        assert!(parse(&f).is_none());
        f[10] = 0;
        f[19] = 1;
        assert!(parse(&f).is_none());
        f[19] = 0;
        f[..2].copy_from_slice(&[0xac, 0xc0]);
        let s = parse(&f).unwrap();
        assert_eq!((s.left_x, s.left_y), (0x2c, 0x40));
        f[10] = 1;
        assert!(parse(&f).is_none());
        f[18] = 0;
        assert!(parse(&f).is_none());
    }
}
