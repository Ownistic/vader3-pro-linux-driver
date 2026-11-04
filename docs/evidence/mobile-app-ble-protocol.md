# Mobile app BLE protocol (Flydigi Game Center 7.2.10.5)

Runtime capture plus firmware cross-reference for the Android app's gamepad
channel. **Hardware was used**: the controller was connected to a rooted phone
with full HCI snooping, and the app was driven on-screen.

Artifacts (Git-ignored, under `local/research/android/`):

| File | What |
| --- | --- |
| `flydigi-game-center-7.2.10.5.apk` | Base APK, SHA-256 `0af8e56f…6ba7a6`, package `com.game.motionelf` |
| `btsnoop-session1.log` | Classic mode: connect, status polling |
| `btsnoop-flashplay.log` | After switching the controller to Flashplay mode |
| `../scripts/btsnoop_att.py` | ATT write/notify extractor |
| `../scripts/gatt_map.py` | Rebuilds the handle→UUID map from the capture's own discovery traffic |

Capture method: `setprop persist.bluetooth.btsnooplogmode full`, Bluetooth
toggled, log at `/data/log/bt/btsnoop_hci.log` (Samsung path, not
`/data/misc/bluetooth/logs/`).

## The app uses NUS, not a private channel

Rebuilt from the capture's own GATT discovery:

| Handle | UUID | Role |
| --- | --- | --- |
| `0x0024` | `6e400002-b5a3-f393-e0a9-e50e24dcca9e` | **write** — every command |
| `0x0027` | `6e400003-…` | **notify** — every reply |
| `0x002c` | `00010203-0405-0607-0809-0a0b0c0d2b12` | inside service `…-0a0b0c0d**1912**` |

That last row corrects an earlier misreading. The UUID ending `2b12` is a
*characteristic* of a service whose UUID ends `1912`; it is the **Telink OTA**
channel (`com/flydigi/sdk/bluetooth/ota/TelinkOtaFactory` is the only class in the
APK that references it). The data channel was never that characteristic — which
explains why host writes there disconnected the controller while NUS writes
worked. **Do not send data to `0x002c`.**

## What the app actually sends

Every write observed, both modes:

| Frame | Count | Firmware handler | Reply |
| --- | --- | --- | --- |
| `a5 a0 00 00` | 23 | `FUN_00010604` | 13-byte `a5 a0 17 …` |
| `ac b0 00` | 13 | `FUN_00010560` (`0xAC`) | 20-byte `ac c0 …` |
| `ba c0 00 00` | 9 | `FUN_000105cc` (`0xBA 0x40`) | 4-byte `aa cc ff 47` |
| `a5 01 00 00` | 3 | `FUN_000105e8` (`0xA5 0x01`) | 20-byte `a5 01 …` |
| `55 00 01 00 ff…ff 47` | 6 | not traced | 4-byte `aa cc …` |
| `13 11 00 00` | 1 | `FUN_0000b830` case `0x13` | none seen |
| `41 00 …00 41` | 1 | not traced | none seen |

The first four map cleanly onto the **six short-payload handlers** that
`FUN_00010980` calls before `FUN_0000b830`. That is the correction that matters:
those handlers are not a mystery side path, they are the app's command set, and
they are reachable on the same NUS characteristic the driver already writes to.

## The 13-byte poll reply is status, not motion

`FUN_00010604` builds it literally:

```c
local_1c[0] = 0xA5;
local_1c[1] = 0xA0;
local_1c[2] = 0x17;
local_1c[3] = *(u8 *)(state + 0x9b);        /* constant 0x50 on this unit */
FUN_0001c354(&iStack_18, state + 0xd2, 8); /* 8 bytes */
/* then an additive checksum over the 12 bytes -> byte 13 */
```

Captured reply, identical in all 42 samples across both modes:

```
a5 a0 17 50 | 00 c7 00 2e 00 bd ff ff | 5d
```

`state+0xd2..0xd9` is written elsewhere from two 32-bit source words, byte by
byte with `>>8 / >>16 / >>24` shifts — the shape of a packed status record, not
an axis pair. **Rotating the controller for ~10 s changed nothing.** So this is a
heartbeat/status frame and it is *not* the gyro, despite looking like four
16-bit values.

This is worth recording precisely because it is a trap: the frame is short, it
has plausible-looking fields, and it answers a poll.

## Flashplay mode is controller-side

The app's own instructions to enter it:

> Press and hold the **FN + Y** button at the same time until the indicator light
> turns blue to change the controller to the "Flashplay" mode.

So Flashplay is not a BLE command — the controller changes personality on a
button combo, and the app then re-connects. Switching *to* it from the app does
send `13 11 00 00` and a 20-byte `41 …` frame first, and the link drops.

## Firmware version confirmed on hardware

The app reports **6.7.5.1** for this unit, matching the analysed image
(`Telink87_For_F3_Ver6.7.5.1.bin`). The static analysis in the other evidence
files therefore describes the firmware that is actually running.

## What this does not settle

- **No motion stream was observed.** Neither mode produced a report whose fields
  changed under rotation.
- The app's gyro/key-mapping UI (`Config management`) is gated behind a Flydigi
  **account login**, so the "enable motion" step you remembered could not be
  reached. That screen is where the missing command would be.
- `55 …` and `41 …` were not traced to firmware handlers.
- Whether the `A5` dialect has a vendor-mode equivalent of `0xEE` is unknown. The
  short handlers that were read (`0xAC`, `0xBA 0x40`, `0xA5 0x01`, `0xA5 0xA0`)
  contain no `state+0x2a` write, and the flag sweep in
  [gyro-deferred.md](gyro-deferred.md) still shows only the two `0xEE` arms.
