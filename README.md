# Flydigi Vader 3 Pro for Linux

This experimental daemon translates connected Vader 3 Pro controllers in Bluetooth
DInput mode into separate virtual DualSense Edge controllers through Linux UHID.
It targets Linux systems with the `hid-playstation` driver. Physical USB and the
2.4 GHz dongle are not supported.

## Capabilities and status

- Sticks, analog triggers, D-pad and standard buttons.
- Automatic discovery of all eligible connected Bluetooth controllers, including
  controllers connected after startup. Each controller has an independent virtual
  pad and lifecycle.
- Six independent extra buttons: C, Z and M1–M4.
- FN opens Steam's Quick Access Menu (the sidebar containing Decky) using an
  ordered Guide + A pulse.
- Rumble feedback forwarded over Bluetooth.
- Experimental motion input maps two uncalibrated components to pitch and yaw.
- A lost controller's virtual pad is removed and recreated with the same identity
  after reconnect. Hardware reconnect behavior still needs acceptance testing.

In an earlier Steam test, all controls worked except gyro Z. That observation
applies to the tested build, not automatically to the current build. I have not
hardware-retested the current pitch correction. The third gyro component is not
available in the implemented motion input, and the virtual acceleration values
are fabricated rather than measured. Calibration and motion remain experimental.

The daemon does not install kernel modules, change Steam settings, or unbind
drivers. Physical-device hiding in Steam is a separate, optional manual setting;
see the [visibility guide](docs/steam-controller-visibility.md). The daemon's
physical event grabs do not remove the controller from Steam.

## Install

### One-command installation

On x86-64 Linux, including Bazzite, install the latest published release as your
normal user:

```sh
curl -fsSL https://raw.githubusercontent.com/Ownistic/vader3-pro-linux-driver/main/install.sh | sh
```

