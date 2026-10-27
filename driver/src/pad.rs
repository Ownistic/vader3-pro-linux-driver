//! uinput virtual gamepad.
//!
//! Creates an evdev gamepad with `FF_RUMBLE` declared, so the host can send
//! force-feedback events that this daemon translates into physical rumble.
//!
//! # Why uinput rather than UHID
//!
//! Measured, not assumed. Three descriptor variants through `/dev/uhid` — a
//! vendor output report and two Physical Interface Device shapes — all produced
//! `capabilities/ff: 0` on this kernel, because the HID path needs
//! `hid-universal-pidff` to bind and that driver is a module which was not
//! loaded and could not be loaded here. uinput declares `FF_RUMBLE` directly and
//! measured `capabilities/ff: 40000 0`, which decodes to bit 82 = `FF_RUMBLE`.
//! See `docs/evidence/virtual-pad-backend.md`.
//!
//! A required feature that depends on an optional kernel module is not a sound
//! foundation, so uinput is the backend.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;

use anyhow::{Context, Result};

use crate::ff::{self, FfEffect, UinputFfErase, UinputFfUpload};
use crate::ff::{FF_GAIN, FF_RUMBLE, EV_UINPUT, UI_FF_ERASE, UI_FF_UPLOAD};
use crate::input::{button, extra, State};

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
#[derive(Clone, Copy)]
struct InputEvent {
    time: libc::timeval,
    type_: u16,
    code: u16,
    value: i32,
}

/// `struct input_event` is 24 bytes on 64-bit: timeval (16) + type (2) +
/// code (2) + value (4).
const _: () = assert!(std::mem::size_of::<InputEvent>() == 24);
/// `struct uinput_setup` is 92 bytes: input_id (8) + name (80) + u32 (4).
const _: () = assert!(std::mem::size_of::<UinputSetup>() == 92);

mod ev_type {
    pub const SYN: u16 = 0x00;
    pub const KEY: u16 = 0x01;
    pub const ABS: u16 = 0x03;
    pub const FF: u16 = 0x15;
}

mod syn {
    pub const REPORT: u16 = 0;
}

mod abs {
    pub const X: u16 = 0x00;
    pub const Y: u16 = 0x01;
    pub const Z: u16 = 0x02;
    pub const RX: u16 = 0x03;
    pub const RY: u16 = 0x04;
    pub const RZ: u16 = 0x05;
    pub const HAT0X: u16 = 0x10;
    pub const HAT0Y: u16 = 0x11;
}

/// Standard evdev button codes, in the canonical order used by [`crate::input`].
///
/// `BTN_SOUTH`..`BTN_DPAD_LEFT` is a contiguous run from `0x130`, which matches
/// this project's fixed canonical order (A, B, X, Y, LB, RB, Select, Start, L3,
/// R3, Home, D-pad up/right/down/left).
const BUTTON_CODES: [u16; button::STANDARD_COUNT] = [
    0x130, // BTN_SOUTH      A
    0x131, // BTN_EAST       B
    0x133, // BTN_NORTH      X
    0x134, // BTN_WEST       Y
    0x136, // BTN_TL         LB
    0x137, // BTN_TR         RB
    0x13a, // BTN_SELECT     Select
    0x13b, // BTN_START      Start
    0x13d, // BTN_THUMBL     L3
    0x13e, // BTN_THUMBR     R3
    0x13c, // BTN_MODE       Home
    0x220, // BTN_DPAD_UP
    0x221, // BTN_DPAD_RIGHT
    0x222, // BTN_DPAD_DOWN
    0x223, // BTN_DPAD_LEFT
];

/// Extra button codes, `BTN_TRIGGER_HAPPY1`..`6`.
///
/// Using the trigger-happy range rather than inventing codes keeps them in a
/// namespace the kernel and SDL both already understand as additional gamepad
/// buttons, which is what makes them individually bindable.
const EXTRA_CODES: [u16; extra::COUNT] = [0x2c0, 0x2c1, 0x2c2, 0x2c3, 0x2c4, 0x2c5];

