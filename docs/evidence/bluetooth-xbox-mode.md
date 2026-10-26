# First connected controller: Xbox-compatible Bluetooth identity

## What was actually observed

After the owner connected the controller, BlueZ reported an Xbox-named connected
device. The owner explicitly confirmed that this device **is the Vader 3 Pro**,
and noted it may be in XInput mode. This establishes human-confirmed model
identity, not independent firmware authentication or a confirmed switch setting.

`python3 tools/inventory.py --vid 045e --pid 02e0 --descriptor` observed:

| Field | Value |
| --- | --- |
| Bus | `0005` (Bluetooth) |
| Vendor/product | `045e:02e0` |
| Descriptor length | 306 bytes |
| Descriptor SHA-256 | `74a5dbcedf6d6a88f9eb6e66fbeb5a255ba800fd084390cbf325e0c4c74492d7` |
| BlueZ services | HID `00001124-0000-1000-8000-00805f9b34fb`, PnP `00001200-0000-1000-8000-00805f9b34fb` |
| Input reports advertised | IDs 1 (gamepad), 2 (system control), 4 (battery) |
| Output report advertised | ID 3, PID-page rumble-style fields |
| Explicit extra-button/IMU collections | Not present in this descriptor |
| Input/output report traffic captured | None |
| Firmware/mode switch setting | Not independently established |

The HID service UUID is consistent with Classic HID, but a connection-level
capture has not yet corroborated Classic versus BLE; do not call that transport
distinction experimentally settled from the UUID alone.

The descriptor advertises four 16-bit stick axes, two padded 10-bit triggers,
a hat, ten button bits, and a separate system-control bit. This does not expose
explicit fields for all six extras or motion. It **does not prove** the hardware
lacks another Bluetooth mode, vendor endpoint, or initialization sequence.
An output descriptor is not evidence of accepted rumble or physical vibration.

## Retention, access, and next action

Redacted inventory retained outside Git at
`/tmp/opencode/flydigi/connected-controller-inventory.json`; sanitized descriptor
fixture in `../../tests/fixtures/vader3pro-bluetooth-xbox-descriptor.json`.
Bluetooth address, names/aliases, and physical paths are omitted from retained
public evidence. HID node numbers are ephemeral and must be rediscovered.

The matched hidraw node was a mode-0600 character device and was **not readable
by the agent account**. No device-node open, grab, permission change, input
capture, feature/output write, or rumble test was attempted. Do not solve access
by making devices world-writable or running the daemon broadly as root.

The owner was asked to switch through the controller's normal **Bluetooth
DInput** procedure and reconnect, without firmware/permanent configuration
writes. Next: rerun the inventory, compare descriptor/identity, then establish
scoped read access before passive reports. Preserve this Xbox-mode baseline.
