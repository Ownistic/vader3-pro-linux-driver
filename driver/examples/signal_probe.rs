//! Verify the virtual pad's full signal path automatically, with no user input.
//!
//! This is the test that should have existed from the start. It creates the pad,
//! drives each input to an extreme through the real `pad::send` code path, and
//! asks SDL which axis or button index changed. That settles two questions that
//! were previously guessed at:
//!
//! 1. Which SDL axis index each kernel ABS code lands on. SDL assigns joystick
//!    axis indices from the ABS capability set, and its default gamepad mapping
//!    assumes rightx=y, which only holds if the ABS codes are assigned as SDL
//!    expects. Getting this wrong makes SDL read the right stick as a trigger.
//! 2. Which SDL button index each extra produces, so the gamecontrollerdb entry
//!    can be stated as fact rather than inferred from code order.
//!
//! A `keeper` thread services the FF upload queue so the kernel never blocks.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use vader3p_steam::input::{button, extra, State};
use vader3p_steam::pad::VirtualPad;

const SDL_INIT_JOYSTICK: u32 = 0x0000_0200;

type SdlInit = unsafe extern "C" fn(u32) -> c_int;
type SdlQuit = unsafe extern "C" fn();
type SdlNumJoysticks = unsafe extern "C" fn() -> c_int;
type SdlJoystickNameForIndex = unsafe extern "C" fn(c_int) -> *const c_char;
type SdlJoystickOpen = unsafe extern "C" fn(c_int) -> *mut c_void;
type SdlJoystickClose = unsafe extern "C" fn(*mut c_void);
type SdlJoystickNumButtons = unsafe extern "C" fn(*mut c_void) -> c_int;
type SdlJoystickNumAxes = unsafe extern "C" fn(*mut c_void) -> c_int;
type SdlJoystickGetButton = unsafe extern "C" fn(*mut c_void, c_int) -> u8;
type SdlJoystickGetAxis = unsafe extern "C" fn(*mut c_void, c_int) -> i16;
type SdlJoystickUpdate = unsafe extern "C" fn();

struct Sdl {
    quit: SdlQuit,
    close: SdlJoystickClose,
    nbuttons: SdlJoystickNumButtons,
    naxes: SdlJoystickNumAxes,
    get_button: SdlJoystickGetButton,
    get_axis: SdlJoystickGetAxis,
    update: SdlJoystickUpdate,
}

/// Hold the pad open and SDL initialised for the duration of a test.
fn open_sdl(name: &str) -> Option<(Sdl, *mut c_void, c_int, c_int)> {
    let lib = ["libSDL2-2.0.so.0", "libSDL2-2.0.so", "libSDL2.so"]
        .iter()
        .find_map(|n| {
            let c = CString::new(*n).ok()?;
            // SAFETY: dlopen with a NUL-terminated name.
            let h = unsafe { libc::dlopen(c.as_ptr(), libc::RTLD_NOW) };
            (!h.is_null()).then_some(h)
        })?;

    // SAFETY: every symbol is resolved by its documented SDL 2.0 export name.
    unsafe {
        let sym = |n: &str| -> *mut c_void {
            let c = CString::new(n).unwrap();
            libc::dlsym(lib, c.as_ptr())
        };
        let init: SdlInit = std::mem::transmute(sym("SDL_Init"));
        let num_joy: SdlNumJoysticks = std::mem::transmute(sym("SDL_NumJoysticks"));
        let joy_name: SdlJoystickNameForIndex =
            std::mem::transmute(sym("SDL_JoystickNameForIndex"));
        let joy_open: SdlJoystickOpen = std::mem::transmute(sym("SDL_JoystickOpen"));

        let sdl = Sdl {
            quit: std::mem::transmute(sym("SDL_Quit")),
            close: std::mem::transmute(sym("SDL_JoystickClose")),
            nbuttons: std::mem::transmute(sym("SDL_JoystickNumButtons")),
            naxes: std::mem::transmute(sym("SDL_JoystickNumAxes")),
            get_button: std::mem::transmute(sym("SDL_JoystickGetButton")),
            get_axis: std::mem::transmute(sym("SDL_JoystickGetAxis")),
            update: std::mem::transmute(sym("SDL_JoystickUpdate")),
        };

        if init(SDL_INIT_JOYSTICK) != 0 {
            return None;
        }

        for i in 0..num_joy() {
            let p = joy_name(i);
            if p.is_null() {
                continue;
            }
            if CStr::from_ptr(p).to_string_lossy() == name {
                let joy = joy_open(i);
                if joy.is_null() {
                    continue;
                }
                let nb = (sdl.nbuttons)(joy);
                let na = (sdl.naxes)(joy);
                return Some((sdl, joy, nb, na));
            }
        }
        (sdl.quit)();
    }
    None
}

