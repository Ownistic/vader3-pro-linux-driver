# Flydigi Vader 3 Pro for Linux

Use a Flydigi Vader 3 Pro with Steam Input on Linux. This experimental daemon
translates each Bluetooth DInput controller into a virtual DualSense Edge using
Linux UHID and `hid-playstation`. USB and the 2.4 GHz dongle are not supported.

## Capabilities and status

- Sticks, analog triggers, D-pad and standard buttons.
- Automatic discovery of eligible Bluetooth controllers, including controllers
  connected after startup. Each gets an independent virtual pad.
- Six independent extra buttons: C, Z and M1–M4.
- FN opens Steam's Quick Access Menu (the sidebar containing Decky) using an
  ordered Guide + A pulse.
- Rumble feedback forwarded over Bluetooth.
- Experimental Steam lightbar colors forwarded over Bluetooth. I observed the
  physical LEDs follow Steam color changes on my Legion Go.
- Experimental two-axis gyro input for pitch and yaw, disabled by default.
- Removal of a disconnected controller's virtual pad and recreation with the
  same identity on reconnect. Physical reconnect behavior remains unverified.

**Gyro Z (roll) does not work.** The third gyro component is not implemented.
Pitch and yaw are uncalibrated, and the virtual acceleration values are
fabricated. The current pitch correction and physical multi-controller behavior
have not been hardware-tested.

The physical controller can still appear in Steam. To hide it, follow the
optional [Steam visibility guide](docs/steam-controller-visibility.md). The daemon
does not edit Steam settings, install kernel modules, or unbind drivers.

## Install

### One-command installation

On x86-64 Linux, including Bazzite, install the latest published release as your
normal user:

```sh
curl -fsSL https://raw.githubusercontent.com/Ownistic/vader3-pro-linux-driver/main/install.sh | sh
```

The installer checks the SHA-256 checksum and installs the binary at
`~/.local/bin/vader3p-steam`. The checksum detects corruption; it is not an
independent signature. Review scripts before piping them to a shell. The command
requires `curl`, GNU coreutils, a published release and the D-Bus runtime library.
See the [compatibility requirements](driver/README.md#compatibility-requirements).
It does not grant device access, pair controllers, change configuration, install
packages, or start or enable a service.

The installer will not replace an existing executable unless you pass
`--replace`. First, read the [migration guide](docs/uhid-migration.md), stop
the verified old service, and back up the executable:

```sh
curl -fsSL https://raw.githubusercontent.com/Ownistic/vader3-pro-linux-driver/main/install.sh | sh -s -- --replace
```

The installer uses the latest non-prerelease GitHub release. Continue with
[device access](#device-access) and [configuration](#configure-and-run).

### Build from source

Prerequisites:

- Rust and `cargo`; see the [compatibility requirements](driver/README.md#compatibility-requirements)
- `pkg-config`, BlueZ and D-Bus development headers (`libdbus-1-dev` on
  Debian/Ubuntu or `dbus-devel` on Fedora)
- Linux kernel support for `/dev/uhid` and `hid-playstation`

Before replacing an existing installation, follow the
[migration guide](docs/uhid-migration.md).

Build and install from the repository root:

```sh
cargo build --manifest-path driver/Cargo.toml --release --locked
install -Dm755 driver/target/release/vader3p-steam ~/.local/bin/vader3p-steam
```

### Device access

Run the daemon as your normal user. It needs read/write access to `/dev/uhid`,
read access to each physical controller's hidraw node, and evdev access for
optional event grabs. `/dev/uhid` permits creation of virtual HID devices; grant
access only to trusted users. See the [device access guide](docs/device-access.md)
for Bazzite, other distributions, and permission checks after reconnecting.

### Configure and run

Pair and connect controllers in Bluetooth DInput mode through your operating
system. The daemon waits for eligible controllers and detects later connections;
it does not pair them.

No Bluetooth address is required. Leave `bluetooth_address` unset to use all
eligible controllers, or set it to a full address to select one. The filter is
validated even when rumble and motion are disabled.

Without `--config`, the daemon reads `~/.config/vader3p-steam/config.toml` (or
the path under `$XDG_CONFIG_HOME`). If the conventional file is absent, it uses
defaults. An explicit `--config PATH` must exist and be valid; the daemon does
not fall back to defaults if it is missing, unreadable or malformed.

To change settings, create `~/.config/vader3p-steam/config.toml`:

```sh
mkdir -p ~/.config/vader3p-steam
```

Add the settings you need, for example:

```toml
# Optional: restrict operation to one controller.
# bluetooth_address = "AA:BB:CC:DD:EE:FF"
rumble = true
led = true

[motion]
enabled = false
```

Set `enabled = true` to try experimental pitch/yaw input. For all options, see
the [configuration reference](ARCHITECTURE.md#configuration-reference).

Set `led = false` to disable host color forwarding. A host color can replace the
controller's configured lighting effect. The daemon cannot restore that effect
on exit. A full power cycle restored the original button-pulse effect on my
controller; other firmware might behave differently.

**Motion warning:** The daemon does not restore the controller's motion selector
on exit. If ordinary input stops after disabling motion, fully power-cycle and
reconnect the controller. Restarting Steam alone does not reset the selector.

Start the daemon in the foreground and stop it with Ctrl+C:

```sh
~/.local/bin/vader3p-steam
```

The daemon needs a valid private per-user runtime directory: `$XDG_RUNTIME_DIR`,
or `/run/user/<uid>` when unset. Test the foreground run before setting up the
optional [user service](docs/uhid-migration.md#optional-user-service-for-a-new-installation).

## References

- [Architecture and configuration reference](ARCHITECTURE.md)
- [Bluetooth protocol reference](VADER3P_PROTOCOL.md)
- [Testing guide](docs/TESTING.md)
- [Migration from the former uinput setup](docs/uhid-migration.md)
- [Manual Steam visibility setting](docs/steam-controller-visibility.md)

## Disclaimer

This software sends Bluetooth commands for rumble, color and experimental motion. Use
it at your own risk: unexpected behavior, loss of functionality, or other damage
may occur. See the [motion warning](#configure-and-run).

The software is provided **as is**, without warranties. To the extent permitted
by applicable law, the authors and contributors are not liable for damages
arising from its use. See the license terms for the full disclaimer and
limitations of liability.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
Proprietary firmware and applications used for research are not included. This
project is not affiliated with Flydigi or Valve.
