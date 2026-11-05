# Switch mode: gyro and rumble over Bluetooth, no vendor protocol

Measured on my Vader 3 Pro, firmware **6.7.5.1**, over Bluetooth, in the
controller's Nintendo Switch personality (FN+Y until the indicator is blue).
Everything below is hardware-observed on this machine, not inferred.

## What the kernel sees

```
0005:057E:2009  "Pro Controller"   driver: hid-nintendo   bus: uhid (HID over GATT)
/dev/hidraw9
  input43 -> event23   "Pro Controller"        keys 14, abs X Y RX RY HAT0X HAT0Y
  input44 -> event22   "Pro Controller (IMU)"  abs X Y Z RX RY RZ
```

Two input nodes, both created by the stock kernel drivers. Nothing in this path
touches the vendor GATT channel, the `A5` command dialect, or vendor mode.

## Gyro is live

30-second capture on `/dev/input/event22` while the controller was rotated:

| Axis | Events | Min | Max | Distinct values |
| --- | --- | --- | --- | --- |
| `ABS_X` | 4128 | -1909 | 5777 | 1447 |
| `ABS_Y` | 4103 | -4509 | 4112 | 1443 |
| `ABS_Z` | 4120 | -1064 | 6402 | 1393 |
| `ABS_RX` | 10088 | -8037000 | 5988000 | 3038 |
| `ABS_RY` | 10093 | -5827000 | 5561000 | 2795 |
| `ABS_RZ` | 10032 | -4234000 | 4394000 | 2676 |

`X Y Z` are the accelerometer, `RX RY RZ` the gyro. Nearly 10 000 events per axis
in 30 s is roughly **330 Hz**, well above the 130 Hz of the DInput input report.
The distinct-value counts show all three gyro axes respond independently — this is
not a duplicated or scaled single axis.

This is the gyro that the whole vendor-mode investigation was chasing. It is
available on stock firmware, over Bluetooth, with no command to send.

## Rumble works through the standard force-feedback API

`/sys/class/input/input43/capabilities/ff` = `107030000 0`, which decodes to
`FF_RUMBLE` (0x50), `FF_PERIODIC` (0x51), `FF_CONSTANT` (0x52), `FF_SQUARE`,
`FF_TRIANGLE`, `FF_SINE` and `FF_GAIN` (0x60).

Verified by uploading a `FF_RUMBLE` effect with `EVIOCSFF`, playing it for 2 s and
stopping it: **I felt it and it stopped on its own.** No NUS write, no
vendor frame, no periodic retransmission — the kernel driver handles all of it.

## The extra buttons do not exist in this mode

Buttons occupy bytes 3–5 of the 49-byte `0x30` input report. Over a 60-second
capture in which I pressed **M3, M4, C and Z**, those three bytes stayed
constant at `00 80 00` across all 400 distinct frames. Every other varying byte
was the timer, the sticks, or IMU samples.

So the four buttons are not filtered by the driver — **the controller never
sends them**. Separately, a key-level capture showed:

| Physical button | Reported as |
| --- | --- |
| M1 | `BTN_THUMBL` (L3) |
| M2 | `BTN_THUMBR` (R3) |
| M3, M4, C, Z | nothing |

That is consistent with the Switch protocol having no spare button slots: a
Switch Pro Controller has exactly 14 buttons, all of which the Vader uses.

## Can a patch get the extras into Switch mode?

Checked, because it is the obvious next question. Three findings, all against a
cheap answer:

1. **There is no secondary channel in this personality.** The connected device
   advertises exactly two GATT services: `00001124` (HID over GATT) and
   `00001200` (PnP Information). The NUS and vendor services that make the
   DInput mode scriptable are **absent**, so there is no side channel to poll
   extra buttons from while the HID stream supplies gyro.
2. **The descriptor has no spare slots.** Report `0x30` carries 48 bytes with the
   Switch button layout, and `0x3F` carries a 16-bit button field; both are fully
   allocated by the Switch protocol. The 105-byte reports `0x31`–`0x33` are the
   NFC/IR reports, which Linux never requests and which carry NFC data.
3. **The Switch descriptor is not a literal in any shipped image.** The live
   170-byte report map is absent from all three firmware files (Telink87 main
   `6.7.5.1`, `f3si` extension `3.0.2`, dongle `0.4.1.7`), so it is assembled at
   runtime rather than stored. By contrast the DInput descriptor *is* a literal
   at `0x23764`, which is why that personality was tractable to analyse.

So a firmware patch would have to locate a runtime-assembled descriptor and the
report builder behind it before adding a new report and its bits, then flash the
result. That is a materially larger job than the DInput work, carries the brick
risk recorded in the patch scope document, and buys only four buttons that are
already working in DInput.

**Conclusion: no host-side route exists, and a patch is not worth it now.**

## Trade-off

| | DInput (+ `vader3p-steam`) | Switch mode (no driver) |
| --- | --- | --- |
| Standard buttons | 15 | 14 |
| M1–M4 | 4 separate | M1/M2 alias L3/R3, M3/M4 lost |
| C | bindable as `misc1` | lost |
| Z | reaches pad, no Steam slot | lost |
| **Gyro** | **no** | **yes, ~330 Hz** |
| Rumble | vendor NUS frames | standard `FF_RUMBLE` |
| Driver needed | yes | no |

Neither mode is a superset. The controller cannot report both sensor data and
the extra buttons at once, because they live in different personalities.

## Why this closes the vendor-mode line of investigation

The vendor report path needs `state+0x2a`, which only the two `0xEE` arms write,
and no Bluetooth command reaches them (see [gyro-deferred.md](gyro-deferred.md)).
Switch mode makes that moot: the motion data is already in the HID stream. The
remaining open question is no longer "can we get gyro over Bluetooth" but
"which personality should the driver support".

## Tooling

Under `local/research/scripts/` (Git-ignored):

| Script | Purpose |
| --- | --- |
| `imu_probe.py` | per-axis event counts, ranges and distinct values |
| `key_probe.py` | timestamped key presses on an evdev node |
| `raw_hidraw.py` | one line per changed raw HID report |
| `ff_rumble.c` | upload/play/stop a `FF_RUMBLE` effect (`cc -O2 -o ff_rumble ff_rumble.c`) |

`ff_rumble.py` in the same directory is superseded by the C version: the ctypes
`ff_effect` layout produced `EINVAL` and was not worth repairing once the kernel
header version worked.
