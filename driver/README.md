# vader3p-steam crate

See the [project README](../README.md) for build, permissions, configuration,
limitations and Steam visibility. This daemon supports Bluetooth DInput and
presents one DualSense Edge UHID device.

## Layout

- `config.rs`: settings, compatibility selector and validation.
- `device.rs`, `input.rs`: target discovery and ordinary physical HID input.
- `ble_worker.rs`, `nus_input.rs`, `motion.rs`: Bluetooth NUS lifecycle and
  experimental input/motion decoding.
- `uhid_runtime.rs`, `edge.rs`, `edge_report.rs`: lifecycle, Edge UHID reports,
  host feedback and stale-state handling.
- `rumble.rs`: Bluetooth NUS frames; no USB fallback.
- `suppress.rs`: reversible evdev grabs, not device removal.

The UHID device survives physical link loss. The daemon neutralizes state on
disconnect and reattaches to the target; this behavior still needs hardware
acceptance. The `examples/` diagnostics are not proof of device behavior.
Historical generic-uinput measurements do not measure this implementation.

Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test --all-targets`, and `cargo build --release` from this directory.
