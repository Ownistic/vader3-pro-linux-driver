//! Integration test for the owner-confirmed M-key bit order.
//!
//! The M1-M4 bit assignment was previously table-derived, because a
//! coordinated multi-key capture could not confirm press order and once
//! reported M2 where M4 was expected. This fixture comes from an ordered
//! capture in which each key was pressed alone three times in turn, so the
//! assignment is observation rather than inference.
//!
//! A regression here would silently swap two rear buttons.

use std::path::Path;

use vader3p_steam::input::{self, extra, REPORT_LEN};

fn load() -> Vec<Vec<u8>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/vader3pro-bluetooth-dinput-mkeys.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));

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
        let hex = &rest[..end];
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("valid hex"))
            .collect();
        out.push(bytes);
    }
    out
}

#[test]
fn fixture_reports_are_all_well_formed() {
    let reports = load();
    assert!(!reports.is_empty(), "fixture must contain reports");

    for (idx, raw) in reports.iter().enumerate() {
        assert_eq!(raw.len(), REPORT_LEN, "report {idx} length");
        input::parse(raw).unwrap_or_else(|e| panic!("report {idx} failed: {e}"));
    }
}

#[test]
fn m_key_bit_order_is_owner_confirmed() {
    // Expected assignment, established by pressing each key alone in order.
    let expected = [
        (0x04u8, extra::M1, "M1"),
        (0x08, extra::M2, "M2"),
        (0x10, extra::M3, "M3"),
        (0x20, extra::M4, "M4"),
    ];

    let mut seen_bits: Vec<u8> = Vec::new();
    for raw in load() {
        let mask = raw[11] & 0x3c; // M1..M4 bits only
        if mask == 0 {
            continue;
        }
        assert!(
            mask.is_power_of_two(),
            "a single M press must set one bit, got {mask:#04x}"
        );
        if seen_bits.last() != Some(&mask) {
            seen_bits.push(mask);
        }
    }

    // The capture pressed M1, M2, M3, M4 in that order, so the sequence of
    // distinct masks must follow the same order.
    let distinct: Vec<u8> = {
        let mut v = Vec::new();
        for m in seen_bits {
            if !v.contains(&m) {
                v.push(m);
            }
        }
        v
    };

    for (i, (mask, _, name)) in expected.iter().enumerate() {
        assert_eq!(
            distinct.get(i),
            Some(mask),
            "{name} should have been the {}th distinct mask; saw {distinct:02x?}",
            i + 1
        );
    }
}

#[test]
fn each_m_key_maps_to_exactly_one_extra_index() {
    for raw in load() {
        let state = input::parse(&raw).unwrap();
        let mask = raw[11] & 0x3c;
        if mask == 0 {
            assert_eq!(state.extras, 0, "no M press means no extra bits");
            continue;
        }
        assert_eq!(
            state.extras.count_ones(),
            1,
            "one physical M press must set exactly one extra"
        );
        let expected_index = match mask {
            0x04 => extra::M1,
            0x08 => extra::M2,
            0x10 => extra::M3,
            0x20 => extra::M4,
            other => panic!("unexpected M mask {other:#04x}"),
        };
        assert!(
            state.extra(expected_index),
            "mask {mask:#04x} should map to extra index {expected_index}"
        );
    }
}

#[test]
fn m_presses_never_produce_dpad_or_standard_buttons() {
    // The historical failure mode was believing M1-M4 shared the D-pad bits.
    // Every M press in this fixture must leave standard buttons completely
    // clear apart from nothing at all.
    for raw in load() {
        let state = input::parse(&raw).unwrap();
        if raw[11] & 0x3c == 0 {
            continue;
        }
        assert_eq!(
            state.buttons, 0,
            "M press must not set any standard button: {:#06x}",
            state.buttons
        );
    }
}
