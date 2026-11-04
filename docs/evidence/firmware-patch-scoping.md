# Firmware patch scoping: gyro while keeping the extra buttons

Goal: one personality that reports **both** the M1–M4/C/Z buttons and motion, so a
single Steam device has everything.

Status: **scope only. No patch has been built, tested, or flashed.**

Target artifact: `Telink87_For_F3_Ver6.7.5.1.bin` (`f3_v6.7.5.1.zip`),
152 068 bytes, SHA-256 `6cba3054…53e20a`. The owner's unit reports **6.7.5.1**
from the mobile app, so this image matches the device.

> Rumble no longer motivates a patch: it works on stock firmware over the Nordic
> UART service ([bluetooth-rumble-solved.md](bluetooth-rumble-solved.md)). The
> update-mechanism, header and risk sections below are retained because they
> apply to any patch.

## The two shapes

### Option A — add motion to Bluetooth DInput (preferred)

Keep the DInput personality, which already has every button, and make the
firmware emit its vendor report (with motion) over the existing link.

Evidence that this is close: the sensor path is gated on a **mode byte**, and both
branches are already understood.

```c
/* FUN_0000a704 */
if ((*(char *)(state + 0x11) == 2) || (*(char *)(state + 0x76) == 1)) {
    FUN_0000a598(param_1);      /* use real sensor values */
} else {
    FUN_0000b0c4();             /* otherwise fill 0x05DC (1500) defaults */
}
```

- `state+0x11` is the personality: `FUN_00011bbc(2)` sets **2** (Switch) and
  `FUN_00011bbc(1)` sets **1** (DInput).
- `state+0x76` is the sensor enable that `0x13 05 01` already sets over Bluetooth
  (`FUN_0000b500`). That command **is** reachable from the host today — it is the
  `0x13` case in `FUN_0000b830`.
- The vendor report builder `FUN_0000094c` exists and reads `state+0xde` to choose
  the vendor layout.

So in principle a host command could put DInput into "use the sensors" state
without changing personality.

**The blocker is not reachability, it is this:** what `FUN_0000094c` writes into
the motion bytes has not been traced to a live source. The two 16-bit fields it
copies (`state+0x90`, `state+0x92`) belong to `FUN_0000a598`, which is a
**min/max calibration** accumulator, not a live sample. Until the actual sample
source is identified, "the vendor report carries gyro" is an assumption inherited
from host-side inference.

> **RESOLVED.** Hardware testing over USB shows the vendor report at 500 Hz with
> fields that are flat when still (spreads of 14–80 counts) and swing by
> thousands when rotated. Vendor mode therefore carries live motion, and the
> sample-source question no longer blocks the plan. See
> [vendor-report-usb.md](vendor-report-usb.md).

Before any patch, that assumption must be tested — see *Recommended order*.

### Option B — add the extra buttons to Switch mode

Checked and rejected for now:

1. Switch mode exposes **only** HID over GATT (`00001124`) and PnP (`00001200`).
   No NUS, no vendor service, so no side channel exists.
2. Every declared button slot is allocated by the Switch protocol; `0x30` has the
   standard layout and `0x3F` a 16-bit field.
3. The 170-byte Switch report map is **not a literal in any shipped image**
   (main, `f3si`, dongle), so it is assembled at runtime. The DInput descriptor by
   contrast is a literal at `0x23764`, which is why that personality is tractable.

A patch here would also require a custom host parser for the added report, since
`hid-nintendo` would ignore it.

## Space available

| Region | Size | Content |
| --- | --- | --- |
| `0x04750`–`0x07fff` | 14 513 B | zero fill, plausible code cave |
| `0x03d6f`–`0x04000` | 641 B | zero fill |
| `0x03b63`–`0x03d64` | 513 B | zero fill |
| `0x23b50`–`0x23d7a` | 554 B | `0xFF` fill |

Space is not the constraint. For Option A the descriptor needs no change at all,
which removes the structural risk that dominated the old rumble plan.

## The update mechanism

`OtaSerialPortUpdater` (Space Station 3.4.4.3):

| Property | Value |
| --- | --- |
| Transport | USB serial, 921600 8N1 |
| Commands | `1` GetBaseAddr, `2` GetVersion, `3` EraseSector, `5` WriteData, `9` ResetDevice |
| Chunk size | 55 bytes |
| Integrity | **CRC32 only**, over the whole image |

```csharp
uint firmwareLength = (uint)DataAllArray.Length;
int  crc            = AppOtasCrcCal(firmwareLength);   /* CRC32 over image */
otaSendBuffer[num++] = 8;                               /* ResetDevice */
```

`crcTable` is the standard reflected CRC-32 table, so a modified image needs its
CRC recomputed.

**No signature verification appears anywhere in the updater.** `AESkey` is
declared, referenced once, and never assigned or used. That does **not** prove the
bootloader behaves the same — the bootloader is not in this image.

## Image header

```
0x00  0e 80 06 03 02 01 5d 02 4b 4e 4c 54 0d 00 88 00
0x10  de 80 00 00 00 00 00 00 04 52 02 00 00 00 00 00
```

`0e 80` is a plausible entry vector; `KNLT` at offset 8 is the Telink tag. The
remaining fields are not established, and the bootloader may check any of them.

## Risks

| Risk | Assessment |
| --- | --- |
| Bootloader verifies a signature | **Unknown.** Would end the project without FlyDigi's key |
| Bootloader rejects a modified image | Possible even with a valid CRC |
| Bricking | No SWD/UART pads identified; recovery likely needs opening the shell |
| Side effects of `0x13 05 01` | **Observed once**: the controller disconnected and the bond was lost on this unit |
| Losing the stock image | Mitigated: 6.7.5.1 retained in `local/research/firmware/` |

The owner has one unit of this model and needs it daily. A brick is not a
recoverable inconvenience.

## Recommended order

1. **Test whether the vendor report carries live motion, over USB.** Wired DInput
   reaches the vendor path unmodified (`FUN_00018138`, verified for rumble on
   `mi_02`). Send `05 EE 01`, rotate the controller, and check bytes 11–13 of the
   resulting `04 FE` frames for movement. **No flashing, no risk** — and it either
   validates Option A or kills it before any code is written.
2. **Identify the motion sample source** in `FUN_0000094c` / `FUN_0000a598` if
   step 1 shows movement. This is the work the patch would depend on.
3. **Establish whether the bootloader verifies uploaded images**, ideally by
   observing a legitimate OTA with the network captured. This bounds everything.
4. **Only then** write the Option A branch: one arm that sets `state+0x76`, in a
   code cave, with the CRC recomputed.

## What is not yet known

- Whether the vendor report carries live motion at all (step 1 decides this).
- Which function produces the live IMU sample that `FUN_0000094c` would copy.
- Whether `0x13 05 01`'s bond reset is inherent or incidental to that selector.
- Whether the bootloader validates a signature, and which header fields it checks.
- Whether any recovery interface exists without disassembly.
