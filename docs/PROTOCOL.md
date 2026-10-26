# Protocol ledger

Status: **two Bluetooth descriptors and one owner-coordinated A/B/C/Z input
capture obtained; no output commands tested**. Descriptor/source observations,
sequence-associated button fields, and unknowns remain distinct from a complete
verified wire specification.

## First physical observation

Before switching modes, the owner-confirmed Vader exposed Bluetooth
`045e:02e0`, an Xbox-compatible descriptor. Its 306-byte descriptor has SHA-256
`74a5dbcedf6d6a88f9eb6e66fbeb5a255ba800fd084390cbf325e0c4c74492d7`.
It advertises standard controls and a rumble-style output report, but no explicit
six-extra or IMU fields. Xbox-mode physical reports/feedback are untested; the
node was unreadable at descriptor acquisition. Mode switch setting was not
independently known.
See [Xbox-mode evidence](evidence/bluetooth-xbox-mode.md).

After a normal mode switch/reconnection it exposes **`d7d7:0041`**, matching the
upstream DInput lead. Its 163-byte descriptor has SHA-256
`227924e94560bea5149d6a75a8bb225143a1e4b0685075dea729a30649bc1f22`.
BlueZ exposes HID-over-GATT and separate writable HID-report/vendor/UART
characteristics. The HID report map itself advertises input only; this does not
settle feedback availability. Later owner-authorized read-only access yielded
the partial A/B/C/Z observations below; motion and all output commands remain
unverified. See [DInput evidence](evidence/bluetooth-dinput-mode.md) and
[passive report evidence](evidence/passive-dinput-abcz.md).

## Observed DInput input frames

The coordinated capture contains 14 ID-1 reports, **21 bytes each**, versus
15 bytes declared by the HID descriptor. Preserve the six additional bytes;
all were zero in this small sample and their meaning is unknown.

Offsets include report ID at byte 0. In the owner-completed A/B/C/Z sequence:
A = byte 9 mask `0x10`; B = byte 9 mask `0x20`; C = byte 11 mask `0x01`;
Z = byte 11 mask `0x02`. Each had a distinct set/clear pair. These are
sequence-associated observations, not a general parser or simultaneous-input
proof. M1–M4, remaining inputs, motion and Steam-editor exposure are unverified.

Six more owner-coordinated passive sessions followed the A/B/C/Z capture.
Consolidated button map (each row confirmed by single-press isolation or a
coordinated ordered batch; details and session provenance in the passive-capture
evidence doc):

| Control | Field / masks |
| --- | --- |
| A / B / X / Y | byte 9 `0x10` / `0x20` / `0x40` / `0x80` |
| LB / RB | byte 10 `0x01` / `0x02` |
| LT / RT | byte 10 `0x04` / `0x08`, analog level byte 13 / byte 14 ramping `0x01`→`0xff` |
| Select / Start | byte 10 `0x10` / `0x20` |
| L3 / R3 | byte 10 `0x40` / `0x80` |
| Home | byte 12 `0x80` |
| C / Z | byte 11 `0x01` / `0x02` |
| D-pad Up/Right/Down/Left | byte 9 **low nibble** hat: `1` / `3` / `5` / `7` (0 = neutral) |
| M1 / M2 / M3 / M4 | byte 11 `0x04` / `0x08` / `0x10` / `0x20` |

Sticks: byte 1/2 = left stick X/Y, byte 3/4 = right stick X/Y. Left-stick
full-left hold = byte 1 `0x80` plateau, full-right = `0x7f`; forward = byte 2
`0x80`; right stick behaves symmetrically (extremes `0x80`/`0x7f` observed).
Idle bytes sit near `0xfd`–`0x01` and are session-dependent; treat reported
values as signed-like centered bytes, calibrating on first idle frames.
LT/RT analog levels ramp byte 13/byte 14 from `0x01` to `0xff` (true analog).

A whole-controller shake test produced no large byte 1–4 swings: the passive
ID-1 stream does not visibly carry gyro/accelerometer data. Native gyro likely
requires a vendor-mode output command; untested.

**Correction to an earlier entry.** M1–M4 and the D-pad are **separate fields**, not
shared bits. An earlier version of this table listed both on byte 11 and called
them indistinguishable; that was wrong. Re-analysis of the raw captures:

- Session 5 (owner-confirmed M1–M4 presses, no D-pad) → byte 11
  `0x04`/`0x08`/`0x10`/`0x20` while byte 9's low nibble stayed `0x00` for all
  25 frames.
- Session 3 (D-pad only) → byte 9 low nibble `1`/`3`/`5`/`7` while byte 11
  stayed `0x00`.
- Session 1 exercised both, but sequentially: D-pad at 80–86 s, M keys at
  105–107 s. No frame anywhere in the seven captures has both fields non-zero.

So the D-pad is a hat in byte 9's low nibble and M1–M4 are discrete bits in
byte 11. The per-M-key bit order below matches FlyDigi's own key table
(M1=`0x04`, M2=`0x08`, M3=`0x10`, M4=`0x20`), but the owner could not confirm
which physical key was pressed when, so the *assignment* is table-derived
rather than owner-confirmed. Its consequence for the project is positive: all
six extra buttons are independently readable in plain DInput, so Steam Input
can receive M1–M4 as distinct buttons without any vendor-mode switch.

Byte 9 low nibble idle value is `0` in every idle frame of every session, so it
behaves like a clean neutral hat rather than a session-centering axis.
Multi-button simultaneity, release-edge behavior, and mode/initialization
commands are still unverified; rumble and Steam Input runtime behavior remain
untested.

