//! HID report descriptor for the virtual controller.
//!
//! # Design choice
//!
//! The virtual device is a **generic desktop gamepad** with four 8-bit axes, a
//! 4-bit hat and 21 buttons (15 standard + 6 independent extras). Rumble is
//! carried by a vendor-defined output report so the kernel creates an
//! `EV_FF`-capable evdev node for it.
//!
//! A generic gamepad was chosen over mimicking a DualSense or Xbox Elite
//! because it needs no proprietary report encoder, no CRC, and no
//! authentication handshake. The trade-off is explicit: **whether Steam Input
//! exposes all 21 buttons individually on a generic gamepad is not yet
//! verified** (PRD acceptance A4/A5). If Steam turns out to collapse the extra
//! buttons, the fallback is a DualSense Edge-style encoder, which is a
//! substantial additional implementation.
//!
//! # Layout
//!
//! Input report, report ID 1:
//!
//! | Bytes | Content |
//! | --- | --- |
//! | 0 | report ID `0x01` |
//! | 1 | X axis (left stick X) |
//! | 2 | Y axis (left stick Y) |
//! | 3 | Z axis (right stick X) |
//! | 4 | Rz axis (right stick Y) |
//! | 5 | hat switch (low nibble, 0-7) |
//! | 6 | buttons 1-8 (A, B, X, Y, LB, RB, Select, Start) |
//! | 7 | buttons 9-16 (L3, R3, Home, D-pad up/right/down/left) |
//! | 8 | buttons 17-21 (C, Z, M1, M2, M3, M4) |
//!
//! Output report, report ID 2: two bytes, left and right motor strength.

/// Report ID for the input report.
pub const INPUT_REPORT_ID: u8 = 1;

/// Report ID for the rumble output report.
pub const OUTPUT_REPORT_ID: u8 = 2;

/// Size of the input report excluding the report ID.
pub const INPUT_PAYLOAD_LEN: usize = 8;

/// Total input report size including the report ID.
pub const INPUT_REPORT_LEN: usize = INPUT_PAYLOAD_LEN + 1;

/// Button count advertised by the descriptor.
pub const BUTTONS: usize = 21;

