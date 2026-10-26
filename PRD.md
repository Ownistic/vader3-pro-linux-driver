# Flydigi Vader 3 Pro: native Steam Input over Bluetooth on Linux

Status: requirements supplied by the project owner; initial setup authorized.

## 1. Objective

Deliver an independent, open-source, fully userspace implementation that connects
a Flydigi Vader 3 Pro over **Bluetooth** to native Steam Input on a Lenovo Legion
Go running Bazzite. Support Gaming Mode and Desktop Mode, including per-game
configuration in Steam's controller editor.

This is an implementation project, not research alone. Investigate, implement,
test, and deliver a usable result. When a requirement is blocked, deliver the
best working partial implementation and precise supporting evidence without
claiming full success.

## 2. Target environment and constraints

- Hardware: Flydigi Vader 3 Pro and Lenovo Legion Go; Bluetooth is mandatory.
- Platform: Bazzite (Fedora Atomic), BlueZ, Linux HID, Steam Gaming/Desktop Mode.
- Preferred data path: controller → Bluetooth HID → userspace protocol translator
  → virtual UHID controller → Steam Input.
- No custom kernel, DKMS module, patched OS image, or writes to immutable system
  partitions. Auxiliary Linux APIs may supplement, but not replace, UHID/native
  Steam controller integration.
- Prefer minimal dependencies and native Linux interfaces.
- Do not interfere with the Legion Go's built-in controller or unrelated devices.
- No permanent SELinux disablement, world-writable device nodes, or broad root
  privileges.
- No firmware changes, flashing, or permanent controller configuration changes
  without separate authorization.
- Never execute unknown Windows binaries outside an appropriately isolated
  environment. Prefer static extraction and analysis.

## 3. Functional requirements

### F1. Standard input

Support A/B/X/Y, D-pad, both sticks and stick clicks, both analog triggers,
LB/RB, Start/Select, Home, and other available system buttons. Preserve expected
ranges, neutral values, button transitions, and simultaneous inputs.

### F2. Six independent extra inputs

Expose **M1, M2, M3, M4, C, and Z** independently, including simultaneous presses.
Each must be individually assignable in Steam's controller configuration editor,
not a duplicate of a standard button.

Keyboard emulation, AntiMicroX mappings, generic evdev events, or four paddles
alone do not satisfy this requirement.

### F3. Native motion

Provide gyroscope and accelerometer data wherever the hardware exposes them.
Steam must recognize a controller sensor and offer native gyro configuration,
including gyro aiming. Mouse-motion emulation is not equivalent.

Investigate sensor initialization, report format, byte order, axis orientation,
units/scaling, calibration, timestamps, and sampling rate. Separate hardware
availability from implementation status.

### F4. Physical Bluetooth rumble

Forward Steam feedback to real controller vibration over Bluetooth. Investigate
HID output/feature reports, vendor commands, initialization and mode switches,
strength, duration, separate channels, and transport/firmware restrictions.

Existing reports that Bluetooth DInput rumble is unsupported are a research
lead, not proof of impossibility or solved support. Establish whether the
hardware can receive such commands. If genuinely unavailable, document the
technical evidence and continue other functionality; never fabricate feedback.

### F5. Lifecycle and safety

- Automatic scoped discovery, startup, disconnect/reconnect, and sleep/wakeup.
- No stuck inputs on loss of the physical controller; explicit neutralization
  and safe cleanup.
- Low-latency processing and minimal idle CPU usage.
- Configurable logging/debugging and recoverable malformed-report handling.
- No duplicate physical controller presented in Steam while translating it.
- Restrict any exclusive access/suppression to the verified target device.

## 4. Research existing implementations first

Inspect current source, not just README files, at:

1. https://github.com/ahungry/vader3
2. https://github.com/libsdl-org/SDL/blob/main/src/joystick/hidapi/SDL_hidapi_flydigi.c
3. https://github.com/BANANASJIM/padctl
4. https://github.com/BANANASJIM/padctl/issues/482
5. https://github.com/BANANASJIM/flydigi-vader5
6. https://github.com/uiltonlopes/flydigi-space-station-mac
7. https://github.com/ValveSoftware/steam-devices

Determine existing Bluetooth report knowledge; missing gyro/rumble support;
Steam-compatible identities, descriptors, and protocols; and whether padctl can
support this through definitions/plugins or needs a transport backend.

Prefer extending a maintained project if it satisfies requirements without
excessive architectural compromise. Assess padctl's Zig architecture, userspace
emulation, UHID, feedback handling, and immutable-distribution support. Choose a
standalone Rust daemon only when materially justified. Follow applicable
licenses; do not copy code before license review.

## 5. Official software acquisition and reverse engineering

Primary target: **Flydigi Space Station 3.4.4.3 for Windows**, supporting Vader 3
hardware through the older .NET/WPF architecture.

- Official page: https://shops.flydigi.com/pages/game-center
- Candidate official installer:
  https://tencent-android.cdn.flydigi.com/PC/Space%20Station3.0/FlydigiSpaceStation_setup_3.4.4.3_Release.exe
- Verify current availability and official provenance; record SHA-256 and check
  the digital signature where practical. Reachability alone is not authenticity.
- Store installers, extracted binaries, proprietary decompilation, and raw
  analysis artifacts outside version control.
- Use REA MCP as the primary static reverse-engineering interface, with ILSpy,
  Ghidra, or other suitable tools as supporting analyzers.
- Investigate 4.x only when it adds relevant evidence; do not silently replace
  the primary target.

Trace UI/command handlers to wire-level operations for identification, Bluetooth
detection/connection, DInput/XInput, descriptors/reports, extra buttons, motion,
rumble, initialization/negotiation, and relevant profile commands.

