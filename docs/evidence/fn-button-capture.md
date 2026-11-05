# FN button: passive Bluetooth capture

## Tested artifacts

- Repository baseline: `a0b40bf` on `main`.
- Running v4 daemon, checked through its live executable:
  `616d9e1b5f2e808561ea2213271e0c5db6136525cbce031caffd6a501da4c715`.
- Isolated passive probe SHA-256:
  `183afdc621e72414e2c7dbb96839e4eec0dfc6bce8064d17c5d52c4cc023e519`.
- Private raw capture SHA-256:
  `21afe2d7410843b3a6f3e666a58e38bc3121cd0f40a60339568c34d8ba983412`.
- Physical Bluetooth descriptor SHA-256:
  `227924e94560bea5149d6a75a8bb225143a1e4b0685075dea729a30649bc1f22`.

The probe and raw capture are retained privately under
`/tmp/opencode/flydigi-fn-a0b40bf/`, with the remote originals under
`/tmp/flydigi-fn-a0b40bf/`. These temporary paths are not durable archives, and raw
controller data is not added to Git.

The probe observed the running daemon's existing NUS subscription and opened
the physical hidraw node read-only. It did not call StartNotify, StopNotify,
WriteValue or any selector/rumble operation. The existing daemon continued its
normal operation. Notification callbacks checked the resolved BlueZ sender,
message type, interface, member and path.

## Procedure and observation

I connected the controller in Bluetooth DInput mode. Once recording was active,
I confirmed completing three isolated FN presses followed by one LB press, with
no FN combination. The recording itself contains four FN-level pulses, not
exactly three. The count discrepancy is retained rather than interpreted as
proof of bounce or a specific press cadence.

The capture ended on request after about 136 seconds, with no recorded
disconnect. It contains 18,152 valid 20-byte NUS samples, 392 with FN set and 21
with LB set. There were no ordinary HID input reports while the NUS stream was
active.

All observed button-field combinations, in byte order `4, 5, 8, 9, 10`:

| Fields | Samples | Interpretation |
| --- | --- | --- |
| `00 00 00 00 00` | 17,739 | Released / neutral |
| `00 00 01 00 00` | 392 | FN held: byte 8 bit 0 |
| `00 04 00 00 00` | 21 | LB marker: byte 5 bit 2 |

Every FN pulse returned to the neutral button fields. The current parser ignores
byte 8 bit 0, which explains why this button has no virtual action in the tested
v4 daemon. The independent LB marker confirms that the recording included the
requested input sequence, rather than only an idle baseline.

## Limits

This establishes an FN press/release signal in the active Bluetooth NUS path.
It does not hardware-verify an ordinary HID FN bit, any new executable, a
Guide + A mapping or Steam's response. Those require separate evidence in the
[implementation record](../phases/03-fn-quick-access.md).
