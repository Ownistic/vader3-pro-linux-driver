# Testing

This guide lists repeatable software checks and a small manual hardware smoke
test. Passing software checks does not establish controller or Steam behavior.

## Offline checks

Run the Rust checks from `driver/`:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
```

Run the Python tests from the repository root:

```sh
python3 -m unittest discover -s tests -v
```

These commands test the source revision in the working tree. A local result is
not a remote CI result and does not validate hardware behavior.

## Manual smoke test

Use a controller and Linux host that you are authorized to test. Connect the
controller in Bluetooth DInput mode and configure its address before starting
the daemon. Follow the device-permission guidance in the [README](../README.md).

1. Build and start the daemon in the foreground.
2. Confirm that the virtual DualSense Edge appears.
3. Check standard buttons, sticks, triggers and the D-pad in a local input test.
4. If available, check C, Z and M1–M4 independently.
5. In Steam Game Mode, highlight a game without launching it. Check a short FN
   tap and a two-second hold. Each press should open only the Quick Access Menu,
   without launching the game, repeating or opening the main Steam menu.
6. Stop the daemon with Ctrl+C and confirm that it exits.

Record the tested source revision, binary, host and controller details with any
result. Distinguish software output from an observed controller or Steam response.
A successful transport write alone does not prove that the controller acted on
it. Do not run uncharacterized commands or change firmware as part of this test.

Motion is experimental and disabled by default. Enabling it changes the
controller's selector, which the daemon does not restore on exit. If ordinary
HID input is absent after disabling motion, fully power-cycle and reconnect the
controller; restarting Steam alone does not reset the selector. Calibration,
physical rumble, motion accuracy, reconnect behavior and end-to-end latency
require separate hardware testing.
