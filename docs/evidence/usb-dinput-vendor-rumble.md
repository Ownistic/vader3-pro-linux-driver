# USB DInput vendor HID: rumble works (hardware-verified)

Status: **observed** — the owner felt the controller vibrate, repeatedly and
sustained. This is the first end-to-end hardware confirmation of a vendor command
in this project, and it corrects the earlier "rumble is blocked in DInput"
conclusion, which was based on the Bluetooth personality only.

Companion to `vader3pro-firmware-rumble.md` (firmware PWM path) and
`bluetooth-dinput-mode.md` (the Bluetooth personality, which is different).

## What was tested

| Item | Value |
|---|---|
| Host | Bazzite-class Linux, kernel reported via `uname -r` at test time |
| Transport | USB cable, direct port `3-6.3.2` (12 Mbps) |
| Controller mode | DInput, native personality `04b4:2412` |
| Device string | `Flydigi VADER3` |
| Firmware installed | **not read** (the version query never answered; see Open items) |
| Firmware analysed | `Telink87_For_F3_Ver6.7.5.1.bin`, SHA-256 `6cba3054…6ae6a53e20a` |
| Requirement tested | "Bluetooth rumble" prerequisite: does the vendor rumble command work at all |

## The finding

`04b4:2412` exposes **four** USB HID interfaces, all bound to `usbhid`. Only one
of them is the command channel, and it is identified in the Windows app by the
device-path substring **`mi_02`** (`HID.cs:120`).

| Interface | Usage page | Descriptor size | Reports | Role |
|---|---|---|---|---|
| `mi_00` | Generic Desktop (`0x01`) | 125 B | input, no report ID | gamepad state |
| `mi_01` | Generic Desktop (`0x01`) | 70 B | input report ID 2 | mouse/keyboard |
| **`mi_02`** | **vendor `0xFFA0`** | **44 B** | **input ID 4 = 31 B, output ID 5 = 31 B** | **command channel** |
| `mi_03` | vendor `0xFFEF` | 31 B | ID 5 = 63 B input/output/feature | secondary vendor |

The `mi_02` report descriptor, read from sysfs and verified byte-identical to the
firmware image at `0x2412c` family:

```
06 a0 ff 09 01 a1 01 09 03 85 04 15 80 25 7f 35 00 45 ff 75 08 95 1f 81 02
09 05 85 05 15 80 25 7f 35 00 45 ff 75 08 95 1f 91 02 c0
```

Note the logical range is **signed** (`-128..127`), which matters for level
values: `0xFA` is `-6` if the device interprets it as signed. `0x70` is safe in
either interpretation and is the value the Windows app itself uses
(`getTestMotorCommand()` → `112, 112`).

## The command that worked

Write **32 bytes** to the `mi_02` hidraw node:

```
05 0F  70 70  00 × 28
│  │   └─┴── left, right motor level
│  └──────── 0x0F = rumble subcommand
└─────────── 0x05 = HID REPORT ID (not a command byte)
```

Sustained rumble by **re-sending**: 14 writes of `05 0F 70 70` at 0.3 s intervals
produced roughly 4.2 s of continuous vibration, then `05 0F 00 00` stopped it. This
matches the firmware's hard 500 ms timeout in `FUN_0001a0cc` — each new command
arrives before the previous expires, so the motors never time out. The owner also
reported that a first burst ran about 5 s while a second felt slower, consistent
with the motors still spinning down between bursts rather than with any protocol
difference.

### What did NOT work, and why it matters

The channel is specific, and two plausible-looking alternatives failed:

| Attempt | Result |
|---|---|
| A 64-byte `05 0F FA FA` written to **both** `mi_03` and `mi_02` in one run | **Felt** (ambiguous which node acted) |
| 12 × `05 0F FA FA` on `mi_03` (`0xFFEF`) alone, 0.35 s apart | **Nothing** |
| 14 × `05 0F 70 70` on `mi_02` (`0xFFA0`), 0.3 s apart | **Sustained** |

So `mi_02` is the confirmed command channel, and it is the one the Windows app
selects via its `mi_02` path check. Writing to `mi_03` is accepted by the kernel
and the HID layer but produces no motor activity on its own, which is exactly
the kind of "write succeeded" false positive this project has to keep
distinguishing from an observed device response.

Level values also matter: `mi_02` declares a **signed** logical range, so `0xFA`
is `-6`. `0x70` is unambiguous and is the app's own default.

## Why the earlier Bluetooth attempts failed

Two distinct reasons, both now explained:

1. **The leading `5` is a HID report ID, not a command byte.** Every Windows
   command builder starts with `5` because it is `REPORT_ID 5` in the `0xFFEF`
   descriptor. Over Bluetooth there is no such output report — the BT DInput
   descriptor declares **seven input items and zero output items** — so a `05 …`
   frame has nowhere to go and the firmware's frame parser discards it.
2. **The USB vendor endpoints in XInput mode stall.** Interfaces `1.1` and `1.2`
   of `045e:028e` reject every write including an all-zeros buffer
   (`errno 5`, endpoint STALL). The only endpoint that accepted writes in XInput
   mode was interface 0's Xbox interrupt OUT `0x05`.

## The consequence that shapes the design

`04b4:2412` in DInput mode advertises **no force feedback**: `EV_FF` is absent
and `ffbit = 0` on its input nodes. So:

- Steam Input cannot rumble a DInput-mode Vader 3 Pro through the normal FF API.
- The hardware *can* rumble; only the standard interface is missing.
- A userspace daemon can bridge this: accept FF from the kernel/Steam, translate
  to `05 0F <L> <R>`, and re-send every <500 ms.

That is a software problem on the host, not a hardware limitation, and it does
not require firmware modification.

## Open items

- **Installed firmware version is still unknown.** The `05 11` query did not
  produce a distinguishable response; the input stream is continuous and the
  reply could not be isolated. The analysed image is 6.7.5.1 but that is the
  downloadable build, not proof of what is flashed.
- Rumble over **Bluetooth DInput** remains untested and is blocked by the absence
  of any output report in that descriptor.
- Motor pair B (bytes 4/5, which the Windows builder never fills) is still
  unverified on this model — see `vader3pro-firmware-rumble.md`.
- The `mi_03` (`0xFFEF`) node also accepts writes; it was not isolated as the
  cause of the first success. `mi_02` is the confirmed channel.

## Reproducing

Helper scripts are private (Git-ignored): `local/research/scripts/`. The
controller's hidraw node number changes on every reconnect, so locate it by
descriptor rather than by name:

```sh
for d in /sys/bus/hid/devices/0003:04B4:2412.*/; do
  xxd -p "$d/report_descriptor" | tr -d '\n' | grep -q '^06a0ff' \
    && ls "$d" | grep hidraw
done
```

Requires write access to that `/dev/hidrawN` node (an ACL is sufficient and
does not grant access to other devices).
