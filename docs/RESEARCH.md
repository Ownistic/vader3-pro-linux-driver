# Research ledger

Status: **initial source triage, two Bluetooth descriptors, and seven coordinated
passive DInput input captures covering the full standard control map**. No output
commands have been sent and no Steam behavior has been verified.
See [input-capture evidence](evidence/passive-dinput-abcz.md). Protocol facts and independent code
are kept separate; no upstream/proprietary implementation has been incorporated.

## Initial comparison

| Source / inspected revision | Source-level observation | Limitation / next step |
| --- | --- | --- |
| [ahungry/vader3](https://github.com/ahungry/vader3/tree/360cc0942337f42f2fcd92e796835a7d2ae92b47) | `hid-vader3/src/hid-vader3.c`: Bluetooth `d7d7:0041`, standard input and extra buttons mapped to `BTN_TRIGGER_HAPPY`; GPL-3.0 | Kernel driver is not an acceptable deployment. Header assertion that hardware lacks FF in this mode is not proof across modes/firmware. Compare its mapping against real captures. |
| [SDL Flydigi source](https://github.com/libsdl-org/SDL/blob/e00dcdc9a58484850195c6797d6f65dffbd57f30/src/joystick/hidapi/SDL_hidapi_flydigi.c) | `HIDAPI_DriverFlydigi_UpdateDeviceIdentity`: IDs 80/81 mean Vader 3 Pro; V1 state handler has M1–M4 and C/Z plus motion; feedback handler emits V1 command `05 0f`; zlib-style source license | V1 device selection requires interface 2. Wired/dongle evidence is not Bluetooth support. Gyro scales are explicitly estimates. SDL support is not Steam editor compatibility. |
| [padctl](https://github.com/BANANASJIM/padctl/tree/049953d0e5588266333af3a91e8e783d82cdd544) | Actual Zig source has hidraw, `DeviceIO`, declarative mappings, generic UHID, a native DualSense Edge USB protocol path, separate IMU output, and feedback routing. `LICENSE` explicitly grants LGPL-2.1-or-later | Credible preferred candidate, not a proven definitions-only solution. Bluetooth discovery/grab details and Steam recognition still need validation. See source details below. |
| [padctl #482](https://github.com/BANANASJIM/padctl/issues/482) | Open Vader 3 Pro request, needs-info label; issue body asks for support | Body contains no descriptor/protocol evidence. Full comment/history investigation remains open. |
| [flydigi-vader5](https://github.com/BANANASJIM/flydigi-vader5/tree/5d8671db056f786d0c708f5df30482167ef6e6ff) | `docs/protocol.md`, `src/gamepad.cpp`: Vader 5 Pro receiver protocol, IMU/rumble implementation; GPL-2.0 | Different model and 2.4 GHz transport; not a Vader 3 Bluetooth specification. Initial source triage only. |
| [Space Station macOS](https://github.com/uiltonlopes/flydigi-space-station-mac/tree/649b9f36cbba80deb7e7ac11309615d44f382a8e) | `docs/protocol.md` and `FlydigiKit/Sources/flydigi-probe/main.swift`: Apex 4/newer-generation protocol leads and Vader 5 USB observations; MIT | Not a verified Vader 3 Bluetooth implementation. Do not transfer handshake/heartbeat assumptions without evidence. |
| [steam-devices](https://github.com/ValveSoftware/steam-devices/tree/22ec85e5ff5ea2e15c56d71a41bcbef46356cd60) | `60-steam-input.rules` and `60-steam-vr.rules`: permissions/wakeup configuration; MIT | Not Steam's controller decoder or a specification proving virtual identity recognition. |

SDL file read from current main; GitHub's path-specific commits endpoint returned
the pinned last-change revision above. Other source pins refer to the initial
inspected versions, not a promise that upstream remains unchanged. License labels
are initial observations, not a complete legal compatibility assessment.

## padctl: direct code findings

All references below are at `049953d0e5588266333af3a91e8e783d82cdd544`.
External source checkout: `/tmp/opencode/flydigi/upstream/padctl/`.

- `src/io/device_io.zig:3–39`: interface for read, write, feature report, pollfd,
  close; physical transport and report processing are separable.
- `src/io/hidraw.zig:60–152`: discovers hidraw devices by VID/PID with optional
  interface constraint using `HIDIOCGRAWINFO`; opens nodes RDWR/nonblocking.
  There is no USB-only bus rejection in the inspected discovery functions.
  **Inference:** existing hidraw transport may read Bluetooth reports without a
  new BlueZ backend if Linux exposes the needed endpoint. Not hardware-verified.
- `src/io/hidraw.zig:154–217`: associated evdev grabbing derives a physical-path
  prefix through `stripInputSuffix`. Verify Bluetooth physical-path topology and
  hidraw visibility separately; evdev grabbing alone does not prove Steam cannot
  also open the physical hidraw endpoint.
- `src/core/state.zig:3–63`: canonical state includes independent M1–M4/C/Z,
  gyro/accelerometer fields, sticks, triggers, touch contacts, and a u64 button
  bitmap. Internal representation is not the apparent six-extra bottleneck.
- `src/device_instance.zig:391–527`: `backend="uhid"` and
  `protocol="dualsense-edge-usb"` select a native codec/runtime carrying motion
  in the primary report. Generic UHID uses generated descriptors and can create
  a separate IMU device. These are distinct paths, not interchangeable proof of
  native Steam gyro support.
- `src/io/uhid_descriptor.zig:1–25, 103–108, 458–506`: generic gamepad descriptor,
  vendor-defined rumble output, and separate Generic Desktop multi-axis IMU
  descriptor builder. The descriptor comments' Steam compatibility claims are
  upstream assertions; the required six-extra Steam experiment has not run.
- `src/device_instance.zig:530–554, 2016–2026`: PID feedback forwarding and an
  explicit non-PID UHID evdev-rumble caveat. HIDAPI feedback and evdev feedback
  must be tested separately; neither establishes a physical Bluetooth command.
- `devices/flydigi/vader4-pro.toml:1–90`: declarative report fields, extra-button
  mapping, and generic output. Different device identity; its own split gyro-Y
  comment describes incomplete mapping. It must not be renamed to Vader 3
  Bluetooth and considered supported.
- `LICENSE:4–7`: LGPL version 2.1 or later, despite GitHub API's NOASSERTION label.

**Current assessment:** start by evaluating a padctl extension rather than
building another daemon. Definitions can represent the button state and many
fields; no evidence yet proves that definitions alone satisfy Bluetooth
initialization, duplicate suppression, native six-extra Steam recognition, or
motion/feedback. A new transport backend is **not yet shown necessary**; a
native virtual protocol or target-specific parsing/control changes may be.
No architecture selection has been finalized.

## Official Space Station 3.4.4.3

See [acquisition record](evidence/installer-3.4.4.3.md). The supplied CDN URL
returned HTTP 200 and the 105741288-byte installer was acquired. Its SHA-256 is
`f6e480b75016288797080ba5257aa8cbc46ebc724401ee841680f8c755ca0d50`.

REA `inspect_pe_resources` observed an x86 PE32 container with 18 resources.
Evidence: `ev_e63ebd351d469ebc6798b6b5a6f6525d1bad34c56e91ff1517678afeb1407b1a`.
This inspection establishes container identity/resource layout, **not** managed
protocol methods, signature validity, or hardware behavior. No native target was
opened, so no native analysis session requires closing. Evidence bundle retained
outside Git at `/tmp/opencode/flydigi/rea-evidence-setup.json`.

7-Zip 26.05 identifies an Inno Setup installer with product-version resource
3.4.4.3 and a certificate table. The official web page returned HTTP 429.
Current-page linkage, trusted publisher verification, extraction, and relevant
.NET method/wire-command tracing remain unresolved.

## Lowest-risk next experiment

1. Preserve the [first Xbox-compatible Bluetooth descriptor](evidence/bluetooth-xbox-mode.md),
   [DInput descriptor/GATT comparison](evidence/bluetooth-dinput-mode.md), and
   [first A/B/C/Z capture](evidence/passive-dinput-abcz.md). Continue M1–M4,
   combined-input and standard-control captures before selecting a parser.
   Owner-authorized read-only ACL enabled the first capture; recheck scoped
   access after reconnection rather than broadening privileges.
2. In parallel with static analysis, evaluate padctl's native protocol options
   against six-extra/motion requirements; do not treat generic evdev or separate
   sensor enumeration as satisfying Steam.
3. Build only the smallest justified virtual report/control exchange and test
   the Steam editor in both modes. Preserve each extra independently and
   simultaneously. No candidate currently passes this experiment.
4. Continue unaffected input work if Bluetooth rumble remains unresolved.
