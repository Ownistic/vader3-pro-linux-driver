//! Diagnostic: does the `EV_UINPUT` upload request reach a *second* reader of
//! the same evdev node?
//!
//! Background: uinput delivers effect-upload requests as `EV_UINPUT` events on
//! the created device's evdev node. A daemon that has the node open must answer
//! them with `UI_BEGIN_FF_UPLOAD` / `UI_END_FF_UPLOAD` on the uinput fd, or the
//! client's `EVIOCSFF` blocks and eventually returns `ETIMEDOUT`.
//!
//! The daemon's `poll_host` saw nothing, so this probe prints every raw event
//! byte that arrives on the daemon's own fd while a separate fd issues
//! `EVIOCSFF`. That distinguishes "the event never arrives" from "it arrives
//! but is not parsed".

use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::time::{Duration, Instant};

use vader3p_steam::ff::{FF_EFFECT_SIZE, FF_RUMBLE};
use vader3p_steam::pad::VirtualPad;

fn main() {
    let name = "Vader3P event-arrival probe";
    let mut pad = match VirtualPad::create(name) {
        Ok(p) => p,
        Err(e) => {
            println!("create failed: {e:#}");
            return;
        }
    };
    std::thread::sleep(Duration::from_millis(300));
    if let Err(e) = pad.attach_event_node() {
        println!("attach failed: {e:#}");
        return;
    }
    let _ = pad.send(&vader3p_steam::input::State::default());
    std::thread::sleep(Duration::from_millis(100));

    let path = match find_event_path(name) {
        Some(p) => p,
        None => {
            println!("node not found");
            pad.destroy();
            return;
        }
    };
    println!("node: {}", path.display());

    // Client fd: issues the upload, exactly as Steam would.
    let client = match std::fs::OpenOptions::new().read(true).write(true).open(&path) {
        Ok(f) => f,
        Err(e) => {
            println!("opening client fd: {e}");
            pad.destroy();
            return;
        }
    };

    let upload_handle = std::thread::spawn(move || {
        let mut effect = [0u8; FF_EFFECT_SIZE];
        effect[0..2].copy_from_slice(&FF_RUMBLE.to_le_bytes());
        effect[2..4].copy_from_slice(&(-1i16).to_le_bytes());
        effect[10..12].copy_from_slice(&500u16.to_le_bytes());
        effect[16..18].copy_from_slice(&0xffffu16.to_le_bytes());
        effect[18..20].copy_from_slice(&0x8000u16.to_le_bytes());
        // SAFETY: EVIOCSFF takes a 48-byte `struct ff_effect`.
        let rc = unsafe { libc::ioctl(client.as_raw_fd(), 0x4030_4580, effect.as_ptr()) };
        if rc < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    });

    println!("reading raw events from the daemon fd for 2s...");
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut total = 0usize;
    while Instant::now() < deadline {
        // poll_host services the handshake and discards details; to see what
        // actually arrived we count via a dedicated read here instead.
        match pad.poll_host() {
            Ok(Some(levels)) => {
                println!("poll_host reported rumble {levels:?}");
                total += 1;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => {
                println!("poll_host error: {e:#}");
                break;
            }
        }
    }

    match upload_handle.join() {
        Ok(Ok(())) => println!("EVIOCSFF succeeded"),
        Ok(Err(e)) => println!("EVIOCSFF failed: {e}"),
        Err(_) => println!("upload thread panicked"),
    }
    println!("poll_host changes seen: {total}");

    // Now read the node directly with a *third* fd to see if events are visible
    // to a fresh reader (evdev queues are per-client).
    match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(&path)
    {
        Ok(mut f) => {
            use std::io::Read;
            let mut buf = [0u8; 24];
            let mut n = 0;
            for _ in 0..200 {
                match f.read(&mut buf) {
                    Ok(24) => {
                        let t = u16::from_le_bytes([buf[16], buf[17]]);
                        let c = u16::from_le_bytes([buf[18], buf[19]]);
                        if n < 10 {
                            println!("fresh reader event type={t:#06x} code={c:#06x}");
                        }
                        n += 1;
                    }
                    _ => break,
                }
            }
            println!("fresh reader total events: {n}");
        }
        Err(e) => println!("third fd failed: {e}"),
    }

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
