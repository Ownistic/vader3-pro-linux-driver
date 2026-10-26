# Connected Bluetooth DInput: first descriptor and GATT inventory

The owner switched modes normally and reconnected the Vader 3 Pro. No firmware
or persistent profile writes were performed by the project.

## Observed metadata

`python3 tools/inventory.py --vid d7d7 --pid 0041 --descriptor` observed:

| Field | Observation |
| --- | --- |
| Bus/vendor/product | Bluetooth `0005`, `d7d7:0041` |
| Descriptor length | 163 bytes |
| Descriptor SHA-256 | `227924e94560bea5149d6a75a8bb225143a1e4b0685075dea729a30649bc1f22` |
| HID descriptor reports | Input report ID 1; no advertised output/feature main items |
| Firmware | Not read/established; PnP version is not a firmware measurement |
| Passive hidraw access | Denied to agent account; mode 0600 |
| Input/output packets / Steam tests | None |

The descriptor contains four signed 8-bit X/Y/Z/Rz axes; four additional 8-bit
Generic Desktop usages `40–43`; a 4-bit hat; 12 Button-page bits with noncontiguous
usages; 16 Consumer-page bits; and two Simulation-page 8-bit usages `c5/c4`.
These are descriptor declarations, **not a verified physical control mapping**.
In particular, do not label usages `40–43` as native IMU data without reports and
motion experiments. This is not SDL's `04 fe` custom-interface layout.

## BlueZ GATT observation (read-only cached object inventory)

BlueZ exposed HID service `1812` and actual `GattService1` / `GattCharacteristic1`
objects for this connected device. This establishes an exposed **HID-over-GATT**
service, unlike the Xbox-mode service listing. No packet trace of the input
notifications or link-level traffic has been acquired.

| Service / characteristic UUID | BlueZ characteristic flags |
| --- | --- |
| HID `1812` / Report `2a4d` (input-side object) | read, notify |
| HID `1812` / Report `2a4d` (separate writable object) | read, write-without-response, write |
| HID / Report Map `2a4b` | read |
| HID / Protocol Mode `2a4e` | read, write-without-response |
| Vendor `00010203-0405-0607-0809-0a0b0c0d1912` / `00010203-0405-0607-0809-0a0b0c0d2b12` | read, write-without-response, notify |
| Nordic UART `6e400001-b5a3-f393-e0a9-e50e24dcca9e` / `6e400002-b5a3-f393-e0a9-e50e24dcca9e` | write |
| Nordic UART / `6e400003-b5a3-f393-e0a9-e50e24dcca9e` | notify |

The inventory was obtained with the system D-Bus ObjectManager
`GetManagedObjects` method, selecting only this device's cached service and
characteristic UUID/Flags properties. No `ReadValue`, `WriteValue`,
`StartNotify`, characteristic acquisition, or controller command was issued.
Addresses and device-specific object prefixes are omitted from public evidence.
GATT object paths/handles are not stable identifiers.

**Important:** a writable GATT characteristic despite an input-only HID report
map is a concrete research lead, not a rumble implementation. Report-reference
semantics and vendor command contents are unknown. Do not infer either working
rumble or hardware impossibility from these flags.

## Retained evidence / next action

- Redacted external inventory:
  `/tmp/opencode/flydigi/connected-controller-dinput-inventory.json`.
- Public sanitized descriptor:
  `../../tests/fixtures/vader3pro-bluetooth-dinput-descriptor.json`.
- Previous mode: [Xbox-compatible descriptor](bluetooth-xbox-mode.md).

Obtain narrowly scoped, temporary read access to the rediscovered target node
with owner authorization, then passively capture neutral, individual/combined
buttons, axes, and controlled motion. No blanket device permissions or root
daemon. Preserve both descriptor identities and trace static Windows Bluetooth
and vendor-GATT code before any output command experiments.
