//! Ask SDL what it makes of the translated pad, using the same library Steam
//! links against.
//!
//! This answers the acceptance question that the kernel capability bitmaps
//! cannot: SDL is what Steam uses to open controllers, and SDL is what decides
//! how many buttons, axes and hats a device has. A device can expose 21 key
//! codes to the kernel and still be seen by SDL as a 15-button pad if SDL's
//! mapping does not cover the remaining usages.
//!
//! Local SDL2 is loaded at runtime through dlopen so the project needs no
//! dependency and no bindgen step.

use std::ffi::{c_char, c_int, c_void, CStr, CString};

// SDL constants.
const SDL_INIT_JOYSTICK: u32 = 0x0000_0200;
const SDL_INIT_GAMECONTROLLER: u32 = 0x0000_2000;

type SdlInit = unsafe extern "C" fn(u32) -> c_int;
type SdlQuit = unsafe extern "C" fn();
type SdlNumJoysticks = unsafe extern "C" fn() -> c_int;
type SdlJoystickNameForIndex = unsafe extern "C" fn(c_int) -> *const c_char;
type SdlJoystickOpen = unsafe extern "C" fn(c_int) -> *mut c_void;
type SdlJoystickClose = unsafe extern "C" fn(*mut c_void);
type SdlJoystickNumButtons = unsafe extern "C" fn(*mut c_void) -> c_int;
type SdlJoystickNumAxes = unsafe extern "C" fn(*mut c_void) -> c_int;
type SdlJoystickNumHats = unsafe extern "C" fn(*mut c_void) -> c_int;
type SdlJoystickInstanceId = unsafe extern "C" fn(*mut c_void) -> i32;
type SdlIsGameController = unsafe extern "C" fn(c_int) -> c_int;
type SdlGameControllerOpen = unsafe extern "C" fn(c_int) -> *mut c_void;
type SdlGameControllerNumButtons = unsafe extern "C" fn(*mut c_void) -> c_int;
type SdlGameControllerGetStringForButton =
    unsafe extern "C" fn(c_int) -> *const c_char;
type SdlGameControllerNumAxes = unsafe extern "C" fn(*mut c_void) -> c_int;
type SdlGameControllerGetStringForAxis = unsafe extern "C" fn(c_int) -> *const c_char;
type SdlJoystickGetButton = unsafe extern "C" fn(*mut c_void, c_int) -> u8;
#[repr(C)]
struct SdlGuid {
    data: [u8; 16],
}
type SdlJoystickGetDeviceGuid = unsafe extern "C" fn(c_int) -> SdlGuid;
/// `struct SDL_GameControllerButtonBind { SDL_GameControllerBindType bindType;
/// union { int button; int axis; struct { int axis; int axis_min; int axis_max; } hat; } value; }`
#[repr(C)]
#[derive(Clone, Copy)]
struct GcButtonBind {
    bind_type: c_int,
    /// Union of `int button`, `int axis`, or `struct {int axis; int axis_min;
    /// int axis_max;} hat`. The whole struct measures 12 bytes per SDL's own
    /// header, so the union is 8 bytes and the hat's min/max are packed into
    /// 16-bit halves of a single int.
    value: [c_int; 2],
}
type SdlGcGetBindForButton =
    unsafe extern "C" fn(*mut c_void, c_int) -> GcButtonBind;
type SdlJoystickUpdate = unsafe extern "C" fn();