## Space Station 3.4.4.3 static analysis (new)

See [space-station static analysis](evidence/space-station-static-analysis.md).
Installer extracted with innoextract 1.10-dev (the packaged 1.9 cannot parse
Inno Setup 6.3); assemblies decompiled with ILSpy 11.1.0. Nothing executed.
Transferable protocol findings:

- Vendor/config commands are 15-byte blobs `05 <subcmd> <payload…> <sum>` where
  the last byte is the running additive checksum. `05 0F <L> <R>` is a motor
  test/rumble command with 0–255 levels; `05 EE 01` enables "origin" (vendor)
  mode and `05 EE 00` restores normal DInput reporting.
- In vendor mode the device emits 32-byte `04 FE …` frames. Space Station maps
  byte 17/19/21/22 → sticks, 9/10 → buttons, 23/24 → analog triggers, 7 → the
  `C, Z, M1–M6` byte, and **bytes 4/5/6 → 12-bit packed gyro X and Y**. The
  passive 21-byte stream carries no motion at all, which explains the failed
  shake test.
- Key order table: index 16–21 = C, Z, M1, M2, M3, M4. That table is what gives
  byte 11 bits `0x04`/`0x08`/`0x10`/`0x20` their M1–M4 meaning.
- Device configs are JSON (`app/default/device_f3/config.fdg`). The shipped
  Vader 3 Pro default remaps `C→THUMBL`, `Z→THUMBR`, `M1→UP`, `M2→DOWN`,
  `M3→RIGHT`, `M4→LEFT` — i.e. the stock profile *forwards* those keys to D-pad
  outputs. That profile, not the report layout, is why they looked merged.
  Serialised form is a 76-byte payload with a 96-byte key map (32 ×
  `{targetKeyId, turboType, turboCount}`) at offset 13.
- Space Station never targets `d7d7`: its HID matching lists `0x045E:0x028E` and
  `0x04B4/0x04B5:0x2410–0x2415`, and its BLE wrapper is used only for the BS1
  cooler. Command formats transfer; the transport does not.

**Correction carried into `docs/PROTOCOL.md`:** M1–M4 are distinct byte-11 bits
and the D-pad is a byte-9 low-nibble hat; they are separate fields. The earlier
"M keys duplicate D-pad bits" claim was wrong and is now retracted.

## Required transport inventory

For each controller mode, record firmware, Bluetooth Classic HID versus BLE/HOGP,
HID bus/VID/PID/version, descriptor bytes and SHA-256, report lengths/IDs, feature
and output paths, mode/initialization sequence, timestamps, and provenance.
Bluetooth HID bus `0005` alone does not distinguish Classic from BLE.

## Mapping worklist

| Fields | Bluetooth layout / units | State |
| --- | --- | --- |
| A/B/X/Y | byte 9 `0x10`-`0x80` | Isolated in captures 1/2; Steam untested |
| LB/RB; Start/Select; L3/R3 | byte 10 `0x01`/`0x02`; `0x10`/`0x20`; `0x40`/`0x80` | Session-6 isolation; Steam untested |
| LT/RT | byte 10 `0x04`/`0x08` + analog byte 13/14 (0x01→0xff) | Session-4 isolation; Steam untested |
| Home | byte 12 `0x80` | Session-4 isolation |
| C/Z | byte 11 `0x01`/`0x02` | Sessions 1/4 |
| D-pad | byte 9 low-nibble hat `1`/`3`/`5`/`7` (0 = neutral) | Sessions 1/3; independent of byte 11 |
| M1–M4 | byte 11 `0x04`/`0x08`/`0x10`/`0x20` | Session-5 isolation; distinct from D-pad |
| LX/LY/RX/RY | byte 1/2/3/4; extremes `0x80` and `0x7f`, idle ≈ `0xfd` | Session-6 click-shake isolation + held plateaus |
| Gyro XYZ and accelerometer XYZ | Not present in passive ID-1 stream (shake test) | Untested; likely vendor command required |
| Rumble channels/strength/duration | Unknown Bluetooth output or feature command | Untested |
| Simultaneous inputs | Not yet exercised | Untested |

Observed DInput descriptor field order (not yet mapped against physical reports):
four signed 8-bit axes; four additional unsigned 8-bit Generic Desktop fields;
4-bit hat; 12 Button bits; 16 Consumer bits; two 8-bit Simulation fields, all
under input report ID 1. A parser must verify actual length, offsets, and
physical meaning against recorded neutral/press/release/motion samples.

## Source leads, not Bluetooth commands

SDL's `SDL_hidapi_flydigi.c` has `HIDAPI_DriverFlydigi_HandleStatePacketV1`,
`HIDAPI_DriverFlydigi_UpdateDeviceIdentity`, and
`HIDAPI_DriverFlydigi_RumbleJoystick`. V1 handling expects `04 fe` input and
uses extra-button bits in byte 7; its feedback command begins `05 0f`.
The V1 device selection requires interface 2 and comments describe a custom
interface on early controllers. This is **not** evidence that the Bluetooth
descriptor exposes this interface, accepts that feedback command, or sends
that input layout. Do not send these bytes to hardware based on this ledger.

Source pins and later findings belong in [RESEARCH.md](RESEARCH.md). Every future
layout must name its transport and evidence; inferred offsets stay explicitly
marked until compared against captures.
