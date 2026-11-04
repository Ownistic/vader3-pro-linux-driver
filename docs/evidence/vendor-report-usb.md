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

## Not yet established

- Whether the vendor report also carries M1–M4/C/Z. Earlier static analysis of
  `FUN_0000094c` says it separates M1–M6 from the D-pad, and bytes 4–6 are the
  sticks, so the buttons should be in the low bytes — but a press-by-press capture
  is needed to confirm before relying on it.
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