The installer downloads the release binary and its SHA-256 checksum from GitHub,
verifies the checksum, and installs `~/.local/bin/vader3p-steam`. The checksum
detects download corruption; it is not an independent signature. Review a script
before piping it to a shell. The command requires `curl`, GNU coreutils, a
published release and the D-Bus runtime library. See the
[compatibility requirements](driver/README.md#compatibility-requirements) for
supported system libraries. It does not install packages, grant device access,
pair controllers, change configuration, or start or enable a service.

The installer refuses to replace an existing executable by default. If you are
replacing an earlier installation, read the [migration guide](docs/uhid-migration.md),
stop the verified old service, and back up the executable first. To explicitly
replace the executable after those steps, use:

```sh
curl -fsSL https://raw.githubusercontent.com/Ownistic/vader3-pro-linux-driver/main/install.sh | sh -s -- --replace
```

Continue with [device access](#device-access) and [configuration](#configure-and-run)
before starting the daemon. Device permissions may need to be granted again after
reconnection.

Maintainers publish binaries by pushing a tag matching the package version in
`driver/Cargo.toml`. The release workflow builds and tests on Ubuntu, then
publishes the binary and checksum. The installer always uses
the latest non-prerelease GitHub release.

### Build from source

Prerequisites:

- Rust and `cargo`; see the [compatibility requirements](driver/README.md#compatibility-requirements)
- `pkg-config`, BlueZ and D-Bus development headers (`libdbus-1-dev` on
  Debian/Ubuntu or `dbus-devel` on Fedora)
- Linux kernel support for `/dev/uhid` and `hid-playstation`

If you are replacing an earlier installation, read the
[migration guide](docs/uhid-migration.md) before replacing its executable. It
helps identify and stop only the old, verified service.

Build and install from the repository root:

```sh
cargo build --manifest-path driver/Cargo.toml --release --locked
install -Dm755 driver/target/release/vader3p-steam ~/.local/bin/vader3p-steam
```

### Device access

Run as your normal user, not as a root daemon. That user needs read/write access
to `/dev/uhid`, read access to each selected physical controller's hidraw node,
and access to associated evdev nodes sufficient for the daemon's event grabs.
Identify each controller and its related nodes before granting access; do not
grant broad device permissions.

From the repository root, inventory candidate devices:

```sh
python3 tools/inventory.py --descriptor
```

Verify the physical controller's identity and its hidraw/evdev node relationship
before granting temporary ACLs. For example, replace the node names with the
verified nodes:

```sh
sudo setfacl -m "u:$USER:rw" /dev/uhid
sudo setfacl -m "u:$USER:r" /dev/hidrawN /dev/input/eventN
getfacl /dev/uhid /dev/hidrawN /dev/input/eventN
```

`/dev/uhid` access lets a user create virtual HID devices, so grant it only to
trusted users. Device nodes can change after reconnect; verify them and reapply
temporary ACLs as needed. For the Bazzite access rule observed on my Legion Go
and an example for other distributions, see the
[device access guide](docs/device-access.md).

### Configure and run

Pair and connect controllers through your operating system in Bluetooth DInput
mode. The daemon does not pair devices or change Bluetooth, Steam, or operating
system settings. You can start it before any controller is connected; it waits
for eligible devices and detects later connections.

No Bluetooth address is required. Leave `bluetooth_address` unset to use every
eligible connected controller. To restrict the daemon to one controller, set the
optional filter to that controller's full Bluetooth address. The filter is
validated even when rumble and motion are disabled.

Configuration is optional. Without `--config`, the daemon reads
`~/.config/vader3p-steam/config.toml` (or the path under `$XDG_CONFIG_HOME`) when
it exists, and uses defaults only when that conventional file is absent. If you
pass `--config PATH`, that exact file must exist and be valid; a missing,
unreadable, or malformed file is an error. The daemon does not fall back to
defaults for an explicit path.

Create the conventional configuration only if you need settings:

```sh
mkdir -p ~/.config/vader3p-steam
```

```toml
# Optional: restrict operation to one controller.
# bluetooth_address = "AA:BB:CC:DD:EE:FF"
rumble = true

[motion]
enabled = false
```

Motion is opt-in; set `enabled = true` only to try experimental pitch/yaw input.
See the [configuration reference](ARCHITECTURE.md#configuration-reference) for
all options and defaults.

Start the daemon in the foreground and stop it with Ctrl+C:

```sh
~/.local/bin/vader3p-steam
```

The daemon needs a valid private per-user runtime directory for its same-user
controller ownership leases. It uses `$XDG_RUNTIME_DIR`, or `/run/user/<uid>` if
that variable is unset. Confirm device behavior directly; a successful transport
write does not prove that the controller acted on it. Test the foreground run
before configuring an optional
[user service](docs/uhid-migration.md#optional-user-service-for-a-new-installation). Motion mode
does not restore the controller's selector on exit; returning to ordinary input
may require a full controller power cycle and reconnect. Restarting Steam alone
does not reset the selector. See [architecture](ARCHITECTURE.md) for details.

## References

- [Architecture and configuration reference](ARCHITECTURE.md)
- [Bluetooth protocol reference](VADER3P_PROTOCOL.md)
- [Testing guide](docs/TESTING.md)
- [Migration from the former uinput setup](docs/uhid-migration.md)
- [Manual Steam visibility setting](docs/steam-controller-visibility.md)

## Disclaimer

This software sends Bluetooth commands to your controller, including commands
for rumble and experimental motion. Use it at your own risk. Unexpected
behavior, loss of functionality, or other damage may occur. Motion mode can
change a controller setting that the daemon does not restore on exit; a full
power cycle and reconnect may be needed to restore ordinary input.

The software is provided **as is**, without warranties. To the extent permitted
by applicable law, the authors and contributors are not liable for damages
arising from its use. See the license terms for the full disclaimer and
limitations of liability.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
Proprietary firmware and applications used for research are not included. This
project is not affiliated with Flydigi or Valve.
