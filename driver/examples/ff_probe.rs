//! Probe which descriptor shapes make the kernel create a force-feedback
//! device.
//!
//! The capability probe showed `capabilities/ff: 0` for the vendor-defined
//! output report, which means Steam has nothing to send rumble to. That is a
//! real gap, not a cosmetic one.
//!
//! Rather than guess at kernel internals, this creates several candidate
//! descriptors in turn and reads back `capabilities/ff` for each. The result
//! decides the architecture: keep UHID and add a PID-style descriptor, or move
//! the virtual pad to uinput where `FF_RUMBLE` is declared directly.
//!
//! Needs root because `/dev/uhid` is mode 0600.

use std::time::{Duration, Instant};

use vader3p_steam::uhid::{Event, UhidDevice};

/// A candidate descriptor plus the output-report semantics it implies.
struct Variant {
    label: &'static str,
    descriptor: Vec<u8>,
}

/// Baseline: the shipped descriptor. Vendor usage under the Game Pad
/// collection. Expected to yield no FF capability.
fn baseline() -> Vec<u8> {
    vader3p_steam::descriptor::DESCRIPTOR.to_vec()
}

/// Physical Interface Device usage page with DC Enable Actuators.
///
/// This is the smallest possible PID declaration. If the kernel's generic HID
/// code creates FF for this, it is the cheapest fix.
fn pid_enable_actuators() -> Vec<u8> {
    let mut d = gamepad_without_output();
    d.extend_from_slice(&[
        0x05, 0x0F, //     Usage Page (Physical Interface Device)
        0x09, 0x97, //     Usage (DC Enable Actuators)
        0x15, 0x00, //     Logical Minimum (0)
        0x25, 0x01, //     Logical Maximum (1)
        0x75, 0x08, //     Report Size (8)
        0x95, 0x01, //     Report Count (1)
        0x91, 0x02, //     Output (Data, Var, Abs)
    ]);
    d.push(0xC0); // End Collection
    d
}

/// PID with Set Effect Report and an EFF-style magnitude field, which is what
/// `hid-pidff` looks for before it will bind.
fn pid_with_effect() -> Vec<u8> {
    let mut d = gamepad_without_output();
    d.extend_from_slice(&[
        // --- PID State Report / effect block ---
        0x05, 0x0F, //     Usage Page (PID)
        0x09, 0x92, //     Usage (PID State Report)
        0xA1, 0x02, //     Collection (Logical)
        0x85, 0x02, //       Report ID (2)
        0x09, 0x9F, //       Usage (Device Paused)
        0x09, 0xA0, //       Usage (Actuators Enabled)
        0x15, 0x00, //       Logical Minimum (0)
        0x25, 0x01, //       Logical Maximum (1)
        0x75, 0x01, //       Report Size (1)
        0x95, 0x02, //       Report Count (2)
        0x91, 0x02, //       Output (Data, Var, Abs)
        0x75, 0x06, //       Report Size (6)
        0x95, 0x01, //       Report Count (1)
        0x91, 0x03, //       Output (Const, Var, Abs)
        0xC0, //     End Collection
        // --- Set Effect Report ---
        0x09, 0x21, //     Usage (Set Effect Report)
        0xA1, 0x02, //     Collection (Logical)
        0x85, 0x03, //       Report ID (3)
        0x09, 0x22, //       Usage (Effect Block Index)
        0x15, 0x01, //       Logical Minimum (1)
        0x25, 0x28, //       Logical Maximum (40)
        0x75, 0x08, //       Report Size (8)
        0x95, 0x01, //       Report Count (1)
        0x91, 0x02, //       Output (Data, Var, Abs)
        0x09, 0x50, //       Usage (Duration)
        0x66, 0x03, 0x10, // Unit (4099: ms)
        0x55, 0x0E, //       Unit Exponent (-2)
        0x15, 0x00, //       Logical Minimum (0)
        0x26, 0xFF, 0x7F, // Logical Maximum (32767)
        0x75, 0x10, //       Report Size (16)
        0x95, 0x01, //       Report Count (1)
        0x91, 0x02, //       Output (Data, Var, Abs)
        0x09, 0x70, //       Usage (Magnitude)
        0x15, 0x00, //       Logical Minimum (0)
        0x26, 0xFF, 0x00, // Logical Maximum (255)
        0x75, 0x08, //       Report Size (8)
        0x95, 0x01, //       Report Count (1)
        0x91, 0x02, //       Output (Data, Var, Abs)
        0x09, 0x71, //       Usage (Start Delay)? re-use as second magnitude
        0x91, 0x02, //       Output (Data, Var, Abs)
        0x65, 0x00, //       Unit (None)
        0x55, 0x00, //       Unit Exponent (0)
        0xC0, //     End Collection
        // --- Effect Operation Report ---
        0x09, 0x77, //     Usage (Effect Operation Report)
        0xA1, 0x02, //     Collection (Logical)
        0x85, 0x04, //       Report ID (4)
        0x09, 0x22, //       Usage (Effect Block Index)
        0x15, 0x01, //       Logical Minimum (1)
        0x25, 0x28, //       Logical Maximum (40)
        0x75, 0x08, //       Report Size (8)
        0x95, 0x01, //       Report Count (1)
        0x91, 0x02, //       Output (Data, Var, Abs)
        0x09, 0x78, //       Usage (Operation)
        0x15, 0x01, //       Logical Minimum (1)
        0x25, 0x03, //       Logical Maximum (3)
        0x75, 0x08, //       Report Size (8)
        0x95, 0x01, //       Report Count (1)
        0x91, 0x02, //       Output (Data, Var, Abs)
        0x09, 0x79, //       Usage (Loop Count)
        0x15, 0x00, //       Logical Minimum (0)
        0x26, 0xFF, 0x00, // Logical Maximum (255)
        0x91, 0x02, //       Output (Data, Var, Abs)
        0xC0, //     End Collection
    ]);
    d.push(0xC0); // End Collection (application)
    d
}