fn main() {
    let lib = ["libSDL2-2.0.so.0", "libSDL2-2.0.so", "libSDL2.so"]
        .iter()
        .find_map(|name| {
            let c = CString::new(*name).ok()?;
            // SAFETY: dlopen with a NUL-terminated name.
            let h = unsafe { libc::dlopen(c.as_ptr(), libc::RTLD_NOW) };
            (!h.is_null()).then_some(h)
        });

    let Some(lib) = lib else {
        println!("SDL2 not found via dlopen");
        return;
    };

    // SAFETY: each symbol is looked up by its documented exported name and cast
    // to a signature matching SDL 2.0's public API.
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
        let joy_buttons: SdlJoystickNumButtons =
            std::mem::transmute(sym("SDL_JoystickNumButtons"));
        let joy_axes: SdlJoystickNumAxes = std::mem::transmute(sym("SDL_JoystickNumAxes"));
        let joy_hats: SdlJoystickNumHats = std::mem::transmute(sym("SDL_JoystickNumHats"));
        let joy_inst: SdlJoystickInstanceId =
            std::mem::transmute(sym("SDL_JoystickInstanceID"));
        let is_gc: SdlIsGameController = std::mem::transmute(sym("SDL_IsGameController"));
        let joy_update: SdlJoystickUpdate = std::mem::transmute(sym("SDL_JoystickUpdate"));
        let joy_guid: SdlJoystickGetDeviceGuid =
            std::mem::transmute(sym("SDL_JoystickGetDeviceGUID"));
        let joy_get_button: SdlJoystickGetButton =
            std::mem::transmute(sym("SDL_JoystickGetButton"));
        let gc_open: SdlGameControllerOpen =
            std::mem::transmute(sym("SDL_GameControllerOpen"));
        let gc_buttons: Option<SdlGameControllerNumButtons> =
            std::mem::transmute::<*mut c_void, Option<SdlGameControllerNumButtons>>(
                sym("SDL_GameControllerNumButtons"),
            );
        let gc_bind: SdlGcGetBindForButton =
            std::mem::transmute(sym("SDL_GameControllerGetBindForButton"));
        let gc_btn_name: Option<SdlGameControllerGetStringForButton> =
            std::mem::transmute::<*mut c_void, Option<SdlGameControllerGetStringForButton>>(
                sym("SDL_GameControllerGetStringForButton"),
            );
        let gc_axes: Option<SdlGameControllerNumAxes> =
            std::mem::transmute::<*mut c_void, Option<SdlGameControllerNumAxes>>(
                sym("SDL_GameControllerNumAxes"),
            );
        let gc_axis_name: Option<SdlGameControllerGetStringForAxis> =
            std::mem::transmute::<*mut c_void, Option<SdlGameControllerGetStringForAxis>>(
                sym("SDL_GameControllerGetStringForAxis"),
            );

        if init(SDL_INIT_JOYSTICK | SDL_INIT_GAMECONTROLLER) != 0 {
            println!("SDL_Init failed");
            return;
        }

        let count = num_joy();
        println!("SDL sees {count} joystick(s)\n");

        for i in 0..count {
            let name_ptr = joy_name(i);
            let name = if name_ptr.is_null() {
                "<null>".to_string()
            } else {
                CStr::from_ptr(name_ptr).to_string_lossy().into_owned()
            };
            let is_controller = is_gc(i) == 1;
            let guid = joy_guid(i);
            let guid_text: String = guid.data.iter().map(|b| format!("{b:02x}")).collect();
            println!("[{i}] {name}");
            println!("    GUID        : {guid_text}");
            println!("    game controller: {is_controller}");

            let joy = joy_open(i);
            if joy.is_null() {
                println!("    (could not open as joystick)\n");
                continue;
            }
            let joy_button_total = joy_buttons(joy);
            let joy_axis_total = joy_axes(joy);
            println!("    instance id : {}", joy_inst(joy));
            println!("    joystick    : {joy_button_total} buttons, {joy_axis_total} axes, {} hats",
                joy_hats(joy));
            joy_close(joy);

            if is_controller {
                let gc = gc_open(i);
                if !gc.is_null() {
                    // These query functions only exist in SDL 2.30+. Report what
                    // the linked build actually provides instead of guessing.
                    match (gc_buttons, gc_axes) {
                        (Some(nb), Some(na)) => {
                            println!("    controller  : {} buttons, {} axes", nb(gc), na(gc));
                        }
                        _ => println!(
                            "    controller  : per-button counts unavailable in this SDL build"
                        ),
                    }
                    // Real bindings. `SDL_GameControllerGetStringForButton`
                    // returns the enum's canonical name, NOT this device's
                    // mapping, so it must not be used to infer coverage.
                    // SDL_CONTROLLER_BUTTON_* enum values 0..=20.
                    const BUTTON_ENUM: [&str; 21] = [
                        "a", "b", "x", "y", "back", "guide", "start", "leftstick",
                        "rightstick", "leftshoulder", "rightshoulder", "dpup", "dpdown",
                        "dpleft", "dpright", "misc1", "paddle1", "paddle2", "paddle3",
                        "paddle4", "touchpad",
                    ];
                    let mut bound = Vec::new();
                    let mut unbound = Vec::new();
                    for (enum_value, label) in BUTTON_ENUM.iter().enumerate() {
                        let bind = gc_bind(gc, enum_value as c_int);
                        // 0 = BINDTYPE_NONE, 1 = BUTTON, 2 = AXIS, 3 = HAT
                        match bind.bind_type {
                            1 => bound.push(format!("{label}=jbtn{}", bind.value[0])),
                            2 => bound.push(format!("{label}=axis{}", bind.value[0])),
                            3 => {
                                // value[0] = hat axis, value[1] packs min/max
                                // as two 16-bit halves (low = min, high = max).
                                let packed = bind.value[1] as u32;
                                let min = packed as i16;
                                let max = (packed >> 16) as i16;
                                bound.push(format!(
                                    "{label}=hat{}[{min}..{max}]",
                                    bind.value[0]
                                ));
                            }
                            _ => unbound.push(*label),
                        }
                    }
                    println!("    bound       : {}", bound.join(", "));
                    println!("    UNBOUND     : {}", if unbound.is_empty() {
                        "(none)".to_string()
                    } else {
                        unbound.join(", ")
                    });
                    if let Some(name_of) = gc_axis_name {
                        let mut axes = Vec::new();
                        for a in 0..joy_axis_total {
                            let p = name_of(a);
                            if p.is_null() {
                                axes.push(format!("a{a}=UNMAPPED"));
                            } else {
                                axes.push(format!(
                                    "a{a}={}",
                                    CStr::from_ptr(p).to_string_lossy()
                                ));
                            }
                        }
                        println!("    axis map    : {}", axes.join(", "));
                    }
                }
            }
            println!();
        }

        // Does a grabbed physical node still deliver events to SDL? That
        // decides whether the duplicate is functional or only cosmetic.
        println!("\n--- reading events for 15 s: press A a few times ---");
        let mut handles = Vec::new();
        let mut nbtn = 0;
        for i in 0..count {
            let h = joy_open(i);
            if !h.is_null() && i == count - 1 {
                nbtn = joy_buttons(h);
            }
            handles.push(h);
        }
        let start = std::time::Instant::now();
        let mut previous: Vec<Vec<u8>> = vec![vec![0; 32]; handles.len()];
        let mut events = 0usize;
        while start.elapsed() < std::time::Duration::from_secs(15) {
            joy_update();
            for (i, h) in handles.iter().enumerate() {
                if h.is_null() {
                    continue;
                }
                for b in 0..nbtn {
                    let now = joy_get_button(*h, b);
                    if now != previous[i][b as usize] {
                        previous[i][b as usize] = now;
                        if now == 1 {
                            events += 1;
                            println!("    [{i}] jbtn{b} PRESSED");
                        }
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(4));
        }
        println!("total press events: {events}");

        quit();
    }
}
