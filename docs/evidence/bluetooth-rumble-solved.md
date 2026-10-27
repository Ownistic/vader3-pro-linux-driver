# Bluetooth DInput rumble: solved

Rumble over Bluetooth DInput works on stock firmware 6.7.5.1 with no
modification. This document records the command, the firmware path that
implements it, the hardware evidence, and the corrections to earlier
conclusions in this project that this result falsifies.

Verified on hardware 2026-10-26 against `d7d7:0041` in Bluetooth DInput mode.

## The command

Write a **4-byte** frame to the Nordic UART RX characteristic:

```
11 <left> <right> <duration_ms>
```

| Field | Meaning |
| --- | --- |
| `0x11` | rumble subcommand |
| `left` | left motor strength, 0–250 (`0xFA` = full) |
| `right` | right motor strength, 0–250 |
| `duration_ms` | milliseconds before the motors stop on their own |

**Transport:** GATT characteristic `6e400002` — the **Nordic UART service**
(the service the firmware labels `OTA`), not the vendor `1A12` service.

**Sustained rumble:** re-send faster than the duration expires. 100 ms intervals
with `duration = 200` produced continuous vibration.

**Stop:** send `11 00 00 00`.

## Hardware evidence

| Test | Frames sent | Result |
| --- | --- | --- |
| Single burst, full strength | 16 × `11 FA FA FA` over 3.2 s | **vibrated**, sustained |
| Control A (zero strength) | 12 × `11 00 00 C8` | no vibration |
| Control B (full strength) | 14 × `11 FA FA C8` | **vibrated** |
| Control C (zero strength) | 11 × `11 00 00 C8` | no vibration |
| Intensity step | low `0x30` → high `0xFA` → medium `0x80`, 2 s each with 2 s silent gaps | felt in **exactly that order**, with distinguishable strength |

The intensity ordering is the strongest evidence: the owner reported weak →
strong → medium, matching the sent sequence. A false positive would not
reproduce a three-level ordering.

## Firmware path

`FUN_00010980` is the GATT write payload dispatcher:

```c
uVar1 = *DAT_00010a28;              /* payload length, 0x8472b8 */
if (uVar1 != 0) {
    *DAT_00010a28 = 0;
    pcVar2 = DAT_00010a2c;          /* payload buffer, 0x8472d4 */
    if (uVar1 < 5) {
        if (uVar1 == 4) {
            FUN_00010604(pcVar2);
        }
        FUN_0001067c(pcVar2);
        FUN_00010560(pcVar2);
        FUN_000105cc(pcVar2);
        FUN_000105e8(pcVar2);
        FUN_000105a4(pcVar2);
        FUN_0000b830(pcVar2, 4);    /* subcommand dispatch */
    }
    else if (uVar1 == 0x14) {
        /* additive checksum over bytes 0..18, compare buf[19] */
        if (DAT_00010a2c[0x13] == cVar4) FUN_000107a0(DAT_00010a2c);
    }
}
```

`FUN_0000b830` then switches on the first byte:

| `buf[0]` | Action |
| --- | --- |
| `0x10` | `FUN_0000cb94(buf[1], buf[2], buf[3], 5)` — key remap |
| `0x11` | `FUN_0000b62c(buf[3], buf[1], buf[2])` — **rumble(duration, left, right)** |
| `0x12` | `FUN_0000ceb4(buf[1], buf[2], buf[3])` |
| `0x13` | mode switches (`buf[1]` = 2 / 4 / 5 / 0x10 / 0x11) |

Note the argument order: the byte at index 3 is the **duration**, and indices
1 and 2 are the motor levels. Sending them the other way round produces a very
short buzz at full strength rather than a sustained one.

`FUN_0000b62c` is the same PWM primitive used by the USB path, so the rumble
hardware behaviour documented in
[vader3pro-firmware-rumble.md](vader3pro-firmware-rumble.md) applies unchanged:
levels clamp to 250, and there is no start/stop latch — only the timeout.

Call chain from the periodic task:

```
FUN_00008a60  (main loop)
  -> FUN_00010838   writer of the payload buffer and length
  -> FUN_00008818   gates on *(0x846ac8+0xd) == 8
       -> FUN_00010980   payload dispatcher
            -> FUN_0000b830 -> FUN_0000b62c   rumble
```

## Why every earlier attempt failed

The handler accepts **only** `len == 4` or `len == 0x14`. Every frame tried
before this used a different length:

| Frame | Length | Outcome |
| --- | --- | --- |
| `BA 70 70 70 70` | 5 | dropped (`len` is not 4 or 20) |
| `05 0F 70 70` + pad | 12 | dropped |
| `A5 12 70 70` + pad | 15 | dropped |
| `5A A5 00 05 0F 70 70` + pad | 20 | took the ATT-opcode path, checksum failed or opcode unrecognised |

This was a length-gate problem, not a transport problem, a descriptor problem,
or a BlueZ problem. Every previous conclusion that blamed those is superseded.

## Corrections to earlier documents

Several conclusions in this project are now falsified. They are listed here
rather than quietly edited, because the reasoning errors matter:

1. **"Bluetooth DInput declares no output report, so rumble is doubtful."**
   Misleading. The descriptor fact is true, but rumble does not use the HID
   output path at all — it uses the Nordic UART service. The descriptor was
   irrelevant to this requirement.

2. **"The inbound command queue is fed only by the wired USB validator, so
   Bluetooth has no path to the command dispatcher."** True for
   `FUN_00018138` / `FUN_000183c8`, but those are not the only dispatchers.
   `FUN_0000b830`, reached from the GATT payload handler, is a separate path
   that the queue analysis did not cover. The conclusion "not possible without
   a device-side change" was wrong.

3. **"A firmware patch is required."** Not required. No device-side change is
   needed for Bluetooth rumble.

4. **"The vendor `1A12` characteristic is the OTA channel."** Wrong. The `OTA`
   label belongs to the Nordic UART service at `0x239c4`, which is the correct
   target. Writes to `1A12` disconnect the controller because it is the wrong
   channel.

5. **"BlueZ blocks HID-service writes, so the device was never asked."** Still
   true as a fact, but it was never the blocker for rumble.

The root error across several documents was treating the absence of a HID
output report as the absence of any write path, when the device exposes a
second, independent command service over GATT.

## Reproduction

```
# one 250 ms full-strength burst
11 FA FA FA

# sustained: repeat faster than the duration
11 FA FA 64   every 50 ms

# stop
11 00 00 00
```

Scripts: `four_byte_rumble.py`, `rumble_ab_test.py`, `rumble_levels.py`
(outside the repository).

## Still open

- Whether the same 4-byte command also reaches `0x10` (key remap) and `0x13`
  (mode switches) over Bluetooth — untested, and `0x10` is the interesting one
  because it could make M1–M4 remapping host-configurable at runtime.
- The exact relation between the `duration_ms` byte and real elapsed time. 200
  was used throughout; finer values are unmeasured.
- Whether the motor-pair B bytes exist in this command form (the 4-byte frame
  carries one pair only).
- Trigger haptics over Bluetooth — untouched, and the firmware implements it on
  a separate subsystem.