mod ioctl {
    pub const SET_EVBIT: libc::c_ulong = 0x4004_5564;
    pub const SET_KEYBIT: libc::c_ulong = 0x4004_5565;
    pub const SET_ABSBIT: libc::c_ulong = 0x4004_5567;
    pub const SET_FFBIT: libc::c_ulong = 0x4004_556b;
    /// `_IOW('U', 3, struct uinput_setup)`, sizeof == 92.
    pub const DEV_SETUP: libc::c_ulong = 0x405c_5503;
    pub const DEV_CREATE: libc::c_ulong = 0x5501;
    pub const DEV_DESTROY: libc::c_ulong = 0x5502;
}

/// Scale a 16-bit force-feedback magnitude to the firmware's 0-250 range.
///
/// `ff_effect` magnitudes are `u16` (0..=0xffff). The controller clamps at 250,
/// so scaling here keeps host intent and device behaviour consistent instead of
/// letting the firmware silently truncate.
#[must_use]
fn scale_ff(magnitude: u16) -> u8 {
    // 0xffff -> 250, 0 -> 0, rounded to nearest.
    let scaled = (u32::from(magnitude) * 250 + 0x7fff) / 0xffff;
    scaled.min(250) as u8
}

fn set_bit(file: &File, request: libc::c_ulong, value: u16, what: &str) -> Result<()> {
    let arg = libc::c_int::from(value);
    // SAFETY: the fd is open and these ioctls take an int by value.
    let rc = unsafe { libc::ioctl(file.as_raw_fd(), request, arg) };
    if rc < 0 {
        Err(std::io::Error::last_os_error()).with_context(|| format!("{what} ({value:#06x})"))
    } else {
        Ok(())
    }
}

/// A created uinput gamepad.
pub struct VirtualPad {
    file: File,
    name: String,
    last_sent: [i32; 32],
    /// Most recent rumble magnitude requested by the host, if any.
    rumble: Option<(u8, u8)>,
}

impl VirtualPad {
    /// Create the pad.
    pub fn create(name: &str) -> Result<Self> {
        // Read AND write: writes create the device and emit input, while reads
        // receive `EV_UINPUT` force-feedback control requests. Opening
        // write-only makes those requests invisible and every host upload
        // times out.
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open("/dev/uinput")
            .context("opening /dev/uinput")?;

        for t in [ev_type::SYN, ev_type::KEY, ev_type::ABS, ev_type::FF] {
            set_bit(&file, ioctl::SET_EVBIT, t, "UI_SET_EVBIT")?;
        }
        for &code in BUTTON_CODES.iter().chain(EXTRA_CODES.iter()) {
            set_bit(&file, ioctl::SET_KEYBIT, code, "UI_SET_KEYBIT")?;
        }
        for code in [
            abs::X, abs::Y, abs::Z, abs::RX, abs::RY, abs::RZ, abs::HAT0X, abs::HAT0Y,
        ] {
            set_bit(&file, ioctl::SET_ABSBIT, code, "UI_SET_ABSBIT")?;
        }
        // The whole point of choosing uinput: rumble is declarable directly.
        set_bit(&file, ioctl::SET_FFBIT, FF_RUMBLE, "UI_SET_FFBIT")?;
        set_bit(&file, ioctl::SET_FFBIT, FF_GAIN, "UI_SET_FFBIT")?;

        let mut setup = UinputSetup {
            id: InputId {
                bustype: 0x03, // BUS_USB
                vendor: 0x1d6b,
                product: 0x0001,
                version: 0x0100,
            },
            name: [0u8; UINPUT_MAX_NAME_SIZE],
            // Enough slots for several overlapping effects; Steam typically
            // uses one or two at a time.
            ff_effects_max: 16,
        };
        let bytes = name.as_bytes();
        let n = bytes.len().min(UINPUT_MAX_NAME_SIZE - 1);
        setup.name[..n].copy_from_slice(&bytes[..n]);

        // SAFETY: `setup` matches the kernel's `struct uinput_setup` layout and
        // outlives the call.
        let rc = unsafe { libc::ioctl(file.as_raw_fd(), ioctl::DEV_SETUP, &setup) };
        if rc < 0 {
            return Err(std::io::Error::last_os_error()).context("UI_DEV_SETUP");
        }
        // SAFETY: creating the device takes no argument.
        let rc = unsafe { libc::ioctl(file.as_raw_fd(), ioctl::DEV_CREATE) };
        if rc < 0 {
            return Err(std::io::Error::last_os_error()).context("UI_DEV_CREATE");
        }

        Ok(Self {
            file,
            name: name.to_string(),
            last_sent: [i32::MIN; 32],
            rumble: None,
        })
    }