/// HID report descriptor.
///
/// 21 buttons exceeds the 16 that fit in two bytes, so buttons are emitted in
/// three groups. Group 3 declares 5 bits to land exactly on 21 rather than
/// padding to 24, which keeps the payload at 8 bytes.
pub const DESCRIPTOR: &[u8] = &[
    0x05, 0x01, // Usage Page (Generic Desktop)
    0x09, 0x05, // Usage (Game Pad)
    0xA1, 0x01, // Collection (Application)
    // --- Report ID 1: input ---
    0x85, INPUT_REPORT_ID, //   Report ID (1)
    0x09, 0x30, //   Usage (X)
    0x09, 0x31, //   Usage (Y)
    0x09, 0x32, //   Usage (Z)
    0x09, 0x35, //   Usage (Rz)
    0x15, 0x00, //   Logical Minimum (0)
    0x26, 0xFF, 0x00, //   Logical Maximum (255)
    0x75, 0x08, //   Report Size (8)
    0x95, 0x04, //   Report Count (4)
    0x81, 0x02, //   Input (Data, Var, Abs)
    // Hat switch: 4 bits of data, padded to a byte.
    0x09, 0x39, //   Usage (Hat switch)
    0x15, 0x00, //   Logical Minimum (0)
    0x25, 0x07, //   Logical Maximum (7)
    0x35, 0x00, //   Physical Minimum (0)
    0x46, 0x3B, 0x01, //   Physical Maximum (315)
    0x65, 0x14, //   Unit (Eng Rot: Degrees)
    0x75, 0x04, //   Report Size (4)
    0x95, 0x01, //   Report Count (1)
    0x81, 0x42, //   Input (Data, Var, Abs, Null State)
    0x65, 0x00, //   Unit (None)
    0x75, 0x04, //   Report Size (4)
    0x95, 0x01, //   Report Count (1)
    0x81, 0x03, //   Input (Const, Var, Abs)  -- padding
    // Buttons 1-8.
    0x05, 0x09, //   Usage Page (Button)
    0x19, 0x01, //   Usage Minimum (1)
    0x29, 0x08, //   Usage Maximum (8)
    0x15, 0x00, //   Logical Minimum (0)
    0x25, 0x01, //   Logical Maximum (1)
    0x75, 0x01, //   Report Size (1)
    0x95, 0x08, //   Report Count (8)
    0x81, 0x02, //   Input (Data, Var, Abs)
    // Buttons 9-16.
    0x19, 0x09, //   Usage Minimum (9)
    0x29, 0x10, //   Usage Maximum (16)
    0x95, 0x08, //   Report Count (8)
    0x81, 0x02, //   Input (Data, Var, Abs)
    // Buttons 17-21: the final five of the 21 button slots.
    // Standard buttons occupy slots 1-15; the six extras occupy 16-21. Slot 16
    // falls in the high bit of byte 7, so the encoder must pack extras bit 0
    // into overall bit 15 rather than starting a fresh byte at 16.
    0x19, 0x11, //   Usage Minimum (17)
    0x29, 0x15, //   Usage Maximum (21)
    0x95, 0x05, //   Report Count (5)
    0x81, 0x02, //   Input (Data, Var, Abs)
    // Pad the button block to a whole byte.
    0x95, 0x03, //   Report Count (3)
    0x81, 0x03, //   Input (Const, Var, Abs)
    // --- Report ID 2: rumble output ---
    0x85, OUTPUT_REPORT_ID, //   Report ID (2)
    0x09, 0x01, //   Usage (Vendor Usage 1)
    0x15, 0x00, //   Logical Minimum (0)
    0x26, 0xFF, 0x00, //   Logical Maximum (255)
    0x75, 0x08, //   Report Size (8)
    0x95, 0x02, //   Report Count (2)
    0x91, 0x02, //   Output (Data, Var, Abs)
    0xC0, // End Collection
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Walk the descriptor's short items and total the bits each Main item
    /// declares. Catches arithmetic mistakes in the button counts, which would
    /// otherwise surface as a silently truncated report.
    #[test]
    fn input_report_bit_count_matches_layout() {
        let mut bits = 0usize;
        let mut global_size = 0usize;
        let mut global_count = 0usize;
        let mut i = 0usize;

        while i < DESCRIPTOR.len() {
            let prefix = DESCRIPTOR[i];
            if prefix == 0xFE {
                panic!("long items are not expected in this descriptor");
            }
            let size_code = (prefix & 0x03) as usize;
            let size = if size_code == 3 { 4 } else { size_code };
            let item_type = (prefix >> 2) & 0x03;
            let tag = (prefix >> 4) & 0x0F;
            let data = &DESCRIPTOR[i + 1..i + 1 + size];

            // Global items: report size (tag 7), report count (tag 9).
            if item_type == 1 {
                let value = match size {
                    1 => u32::from(data[0]),
                    2 => u32::from(u16::from_le_bytes([data[0], data[1]])),
                    _ => 0,
                };
                if tag == 7 {
                    global_size = value as usize;
                } else if tag == 9 {
                    global_count = value as usize;
                }
            }
            // Main items: Input (tag 8), Output (tag 9).
            if item_type == 0 && (tag == 8 || tag == 9) {
                bits += global_size * global_count;
            }
            i += 1 + size;
        }

        // 4 axes (32) + hat (4) + pad (4) + 8 + 8 + 5 + 3 buttons = 64, plus
        // the two-byte output report.
        assert_eq!(bits, 64 + 16, "unexpected total bit count");
    }

    #[test]
    fn descriptor_declares_21_buttons() {
        // Button page usages must reach 21 so the extras are addressable.
        let has_21 = DESCRIPTOR
            .windows(2)
            .any(|w| w[0] == 0x29 && w[1] == 0x15);
        assert!(has_21, "descriptor must declare button usage up to 21");
    }

    #[test]
    fn descriptor_is_well_formed() {
        // Every item must fit inside the buffer.
        let mut i = 0usize;
        while i < DESCRIPTOR.len() {
            let size_code = (DESCRIPTOR[i] & 0x03) as usize;
            let size = if size_code == 3 { 4 } else { size_code };
            assert!(
                i + 1 + size <= DESCRIPTOR.len(),
                "item at {i} overruns the descriptor"
            );
            i += 1 + size;
        }
        assert_eq!(i, DESCRIPTOR.len(), "descriptor ends on an item boundary");
    }
}
