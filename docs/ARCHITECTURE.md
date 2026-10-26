# Architecture

Status: **runtime decision pending research**. See [PRD](../PRD.md) and
[research phase](phases/01-research.md).

## Preferred data path

```text
Vader 3 Pro — Bluetooth HID — physical transport/parser
                                      |
                               canonical state
                                      |
                         Steam-compatible encoder — UHID — Steam Input
                                      ^                       |
                                      +— feedback translator —+
```

Responsibilities: scoped device discovery, physical input parsing, canonical
state, UHID lifecycle, Steam report encoding/control replies, motion/calibration,
feedback translation, configuration/logging/lifecycle. Keep transport-specific
parsers separate from canonical state and virtual identity.

## Decisions deliberately not made

- padctl/Zig extension versus standalone Rust daemon.
- Virtual Flydigi versus Steam Deck/other identity versus Edge/Elite.
- Suppression/permission strategy before physical identity and safe cleanup are
  established. A VID/PID pair or an unrestricted grab is not sufficient.
- Rumble capability or sensor units based only on USB/receiver code.

These are material decisions requiring source/hardware evidence and review.
Python in `tools/` is development-only and carries no runtime-language commitment.

## Safety invariants for the eventual runtime

Never match/suppress built-in Legion Go devices. Neutralize state on physical
disconnect before releasing the virtual endpoint. Handle UHID control/feedback
events rather than merely emitting input reports. Open only verified target
devices; support reconnection without polling busy loops. Gate experimental
output commands and avoid permanent controller writes.

## Deployment boundary

No active service or udev suppression is installed by setup. Later deployment
must use scoped `/etc/udev/rules.d/` rules where required, a user service under
`~/.config/systemd/user/`, userspace binaries, and persistent configuration.
No immutable partition edits, global writable nodes, or SELinux disablement.