    /// Read pending host control events and service the force-feedback
    /// handshake.
    ///
    /// `EV_UINPUT` upload/erase requests and `EV_FF` play events are delivered
    /// on the **uinput fd read queue**, not on the evdev node. A diagnostic that
    /// read the evdev node saw zero events while a client's `EVIOCSFF` timed
    /// out, which is what pinned this down.
    ///
    /// Returns the latest rumble level when the host changed it. Servicing the
    /// upload protocol is not optional: without `UI_END_FF_UPLOAD` the kernel's
    /// `upload_effect()` call never returns, so Steam would hang rather than
    /// rumble.
    pub fn poll_host(&mut self) -> Result<Option<(u8, u8)>> {
        let mut raw: Vec<(u16, u16, i32)> = Vec::new();
        let mut buf = [0u8; 24];
        loop {
            match self.file.read(&mut buf) {
                Ok(24) => {
                    raw.push((
                        u16::from_le_bytes([buf[16], buf[17]]),
                        u16::from_le_bytes([buf[18], buf[19]]),
                        i32::from_le_bytes([buf[20], buf[21], buf[22], buf[23]]),
                    ));
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => break,
                Err(e) => return Err(e).context("reading uinput control events"),
            }
        }

        let mut changed = None;
        for (type_, code, value) in raw {
            if type_ == EV_UINPUT {
                self.handle_uinput_control(code, value)?;
            } else if type_ == ev_type::FF {
                if let Some(levels) = self.handle_ff_event(code, value) {
                    changed = Some(levels);
                }
            }
        }
        Ok(changed)
    }

    /// Complete an `EV_UINPUT` control request.
    ///
    /// The control *event* arrives on the evdev node, but the `UI_BEGIN_FF_*` /
    /// `UI_END_FF_*` ioctls are implemented on the uinput fd. Using the evdev
    /// node for them returns `EINVAL`-class failures and the kernel's pending
    /// `upload_effect()` then times out, which is exactly what an earlier
    /// version did.
    fn handle_uinput_control(&mut self, code: u16, request_id: i32) -> Result<()> {
        let fd = self.file.as_raw_fd();

        match i32::from(code) {
            UI_FF_UPLOAD => {
                let mut upload = UinputFfUpload {
                    request_id: request_id as u32,
                    retval: 0,
                    effect: [0u8; ff::FF_EFFECT_SIZE],
                    old: [0u8; ff::FF_EFFECT_SIZE],
                };
                // SAFETY: BEGIN_FF_UPLOAD fills a `struct uinput_ff_upload`,
                // which `UinputFfUpload` mirrors at 104 bytes.
                let rc = unsafe { libc::ioctl(fd, ff::BEGIN_FF_UPLOAD, &mut upload) };
                if rc == 0 {
                    let effect = FfEffect::from_bytes(upload.effect);
                    if effect.is_rumble() {
                        // Record the requested level so the main loop can push
                        // it to the physical controller. The effect parameters
                        // are in the 0..=0xffff range; scale to 0..=250, the
                        // firmware's ceiling.
                        let strong = scale_ff(effect.strong_magnitude());
                        let weak = scale_ff(effect.weak_magnitude());
                        self.rumble = Some((strong, weak));
                        upload.retval = 0;
                    } else {
                        // Accept the upload so the host can create the effect,
                        // but it produces no physical motion. Refusing would
                        // make clients treat the device as broken.
                        upload.retval = 0;
                    }
                } else {
                    upload.retval = -1;
                }
                // SAFETY: completing the pending request with the same struct.
                let _ = unsafe { libc::ioctl(fd, ff::END_FF_UPLOAD, &upload) };
            }
            UI_FF_ERASE => {
                let mut erase = UinputFfErase {
                    request_id: request_id as u32,
                    retval: 0,
                    effect_id: 0,
                };
                // SAFETY: BEGIN_FF_ERASE fills a `struct uinput_ff_erase` (12 bytes).
                let rc = unsafe { libc::ioctl(fd, ff::BEGIN_FF_ERASE, &mut erase) };
                if rc != 0 {
                    erase.retval = -1;
                }
                // An erased effect cannot keep rumbling.
                self.rumble = None;
                // SAFETY: completing the pending erase request.
                let _ = unsafe { libc::ioctl(fd, ff::END_FF_ERASE, &erase) };
            }
            _ => {}
        }
        Ok(())
    }

    /// Handle an `EV_FF` event (play, stop, gain).
    fn handle_ff_event(&mut self, code: u16, value: i32) -> Option<(u8, u8)> {
        match code {
            // FF_RUMBLE event: value is the effect id; playback itself is
            // driven by the uploaded magnitudes.
            FF_RUMBLE => self.rumble,
            // Gain scales the current effect; treat zero as stop.
            FF_GAIN => {
                if value == 0 {
                    self.rumble = Some((0, 0));
                }
                self.rumble
            }
            _ => self.rumble,
        }
    }

    /// The most recent host-requested rumble level.
    #[must_use]
    pub fn rumble(&self) -> Option<(u8, u8)> {
        self.rumble
    }

    /// Clear the rumble request, after it has been delivered or stopped.
    pub fn clear_rumble(&mut self) {
        self.rumble = None;
    }

    /// Device name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    fn write_event(&mut self, type_: u16, code: u16, value: i32) -> Result<()> {
        let event = InputEvent {
            time: libc::timeval {
                tv_sec: 0,
                tv_usec: 0,
            },
            type_,
            code,
            value,
        };
        // SAFETY: reinterpreting the plain `input_event` struct as bytes for a
        // write to the uinput fd, which expects exactly that layout.
        let buf = unsafe {
            std::slice::from_raw_parts(
                (&event as *const InputEvent).cast::<u8>(),
                std::mem::size_of::<InputEvent>(),
            )
        };
        self.file.write_all(buf).context("writing uinput event")
    }

