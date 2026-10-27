//! Create the virtual pad, report what the kernel derived from the descriptor,
//! then tear it down.
//!
//! `Start` only proves the descriptor parsed. It does not prove the axis or
//! button counts are what the encoder assumes. This example reads back the
//! kernel's own view via sysfs, which is the same view Steam's device layer
//! starts from.
//!
//! Needs root because `/dev/uhid` is mode 0600.

use std::time::{Duration, Instant};

use vader3p_steam::uhid::{Event, UhidDevice};

fn main() {
    let name = "Vader3P capability probe";
    let mut dev = match UhidDevice::create(name) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("create failed: {e:#}");
            std::process::exit(1);
        }
    };
    println!("created: {}", dev.name());

    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        match dev.poll_event() {
            Ok(Some(Event::Start)) => {
                println!("kernel Start received");
                break;
            }
            Ok(Some(other)) => println!("event: {other:?}"),
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => {
                eprintln!("poll error: {e:#}");
                break;
            }
        }
    }

    // Give udev a moment to publish the device.
    std::thread::sleep(Duration::from_millis(300));

    match find_event_node(name) {
        Some((dev_path, sys_path)) => {
            println!("event node: {}", dev_path.display());
            println!("sysfs: {}", sys_path.display());
            // The kernel's own view of what the descriptor declared. This is
            // the authoritative check: a well-formed struct with a bad
            // descriptor still produces Start, but the bitmaps would show it.
            for cap in ["ev", "key", "abs", "rel", "ff"] {
                let f = sys_path.join("device/capabilities").join(cap);
                match std::fs::read_to_string(&f) {
                    Ok(text) => println!("capabilities/{cap}: {}", text.trim()),
                    Err(_) => println!("capabilities/{cap}: (absent)"),
                }
            }
            // Decode the key bitmap so the button count is readable directly.
            if let Ok(text) = std::fs::read_to_string(sys_path.join("device/capabilities/key")) {
                let count = count_bits(&text);
                println!("key bits set: {count}");
            }
            if let Ok(text) = std::fs::read_to_string(sys_path.join("device/capabilities/abs")) {
                let count = count_bits(&text);
                println!("abs bits set: {count}");
            }
        }
        None => println!("event node not found in /sys/class/input"),
    }

    // Exercise the encode path once so a panic would surface here.
    let state = vader3p_steam::input::State {
        extras: 0x3f, // all six extras
        ..Default::default()
    };
    let report = vader3p_steam::encode::encode(&state);
    println!("input report ({} bytes): {:02x?}", report.len(), report);
    if let Err(e) = dev.send_input(&report) {
        eprintln!("send failed: {e:#}");
    } else {
        println!("input report accepted");
    }

    std::thread::sleep(Duration::from_millis(200));
    dev.destroy();
    println!("destroyed");
}

/// Locate the evdev node whose sysfs name matches the virtual device.
///
/// Returns both the `/dev/input` path and the `/sys/class/input` path, because
/// the capability bitmaps live under sysfs and the readable node under dev.
fn find_event_node(name: &str) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let entries = std::fs::read_dir("/sys/class/input").ok()?;
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let text = file_name.to_string_lossy().to_string();
        if !text.starts_with("event") {
            continue;
        }
        let name_path = entry.path().join("device/name");
        if let Ok(device_name) = std::fs::read_to_string(name_path) {
            if device_name.trim() == name {
                return Some((std::path::Path::new("/dev/input").join(&text), entry.path()));
            }
        }
    }
    None
}

/// Count set bits in a sysfs capability bitmap such as `1f 0 0 ...`.
fn count_bits(bitmap: &str) -> u32 {
    bitmap
        .split_whitespace()
        .filter_map(|word| u64::from_str_radix(word, 16).ok())
        .map(|word| word.count_ones())
        .sum()
}
