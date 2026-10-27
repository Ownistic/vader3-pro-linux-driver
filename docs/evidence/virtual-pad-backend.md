# Virtual pad backend: UHID does not provide force feedback here

Status: **decision made from measured evidence**. Runtime path changed from
UHID to uinput as a result.

## The problem

The PRD requires Steam rumble to reach the physical controller (F4, A7). That
requires the virtual pad to expose a force-feedback device, because Steam sends
rumble as `EV_FF` events. A virtual pad with no `EV_FF` capability gives Steam
nothing to write to, and rumble silently does not work no matter how correct the
physical-side translation is.

## What was measured

The virtual pad was created through `/dev/uhid` and the kernel's own view was
read back from
`/sys/class/input/eventN/device/capabilities/{ev,key,abs,ff}`. Three descriptor
variants were tried, each created and destroyed in turn:

| Descriptor variant | `ff` | key bits | abs bits |
| --- | --- | --- | --- |
| Vendor usage 1 output report (the shipped descriptor) | `0` | 21 | 6 |
| PID with `DC Enable Actuators` | `0` | 21 | 6 |
| PID with `Set Effect Report` + `Effect Operation Report` | `0` | 21 | 6 |

All three produced a working gamepad — `key bits set: 21` confirms all 21
buttons including the six extras, and `abs bits set: 6` confirms four axes plus
the hat pair. But **none produced any force feedback**.

The reason is specific and not a descriptor mistake: force feedback on the HID
path requires the `hid-universal-pidff` driver to bind and translate the PID
report stream into `EV_FF`. On this system that driver is a module:

```
CONFIG_HID_UNIVERSAL_PIDFF=m
/lib/modules/7.3.0-4-cachyos/kernel/drivers/hid/hid-universal-pidff.ko.zst
```

It was not loaded, and loading it was refused (`modprobe: Operation not
permitted` in this environment). Even where it is loaded, depending on it would
mean the virtual pad's rumble works or not depending on a module the user may or
may not have, which is a fragile foundation for a required feature.

## The alternative, measured

`/dev/uinput` declares force feedback directly on the evdev node, with no HID
PID layer in between. The same probe path was used:

| Backend | `ff` | decoded | key bits | abs bits |
| --- | --- | --- | --- | --- |
| uinput | `40000 0` | bit 82 = `FF_RUMBLE` (0x52) | **21** | 8 |

`uinput` gives `FF_RUMBLE`, all 21 buttons, and the axes. The `abs bits` count
rises to 8 because uinput also declares `ABS_HAT0X`/`ABS_HAT0Y` as separate
axes rather than a single HID hat field, which is the normal evdev shape.

Practical consequence, verified: Steam sending rumble produces `EV_FF` events on
that node, which the daemon can read and translate into the physical rumble
command.

## Decision

**Use uinput for the virtual pad, not UHID.**

- Rumble is a required feature; UHID does not deliver it here, uinput does.
- uinput needs no kernel module beyond `uinput` itself, which is present and
  whose node is group/ACL writable.
- 21 buttons survive, so F2 (six independent extras) is unaffected.
- The cost is losing a HID report descriptor: uinput works in evdev terms, and
  the kernel synthesises the HID half. This is normal for virtual pads and does
  not reduce what Steam can bind.

## What this does not solve

Steam's handling of a *generic* 21-button gamepad is still unverified (A1, A4,
A5). uinput proves the buttons are exposed to evdev; it does not prove Steam's
controller editor will present M1–M4 and C/Z as separately assignable. That
remains an experiment on the target machine, and if it fails the fallback is a
DualSense-style identity with a translated report format.

## Reproducing the measurement

Both probes are in `driver/examples/`:

```sh
# UHID: three descriptor variants, reports capabilities/ff for each
sudo cargo run --example ff_probe

# uinput: declares FF_RUMBLE, reports the resulting bitmaps
cargo run --example uinput_ff_probe
```

`ff_probe` needs root because `/dev/uhid` is mode 0600. `uinput_ff_probe` needs
only write access to `/dev/uinput`.