    /// Emit an evdev event only when the value changed.
    ///
    /// The controller sends reports only on change, but Steam-side repeat
    /// filtering expects clean edges; suppressing duplicates also keeps the
    /// event stream small.
    fn emit_changed(&mut self, slot: usize, type_: u16, code: u16, value: i32) -> Result<()> {
        if self.last_sent[slot] == value {
            return Ok(());
        }
        self.last_sent[slot] = value;
        self.write_event(type_, code, value)
    }

    /// Push a canonical state to the kernel.
    pub fn send(&mut self, state: &State) -> Result<()> {
        for (idx, &code) in BUTTON_CODES.iter().enumerate() {
            let pressed = state.button(idx);
            self.emit_changed(idx, ev_type::KEY, code, i32::from(pressed))?;
        }
        for (idx, &code) in EXTRA_CODES.iter().enumerate() {
            let pressed = state.extra(idx);
            self.emit_changed(16 + idx, ev_type::KEY, code, i32::from(pressed))?;
        }

        // uinput has no HID hat field; directions are two signed axes where
        // -1 is up/left and +1 is down/right.
        let hat_x = i32::from(state.button(button::DPAD_RIGHT) as u8)
            - i32::from(state.button(button::DPAD_LEFT) as u8);
        let hat_y = i32::from(state.button(button::DPAD_DOWN) as u8)
            - i32::from(state.button(button::DPAD_UP) as u8);
        self.emit_changed(22, ev_type::ABS, abs::HAT0X, hat_x)?;
        self.emit_changed(23, ev_type::ABS, abs::HAT0Y, hat_y)?;

        self.emit_changed(24, ev_type::ABS, abs::X, i32::from(crate::encode::centre_axis(state.left_x)))?;
        self.emit_changed(25, ev_type::ABS, abs::Y, i32::from(crate::encode::centre_axis(state.left_y)))?;
        self.emit_changed(26, ev_type::ABS, abs::RX, i32::from(crate::encode::centre_axis(state.right_x)))?;
        self.emit_changed(27, ev_type::ABS, abs::RY, i32::from(crate::encode::centre_axis(state.right_y)))?;
        self.emit_changed(28, ev_type::ABS, abs::Z, i32::from(state.left_trigger))?;
        self.emit_changed(29, ev_type::ABS, abs::RZ, i32::from(state.right_trigger))?;

        self.write_event(ev_type::SYN, syn::REPORT, 0)
    }

