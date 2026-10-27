//! Integration tests against the recorded capture fixture.
//!
//! These are the tests that tie the parser to real hardware output rather than
//! to synthetic byte patterns. The fixture is the owner-confirmed A/B/C/Z
//! capture (sanitised), so a regression here means the parser no longer agrees
//! with what the controller actually sends.

use std::path::Path;

use vader3p_steam::encode;
use vader3p_steam::input::{self, button, extra, REPORT_LEN};

/// Extract every `hex` value from the fixture's `reports` array.
///
/// The fixture stores one object per line as `{"elapsed_ns": N, "hex": "…"}`,
/// so this searches for the key anywhere in a line rather than requiring it at
/// the start. A full JSON dependency is not justified for one flat array.
fn report_hexes(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some(pos) = line.find("\"hex\"") else {
            continue;
        };
        let rest = &line[pos + 5..];
        let Some(start) = rest.find('"') else {
            continue;
        };
        let rest = &rest[start + 1..];
        let Some(end) = rest.find('"') else { continue };
        out.push(rest[..end].to_string());
    }
    out
}

fn load_fixture() -> Vec<Vec<u8>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/vader3pro-bluetooth-dinput-abcz.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    report_hexes(&text)
        .iter()
        .map(|h| {
            let bytes: Vec<u8> = (0..h.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&h[i..i + 2], 16).expect("valid hex"))
                .collect();
            bytes
        })
        .collect()
}

#[test]
fn fixture_reports_are_all_parseable() {
    let reports = load_fixture();
    assert!(!reports.is_empty(), "fixture must contain reports");

    for (idx, raw) in reports.iter().enumerate() {
        assert_eq!(raw.len(), REPORT_LEN, "report {idx} length");
        let state =
            input::parse(raw).unwrap_or_else(|e| panic!("report {idx} failed to parse: {e}"));
        // A parse must produce a state whose bits fit the declared button count.
        assert!(state.buttons < (1 << button::STANDARD_COUNT));
        assert!(state.extras < (1 << extra::COUNT));
    }
}

#[test]
fn fixture_shows_a_b_press_and_release() {
    // The owner pressed A then B, each with a release. Count transitions rather
    // than assuming which report is which, so the test survives reordering.
    let reports = load_fixture();
    let states: Vec<_> = reports.iter().map(|r| input::parse(r).unwrap()).collect();

    let a_edges = states
        .windows(2)
        .filter(|w| w[0].button(button::A) != w[1].button(button::A))
        .count();
    let b_edges = states
        .windows(2)
        .filter(|w| w[0].button(button::B) != w[1].button(button::B))
        .count();

    assert!(
        a_edges >= 2,
        "A should both press and release, got {a_edges} edges"
    );
    assert!(
        b_edges >= 2,
        "B should both press and release, got {b_edges} edges"
    );
}

#[test]
fn fixture_a_and_b_are_never_both_pressed() {
    // The capture was sequential, so a frame with both A and B indicates the
    // parser is merging fields that the device keeps separate.
    for raw in load_fixture() {
        let s = input::parse(&raw).unwrap();
        assert!(
            !(s.button(button::A) && s.button(button::B)),
            "A and B set in one frame: {:02x?}",
            raw
        );
    }
}

#[test]
fn fixture_c_and_z_use_byte_11_not_byte_9() {
    // C and Z must appear as extras, never as face buttons.
    let mut saw_c = false;
    let mut saw_z = false;
    for raw in load_fixture() {
        let s = input::parse(&raw).unwrap();
        if s.extra(extra::C) {
            saw_c = true;
            assert_eq!(raw[11] & 0x01, 0x01, "C must come from byte 11 bit 0");
        }
        if s.extra(extra::Z) {
            saw_z = true;
            assert_eq!(raw[11] & 0x02, 0x02, "Z must come from byte 11 bit 1");
        }
        assert!(
            !s.extra(extra::C) || !s.extra(extra::Z),
            "capture was sequential; both C and Z in one frame"
        );
    }
    assert!(saw_c && saw_z, "fixture should contain both C and Z");
}

#[test]
fn every_fixture_report_encodes_to_a_full_report() {
    for raw in load_fixture() {
        let state = input::parse(&raw).unwrap();
        let out = encode::encode(&state);
        assert_eq!(out.len(), vader3p_steam::descriptor::INPUT_REPORT_LEN);
        assert_eq!(out[0], vader3p_steam::descriptor::INPUT_REPORT_ID);
    }
}

#[test]
fn padding_tail_is_ignored_by_real_reports() {
    // Some raw captures had non-zero padding. Whatever sits in bytes 15-20 must
    // not reach the output, or a hidden field could silently influence input.
    let reports = load_fixture();
    for raw in reports {
        let mut mutated = raw.clone();
        mutated[15..].fill(0xa5);
        if raw.len() < REPORT_LEN {
            continue;
        }
        let a = input::parse(&raw).unwrap();
        let b = input::parse(&mutated).unwrap();
        assert_eq!(a, b, "padding bytes changed the parsed state");
    }
}
