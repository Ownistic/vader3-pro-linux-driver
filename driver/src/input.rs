//! Physical Bluetooth DInput report parsing.
//!
//! The Vader 3 Pro in Bluetooth DInput mode emits 21-byte HID report-ID-1
//! frames although its descriptor declares 15 bytes including the ID. The
//! firmware builds a 20-byte payload, writes only indices 0-13 and transmits
//! all 20, so bytes 15-20 are uninitialised padding. Never truncate to the
//! descriptor length and never interpret the tail.
//!
//! Byte map, established from owner-coordinated passive captures and crossed
//! against the firmware's own key table (see `docs/PROTOCOL.md`):
//!
//! | Offset | Meaning |
//! | --- | --- |
//! | 0 | report ID (always `0x01`) |
//! | 1, 2 | left stick X, Y |
//! | 3, 4 | right stick X, Y |
//! | 5-8 | always zero in this personality |
//! | 9 low nibble | D-pad hat: 1=up 3=right 5=down 7=left, 0=neutral |
//! | 9 high nibble | A `0x10`, B `0x20`, X `0x40`, Y `0x80` |
//! | 10 | LB `0x01`, RB `0x02`, LT `0x04`, RT `0x08`, Select `0x10`, Start `0x20`, L3 `0x40`, R3 `0x80` |
//! | 11 | C `0x01`, Z `0x02`, M1 `0x04`, M2 `0x08`, M3 `0x10`, M4 `0x20` |
//! | 12 | Home `0x80` |
//! | 13 | left trigger analog level |
//! | 14 | right trigger analog level |
//!
//! This stream carries no motion data: a whole-controller shake produced no
//! movement in bytes 1-4. Gyro lives only in vendor mode.

/// Report length produced by the device in Bluetooth DInput mode.
pub const REPORT_LEN: usize = 21;

/// Report ID for the input report.
pub const REPORT_ID: u8 = 0x01;

/// Number of standard buttons in the canonical state.
pub const BUTTON_COUNT: usize = 16;

/// Canonical button indices. The order is fixed and shared with the report
/// encoder; do not reorder without updating both.
pub mod button {
    pub const A: usize = 0;
    pub const B: usize = 1;
    pub const X: usize = 2;
    pub const Y: usize = 3;
    pub const LB: usize = 4;
    pub const RB: usize = 5;
    pub const SELECT: usize = 6;
    pub const START: usize = 7;
    pub const L3: usize = 8;
    pub const R3: usize = 9;
    pub const HOME: usize = 10;
    pub const DPAD_UP: usize = 11;
    pub const DPAD_RIGHT: usize = 12;
    pub const DPAD_DOWN: usize = 13;
    pub const DPAD_LEFT: usize = 14;

    /// Number of standard (non-extra) buttons.
    pub const STANDARD_COUNT: usize = 15;
}

/// Canonical indices for the six buttons the project must expose independently.
pub mod extra {
    pub const C: usize = 0;
    pub const Z: usize = 1;
    pub const M1: usize = 2;
    pub const M2: usize = 3;
    pub const M3: usize = 4;
    pub const M4: usize = 5;

    /// Number of extra buttons.
    pub const COUNT: usize = 6;
}

/// Names in canonical order, for logging and test messages.
pub const STANDARD_NAMES: [&str; button::STANDARD_COUNT] = [
    "a",
    "b",
    "x",
    "y",
    "lb",
    "rb",
    "select",
    "start",
    "l3",
    "r3",
    "home",
    "dpad_up",
    "dpad_right",
    "dpad_down",
    "dpad_left",
];

/// Names in canonical order for the six independent extras.
pub const EXTRA_NAMES: [&str; extra::COUNT] = ["c", "z", "m1", "m2", "m3", "m4"];

/// Fully decoded controller state.
///
/// Sticks are stored as `u8` exactly as the device reports them. The device
/// centres near `0xfd` and behaves like a signed byte offset from that
/// session-dependent centre, so the encoder applies centring rather than the
/// parser: keeping the raw value here means a re-centre does not need to
/// re-read history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct State {
    pub left_x: u8,
    pub left_y: u8,
    pub right_x: u8,
    pub right_y: u8,
    pub left_trigger: u8,
    pub right_trigger: u8,
    /// Bit `i` is set when standard button `i` is pressed.
    pub buttons: u16,
    /// Bit `i` is set when extra button `i` is pressed.
    pub extras: u8,
}

impl State {
    /// True when standard button `idx` is pressed.
    #[must_use]
    pub fn button(&self, idx: usize) -> bool {
        idx < button::STANDARD_COUNT && (self.buttons >> idx) & 1 == 1
    }

    /// True when extra button `idx` is pressed.
    #[must_use]
    pub fn extra(&self, idx: usize) -> bool {
        idx < extra::COUNT && (self.extras >> idx) & 1 == 1
    }
}