Document exact layouts, offsets, byte order, commands, responses, and sequence
requirements. Cite methods, assemblies/CIL locations, pseudocode references, or
captures. Separate verified observations, inferences, hypotheses, and unknowns.
Do not redistribute official binaries or substantial proprietary source. Build
an independent implementation from documented protocol facts. Unrelated
licensing systems, online services, and protected infrastructure are out of scope.

## 6. Physical Bluetooth investigation

Determine Bluetooth Classic HID versus BLE HID over GATT for each relevant mode.
Inspect real descriptors, bus/vendor/product IDs, input reports, output/feature
paths, and Linux device topology. Compare Bluetooth DInput/XInput modes without
assuming USB or receiver layouts apply.

Use bluetoothctl, btmon, hid-recorder, evtest, libevdev, hidraw, and BlueZ
diagnostics as appropriate. Produce a protocol specification and a mapping for
every button, axis, and motion field.

If hardware is unavailable, deliver executable diagnostic utilities and precise
Legion Go instructions. Store sanitized, permission-cleared captures as regression
fixtures; keep sensitive addresses and raw captures out of version control.

## 7. Steam Input architecture decision

Prioritize an early real Steam experiment: it is the highest integration risk.
Compare:

| Candidate | Investigation required |
| --- | --- |
| Virtual Flydigi Vader 3 Pro | Identity, descriptor, initialization, and report format Steam actually supports; translation from Bluetooth. VID/PID spoofing alone is insufficient. |
| Virtual Steam Deck or another supported controller | All six independent extras, native motion, feedback, and real Steam compatibility. |
| DualSense Edge / Xbox Elite | Independent control count, native gyro, rumble, and Steam editor behavior. Do not sacrifice C/Z or gyro merely to obtain four paddles. |

Select the strongest experimentally supported option, even if implementation
requires additional work. Linux enumeration and synthetic UHID success do not
prove Steam compatibility. Keep the decision open until sufficient evidence is
available; report an unavailable hardware experiment as blocked or untested.

## 8. Implementation boundaries

Separate these responsibilities:

1. Bluetooth device discovery and transport.
2. Physical report parsing.
3. Canonical controller state.
4. UHID virtual-device lifecycle.
5. Steam-compatible report encoding.
6. Motion decoding/calibration.
7. Rumble translation/forwarding.
8. Configuration, logging, and lifecycle management.

Use Rust for a justified standalone implementation, or follow padctl's existing
Zig architecture for an extension. Avoid overengineering the prototype. Do not
commit to a new driver or guessed protocol before upstream research.

## 9. Testing and acceptance

Automated tests must cover recorded/synthetic reports, all standard buttons,
each extra independently and simultaneously, analog ranges, motion decoding and
calibration/orientation, press/release ordering, disconnect/reconnect, rumble
translation, malformed input, and recovery. Clearly label synthetic versus real
fixtures; hypothetical protocol tests do not establish hardware behavior.

Run separate real-hardware acceptance suites for Gaming Mode and Desktop Mode:

| ID | Required outcome |
| --- | --- |
| A1 | Steam recognizes the intended virtual controller. |
| A2 | Physical controller does not appear as a duplicate. |
| A3 | All standard buttons and analog controls work as expected. |
| A4 | M1–M4 are individually assignable in Steam's editor. |
| A5 | C and Z are individually assignable in Steam's editor. |
| A6 | Native gyro settings and gyro aiming are available and functional. |
| A7 | Steam rumble reaches the hardware through Bluetooth. |
| A8 | Per-game configuration works. |
| A9 | Reconnection/sleep recovery needs no service restart and leaves no stuck input. |
| A10 | Built-in Legion Go controls and unrelated devices are unaffected. |
| A11 | Scoped permissions, startup, reboot, update, and uninstall behavior are reproducible on Bazzite. |

Report every requirement as **verified**, **partially verified**, **blocked**, or
**untested**, citing exact commands/actions, artifact versions, and observations.
Never claim a hardware test occurred when it did not. An unavailable required
check prevents full completion.

## 10. Bazzite delivery

Provide a reproducible install/update/uninstall method using userspace binaries,
persistent configuration, a systemd user service, scoped udev permissions, and
Fedora Atomic-compatible paths. Document diagnostic commands, recovery, and
behavior across reboot and OS updates where tested. Do not install active
device-suppression rules or enable a service before a safe translator is ready.

## 11. Milestones and durable outputs

| Phase | Outcome |
| --- | --- |
| 1. Research and feasibility | Upstream/source and Windows analysis, real Bluetooth investigation, rumble feasibility, justified architecture choice. |
| 2. Input prototype | Bluetooth parser and Steam-recognizable virtual controller; ordinary and extra-button validation. |
| 3. Steam Input integration | All six extras independently configurable in Steam. |
| 4. Motion | Native motion support and Steam validation. |
| 5. Rumble | Working Bluetooth vibration, or precise evidence-based limitation. |
| 6. Deployment and stabilization | Reconnection, permissions, services, regression tests, reproducible Bazzite deployment. |

Maintain `README.md`, `docs/RESEARCH.md`, `docs/PROTOCOL.md`,
`docs/ARCHITECTURE.md`, `docs/TESTING.md`, `PLAN.md`, and `PROGRESS.md`.
Track evidence, blockers, next actions, saved artifacts, and decisions so another
session can resume from disk. Initial setup should create this foundation and
safe diagnostic scaffolding, not falsely certify later milestones.

## 12. Final success definition

Success requires Bluetooth standard input, all six independent Steam-assignable
extras, native gyro, physical rumble, reliable reconnects, no custom kernel/image
changes, and reproducible installation on the target system. Any unmet item must
remain explicitly visible with its limitation, evidence, and remaining work.
