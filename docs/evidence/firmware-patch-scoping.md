# Firmware patch scoping: Bluetooth rumble on the Vader 3 Pro

Scope assessment for adding a Bluetooth command path to firmware 6.7.5.1 so that
rumble can be driven over the Bluetooth DInput link. This document records what
a patch would have to change, what is and is not known about the update
mechanism, and the risks. **No patch has been built, tested, or flashed.**

Target artifact: `Telink87_For_F3_Ver6.7.5.1.bin`
(`f3_v6.7.5.1.zip`), 152068 bytes, SHA-256 `6cba3054…53e20a`.

## Why a patch is the remaining option

Rumble works over USB DInput (`05 0F` to `mi_02`, verified on hardware). Over
Bluetooth DInput it cannot work through any host-side route, for two independent
reasons established in
[bluetooth-queue-topology.md](bluetooth-queue-topology.md):

1. The Bluetooth DInput descriptor declares **zero output reports**, so there is
   no HID channel for a host to send anything.
2. The inbound command queue (`0x00845e14`) is fed only by the wired USB frame
   validator (`FUN_00000d54` ← `FUN_00001014` ← hardware register `0x00800f20`).

A Bluetooth write can reach `FUN_00019fa4`, and that function does dispatch to
`FUN_000183c8`, whose `A5 12` subcommand calls the rumble primitive
`FUN_0000b62c(500, L, R)`. But hardware testing of exactly that frame over the
Nordic UART service produced no vibration, so either the write does not reach
`FUN_00019fa4`, or the path needs preconditions not yet met. **This is the one
open question that would make a patch unnecessary**, and it is unresolved.

## What the patch would have to do

Two viable shapes, in increasing invasiveness.

### Option A: reach rumble from the existing Bluetooth dispatcher

Add a rumble branch to `FUN_00018828`, which is already called from the
Bluetooth receive path (`FUN_00019a50` → `FUN_00018828`) with
`buf = 0x008477cc`, `len = *(0x0084652a)`.

The function already has the exact rumble pattern in its short-frame branch:

```c
FUN_0000b62c(500, cVar5, cVar4);          /* pair A */
FUN_0000b6a4(500, pcVar2[3], pcVar2[4]);  /* pair B */
```

**Size:** roughly 20–30 bytes of TC32. A trampoline into a code cave is
unnecessary — the branch can be inserted in place.

**Problem:** this only helps if some GATT write reaches `FUN_00018828`, which is
unproven and which the hardware tests contradict. Option A is therefore
conditional on resolving that first.

### Option B: add a Bluetooth output report and route it to rumble

Follow the Bluetooth **XInput** personality, whose descriptor at `0x24098`
declares an output report the DInput descriptor lacks. That gives a working
host→device HID channel over Bluetooth, and the report handler can call
`FUN_0000b62c` directly.

**Size:** descriptor edit plus handler, tens of bytes.

**Problem:** the DInput descriptor is a fixed 163-byte blob at `0x23764`, and
the attribute table pointers around `0x23800`–`0x23a10` are position-sensitive.
Appending an output report item changes the descriptor length, which the
attribute entry stores. There is no room to grow in place — the region
`0x23764`–`0x23800` holds the descriptor and the attribute table starts
immediately after at `0x23808`.

### Space available

| Region | Size | Content |
| --- | --- | --- |
| `0x04750`–`0x07fff` | 14513 B | zero fill, plausible code cave |
| `0x03d6f`–`0x04000` | 641 B | zero fill |
| `0x03b63`–`0x03d64` | 513 B | zero fill |
| `0x23b50`–`0x23d7a` | 554 B | `0xFF` fill |

Space is not the constraint.

## The update mechanism

`OtaSerialPortUpdater` (decompiled from Space Station 3.4.4.3) is the updater:

| Property | Value |
| --- | --- |
| Transport | USB serial port, 921600 8N1 |
| Commands | `1` GetBaseAddr, `2` GetVersion, `3` EraseSector, `5` WriteData, `9` ResetDevice |
| Chunk size | 55 bytes |
| Integrity | **CRC32 only**, computed over the whole image |

Integrity check at reset (`CmdType.RestDevice`):

```csharp
uint firmwareLength = (uint)DataAllArray.Length;
int  crc            = AppOtasCrcCal(firmwareLength);   /* CRC32 over image */
otaSendBuffer[num++] = 8;                               /* ResetDevice */
Buffer.BlockCopy(BitConverter.GetBytes(firmwareLength), 0, otaSendBuffer, num, 4);
Buffer.BlockCopy(BitConverter.GetBytes(crc),            0, otaSendBuffer, num + 4, 4);
```

`crcTable` is the standard reflected CRC-32 table (first entries `0`,
`1996959894`, `3993919788`…), so a modified image needs its CRC recomputed.

**No signature or asymmetric verification appears anywhere in the updater.**
`FirmwareUpgradeUtils.AESkey` is declared as `public static string AESkey` and
referenced exactly once in the whole decompiled application — its own
declaration. It is never assigned and never used.

**This is the single most important unknown.** Absence of verification in the
Windows application does not prove absence in the bootloader. The bootloader is
not in the OTA image we have. Without it, we cannot rule out that the device
verifies a signature before accepting a new image, and no amount of application
analysis settles it.

## Image header

First 32 bytes of the OTA image:

```
0x00  0e 80 06 03 02 01 5d 02 4b 4e 4c 54 0d 00 88 00
0x10  de 80 00 00 00 00 00 00 04 52 02 00 00 00 00 00
```

`0e 80` reads as 0x800E little-endian, a plausible entry vector. `KNLT` at
offset 8 is the ASCII tag used by the Telink OTA toolchain. The meaning of the
remaining fields is not established here and would need to be, because the
bootloader may validate any of them.

## Risks

| Risk | Assessment |
| --- | --- |
| Device verifies a signature | **Unknown.** Would make the patch impossible without FlyDigi's key |
| Bootloader rejects modified image | Possible even with valid CRC; header fields may be checked |
| Bricking | Interrupted flash write, or an image that passes CRC but fails a later check, leaves a device with no working firmware |
| No recovery path | No SWD/UART pads identified on the controller. Recovery likely requires opening the shell |
| Losing the stock image | Mitigated: the stock 6.7.5.1 image is retained in `local/research/firmware/` |

The device is the owner's only unit of this model and is required for daily use.
A brick is not a recoverable inconvenience here.

## Recommended order

1. **Resolve the `FUN_00019fa4` reachability question.** If a GATT write can
   reach that handler, `A5 12` rumble may already be possible and no patch is
   needed. This is cheap, needs no flashing, and the answer changes everything.2. **Establish whether the device verifies uploaded images.** Read the installed
   version, and if possible observe a legitimate OTA update. This bounds the
   whole project.
3. **Only then** implement the minimal branch from Option A, if step 1 shows the
   handler is reachable but the existing subcommands are wrong.
4. **Option B** last, because the descriptor is fixed-size and the change is
   structurally riskier.

## What is not yet known

- Whether the installed firmware matches 6.7.5.1.
- Whether the bootloader validates a signature, and what header fields it checks.
- Whether `FUN_00019fa4` is reachable from any GATT write.
- The meaning of the OTA header fields beyond the entry vector and `KNLT` tag.
- Whether the device exposes any recovery interface without disassembly.