/// The input half of the gamepad descriptor, with no output report, so variants
/// can append their own.
fn gamepad_without_output() -> Vec<u8> {
    let full = vader3p_steam::descriptor::DESCRIPTOR;
    // The shipped descriptor's output report block begins at the
    // `0x85, 0x02` report-ID item after the button padding item `0x81, 0x03`.
    // Cut there and keep the opening items, then close the collection in the
    // variant so each candidate controls its own ending.
    let mut out = Vec::new();
    let mut i = 0;
    while i < full.len() {
        let prefix = full[i];
        let size_code = (prefix & 0x03) as usize;
        let size = if size_code == 3 { 4 } else { size_code };
        // Stop before the output report ID 2 block.
        if prefix == 0x85 && full.get(i + 1) == Some(&0x02) {
            break;
        }
        out.extend_from_slice(&full[i..i + 1 + size]);
        i += 1 + size;
    }
    out
}

fn main() {
    let variants = [
        
        Variant {
            label: "baseline: vendor usage 1",
            descriptor: baseline(),
        },
        Variant {
            label: "PID: DC Enable Actuators",
            descriptor: pid_enable_actuators(),
        },
        Variant {
            label: "PID: Set Effect + Effect Operation",
            descriptor: pid_with_effect(),
        },
    ];

    for (idx, variant) in variants.iter().enumerate() {
        let name = format!("FF probe {idx}");
        print!("{}: ", variant.label);

        let mut dev = match UhidDevice::create_with_descriptor(&name, &variant.descriptor) {
            Ok(d) => d,
            Err(e) => {
                println!("create failed: {e:#}");
                continue;
            }
        };

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut started = false;
        while Instant::now() < deadline {
            match dev.poll_event() {
                Ok(Some(Event::Start)) => {
                    started = true;
                    break;
                }
                Ok(Some(_)) => {}
                Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                Err(_) => break,
            }
        }
        if !started {
            println!("no Start event (descriptor rejected)");
            continue;
        }

        std::thread::sleep(Duration::from_millis(250));
        let ff = read_capability(&name, "ff");
        let keys = read_capability(&name, "key").map(|t| count_bits(&t));
        let abs = read_capability(&name, "abs").map(|t| count_bits(&t));
        println!(
            "started, ff={:?} key_bits={:?} abs_bits={:?}",
            ff.as_deref().map(str::trim),
            keys,
            abs
        );
        dev.destroy();
        std::thread::sleep(Duration::from_millis(150));
    }
}

fn read_capability(name: &str, cap: &str) -> Option<String> {
    let entries = std::fs::read_dir("/sys/class/input").ok()?;
    for entry in entries.flatten() {
        let text = entry.file_name().to_string_lossy().to_string();
        if !text.starts_with("event") {
            continue;
        }
        let name_path = entry.path().join("device/name");
        if let Ok(device_name) = std::fs::read_to_string(name_path) {
            if device_name.trim() == name {
                let f = entry.path().join("device/capabilities").join(cap);
                return std::fs::read_to_string(f).ok();
            }
        }
    }
    None
}

fn count_bits(bitmap: &str) -> u32 {
    bitmap
        .split_whitespace()
        .filter_map(|w| u64::from_str_radix(w, 16).ok())
        .map(|w| w.count_ones())
        .sum()
}
