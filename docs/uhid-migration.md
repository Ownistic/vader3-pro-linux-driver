# Migrate to the UHID-only daemon

The current daemon creates a separate virtual Bluetooth DualSense Edge through
`/dev/uhid` for each eligible connected controller. It no longer creates the
former generic uinput pad, so the old SDL mapping and service are not part of the
current setup. This guide only describes files from the
former README instructions. Inspect your installation before changing anything;
do not assume every path below exists or is owned by this project.

## Before changing an existing installation

Back up the old executable, its private configuration, the user unit, and any
mapping override that you own and might need for rollback. Inspect the actual
paths and preserve any local edits. The former instructions used these paths:

- Executable: `~/.local/bin/vader3p-steam`
- Configuration: `~/.config/vader3p-steam/config.toml`
- User unit: `~/.config/systemd/user/vader3p-steam.service`
- SDL environment drop-in: `~/.config/environment.d/vader3p-steam.conf`

Inspect the known unit before stopping or replacing anything:

```sh
systemctl --user status vader3p-steam.service
systemctl --user cat vader3p-steam.service
systemctl --user show vader3p-steam.service
```

Proceed only if those results identify the old Vader 3 Pro translator and its
owned unit. Stop and disable only that unit before replacing the executable:

```sh
systemctl --user stop vader3p-steam.service
systemctl --user disable vader3p-steam.service
```

Do not stop similarly named units that you have not verified. Do not run
`systemctl --user revert`, remove files with wildcards, or delete the whole
`~/.config/systemd/user/` directory. Those actions can remove unrelated services
or overrides. Preserve the old unit and configuration backups until the new
foreground run passes.

If you want to replace the old unit, first verify that the exact user unit file
is yours and matches the former setup. After backing it up, move only
`~/.config/systemd/user/vader3p-steam.service` outside the unit directory. Keep
any `vader3p-steam.service.d/` overrides unless you have separately verified
that each file belongs to this setup. If the unit has custom settings, preserve
them and edit only the required command rather than replacing the unit blindly.

## Install and test the new binary

Build from this repository as described in the [README](../README.md). Stop the
verified old unit before starting the new daemon. Same-user ownership leases
exclude only cooperating new instances; they cannot fence an old or otherwise
uncooperative daemon. After the old unit is stopped, install the new binary at
the path your setup uses. The former instructions used
`~/.local/bin/vader3p-steam`. Create or update the configuration using the
current README example. `bluetooth_address` is optional: when unset, all eligible
connected controllers are selected; when set, it is a validated filter for one
controller. Rumble and motion do not require an address. The existing
`[pad] backend = "uhid"` selector remains valid. If
your configuration explicitly selects `uinput`, remove or change that selector;
the current daemon rejects it rather than choosing another backend. Motion
remains opt-in with `[motion] enabled = false` by default.

Inspect the service's `ExecStart` for an explicit `--config PATH`. Verify that
the exact file exists, is readable, and contains valid configuration before
starting the service. A missing explicit path is an error; the daemon does not
fall back to the conventional configuration or defaults. Without `--config`,
it reads the conventional file when present and uses defaults only when that file
is absent. The example unit below specifies an explicit path, so create that
file before starting the unit.

Check the current [device-permission guidance](../README.md#device-access).
The daemon needs access to `/dev/uhid`, the physical controller's hidraw node,
and its evdev nodes. Grant access only to verified device nodes. The kernel must
provide `hid-playstation`. Do not broaden permissions or unbind kernel drivers
to migrate.

Pair and connect controllers in Bluetooth DInput mode through the operating
system; the daemon does not pair or connect devices or change system settings.
Run the daemon in the foreground and verify virtual devices and input behavior
before installing a service. It can start with no controllers connected and
detect later connections. Its private per-user runtime directory must be valid
for controller ownership leases. Do not treat a successful command or transport
write as proof of hardware behavior. Stop the foreground process with Ctrl+C
before starting a service.

## Remove only the old SDL mapping, if present

The old README created `~/.config/environment.d/vader3p-steam.conf` with an
`SDL_GAMECONTROLLERCONFIG` value from `driver/contrib/gamecontrollerdb.txt`.
That mapping targeted the old generic pad. The current virtual DualSense Edge
does not need it.

Inspect the exact file and its contents first. If it contains only the mapping
you own, move that file to a clearly named backup outside `environment.d/`.
For example, after verifying the file and choosing a free backup name:

```sh
mkdir -p ~/.local/state/vader3p-steam-migration
mv ~/.config/environment.d/vader3p-steam.conf \
  ~/.local/state/vader3p-steam-migration/vader3p-steam.conf.bak
```

If the file contains other environment entries, back it up and remove only the
`SDL_GAMECONTROLLERCONFIG` entry that belongs to this old mapping. Preserve all
other entries. Do not remove other mapping files or the `environment.d/`
directory. An SDL variable imported by a Steam launcher or user manager may
remain in the current session after the source entry is removed. Fully exit and
restart Steam; if the launcher previously imported the value, restart the
session as well. Do not unset every SDL variable globally.

## Optional user service for a new installation

Use this only after the foreground run and permissions work. If
`~/.config/systemd/user/vader3p-steam.service` already exists, inspect and back
it up; do not overwrite custom unit settings blindly. Create a new unit only
when that path is free, with these contents:

```ini
[Unit]
Description=Flydigi Vader 3 Pro UHID translator
After=bluetooth.target

[Service]
ExecStart=%h/.local/bin/vader3p-steam --config %h/.config/vader3p-steam/config.toml
Restart=on-failure
RestartSec=3

[Install]
WantedBy=default.target
```

This unit does not grant device permissions, configure SDL, or enable
autostart. After saving the unit, reload and start it for a test. Enable it only
after that test succeeds and you want it to start with the user session:

```sh
systemctl --user daemon-reload
systemctl --user start vader3p-steam.service
systemctl --user status vader3p-steam.service
journalctl --user -u vader3p-steam.service
```

To enable autostart after verification, run
`systemctl --user enable vader3p-steam.service`. To stop and disable this unit,
run `systemctl --user stop vader3p-steam.service` and
`systemctl --user disable vader3p-steam.service`.

## Rollback and Steam visibility

Rollback is optional. To restore the former setup intentionally, first stop the
new daemon or unit. Then restore the backed-up old executable, configuration,
unit, and only the mapping file or entry that you owned. Review and enable the
old unit manually if you choose to run it. This guide does not restore or start
the old translator automatically.

A separate Steam physical-device ignore setting for the physical controller
(`04b4:2412`) persists independently of this daemon. Stopping the daemon does
not reverse that Steam setting. Review it separately when returning to native
Steam use. Do not automatically unhide the physical device as part of rollback;
the old generic translator may have depended on that setting to avoid duplicate
devices.