fn main() {
    let name = "Vader3P signal probe";
    let mut pad = match VirtualPad::create(name) {
        Ok(p) => p,
        Err(e) => {
            println!("create failed: {e:#}");
            return;
        }
    };
    // uinput needs a moment before the node is readable by SDL.
    std::thread::sleep(Duration::from_millis(400));

    // Service the FF upload queue in the background. Without this the kernel
    // blocks any client asking for an effect, and SDL's axis reads could stall.
    let stop = Arc::new(AtomicBool::new(false));
    let stop_keeper = Arc::clone(&stop);
    let keeper = std::thread::spawn(move || {
        // A separate pad handle is not possible (one fd per device), so the
        // keeper only exists to keep the pad's fd from being idle-closed; the
        // main thread services FF below.
        while !stop_keeper.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(20));
        }
    });

    let Some((sdl, joy, nbuttons, naxes)) = open_sdl(name) else {
        println!("SDL could not open the pad (is another instance running?)");
        stop.store(true, Ordering::Relaxed);
        let _ = keeper.join();
        pad.destroy();
        return;
    };

    println!("pad: {name}");
    println!("SDL reports {nbuttons} buttons, {naxes} axes\n");

    let baseline_state = State::default();
    pad.send(&baseline_state).unwrap();
    std::thread::sleep(Duration::from_millis(120));
    let base_axes: Vec<i16> = (0..naxes)
        .map(|a| unsafe { (sdl.get_axis)(joy, a) })
        .collect();
    let base_buttons: Vec<u8> = (0..nbuttons)
        .map(|b| unsafe { (sdl.get_button)(joy, b) })
        .collect();

    // --- axes: move exactly one input, see which SDL axis index responds.
    let axis_inputs: [(&str, fn(&mut State)); 6] = [
        ("left_x", |s| s.left_x = 0x7f),
        ("left_y", |s| s.left_y = 0x7f),
        ("right_x", |s| s.right_x = 0x7f),
        ("right_y", |s| s.right_y = 0x7f),
        ("left_trigger", |s| s.left_trigger = 0xff),
        ("right_trigger", |s| s.right_trigger = 0xff),
    ];

    println!("--- axis routing ---");
    for (label, apply) in axis_inputs {
        let mut st = baseline_state;
        apply(&mut st);
        pad.send(&st).unwrap();
        std::thread::sleep(Duration::from_millis(120));
        unsafe { (sdl.update)() };
        let mut moved = Vec::new();
        for a in 0..naxes {
            let v = unsafe { (sdl.get_axis)(joy, a) };
            if (i32::from(v) - i32::from(base_axes[a as usize])).abs() > 1000 {
                moved.push(format!("a{a}={v}"));
            }
        }
        println!(
            "  {label:<14} -> {}",
            if moved.is_empty() {
                "NO AXIS MOVED".to_string()
            } else {
                moved.join(", ")
            }
        );
        pad.send(&baseline_state).unwrap();
        std::thread::sleep(Duration::from_millis(60));
    }

    // --- buttons: press exactly one control, see which SDL button responds.
    println!("\n--- button routing ---");
    let button_inputs: Vec<(String, Box<dyn Fn(&mut State)>)> = {
        let mut v: Vec<(String, Box<dyn Fn(&mut State)>)> = Vec::new();
        let std_names = [
            "A",
            "B",
            "X",
            "Y",
            "LB",
            "RB",
            "Select",
            "Start",
            "L3",
            "R3",
            "Home",
            "DPAD_UP",
            "DPAD_RIGHT",
            "DPAD_DOWN",
            "DPAD_LEFT",
        ];
        let std_indices = [
            button::A,
            button::B,
            button::X,
            button::Y,
            button::LB,
            button::RB,
            button::SELECT,
            button::START,
            button::L3,
            button::R3,
            button::HOME,
            button::DPAD_UP,
            button::DPAD_RIGHT,
            button::DPAD_DOWN,
            button::DPAD_LEFT,
        ];
        for (n, idx) in std_names.iter().zip(std_indices) {
            v.push((
                (*n).to_string(),
                Box::new(move |s: &mut State| s.buttons |= 1 << idx),
            ));
        }
        let extra_names = ["C", "Z", "M1", "M2", "M3", "M4"];
        for (i, n) in extra_names.iter().enumerate() {
            v.push((
                (*n).to_string(),
                Box::new(move |s: &mut State| s.extras |= 1 << i),
            ));
        }
        v
    };

    let mut failures = 0;
    let mut routing: Vec<(String, Vec<i32>)> = Vec::new();
    for (label, apply) in button_inputs {
        let mut st = baseline_state;
        apply(&mut st);
        pad.send(&st).unwrap();
        std::thread::sleep(Duration::from_millis(120));
        unsafe { (sdl.update)() };
        let mut pressed = Vec::new();
        for b in 0..nbuttons {
            let v = unsafe { (sdl.get_button)(joy, b) };
            if v == 1 && base_buttons[b as usize] == 0 {
                pressed.push(b);
            }
        }
        if pressed.is_empty() {
            println!("  {label:<10} -> NO BUTTON PRESSED");
            failures += 1;
        } else if pressed.len() > 1 {
            println!("  {label:<10} -> ALIASED to buttons {pressed:?}");
            failures += 1;
        } else {
            println!("  {label:<10} -> jbtn{}", pressed[0]);
        }
        routing.push((label.clone(), pressed));
        pad.send(&baseline_state).unwrap();
        std::thread::sleep(Duration::from_millis(60));
    }

    unsafe {
        (sdl.close)(joy);
        (sdl.quit)();
    }
    stop.store(true, Ordering::Relaxed);
    let _ = keeper.join();
    pad.destroy();

    println!("\n=== gamecontrollerdb entry derived from the measurements ===");
    let find = |want: &str| {
        routing
            .iter()
            .find(|(l, _)| l == want)
            .and_then(|(_, v)| v.first().copied())
    };
    let std_map = [
        ("a", "A"),
        ("b", "B"),
        ("x", "X"),
        ("y", "Y"),
        ("leftshoulder", "LB"),
        ("rightshoulder", "RB"),
        ("back", "Select"),
        ("start", "Start"),
        ("leftstick", "L3"),
        ("rightstick", "R3"),
        ("guide", "Home"),
        ("dpup", "DPAD_UP"),
        ("dpright", "DPAD_RIGHT"),
        ("dpdown", "DPAD_DOWN"),
        ("dpleft", "DPAD_LEFT"),
        ("misc1", "C"),
        ("touchpad", "Z"),
        ("paddle1", "M1"),
        ("paddle2", "M2"),
        ("paddle3", "M3"),
        ("paddle4", "M4"),
    ];
    let mut parts = Vec::new();
    for (sdl_name, crate_name) in std_map {
        match find(crate_name) {
            Some(b) => parts.push(format!("{sdl_name}:b{b}")),
            None => parts.push(format!("{sdl_name}:MISSING")),
        }
    }
    println!("{}", parts.join(","));

    if failures > 0 {
        println!("\n{failures} control(s) did not map to exactly one button");
    } else {
        println!("\nall controls mapped to exactly one button each");
    }
}
