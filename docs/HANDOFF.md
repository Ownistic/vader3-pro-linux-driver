# Handoff — 2026-11-03

State of the project at the end of the session that took gyro from "deferred" to
"answered, with one option left untested".

Repository: `git@github.com:Ownistic/vader3-pro-linux-driver.git`, branch `main`,
HEAD `bd0be0b`. Working tree clean. `cargo test`: **107 passed**.

## What is delivered and working

The userspace driver is installed and running on the Legion Go as a systemd user
service, with an SDL mapping drop-in so the mapping survives reboots.

| Capability | State |
| --- | --- |
| Sticks, triggers, 15 standard buttons | correct in Steam |
| M1–M4, C | bind as paddles and `misc1` |
| Z | reaches the pad; no Steam slot exists for it |
| Rumble | play/stop fixed; sustained rumble via NUS |
| Reconnects | self-healing across `hidraw8`/`hidraw9` |
| Axes | conventional `ABS_*` layout, verified in Steam |

Nothing in the driver changed this session. All work here was research.

## What this session established

Ordered by what a future session is most likely to need.

**1. Switch mode gives gyro over Bluetooth on stock firmware.** Pairing the
controller as a Pro Controller (`057E:2009`) brings up `hid-nintendo`, which
exposes a second evdev node with `ABS_X/Y/Z` (accelerometer) and `ABS_RX/RY/RZ`
(gyro) at roughly 330 Hz, plus rumble through the standard force-feedback API.
No driver is involved. Cost: M3, M4, C and Z are never transmitted in this
personality, and M1/M2 alias L3/R3.
Evidence: `docs/evidence/switch-mode.md`.

**2. The vendor report carries extras *and* motion together, over USB.** Sending
`05 EE 01` on the `mi_02` interface switches the stream to 32-byte report `0xFE`
at exactly 500 Hz. Bytes 11–16 are three angular-rate pairs (still ≈ 14–80 counts
of spread; rotating ≈ 5 000–6 800), bytes 29–30 a wide-range pair, byte 7 the
extras (C `0x01`, Z `0x02`, M1–M4 `0x04`–`0x20`), byte 9 the standard buttons.
Evidence: `docs/evidence/vendor-report-usb.md`.

**3. Bluetooth cannot emit that report, by three independent routes.**
`0xEE` exists in two dispatchers but has no Bluetooth producer; the physical chord
`FN + M1 + M2` does set the same flag and acknowledges with a rumble, yet the
Bluetooth stream stays report `0x01` at 21 bytes across two captures (289 and 275
reports, both all-`0x01`). Order does not matter. The Bluetooth builder is
separate from the wired one.
Evidence: `docs/evidence/fe-report-toggle.md`.

**4. The full Bluetooth command surface is now documented**, including the
structural fact that frames of **5–19 bytes reach no handler at all** — which
explains several historical silent failures.
Evidence: `docs/evidence/ble-command-reference.md`.

**5. The bootloader almost certainly checks CRC only.** No 128- or 256-byte
incompressible block exists anywhere in the image, and the updater transmits only
length plus CRC32, so a signature could not be validated. This is an inference
from the interface contract, not an observation of the verifying code.
Evidence: `docs/evidence/bootloader-verification.md`.

## Negative results — do not repeat these

- **Bluetooth DInput motion.** Tried and closed by three routes: vendor command
  (`0xEE`), GATT output path, and the physical chord. All three either cannot
  reach the builder or do not change the Bluetooth report.
- **`A5 A0` is not a battery query.** The `0x50` byte is a *fallback* written when
  the real source is invalid, so it reads 80 % forever. Do not expose it.
- **The `U` opcode is not an arbitrary write.** `0x8472e8` is the key-remap table.
- **The Switch descriptor is not in any shipped image.** It is assembled at
  runtime, which is why a Switch-mode patch is expensive.
- **SDL cannot honour a touchpad mapping on a uinput device.** Six extras cannot
  all be bindable; one slot is unreachable by design.

## The one promising lead left

`13 04 00 00` is matched by `FUN_000105a4` against the literal at flash `0x23f90`,
and calls `FUN_00013dc4()` — a full peripheral teardown with a busy-wait and RF
shutdown — plus `FUN_00013e60()`, which writes registers `0x308`, `4`, `1`.

**It is plausibly the entry point into the firmware-update mode**, which would be
the recovery path that made flashing risky. It is also plausibly just a reset.

Untested. Testing it while wired is the cheap experiment: if the device
re-enumerates as something new, that is a finding; if it simply resets, it costs a
reconnect. Over Bluetooth it would drop the link.

## Next candidates, cheapest first

1. **Six uncharacterised reachable handlers** — `12`, `13 02`, `13 10`, `AC`,
   `BA 40`, `A5 01`. These are where dormant capabilities would live: LED control,
   deadzones, calibration, polling rate. All are reachable over the channel the
   driver already uses.
2. **Derive the vendor report axis layout from `FUN_0000094c`.** The stores are
   authoritative and need no hardware. Captures narrowed it to bytes 19, 21, 22
   (stick-like) and 23, 24 (trigger-like) but could not fix the pairing, so no
   decode code was written against it.
3. **`13 04 00 00`**, as above, with the wiring caveat.

## Environment notes that cost time

- **`local/research/` is gitignored and holds everything**: firmware images,
  decompiled Space Station C#, the Ghidra project, the Android APK and extracted
  assets, captures, and the analysis scripts. It survives reboots; `/tmp` does not.
- The Ghidra TC32 processor module lives **outside** the repository and its
  `tnegs` patch reverts on system-level changes. Re-apply it after any reinstall,
  then verify against function `0x10a30` (780 functions should decompile).
- The flat decompile used for all analysis is
  `local/research/scripts/vader3pro-fixed-decompile.c` — this is the file quoted
  throughout `docs/evidence/`.
- **hidraw ACLs reset on every reconnect.** Expect to ask the owner for
  `sudo chmod a+r /dev/hidrawN` after each power cycle.
- The controller's node moves between `hidraw8` and `hidraw9` across reconnects;
  resolve it by walking `/sys/bus/hid/devices` rather than assuming a number.
- **Capture timing races.** Short windows finish before the owner starts. Use
  long windows (3+ minutes) and analyse by activity detection.
- A transient **digitizer personality** appeared once (`04B4:2412`, 196-byte
  touch-screen descriptor, three finger contacts). It reports only on touch, so
  captures against it are silent. A reconnect restores the gamepad descriptor.
  Cause not isolated.

## Owner preferences worth honouring

- English only, in replies, code, docs and commits.
- Hardware experiments are authorized, including commands that may disconnect.
  When the owner must act — reconnect, grant access, press a button, confirm a
  felt result — **stop and ask with the `question` tool** rather than guessing.
- Never present a successful write as proof the device acted. Only an observed
  response counts.
- Button and control tests must be **one control at a time**, because the owner
  cannot see most console output. Sequences of many presses get confused.
