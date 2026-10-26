# Flydigi Vader 3 Pro — Bluetooth Steam Input on Linux

**Status: research and project scaffolding, not a working controller driver.**
Goal: native Steam Input on Bazzite/Lenovo Legion Go over Bluetooth, with all
standard controls, independent M1–M4/C/Z, native gyro, real rumble, and reliable
reconnection. No custom kernel modules or immutable OS changes.

Requirements: [PRD.md](PRD.md). Roadmap: [PLAN.md](PLAN.md).
Current evidence/blockers: [PROGRESS.md](PROGRESS.md).

## Run the safe diagnostic

Requires Linux and Python 3.10+; no third-party Python packages. From the checkout:

```sh
python3 tools/inventory.py --help
python3 tools/inventory.py --descriptor
python3 -m unittest discover -s tests -v
```

The tool reads **sysfs metadata only**, defaults to vendor `d7d7` as an upstream
research lead, and omits names, addresses, serials, and physical paths. It does
not open device nodes, capture live reports, grab controllers, change permissions,
or send any commands. If your controller uses another identity, supply its
observed hexadecimal `--vid` and optionally `--pid`; do not guess a runtime
match from the default. Review JSON manually before sharing it.

Exit 0 means metadata enumeration completed (possibly with **zero matches**), not
that controller support works. Exit 2 means incomplete metadata/error; do not
rerun with broad root privileges to hide it. Bluetooth bus ID alone does not
distinguish Classic HID from BLE.

On the Legion Go, pair normally through Bazzite, run the diagnostic once per
available controller mode, and record the mode/firmware separately. See
[testing](docs/TESTING.md) for the next hardware evidence and Steam checks.

## Project structure

- `docs/RESEARCH.md`: upstream and proprietary-artifact findings/limits.
- `docs/PROTOCOL.md`: transport-specific mappings and unknowns.
- `docs/ARCHITECTURE.md`: boundaries and pending decisions.
- `docs/TESTING.md`: automated checks and real-hardware acceptance matrix.
- `docs/phases/`: scoped setup and feasibility gates.
- `docs/evidence/`: retention rules and acquisition record.
- `tools/`: independent read-only development diagnostics, not a runtime daemon.
- `tests/`: synthetic sysfs tests, two real descriptor fixtures, and a sanitized
  owner-coordinated Bluetooth A/B/C/Z capture; no Steam acceptance claim.

## Safety and deployment status

No runtime, systemd service, udev rule, or installation script is enabled or
installed by this scaffold. Runtime language and virtual identity remain open
until padctl source review and Steam experiments justify a choice. Do not use
arbitrary HID writes, firmware changes, or global device-suppression rules.

Keep official installers, extracted DLLs/decompilation, raw Bluetooth captures,
and local identities **outside Git**. See [evidence policy](docs/evidence/README.md)
and [installer acquisition](docs/evidence/installer-3.4.4.3.md).

## License

Project distribution license is pending owner selection and upstream compatibility
review. No third-party source has been incorporated. Upstream licenses remain
applicable to any future reuse; this scaffold does not grant rights to official
Flydigi software or claim affiliation with Flydigi or Valve.
