# vader3p-steam crate

See the [project README](../README.md) for build, permissions, configuration,
limitations and Steam visibility. This daemon supports Bluetooth DInput and
presents one independent Bluetooth DualSense Edge UHID device for each
eligible connected controller.

## Compatibility requirements

The daemon is pre-1.0. Source builds require Rust 1.75 or later. The published
x86-64 Linux binary is built on Ubuntu 22.04 and requires glibc 2.35 or later
and the D-Bus runtime library.

## Layout

- `config.rs`: settings, compatibility selector and validation.
- `device.rs`, `input.rs`: target discovery and ordinary physical HID input.
- `ble_worker.rs`, `nus_input.rs`, `motion.rs`: Bluetooth NUS lifecycle and
  experimental input/motion decoding.
- `uhid_runtime.rs`, `edge.rs`, `edge_report.rs`: lifecycle, Edge UHID reports,
  host feedback and stale-state handling.
- `rumble.rs`: Bluetooth NUS frames; no USB fallback.
- `suppress.rs`: reversible evdev grabs, not device removal.

The manager waits when no eligible controller is connected and detects later
connections. On controller loss, its session requests removal of that virtual
pad; a reconnect starts a fresh pad with the same identity. Other sessions
continue independently. Physical multi-controller behavior still needs hardware
acceptance. The `examples/` diagnostics are not proof of physical-device or Steam
behavior.
Historical generic-uinput measurements do not measure this implementation.

Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test --all-targets`, and `cargo build --release` from this directory.
