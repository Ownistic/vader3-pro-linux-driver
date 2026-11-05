# Vader 3 Pro Linux Driver

This pre-1.0 userspace daemon translates a Flydigi Vader 3 Pro in Bluetooth
DInput mode into a virtual DualSense Edge via `/dev/uhid`. Linux needs the
`hid-playstation` driver. USB and the 2.4 GHz dongle are not supported product
transports. The daemon does not install kernel modules or change Steam settings.

The owner reported that all controls except gyro Z worked on the earlier v4
test build. Steam visibility was confirmed with only the emulated DualSense Edge
listed after a separate physical-device ignore setting. Those observations do
not validate this build. Sensor calibration, reconnect, latency, and end-to-end
rumble remain hardware acceptance gates. See [v4 evidence](docs/evidence/uhid-runtime-retest-2026-11-04.md)
and [testing](docs/TESTING.md). Historical uinput measurements do not establish
UHID performance. Full PRD acceptance is not claimed.

## Build

If you are upgrading an existing installation, follow the
[migration guide](docs/uhid-migration.md) before replacing its executable or
starting a second daemon.

Install Rust 1.75 or later, pkg-config, and libdbus development headers
(`libdbus-1-dev` on Debian/Ubuntu, `dbus-devel` on Fedora, `dbus` on Arch).
From this checkout:

```sh
cd driver
cargo build --release
install -Dm755 target/release/vader3p-steam ~/.local/bin/vader3p-steam
```

## Device permissions

The user running the daemon needs read/write access to `/dev/uhid`, read access
to the target physical hidraw node, and access sufficient to open and grab the
target physical evdev nodes. Identify the actual nodes and IDs first using
`python3 tools/inventory.py --descriptor` from the repository root and sysfs;
the observed Bluetooth identity has included `04b4:2412`, not only `d7d7:0041`.
Bus 5 alone does not prove a physical Bluetooth link. Inspect the matched
descriptor and node relationships before granting access.

For a local session, an administrator can grant a temporary ACL on each verified
node (replace the placeholders with the actual device nodes):

```sh
sudo setfacl -m "u:$USER:rw" /dev/uhid
sudo setfacl -m "u:$USER:r" /dev/hidrawN
sudo setfacl -m "u:$USER:r" /dev/input/eventN
getfacl /dev/uhid /dev/hidrawN /dev/input/eventN
```

Grant each associated physical event node separately. Device nodes can change on
reconnect; recheck identity and ACLs then. For persistent access, have an
administrator configure narrowly scoped udev/logind ACL rules for the actual
controller and `/dev/uhid`, respecting local security policy. Do not make nodes
world-writable or disable SELinux. Access to `/dev/uhid` permits creating virtual
HID devices and should only be granted to trusted users.

## Configure and run

Connect the controller in Bluetooth DInput mode. Create
`~/.config/vader3p-steam/config.toml` (or pass `--config PATH`):

```toml
bluetooth_address = "AA:BB:CC:DD:EE:FF" # use the actual address
rumble = true
suppress_physical = true
log_level = "info"

[motion]
enabled = false # opt in only when experimental motion is desired
scale = 1
stale_ms = 100
swap_xy = true
invert_x = true
invert_y = true
```

Find the address privately with `bluetoothctl devices Connected`. An address is
required whenever rumble or motion is enabled; rumble defaults to true. For
input only without an address, explicitly set `rumble = false` and leave motion
disabled. `virtual_name` defaults to `Flydigi Vader 3 Pro (UHID DualSense Edge)`;
`poll_ms` defaults to 4 but is accepted only for compatibility and has no effect;
`log_level` defaults to `info`. Existing configurations with
`[pad] backend = "uhid"` remain valid; explicit `uinput` is rejected. The
`vendor_id` and `product_id` fields remain compatibility metadata; discovery
uses the known target identity or recorded descriptor, not arbitrary configured
IDs. A descriptor-matched device on bus 5 may still be a forwarded dongle.

Run `~/.local/bin/vader3p-steam`, then stop with Ctrl+C. With motion disabled,
ordinary 21-byte physical HID reports supply input. When motion is enabled,
the experimental NUS stream supplies canonical input once observed; stale or
lost NUS state is neutralized. Motion uses two uncalibrated values: by default
the second maps to inverted X (pitch) and the first to inverted Y (yaw).
`invert_x` and `invert_y` independently control those emulated-axis signs after
swapping. The unknown third value is zero and acceleration is fabricated, not
measured. `scale` is a nonzero signed
i16, and `stale_ms` must be 1–1000. Selector write success does not prove device
behavior; status and notification counts are observed separately.

The daemon does not restore the motion selector on exit. If the controller is
still streaming NUS, ordinary 21-byte HID input may be absent after disabling
motion or returning to native use. A full controller power cycle and reconnect
cleared the stream in the recorded firmware 6.7.5.1 test; check input after doing
so. Restarting Steam alone does not reset the controller selector.

Host feedback is forwarded only through Bluetooth NUS, not a USB hidraw sink.
The Bluetooth rumble frame is exactly 4 bytes: `11 <left> <right> <duration_ms>`.
Check logs for errors, and confirm actual motor response rather than assuming a
successful write proves vibration.

`suppress_physical = true` uses a reversible EVIOCGRAB on physical event nodes;
it blocks evdev events but does not remove the device or prevent hidraw access.
Steam may still list both devices. For the optional, manual Steam-only ignore
setting and its reversal, see [controller visibility](docs/steam-controller-visibility.md).
The daemon never edits Steam configuration or binds/unbinds kernel drivers.

## Upgrade from the former uinput setup

The current daemon creates only the virtual DualSense Edge through UHID. It does
not need the old SDL mapping or uinput service. Follow the [migration guide](docs/uhid-migration.md)
before replacing an existing installation. The guide identifies the former
artifact paths, verifies ownership before changing them, and does not recreate or
enable a service automatically.

For a new installation, first run the daemon in the foreground and confirm that
the controller works. The daemon requires the controller to be connected at
startup. If you then want a user service, use the optional unit and steps in the
[migration guide](docs/uhid-migration.md). Do not enable it until its
configuration and device permissions work in the foreground.

## Development and evidence

```sh
cd driver
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
cd ..
python3 -m unittest discover -s tests -v
```

See [architecture](docs/ARCHITECTURE.md), [protocol](docs/PROTOCOL.md),
[testing](docs/TESTING.md), [evidence](docs/evidence/), and [PRD](PRD.md).
The BLE and UHID examples are diagnostics, not hardware acceptance tests.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
Contributions are dual-licensed under the same terms unless explicitly stated
otherwise. Proprietary firmware and applications used for research are not
included. This project is not affiliated with Flydigi or Valve.
