//! Full-pipeline check: create the pad through the library, attach the event
//! node, and verify the force-feedback handshake round-trips.
//!
//! This is the check that matters for A7 (Steam rumble reaching the hardware):
//! it proves the daemon can accept an effect upload without blocking, which is
//! the prerequisite for receiving any rumble at all.
//!
//! A self-driven upload is issued through the same ioctl path a client such as
//! Steam would use, so the handshake is exercised end to end.

use std::os::unix::io::AsRawFd;
use std::time::Duration;

use vader3p_steam::ff::{FF_EFFECT_SIZE, FF_RUMBLE};
use vader3p_steam::pad::VirtualPad;

fn main() {
    let name = "Vader3P handshake probe";
    let mut pad = match VirtualPad::create(name) {
        Ok(p) => p,
        Err(e) => {
            println!("create failed: {e:#}");
            return;
        }
    };
    println!("pad created");

    std::thread::sleep(Duration::from_millis(300));

    // Push a neutral frame so the device is live.
    if let Err(e) = pad.send(&vader3p_steam::input::State::default()) {
        println!("send failed: {e:#}");
    }
    std::thread::sleep(Duration::from_millis(100));

    // Ask the kernel for the effect capacity; this is what a rumble client uses
    // to decide whether the device can play effects at all.
    let mut effects: libc::c_int = 0;
    let path = find_event_path(name);
    let Some(path) = path else {
        println!("event node not found");
        pad.destroy();
        return;
    };
    println!("event node: {}", path.display());

    let node = match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
    {
        Ok(f) => f,
        Err(e) => {
            println!("opening node: {e}");
            pad.destroy();
            return;
        }
    };

    // SAFETY: EVIOCGEFFECTS writes one int.
    let rc = unsafe { libc::ioctl(node.as_raw_fd(), 0x8004_4584, &mut effects) };
    println!("EVIOCGEFFECTS: rc={rc} capacity={effects}");

    // Upload a rumble effect exactly as a client would. The kernel will raise
    // EV_UINPUT/UI_FF_UPLOAD, which our poll_host must answer or this blocks.
    let mut effect = [0u8; FF_EFFECT_SIZE];
    effect[0..2].copy_from_slice(&FF_RUMBLE.to_le_bytes());
    effect[2..4].copy_from_slice(&(-1i16).to_le_bytes()); // id -1: assign one
    effect[10..12].copy_from_slice(&500u16.to_le_bytes()); // 500 ms
    effect[16..18].copy_from_slice(&0xffffu16.to_le_bytes()); // strong
    effect[18..20].copy_from_slice(&0x8000u16.to_le_bytes()); // weak

    println!("uploading a rumble effect (this blocks unless the handshake works)");

    // The upload ioctl blocks until answered, so run it on a side thread while
    // the main thread services poll_host.
    let handle = std::thread::spawn(move || {
        // SAFETY: EVIOCSFF takes a `struct ff_effect` (48 bytes), which the
        // byte array mirrors.
        // EVIOCSFF = _IOW('E', 0x80, struct ff_effect); sizeof(ff_effect)==48
        let rc = unsafe { libc::ioctl(node.as_raw_fd(), 0x4030_4580, effect.as_ptr()) };
        if rc < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(i32::from_le_bytes([effect[2], effect[3], 0, 0]))
        }
    });

    // Service the handshake for up to two seconds.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut serviced = 0usize;
    while std::time::Instant::now() < deadline && !handle.is_finished() {
        match pad.poll_host() {
            Ok(Some(levels)) => {
                println!("host rumble received: {levels:?}");
                serviced += 1;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => {
                println!("poll_host error: {e:#}");
                break;
            }
        }
    }

    match handle.join() {
        Ok(Ok(id)) => println!("upload returned (assigned id field={id})"),
        Ok(Err(e)) => println!("upload ioctl failed: {e}"),
        Err(_) => println!("upload thread panicked"),
    }
    println!("handshake events serviced: {serviced}");

    pad.destroy();
    println!("destroyed");
}

fn find_event_path(name: &str) -> Option<std::path::PathBuf> {
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
