//! Probe whether a uinput gamepad can declare force feedback.
//!
//! UHID goes through the HID stack, where force feedback needs the
//! `hid-universal-pidff` driver to bind to a PID descriptor. That driver is a
//! module which is not loaded here, so every UHID variant reported
//! `capabilities/ff: 0` and Steam would have nothing to send rumble to.
//!
//! uinput is a different path: it declares `FF_RUMBLE` directly on the evdev
//! node, with no HID PID layer involved. This probe checks whether that
//! produces a usable force-feedback device, and whether the extended button
//! range survives.
//!
//! Needs write access to `/dev/uinput`, which the current account has via ACL.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;

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

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct InputEvent {
    time: libc::timeval,
    type_: u16,
    code: u16,
    value: i32,
}

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_ABS: u16 = 0x03;
const EV_FF: u16 = 0x15;

const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;
const ABS_Z: u16 = 0x02;
const ABS_RX: u16 = 0x03;
const ABS_RY: u16 = 0x04;
const ABS_RZ: u16 = 0x05;
const ABS_HAT0X: u16 = 0x10;
const ABS_HAT0Y: u16 = 0x11;

const BTN_SOUTH: u16 = 0x130;
const BTN_TRIGGER_HAPPY1: u16 = 0x2c0;

// FF_RUMBLE = 0x50 per linux/input.h (0x52 is FF_CONSTANT).
const FF_RUMBLE: u16 = 0x50;

const UI_SET_EVBIT: libc::c_ulong = 0x4004_5564;
const UI_SET_KEYBIT: libc::c_ulong = 0x4004_5565;
const UI_SET_ABSBIT: libc::c_ulong = 0x4004_5567;
const UI_SET_FFBIT: libc::c_ulong = 0x4004_556b;
// _IOW('U', 3, struct uinput_setup), sizeof == 92
const UI_DEV_SETUP: libc::c_ulong = 0x405c_5503;
const UI_DEV_CREATE: libc::c_ulong = 0x5501;
const UI_DEV_DESTROY: libc::c_ulong = 0x5502;

fn ioctl_int(file: &File, request: libc::c_ulong, value: libc::c_int) -> std::io::Result<()> {
    // SAFETY: `request` is a UI_SET_* ioctl that takes an int argument and the
    // fd is valid for the borrow.
    let rc = unsafe { libc::ioctl(file.as_raw_fd(), request, value) };
    if rc < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn main() {
    let name = "Vader3P uinput FF probe";
    let file = match OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open("/dev/uinput")
    {
        Ok(f) => f,
        Err(e) => {
            println!("cannot open /dev/uinput: {e}");
            return;
        }
    };

    // Declare the event types we will emit.
    for ev in [EV_SYN, EV_KEY, EV_ABS, EV_FF] {
        if let Err(e) = ioctl_int(&file, UI_SET_EVBIT, ev.into()) {
            println!("UI_SET_EVBIT({ev:#x}) failed: {e}");
            return;
        }
    }

    // Standard gamepad buttons plus the six extras.
    for btn in BTN_SOUTH..=BTN_SOUTH + 14 {
        let _ = ioctl_int(&file, UI_SET_KEYBIT, btn.into());
    }
    for i in 0..6u16 {
        let _ = ioctl_int(&file, UI_SET_KEYBIT, (BTN_TRIGGER_HAPPY1 + i).into());
    }

    // Four absolute axes plus a hat.
    for abs in [ABS_X, ABS_Y, ABS_Z, ABS_RX, ABS_RY, ABS_RZ, ABS_HAT0X, ABS_HAT0Y] {
        let _ = ioctl_int(&file, UI_SET_ABSBIT, abs.into());
    }

    // The critical bit: FF_RUMBLE on the evdev node, which is what Steam needs.
    if let Err(e) = ioctl_int(&file, UI_SET_FFBIT, FF_RUMBLE.into()) {
        println!("UI_SET_FFBIT(FF_RUMBLE) failed: {e}");
        return;
    }
    println!("FF_RUMBLE declared on the uinput device");

    let mut setup = UinputSetup {
        id: InputId {
            bustype: 0x03, // BUS_USB
            vendor: 0x1d6b,
            product: 0x0001,
            version: 0x0100,
        },
        name: [0u8; UINPUT_MAX_NAME_SIZE],
        ff_effects_max: 16,
    };
    let bytes = name.as_bytes();
    let n = bytes.len().min(UINPUT_MAX_NAME_SIZE - 1);
    setup.name[..n].copy_from_slice(&bytes[..n]);

    // SAFETY: `setup` is a correctly laid out `uinput_setup` and outlives the call.
    let rc = unsafe { libc::ioctl(file.as_raw_fd(), UI_DEV_SETUP, &setup) };
    if rc < 0 {
        println!("UI_DEV_SETUP failed: {}", std::io::Error::last_os_error());
        return;
    }
    // SAFETY: creating the device takes no argument.
    let rc = unsafe { libc::ioctl(file.as_raw_fd(), UI_DEV_CREATE) };
    if rc < 0 {
        println!("UI_DEV_CREATE failed: {}", std::io::Error::last_os_error());
        return;
    }
    println!("device created: {name}");

    std::thread::sleep(std::time::Duration::from_millis(400));

    match find_node(name) {
        Some(sys) => {
            for cap in ["ev", "key", "abs", "ff"] {
                let f = sys.join("device/capabilities").join(cap);
                match std::fs::read_to_string(&f) {
                    Ok(t) => println!("capabilities/{cap}: {}", t.trim()),
                    Err(_) => println!("capabilities/{cap}: (absent)"),
                }
            }
            if let Ok(t) = std::fs::read_to_string(sys.join("device/capabilities/ff")) {
                println!("ff bits set: {}", count_bits(&t));
            }
            if let Ok(t) = std::fs::read_to_string(sys.join("device/capabilities/key")) {
                println!("key bits set: {}", count_bits(&t));
            }
            if let Ok(t) = std::fs::read_to_string(sys.join("device/capabilities/abs")) {
                println!("abs bits set: {}", count_bits(&t));
            }
        }
        None => println!("event node not found"),
    }

    std::io::stdout().flush().ok();
    std::thread::sleep(std::time::Duration::from_millis(200));

    // SAFETY: destroying the device takes no argument.
    unsafe {
        libc::ioctl(file.as_raw_fd(), UI_DEV_DESTROY);
    }
    println!("destroyed");
}

fn find_node(name: &str) -> Option<std::path::PathBuf> {
    for entry in std::fs::read_dir("/sys/class/input").ok()?.flatten() {
        let text = entry.file_name().to_string_lossy().to_string();
        if !text.starts_with("event") {
            continue;
        }
        if let Ok(device_name) = std::fs::read_to_string(entry.path().join("device/name")) {
            if device_name.trim() == name {
                return Some(entry.path());
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

#[allow(dead_code)]
fn emit(file: &mut File, type_: u16, code: u16, value: i32) {
    let ev = InputEvent {
        time: libc::timeval {
            tv_sec: 0,
            tv_usec: 0,
        },
        type_,
        code,
        value,
    };
    // SAFETY: writing the `input_event` struct bytes as a plain buffer.
    let buf = unsafe {
        std::slice::from_raw_parts(
            (&ev as *const InputEvent).cast::<u8>(),
            std::mem::size_of::<InputEvent>(),
        )
    };
    let _ = file.write_all(buf);
}

#[allow(dead_code)]
const _: () = {
    assert!(std::mem::size_of::<InputEvent>() == 24);
    assert!(std::mem::size_of::<UinputSetup>() == 92);
};
