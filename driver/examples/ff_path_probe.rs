//! End-to-end FF path probe.
//!
//! Creates the uinput pad, then watches `/dev/input/eventN` for `EV_FF` and
//! upload events, exactly as the daemon will. The aim is to prove the host can
//! drive rumble through this device at all, because a declared capability that
//! never delivers an event would still leave Steam unable to vibrate the pad.
//!
//! Needs write access to `/dev/uinput` and read access to the created event node.

use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::time::{Duration, Instant};

use vader3p_steam::pad::VirtualPad;

const UINPUT_MAX_NAME_SIZE: usize = 80;

#[repr(C)]
struct InputId {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
}

#[repr(C)]
struct UinputSetup {
    id: InputId,
    name: [u8; UINPUT_MAX_NAME_SIZE],
    ff_effects_max: u32,
}

const UI_DEV_SETUP: libc::c_ulong = 0x405c_5503;
const UI_DEV_CREATE: libc::c_ulong = 0x5501;
const UI_DEV_DESTROY: libc::c_ulong = 0x5502;
const UI_SET_EVBIT: libc::c_ulong = 0x4004_5564;
const UI_SET_FFBIT: libc::c_ulong = 0x4004_5565;

fn main() {
    let name = "Vader3P ff-path probe";
    println!("creating pad via lib: {}", name);
    let mut pad = match VirtualPad::create(name) {
        Ok(p) => p,
        Err(e) => {
            println!("VirtualPad::create failed: {e:#}");
            return;
        }
    };
    println!("pad created: {}", pad.name());
    std::thread::sleep(Duration::from_millis(300));

    // Push a neutral frame so the node is live.
    if let Err(e) = pad.send(&vader3p_steam::input::State::default()) {
        println!("send failed: {e:#}");
    }

    let node = find_event_node(name);
    match &node {
        Some(p) => println!("event node: {}", p.display()),
        None => {
            println!("event node not found; cannot test the FF path");
            pad.destroy();
            return;
        }
    }

    let node = node.unwrap();
    let mut ev = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(&node)
    {
        Ok(f) => f,
        Err(e) => {
            println!("cannot read {}: {e}", node.display());
            pad.destroy();
            return;
        }
    };

    // Ask the kernel whether this node is actually grabbable by a reader and
    // report its current FF effect capacity, which is what a rumble client uses.
    let mut effects: libc::c_int = 0;
    // SAFETY: EVIOCGEFFECTS writes one int into `effects`.
    let rc = unsafe { libc::ioctl(ev.as_raw_fd(), 0x8004_4584, &mut effects) };
    if rc == 0 {
        println!("EVIOCGEFFECTS: {effects} playable effects");
    } else {
        println!("EVIOCGEFFECTS failed: {}", std::io::Error::last_os_error());
    }

    println!("watching for EV_FF for 4s; play a rumble on the virtual pad now");
    let deadline = Instant::now() + Duration::from_secs(4);
    let mut saw_ff = 0usize;
    let mut saw_upload = 0usize;
    let mut buf = [0u8; 24];

    while Instant::now() < deadline {
        match ev.read(&mut buf) {
            Ok(24) => {
                let type_ = u16::from_le_bytes([buf[16], buf[17]]);
                let code = u16::from_le_bytes([buf[18], buf[19]]);
                let value = i32::from_le_bytes([buf[20], buf[21], buf[22], buf[23]]);
                if type_ == 0x15 {
                    saw_ff += 1;
                    println!("EV_FF code={code:#06x} value={value}");
                } else if type_ == 0x0101 {
                    saw_upload += 1;
                    println!("EV_UINPUT code={code} value={value} (effect upload/erase)");
                }
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => {
                println!("read error: {e}");
                break;
            }
        }
    }

    println!("EV_FF events: {saw_ff}, EV_UINPUT events: {saw_upload}");
    pad.destroy();
    println!("destroyed");

    // Silence unused warnings for the duplicated constants kept as ABI
    // documentation while the real implementation lives in `pad`.
    let _ = (
        UinputSetup {
            id: InputId {
                bustype: 3,
                vendor: 0,
                product: 0,
                version: 0,
            },
            name: [0u8; UINPUT_MAX_NAME_SIZE],
            ff_effects_max: 16,
        },
        UI_DEV_SETUP,
        UI_DEV_CREATE,
        UI_DEV_DESTROY,
        UI_SET_EVBIT,
        UI_SET_FFBIT,
    );
}

fn find_event_node(name: &str) -> Option<std::path::PathBuf> {
    for entry in std::fs::read_dir("/sys/class/input").ok()?.flatten() {
        let text = entry.file_name().to_string_lossy().to_string();
        if !text.starts_with("event") {
            continue;
        }
        if let Ok(device_name) = std::fs::read_to_string(entry.path().join("device/name")) {
            if device_name.trim() == name {
                return Some(std::path::Path::new("/dev/input").join(&text));
            }
        }
    }
    None
}

#[allow(dead_code)]
fn unused(_: File) {}
