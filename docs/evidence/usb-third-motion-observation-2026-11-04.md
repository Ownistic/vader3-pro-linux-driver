# USB third motion component — 2026-11-04

## Scope and versions

Owner reported the running UHID candidate correct except the missing third
gyro axis. Its daemon SHA-256 remains
`616d9e1b5f2e808561ea2213271e0c5db6136525cbce031caffd6a501da4c715`.
No product code or daemon deployment changed during this investigation.

The owner selected USB cable investigation. Hardware: controller connected
directly to the Legion Go / Bazzite (`ssh legion`). Bluetooth disconnected on
USB connection. First cable produced no physical USB enumeration; after the
owner retried a known data cable, USB enumerated. No controller command,
firmware write, mode switch or stream-enable command was sent by this pass.

Firmware analysis contract:
[USB full-motion report](usb-full-motion-contract-static.md), artifact version
6.7.5.1, SHA-256
`6cba30540a0c3ba16acad2b62bb4dd64dfccc28add04914c3720c6ae6a53e20a`.
The preceding standard BLE firmware revision read returned `6.7.5.1`; this is
a version-string match, not a hash of the installed image.

## Descriptor-level observations

Physical USB device `04b4:2412`, product `Flydigi VADER3`, four readable HID
interfaces at the time of capture:

| Node then | Descriptor bytes | Descriptor SHA-256 | Observed role |
| --- | ---: | --- | --- |
| `/dev/hidraw9` | 125 | `93fa34f70ff8a3182bd885c79c1e3f0a5788f6ced8d7ca435bb8ad0cbb9acfd5` | ordinary gamepad; 9-byte reports |
| `/dev/hidraw10` | 70 | `1dd301f8aea75b84aae7639d0c75e015965f7ff25a0dcc454a61378de58ac1c9` | mouse-like descriptor |
| `/dev/hidraw11` | 44 | `179cdab7b42e67f08b19ed11aa3222766441cbcec28d91d37fea9061d7803747` | vendor page `FFA0`, ID 4 input, ID 5 output |
| `/dev/hidraw12` | 31 | `5c67f115658285001c4cb95250128c987242999145a0e15f073645f47d47c204` | separate vendor collection |

Node numbers are observations, not stable discovery identifiers. The emulated
`054c:0df2` controller at `/dev/hidraw8` is not a physical Flydigi interface.

## Passive report observations

Five-second read-only capture from all physical HID interfaces:

- 2,469 reports on the vendor interface: exactly 32 bytes, prefix `04 FE 66`.
- 2,416 ordinary gamepad reports: exactly 9 bytes, not the Bluetooth 21-byte
  DInput shape expected by the existing canonical parser.
- No writes and no activation command were required for these observations.

Private remote capture directory:
`/tmp/flydigi-usb-passive-e0u2ei9g/` (directory mode 0700; raw captures 0600).
Raw reports are not committed or printed in full.

## Owner-coordinated roll observation

Owner selected **Rolling now** after instructions to lift one grip and lower
the other, reverse, and keep sticks/buttons untouched. An immediate eight-second
passive capture of the vendor interface received 3,943 matching reports and no
other report shapes. Fields were independently decoded from the static contract:

| Firmware state word | USB byte positions, low/high | Minimum | Maximum | Changed samples |
| --- | --- | ---: | ---: | ---: |
| `+ae` | 18, 20 | -17 | 13 | 471 |
| `+b0` | 26, 27 | -38 | 46 | 597 |
| `+b2` | 29, 30 | -8,322 | 8,501 | 798 |
| `+b4` | 11, 12 | -3,089 | 3,568 | 429 |
| `+b6` | 13, 14 | -1,760 | 518 | 428 |
| `+b8` | 15, 16 | 2,005 | 4,433 | 428 |

Words were read as signed 16-bit little-endian values, including the
non-contiguous `+ae` pair. Capture duration: 8.000154676999955 seconds.
Private capture: `/tmp/flydigi-usb-roll-rpgdmmhi/reports.bin`, SHA-256
`652b79191728ca738f7548d66c8bfb9689bdf799f90cb828f845fd23ccc216c8`.

## Interpretation and limits

The dominant `+b2` response during owner-confirmed roll, with the two known
components comparatively small, is strong hardware evidence for the missing
roll component on this USB path. The changing sample words also show that this
was not merely a fixed USB report being replayed by a stale builder.

This does not establish physical units, scale calibration, third-axis sign,
all three USB-to-Steam assignments, alternative debug-frame discrimination, or
USB rumble/control mapping. The vendor report contains ordinary inputs too, but
that decoder still needs independent implementation and hardware checks.

The working driver has not gained USB full-motion support from this research.
In particular, a USB third word does not recover one on the Bluetooth link.
Integration must identify the correct physical vendor interface, maintain all
ordinary/extra controls, handle stale data/reconnect, suppress sibling physical
input interfaces safely, and use a proven USB rumble path rather than assuming
the Bluetooth writer still applies.

The owner subsequently chose **Bluetooth-only scope**, declining USB backend
integration. USB was unplugged, Bluetooth DInput reconnected, and the unchanged
v4 daemon logged physical reattachment and fresh NUS state. The USB result is
research evidence and a third-component reference only. No USB implementation
is authorized by the cable experiment; continue the Bluetooth investigation.

## Matching Bluetooth roll capture

After restoring Bluetooth, a passive D-Bus observer received 267 NUS
notifications in two seconds from the daemon's existing subscription. It did
not call `StartNotify`, change the selector or send controller commands.

The owner then selected **Rolling now** for the same grip-up/grip-down movement.
An immediate 8.000990951000858-second capture observed:

- 1,067 notifications, all 20 bytes with the observed `FE 00` suffix.
- Known signed-12 motion pair ranges: -129..62 and -311..326.
- Bytes 14-17 were `00 00 00 00` in every sample.
- Bytes 4-10 were zero in every sample. Only ordinary stick bytes 1-3 and the
  known motion packing bytes 11-13 varied; byte 0 was fixed at 124.
- No additional notification shape appeared in this window.
- No controller commands and no `StartNotify` calls were made by the observer.

Private remote capture: `/tmp/flydigi-ble-roll-dvvrwq03/reports.bin`, SHA-256
`a1ed01015c16d10a0f32215c24f056dc8dd007b8c500294a4d6fa13d4c504ce3`.

This supports the two-component interpretation for the **current standard
stream**. It does not exclude a different request, selector or callback path.
The official Android BLE SDK is the next selected static target for such a
positive protocol lead. Product code remains unchanged during this research.