/// Why a report could not be parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    /// The slice is shorter than [`REPORT_LEN`].
    TooShort { got: usize },
    /// The report ID is not [`REPORT_ID`].
    WrongId(u8),
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooShort { got } => {
                write!(f, "report too short: got {got} bytes, need {REPORT_LEN}")
            }
            Self::WrongId(id) => write!(f, "unexpected report ID {id:#04x}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Decode the D-pad hat value from byte 9's low nibble.
///
/// The device reports a clean `0` when centred in every captured idle frame.
/// Unseen values are treated as neutral rather than guessed, so a firmware
/// change produces no input rather than a wrong direction.
fn decode_hat(nibble: u8) -> u16 {
    let bit = match nibble {
        1 => button::DPAD_UP,
        3 => button::DPAD_RIGHT,
        5 => button::DPAD_DOWN,
        7 => button::DPAD_LEFT,
        _ => return 0,
    };
    1 << bit
}

/// Parse one 21-byte Bluetooth DInput report.
pub fn parse(report: &[u8]) -> Result<State, ParseError> {
    if report.len() < REPORT_LEN {
        return Err(ParseError::TooShort { got: report.len() });
    }
    if report[0] != REPORT_ID {
        return Err(ParseError::WrongId(report[0]));
    }

    let b9 = report[9];
    let b10 = report[10];
    let b11 = report[11];
    let b12 = report[12];

    let mut buttons = decode_hat(b9 & 0x0f);

    // Byte 9 high nibble: A, B, X, Y.
    if b9 & 0x10 != 0 {
        buttons |= 1 << button::A;
    }
    if b9 & 0x20 != 0 {
        buttons |= 1 << button::B;
    }
    if b9 & 0x40 != 0 {
        buttons |= 1 << button::X;
    }
    if b9 & 0x80 != 0 {
        buttons |= 1 << button::Y;
    }

    // Byte 10: shoulder, trigger flags and menu/stick clicks.
    if b10 & 0x01 != 0 {
        buttons |= 1 << button::LB;
    }
    if b10 & 0x02 != 0 {
        buttons |= 1 << button::RB;
    }
    if b10 & 0x10 != 0 {
        buttons |= 1 << button::SELECT;
    }
    if b10 & 0x20 != 0 {
        buttons |= 1 << button::START;
    }
    if b10 & 0x40 != 0 {
        buttons |= 1 << button::L3;
    }
    if b10 & 0x80 != 0 {
        buttons |= 1 << button::R3;
    }

    if b12 & 0x80 != 0 {
        buttons |= 1 << button::HOME;
    }

    // Byte 11: the six independent extras.
    let mut extras = 0u8;
    if b11 & 0x01 != 0 {
        extras |= 1 << extra::C;
    }
    if b11 & 0x02 != 0 {
        extras |= 1 << extra::Z;
    }
    if b11 & 0x04 != 0 {
        extras |= 1 << extra::M1;
    }
    if b11 & 0x08 != 0 {
        extras |= 1 << extra::M2;
    }
    if b11 & 0x10 != 0 {
        extras |= 1 << extra::M3;
    }
    if b11 & 0x20 != 0 {
        extras |= 1 << extra::M4;
    }

    // Trigger flags gate the analog level. Reporting a level while the flag is
    // clear would show a resting trigger as partially pressed, because the
    // analog byte is not forced to zero when the trigger is released.
    let left_trigger = if b10 & 0x04 != 0 { report[13] } else { 0 };
    let right_trigger = if b10 & 0x08 != 0 { report[14] } else { 0 };

    Ok(State {
        left_x: report[1],
        left_y: report[2],
        right_x: report[3],
        right_y: report[4],
        left_trigger,
        right_trigger,
        buttons,
        extras,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a report with all-neutral control bytes.
    fn neutral() -> [u8; REPORT_LEN] {
        let mut r = [0u8; REPORT_LEN];
        r[0] = REPORT_ID;
        r[1] = 0xfd;
        r[2] = 0xfd;
        r[3] = 0xfd;
        r[4] = 0xfd;
        r
    }

    #[test]
    fn rejects_short_reports() {
        assert_eq!(
            parse(&[REPORT_ID; 20]),
            Err(ParseError::TooShort { got: 20 })
        );
    }

    #[test]
    fn rejects_wrong_report_id() {
        let mut r = neutral();
        r[0] = 0x02;
        assert_eq!(parse(&r), Err(ParseError::WrongId(0x02)));
    }

    #[test]
    fn accepts_padding_tail() {
        // Bytes 15-20 are uninitialised padding; junk there must not matter.
        let mut r = neutral();
        r[15..].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef, 0x00, 0xff]);
        assert_eq!(
            parse(&r).unwrap(),
            State {
                left_x: 0xfd,
                left_y: 0xfd,
                right_x: 0xfd,
                right_y: 0xfd,
                ..State::default()
            }
        );
    }

    #[test]
    fn decodes_face_buttons() {
        for (mask, idx) in [
            (0x10u8, button::A),
            (0x20, button::B),
            (0x40, button::X),
            (0x80, button::Y),
        ] {
            let mut r = neutral();
            r[9] = mask;
            let s = parse(&r).unwrap();
            assert!(s.button(idx), "mask {mask:#04x} should set index {idx}");
            assert_eq!(s.buttons.count_ones(), 1);
        }
    }

    #[test]
    fn decodes_dpad_hat() {
        for (nibble, idx) in [
            (1u8, button::DPAD_UP),
            (3, button::DPAD_RIGHT),
            (5, button::DPAD_DOWN),
            (7, button::DPAD_LEFT),
        ] {
            let mut r = neutral();
            r[9] = nibble;
            let s = parse(&r).unwrap();
            assert!(s.button(idx), "hat {nibble} should set index {idx}");
            assert_eq!(s.buttons.count_ones(), 1);
        }
    }

    #[test]
    fn unknown_hat_value_is_neutral() {
        // 2, 4, 6 and 8..=15 are not observed directions. Better silence than a
        // guessed diagonal.
        for nibble in [0u8, 2, 4, 6, 8, 15] {
            let mut r = neutral();
            r[9] = nibble;
            let s = parse(&r).unwrap();
            assert_eq!(
                s.buttons & 0xf800,
                0,
                "hat {nibble} must not press a direction"
            );
        }
    }

    #[test]
    fn decodes_byte10_controls() {
        for (mask, idx) in [
            (0x01u8, button::LB),
            (0x02, button::RB),
            (0x10, button::SELECT),
            (0x20, button::START),
            (0x40, button::L3),
            (0x80, button::R3),
        ] {
            let mut r = neutral();
            r[10] = mask;
            let s = parse(&r).unwrap();
            assert!(s.button(idx), "mask {mask:#04x} should set index {idx}");
            assert_eq!(s.buttons.count_ones(), 1);
        }
    }

    #[test]
    fn decodes_home() {
        let mut r = neutral();
        r[12] = 0x80;
        let s = parse(&r).unwrap();
        assert!(s.button(button::HOME));
        assert_eq!(s.buttons.count_ones(), 1);
    }

    #[test]
    fn decodes_all_six_extras_independently() {
        for (mask, idx) in [
            (0x01u8, extra::C),
            (0x02, extra::Z),
            (0x04, extra::M1),
            (0x08, extra::M2),
            (0x10, extra::M3),
            (0x20, extra::M4),
        ] {
            let mut r = neutral();
            r[11] = mask;
            let s = parse(&r).unwrap();
            assert!(s.extra(idx), "mask {mask:#04x} should set extra {idx}");
            assert_eq!(s.extras.count_ones(), 1);
            assert_eq!(s.buttons, 0, "extra must not alias a standard button");
        }
    }

    #[test]
    fn decodes_all_six_extras_simultaneously() {
        // The PRD requires simultaneous presses, so this is a real acceptance
        // shape rather than a convenience test.
        let mut r = neutral();
        r[11] = 0x3f;
        let s = parse(&r).unwrap();
        assert_eq!(s.extras.count_ones(), 6);
        for idx in 0..extra::COUNT {
            assert!(s.extra(idx), "extra {idx} must be set");
        }
    }

    #[test]
    fn extras_do_not_alias_dpad() {
        // Regression guard: an earlier analysis wrongly claimed M1-M4 shared
        // the D-pad bits. They are separate fields.
        let mut r = neutral();
        r[11] = 0x3f;
        let s = parse(&r).unwrap();
        assert_eq!(
            s.buttons
                & ((1 << button::DPAD_UP)
                    | (1 << button::DPAD_RIGHT)
                    | (1 << button::DPAD_DOWN)
                    | (1 << button::DPAD_LEFT)),
            0
        );
    }

    #[test]
    fn trigger_level_gated_by_flag() {
        let mut r = neutral();
        r[13] = 0xff;
        r[14] = 0xff;
        assert_eq!(parse(&r).unwrap().left_trigger, 0);
        assert_eq!(parse(&r).unwrap().right_trigger, 0);

        r[10] = 0x0c;
        let s = parse(&r).unwrap();
        assert_eq!(s.left_trigger, 0xff);
        assert_eq!(s.right_trigger, 0xff);
    }

    #[test]
    fn decodes_sticks() {
        let mut r = neutral();
        r[1] = 0x00;
        r[2] = 0xff;
        r[3] = 0x80;
        r[4] = 0x7f;
        let s = parse(&r).unwrap();
        assert_eq!(
            (s.left_x, s.left_y, s.right_x, s.right_y),
            (0x00, 0xff, 0x80, 0x7f)
        );
    }

    #[test]
    fn full_press_and_release_sequence() {
        // A real captured set/clear pair for A from the recorded fixture shape.
        let mut pressed = neutral();
        pressed[9] = 0x10;
        let mut released = neutral();
        released[9] = 0x00;

        let on = parse(&pressed).unwrap();
        let off = parse(&released).unwrap();
        assert!(on.button(button::A));
        assert!(!off.button(button::A));
    }
}
