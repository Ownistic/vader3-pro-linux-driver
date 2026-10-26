# Protocol ledger

Status: **two real Bluetooth descriptors obtained; no input/output reports
captured**. This document distinguishes source-level observations and descriptor
metadata from a verified wire specification.

## First physical observation

Before switching modes, the owner-confirmed Vader exposed Bluetooth
`045e:02e0`, an Xbox-compatible descriptor. Its 306-byte descriptor has SHA-256
`74a5dbcedf6d6a88f9eb6e66fbeb5a255ba800fd084390cbf325e0c4c74492d7`.
It advertises standard controls and a rumble-style output report, but no explicit
six-extra or IMU fields. Physical reports/feedback are untested; access to the
hidraw node is currently denied. Mode switch setting is not independently known.
See [Xbox-mode evidence](evidence/bluetooth-xbox-mode.md).

After a normal mode switch/reconnection it exposes **`d7d7:0041`**, matching the
upstream DInput lead. Its 163-byte descriptor has SHA-256
`227924e94560bea5149d6a75a8bb225143a1e4b0685075dea729a30649bc1f22`.
BlueZ exposes HID-over-GATT and separate writable HID-report/vendor/UART
characteristics. The HID report map itself advertises input only; this does not
settle feedback availability. Physical input mapping/motion and all output
commands remain unverified. See [DInput evidence](evidence/bluetooth-dinput-mode.md).

## Required transport inventory

For each controller mode, record firmware, Bluetooth Classic HID versus BLE/HOGP,
HID bus/VID/PID/version, descriptor bytes and SHA-256, report lengths/IDs, feature
and output paths, mode/initialization sequence, timestamps, and provenance.
Bluetooth HID bus `0005` alone does not distinguish Classic from BLE.

## Mapping worklist

| Fields | Bluetooth layout / units | State |
| --- | --- | --- |
| A/B/X/Y; D-pad; LB/RB; Start/Select; Home/system | Unknown | Untested |
| LX/LY/RX/RY; L3/R3; LT/RT | Unknown | Untested |
| M1/M2/M3/M4/C/Z | Unknown; independent bits must be proven | Untested |
| Gyro XYZ and accelerometer XYZ | Unknown initialization, endian, scale, orientation, calibration/rate | Untested |
| Rumble channels/strength/duration | Unknown Bluetooth output or feature command | Untested |

Observed DInput descriptor field order (not yet mapped against physical reports):
four signed 8-bit axes; four additional unsigned 8-bit Generic Desktop fields;
4-bit hat; 12 Button bits; 16 Consumer bits; two 8-bit Simulation fields, all
under input report ID 1. A parser must verify actual length, offsets, and
physical meaning against recorded neutral/press/release/motion samples.

## Source leads, not Bluetooth commands

SDL's `SDL_hidapi_flydigi.c` has `HIDAPI_DriverFlydigi_HandleStatePacketV1`,
`HIDAPI_DriverFlydigi_UpdateDeviceIdentity`, and
`HIDAPI_DriverFlydigi_RumbleJoystick`. V1 handling expects `04 fe` input and
uses extra-button bits in byte 7; its feedback command begins `05 0f`.
The V1 device selection requires interface 2 and comments describe a custom
interface on early controllers. This is **not** evidence that the Bluetooth
descriptor exposes this interface, accepts that feedback command, or sends
that input layout. Do not send these bytes to hardware based on this ledger.

Source pins and later findings belong in [RESEARCH.md](RESEARCH.md). Every future
layout must name its transport and evidence; inferred offsets stay explicitly
marked until compared against captures.
