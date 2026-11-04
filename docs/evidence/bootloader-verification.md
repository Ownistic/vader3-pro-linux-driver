# Does the bootloader verify uploaded firmware?

> **Correction:** This historical analysis does not establish that the image
> lacks a signature or that the bootloader is CRC-only. A windowed entropy test
> cannot exclude a shorter, encoded, distributed, or external signature. The
> updater's displayed reset fields do not describe every byte of the image or
> every possible device-side check. The verifier and recovery path remain
> unknown. The suggested modified-image flash is not established as safe and
> must not be treated as a harmless or lossless test.
> The cited `OtaSerialPortUpdater` is referenced in the saved Space Station
> decompilation by `FmUpgradLCDViewModel.StartScreenUpgrade`, specifically its
> screen `freq` branch. Its protocol has not been connected to the Vader 3 Pro
> main-MCU updater. The CRC and command table below must not be attributed to
> that controller without an independently traced caller.

The question that bounds any firmware patch: if the device checks a signature we do
not have, a modified image can never be flashed.

Nothing here is a hardware test. Everything below is static analysis of the OTA
image and of the vendor's updater.

Target: `Telink87_For_F3_Ver6.7.5.1.bin`, 152 068 bytes, SHA-256 `6cba3054…`.

## Evidence that the contract is CRC-only

**1. The image contains no signature.** An RSA-2048 signature is 256 bytes of
incompressible data, and even an ECDSA signature is 64. Scanning the whole image
with sliding windows:

| Window | Regions with entropy > 7.85 bits/byte |
| --- | --- |
| 128 bytes | **0** |
| 256 bytes | **0** |

The head is 32 bytes of structured metadata followed by code; the tail is
structured (entropy 3.22) rather than random. There is nowhere for a signature to
live, so there is nothing for a bootloader to verify against.

**2. The updater transmits no signature.** At reset it sends only the image length
and a CRC32:

```csharp
otaSendBuffer[num++] = 8;                               /* ResetDevice */
Buffer.BlockCopy(BitConverter.GetBytes(firmwareLength), 0, otaSendBuffer, num, 4);
Buffer.BlockCopy(BitConverter.GetBytes(crc),            0, otaSendBuffer, num + 4, 4);
```

The whole command set is `1` GetBaseAddr, `2` GetVersion, `3` EraseSector,
`5` WriteData, `9` ResetDevice. There is no key exchange, no signature slot, and no
verification result reported back.

**3. The application's only crypto field is dead.** `FirmwareUpgradeUtils.AESkey`
is declared `public static string`, referenced exactly once — its own declaration —
and never assigned or read.

Taken together: the host side is a CRC-only protocol, and the image has no
signature to check. If the bootloader verified signatures, the vendor's own
updater could not work.

## What this does not prove

The bootloader itself is **not in any image we hold**. The shipped file is the
application; the bootloader precedes it and is never transmitted. So its actual
checks cannot be read, and "CRC-only" remains an inference from the interface
contract rather than a direct observation of the verifying code.

A CRC-only bootloader still rejects a corrupt image, and it may also validate
header fields whose meaning is not established:

```
0x00  0e 80 06 03 02 01 5d 02 4b 4e 4c 54 0d 00 88 00
0x10  de 80 00 00 00 00 00 00 04 52 02 00 00 00 00 00
```

`0e 80` and `de 80` look like little-endian addresses; `KNLT` at offset 8 is the
Telink tag. Anything else in those 32 bytes could be checked.

## Recovery, which is the real constraint

Even if the gate is permissive, a failed flash has to be recoverable, and it is not:

- **No read command.** The OTA protocol can erase and write but not read, so the
  bootloader cannot be dumped from the device to inspect its checks.
- **No separate boot-mode device for this model.** `SerialPortManager.GetPortWithVidPid("FFAA", "5555")`
  appears only in `FmUpgradLCDViewModel`, the LCD-model upgrade path — not in the
  Vader 3 Pro flow. The controller's update runs through its normal interface, so
  entering the bootloader depends on the application still running.
- **No SWD/UART pads identified.** Recovery would mean opening the shell.

That combination — unusable recovery, unverifiable gate, one daily-use unit — is
why a patch should not be attempted until the gate is settled by a low-risk test.

## The low-risk test, if the question is ever pursued

Rather than flashing a functional change, flash a **no-op modification**: alter a
single byte in a region that is provably unused (for example inside the zero-fill
code cave at `0x04750`), recompute the CRC32, and flash. The firmware behaviour is
unchanged by construction, so:

- If the device boots and still reports **6.7.5.1**, the gate accepted a modified
  image and accepts a recomputed CRC — the patch route is open.
- If it rejects the image, nothing was lost except the flash attempt, and we learn
  the gate is stricter than the interface suggested.

This isolates the verification question from the patch itself, which is the right
order. It is still a flash write with the recovery limits above.

## Conclusion

**Statically, the evidence points to CRC-only and no signature.** That is the
strongest conclusion the artifacts support, and it is an inference, not a
demonstration. The decisive information sits in code we cannot obtain, on a device
that cannot be dumped, with no confirmed recovery path.
