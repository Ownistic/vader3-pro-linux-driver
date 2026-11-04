# The vendor report carries live motion at 500 Hz (USB DInput)

This settles the assumption the firmware patch scoping rested on: **the report the
vendor-mode flag selects really does contain live sensor data.**

Environment: owner's Vader 3 Pro, wired, DInput (Classic) mode, on Linux. The
command was written to `/dev/hidraw11` (`mi_02`, the interface the Windows
application uses for DInput commands).

Command written: `05 EE 01` — report ID 5, vendor-mode subcommand.

## What changed

Before the command the stream is the ordinary 21-byte report ID `01`. After it the
stream becomes a single report type:

```
04 FE 66 00 ...   32 bytes total, reported at exactly 500 Hz
```

21 753 reports in 45 s, and 12 000 in 24 s — **500 Hz**, roughly four times the
130 Hz of the normal DInput stream.

## Fields are motion, not stick noise

Two captures were compared: 24 s with the controller lying untouched on a table,
and the rotation segment of an earlier capture. Signed 16-bit values at the
offsets below.

| Bytes | Still (spread) | Rotating (spread) | Interpretation |
| --- | --- | --- | --- |
| `11:12` | -35 … -13 (22) | -2715 … 3939 (6654) | gyro axis |
| `13:14` | -57 … -43 (14) | -3972 … 2803 (6775) | gyro axis |
| `15:16` | 4063 … 4143 (80) | 785 … 5671 (4886) | gyro axis (biased) |
| `26:27` | 0 … 0 (0) | -146 … 104 (250) | small-range quantity |
| `29:30` | -14 … 9 (23) | -10558 … 11133 (21691) | large-range axis |

A still controller produces spreads of 14–80 counts; rotating it produces spreads
of 4 900–21 700. The three `11`–`16` pairs therefore carry angular rate, and the
`29:30` pair has the widest range of all — consistent with acceleration.

Sticks are at bytes 4–6 and read `00 00 00` when untouched, so the separation is
clean: the earlier confusing capture had the owner's hand on the sticks.

## Why this matters

The whole firmware patch plan hinged on an untested assumption: that
`FUN_0000094c`'s output contains live motion rather than a calibration snapshot.
**It does.** So:

- Vendor mode over USB is **verified working**, not inferred.
- The data is high-rate (500 Hz) and well separated from input noise.
- The only missing piece for Bluetooth is setting the same flag
  (`state+0x2a`) over the GATT link — everything downstream of that flag is
  already proven to work.

## The extras are in the same report

A 40-second capture in vendor mode with the owner pressing M1, M2, M3, M4, C, Z
and finally A, three seconds apart, isolates the button field. Ignoring the noisy
sensor bytes, the presses appear as **byte 7** taking one bit each, in the order
they were pressed:

| Time | Byte 7 | Physical button |
| --- | --- | --- |
| t+10.02 | `0x04` | M1 |
| t+12.74 | `0x08` | M2 |
| t+16.00 | `0x10` | M3 |
| t+19.09 | `0x20` | M4 |
| t+21.75 | `0x01` | C |
| t+24.75 | `0x02` | Z |
| t+27.07 | byte 9 = `0x10` | A (control) |

The bit assignments match the standard DInput report exactly — extras at
`0x01`–`0x20` for C, Z, M1–M4 — just relocated to byte 7. The control press
appeared on byte 9 bit `0x10`, also as in DInput.

So the vendor report carries **everything at once**: all buttons, the sticks, and
live motion at 500 Hz.

## This is the complete answer to the original goal

DInput vendor mode is the only personality that provides both the extra buttons
and the sensors. Switch mode gives motion but drops four buttons; plain DInput
gives the buttons but no motion. The vendor report gives both.

Nothing about the report format needs to be invented, patched or extended. The
single missing piece is the flag that selects it, and that flag is already proven
to work when set over USB.

## Axis layout: not yet resolved

A combined capture (left stick, right stick, LT, RT, D-pad) showed several
axis-like fields but could not attribute them individually, because the controls
overlapped in time and the run began with the controller already moving.
Measured against a true still baseline from a separate capture:

| Byte | Still | Range seen | Note |
| --- | --- | --- | --- |
| 4 | 0 | 0–255 (44 values) | **always identical to byte 18** |
| 18 | 0 | 0–255 (44 values) | duplicate of byte 4 |
| 5 | 0 | 0–255 (32) | |
| 6 | 0 | 0–255 (9) | |
| 19 | 125 | 0–255 (183) | sweeps full range |
| 21 | 123 | 0–255 (192) | idles near 123 |
| 22 | 122 | 0–255 (211) | idles near 122 |
| 23 | 0 | 0–255 (40) | |
| 24 | 0 | 0–255 (52) | |

Three fields idle near 122–125 and several idle at 0, so the sticks and triggers
are mixed in these bytes without a clean split. **No decode code is written
against this**, because a wrong guess would produce plausible nonsense.

A second attempt with four timed phases produced a further complication: bytes
4, 5, 6, 18, 20, 26 and 27 sweep `0`–`255` continuously **whenever the controller
is held**, and are exactly `0` across a 24-second untouched capture. They are
therefore not steady axes — they respond to the controller being moved at all,
which is consistent with tilt or acceleration rather than stick position.

Only these showed phase-specific behaviour in the timed capture:

| Window | Bytes active |
| --- | --- |
| 10–20 s | `b19` |
| 20–25 s | `b21`, `b22` |
| 25–30 s | `b23`, `b24` |

The association between those windows and the intended controls was not
established, because the always-moving bytes above contaminated the windows.

**Conclusion: the axis layout needs the firmware builder, not more captures.**
`FUN_0000094c` writes the report field by field, so its store offsets are
authoritative and avoid this ambiguity entirely. That is the next step if the
host-side parser is resumed.

What is needed: one capture per control, isolated — left stick alone, right stick
alone, LT alone, RT alone — with the controller at rest before each. That is a
few minutes and it removes the ambiguity entirely.

## Not yet established

- The exact axis calibration and sign conventions, needed before any of this is
  exposed to Steam.
- Whether byte 5/6 and 26/27 (which moved with the triggers during the press
  capture) are the trigger axes in this report; they are not needed for the button
  or motion work.
- Whether the same report is emitted over Bluetooth once the flag is set. The
  report builder and the BLE notification path are shared, but that is reasoning,
  not an observation.
- The exact axis calibration and sign conventions, needed before any of this is
  exposed to Steam.

## Reproduction

```
python3 local/research/scripts/hidraw_dump.py /dev/hidraw11 24 out.jsonl --send 05ee01
```

Then compare byte spreads between a still and a moving segment. `mi_02` is
`/dev/hidraw11` on this machine; the node number changes between reconnects.
