# Testing

This guide lists repeatable software checks, a neutral virtual-kernel probe, and
a small manual hardware smoke test. Passing software checks does not establish
physical-controller or Steam behavior.

## Offline checks

Run the Rust checks from `driver/`:

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked --all-targets
cargo build --release --locked
```

The manifest sets Rust 1.75 as the minimum supported version. The committed
`Cargo.lock` keeps dependency resolution compatible with that minimum. If Rust
1.75 is already installed through rustup, you can check it with:

```sh
cargo +1.75 test --locked --all-targets
```

This optional command checks the minimum toolchain; it does not install Rust or
replace the default checks above.

Run the Python tests from the repository root:

```sh
python3 -m unittest discover -s tests -v
```

These commands test the source revision in the working tree. A local result is
not a remote CI result and does not validate hardware behavior.

## Manual smoke test

Use a controller and Linux host that you are authorized to test. Connect the
controller in Bluetooth DInput mode through the operating system. The daemon
does not pair or connect controllers. No address configuration is required.
Without a `bluetooth_address` filter, the daemon discovers all eligible
connected controllers and also detects connections made after startup. Follow
the device-permission guidance in the [README](../README.md).

1. Build and start the daemon in the foreground.
2. Confirm that a separate virtual DualSense Edge appears for each connected,
   eligible controller.
3. Check standard buttons, sticks, triggers and the D-pad for each pad in a
   local input test.
4. If available, check C, Z and M1–M4 independently on each pad.
5. In Steam Game Mode, highlight a game without launching it. Check a short FN
   tap and a two-second hold. Each press should open only the Quick Access Menu,
   without launching the game, repeating or opening the main Steam menu.
6. Disconnect one controller and verify only its virtual pad is requested for
   removal. Steam's device-list refresh is not guaranteed by that request.
7. Reconnect that controller and verify a fresh virtual pad appears. Confirm
   that the other controller continues to work.
8. Stop the daemon with Ctrl+C and confirm that it exits.

Record the tested source revision, binary, host and controller details with any
result. Distinguish software output from an observed controller or Steam response.
A successful transport write alone does not prove that the controller acted on
it. Do not run uncharacterized commands or change firmware as part of this test.

Motion is experimental and disabled by default. Enabling it changes the
controller's selector, which the daemon does not restore on exit. If ordinary
HID input is absent after disabling motion, fully power-cycle and reconnect the
controller; restarting Steam alone does not reset the selector. Calibration,
physical rumble, motion accuracy, reconnect behavior, Steam UI refresh and
end-to-end latency require separate hardware testing.

## Neutral virtual-kernel probe

The following optional example creates synthetic virtual devices only. It opens
no physical controller or BlueZ connection, sends only neutral input, and
removes and recreates one of two pads during its 12-second run:

```sh
cargo run --manifest-path driver/Cargo.toml --release --locked --example uhid_bluetooth_probe
```

The devices can appear briefly in the kernel and desktop input lists. Do not run
this example during attended gameplay. Its kernel binding and raw-report output
do not verify Steam's UI, physical multi-controller discovery, NUS, rumble, or
gyro hardware behavior.

One development-kernel observation used synthetic identities on kernel
`7.3.1-1-cachyos`. Two virtual pads bound to `hid-playstation`; independent raw
input reads returned 78-byte report `31` with a valid CRC and neutral controls.
Removing pad A left pad B's same HID instance present. Recreating A used the
same virtual identity with a new HID instance, and both pads disappeared after
the probe exited. This is virtual-device and kernel evidence only, not Steam UI
or physical-controller validation.