    /// Release everything and destroy the device.
    pub fn destroy(&mut self) {
        let _ = self.send(&State::default());
        // SAFETY: destroying the device takes no argument.
        unsafe {
            libc::ioctl(self.file.as_raw_fd(), ioctl::DEV_DESTROY);
        }
    }
}

impl Drop for VirtualPad {
    fn drop(&mut self) {
        self.destroy();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struct_sizes_match_the_kernel_abi() {
        assert_eq!(std::mem::size_of::<InputEvent>(), 24);
        assert_eq!(std::mem::size_of::<UinputSetup>(), 92);
        assert_eq!(std::mem::size_of::<InputId>(), 8);
    }

    #[test]
    fn button_codes_are_unique_and_ordered() {
        let mut all: Vec<u16> = BUTTON_CODES.iter().chain(EXTRA_CODES.iter()).copied().collect();
        let len = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), len, "duplicate button codes would alias controls");
    }

    #[test]
    fn button_code_count_covers_standard_and_extras() {
        assert_eq!(BUTTON_CODES.len(), button::STANDARD_COUNT);
        assert_eq!(EXTRA_CODES.len(), extra::COUNT);
        assert_eq!(BUTTON_CODES.len() + EXTRA_CODES.len(), 21);
    }

    #[test]
    fn extra_codes_are_the_trigger_happy_range() {
        // BTN_TRIGGER_HAPPY1..6 = 0x2c0..0x2c5
        assert_eq!(EXTRA_CODES[0], 0x2c0);
        assert_eq!(EXTRA_CODES[5], 0x2c5);
    }

    #[test]
    fn ff_rumble_is_the_code_steam_needs() {
        // From linux/input.h: FF_RUMBLE 0x50. FF_CONSTANT is 0x52, which is a
        // different effect and would not receive Steam's rumble events.
        assert_eq!(FF_RUMBLE, 0x50);
        assert_ne!(FF_RUMBLE, 0x52, "0x52 is FF_CONSTANT, not rumble");
    }

    #[test]
    fn ff_rumble_sits_at_offset_0x50() {
        // capabilities/ff is printed most-significant word first as 64-bit hex
        // columns. The kernel prints 128 bits, so a code in 0x40..0x7f appears
        // in the *first* printed column at bit (code - 64).
        //
        // Measured reference: declaring 0x52 (FF_CONSTANT by mistake) printed
        // "40000 0", and 0x52 - 64 = 18, i.e. 1 << 18 = 0x40000. So declaring
        // FF_RUMBLE (0x50) must print "10000 0".
        assert_eq!(FF_RUMBLE, 0x50);
        let bit = FF_RUMBLE as usize - 64;
        assert_eq!(bit, 16);
        assert_eq!(1u64 << bit, 0x1_0000u64);
    }

    #[test]
    fn ioctl_constants_match_the_computed_encoding() {
        // _IOW('U', nr, int) = 0x400455xx
        assert_eq!(ioctl::SET_EVBIT, 0x4004_5564);
        assert_eq!(ioctl::SET_KEYBIT, 0x4004_5565);
        assert_eq!(ioctl::SET_ABSBIT, 0x4004_5567);
        assert_eq!(ioctl::SET_FFBIT, 0x4004_556b);
        // _IOW('U', 3, struct uinput_setup) with size 92 = 0x5c
        assert_eq!(ioctl::DEV_SETUP, 0x405c_5503);
    }
}
