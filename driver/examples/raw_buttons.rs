//! Report which raw joystick button index each physical press produces.
//!
//! SDL's default controller mapping binds only the 15 standard buttons, so the
//! six extras exist as joystick buttons but are unbound as gamepad controls.
//! To write a mapping that binds them, the exact joystick index of each extra
//! must be known rather than assumed.
//!
//! Prints a transition log with timestamps, so the operator can be asked for one
//! key at a time and each answer checked before moving on.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::time::{Duration, Instant};

const SDL_INIT_JOYSTICK: u32 = 0x0000_0200;

type SdlInit = unsafe extern "C" fn(u32) -> c_int;
type SdlQuit = unsafe extern "C" fn();
type SdlNumJoysticks = unsafe extern "C" fn() -> c_int;
type SdlJoystickNameForIndex = unsafe extern "C" fn(c_int) -> *const c_char;
type SdlJoystickOpen = unsafe extern "C" fn(c_int) -> *mut c_void;
type SdlJoystickClose = unsafe extern "C" fn(*mut c_void);
type SdlJoystickNumButtons = unsafe extern "C" fn(*mut c_void) -> c_int;
type SdlJoystickGetButton = unsafe extern "C" fn(*mut c_void, c_int) -> u8;
type SdlJoystickGetDeviceGUID = unsafe extern "C" fn(c_int) -> SdlGuid;

#[repr(C)]
struct SdlGuid {
    data: [u8; 16],
}

fn main() {
    let lib = ["libSDL2-2.0.so.0", "libSDL2-2.0.so", "libSDL2.so"]
        .iter()
        .find_map(|name| {
            let c = CString::new(*name).ok()?;
            // SAFETY: dlopen with a NUL-terminated library name.
            let h = unsafe { libc::dlopen(c.as_ptr(), libc::RTLD_NOW) };
            (!h.is_null()).then_some(h)
        });
    let Some(lib) = lib else {
        println!("SDL2 not found");
        return;
    };

    // SAFETY: symbols are resolved by their documented SDL 2.0 names.
    unsafe {
        let sym = |name: &str| -> *mut c_void {
            let c = CString::new(name).unwrap();
            libc::dlsym(lib, c.as_ptr())
        };
        let init: SdlInit = std::mem::transmute(sym("SDL_Init"));
        let quit: SdlQuit = std::mem::transmute(sym("SDL_Quit"));
        let num_joy: SdlNumJoysticks = std::mem::transmute(sym("SDL_NumJoysticks"));
        let joy_name: SdlJoystickNameForIndex =
            std::mem::transmute(sym("SDL_JoystickNameForIndex"));
        let joy_open: SdlJoystickOpen = std::mem::transmute(sym("SDL_JoystickOpen"));
        let joy_close: SdlJoystickClose = std::mem::transmute(sym("SDL_JoystickClose"));
        let joy_nbtn: SdlJoystickNumButtons = std::mem::transmute(sym("SDL_JoystickNumButtons"));
        let get_button: SdlJoystickGetButton = std::mem::transmute(sym("SDL_JoystickGetButton"));
        let get_guid: SdlJoystickGetDeviceGUID =
            std::mem::transmute(sym("SDL_JoystickGetDeviceGUID"));

        if init(SDL_INIT_JOYSTICK) != 0 {
            println!("SDL_Init failed");
            return;
        }

        // Locate the translated pad by name so the physical node is not observed.
        let mut target = None;
        for i in 0..num_joy() {
            let p = joy_name(i);
            if p.is_null() {
                continue;
            }
            let name = CStr::from_ptr(p).to_string_lossy();
            if name.contains("translated") {
                target = Some(i);
                break;
            }
        }
        let Some(index) = target else {
            println!("translated pad not found; is the daemon running?");
            quit();
            return;
        };

        let guid = get_guid(index);
        println!(
            "GUID: {}",
            guid.data
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );

        let joy = joy_open(index);
        if joy.is_null() {
            println!("could not open joystick {index}");
            quit();
            return;
        }
        let nbtn = joy_nbtn(joy);
        println!("buttons: {nbtn}");
        println!();
        println!("Reading for 120 s. Press ONE control at a time.");
        println!("Each press prints the button index that changed.");
        println!();

        let mut previous = vec![0u8; nbtn as usize];
        let start = Instant::now();
        let mut last_report = Instant::now();
        while start.elapsed() < Duration::from_secs(120) {
            // SDL_JoystickUpdate is pumped via SDL_PumpEvents in a real app; for
            // the polling API SDL_JoystickGetButton reads current state, so a
            // short sleep is enough.
            for b in 0..nbtn {
                let now = get_button(joy, b);
                if now != previous[b as usize] {
                    previous[b as usize] = now;
                    // Timestamp relative to start, so presses can be ordered.
                    println!(
                        "t={:6.2}s  jbtn{:2} -> {}",
                        start.elapsed().as_secs_f32(),
                        b,
                        if now == 1 { "PRESSED" } else { "released" }
                    );
                    last_report = Instant::now();
                }
            }
            let _ = last_report;
            std::thread::sleep(Duration::from_millis(4));
        }

        joy_close(joy);
        quit();
    }
}
