# Vader 3 Pro Bluetooth protocol

This reference describes the Bluetooth DInput frames used by `vader3p-steam`.
It separates implemented packet handling from physical observations and static
research. Byte offsets start at zero. Masks and packet examples are hexadecimal.

The physical protocol is separate from the virtual DualSense Edge report format.
The virtual device's USB-shaped identity does not add physical USB support.
See [architecture](ARCHITECTURE.md) for runtime ownership and configuration.

## Bluetooth services

Ordinary input arrives through HID-over-GATT and is read through Linux hidraw.
Motion-mode input and rumble use the Nordic UART Service (NUS):

| Endpoint | UUID | Host operation |
| --- | --- | --- |
| NUS service | `6e400001-b5a3-f393-e0a9-e50e24dcca9e` | Service identity |
| RX | `6e400002-b5a3-f393-e0a9-e50e24dcca9e` | Write commands |
| TX | `6e400003-b5a3-f393-e0a9-e50e24dcca9e` | Subscribe to notifications |

The worker resolves RX and TX by characteristic UUID on services belonging to
the matching BlueZ device. It does not independently require the service UUID.
See [ble_worker.rs](driver/src/ble_worker.rs).

## Ordinary HID input

The [HID parser](driver/src/input.rs) requires at least 21 bytes and report ID
`01`. Captured reports were 21 bytes despite a shorter descriptor declaration.
Offsets include the report ID. Bytes beyond the interpreted fields are ignored.

| Offset | Field |
| --- | --- |
| 0 | Report ID `01` |
| 1–4 | Left X, left Y, right X, right Y stick bytes |
| 5–8 | Not interpreted |
| 9, low nibble | D-pad hat: `1` up, `3` right, `5` down, `7` left; other values decode as neutral |
| 9, high nibble | A `10`, B `20`, X `40`, Y `80` |
| 10 | LB `01`, RB `02`, LT flag `04`, RT flag `08`, Select `10`, Start `20`, L3 `40`, R3 `80` |
| 11 | C `01`, Z `02`, M1 `04`, M2 `08`, M3 `10`, M4 `20`, FN `80` |
| 12 | Home `80`; other bits ignored |
| 13 | Left trigger level, `00`–`ff` |
| 14 | Right trigger level, `00`–`ff` |
| 15–20 | Not interpreted |

The trigger flags gate the analog levels. A clear flag produces a zero trigger,
even if its analog byte still contains a nonzero value.

Stick bytes are interpreted as signed `i8`. The virtual encoder adds 128 to
produce unsigned `0`–`255` values. It does not apply a stick deadzone or calibrate
the center. The decoder accepts only the observed cardinal HID hat values; it
does not infer undocumented diagonal values.

Physical button checks and resting values informed this layout. Captures,
firmware interpretation and parser behavior are different forms of evidence.
FN at HID byte 11 mask `80` is a static correlation, not an ordinary HID hardware
capture. Byte 12 mask `01` is not FN and is ignored.

## NUS input and motion

The runtime accepts a combined input/motion sample only when:

- The notification is exactly 20 bytes.
- Bytes 18–19 are `fe 00`.
- Byte 10 is `00`.

The [motion decoder](driver/src/motion.rs) checks length and suffix. The
[NUS input decoder](driver/src/nus_input.rs) also checks byte 10. The worker
requires both parsers to accept the sample.

| Offset | Field |
| --- | --- |
| 0–3 | Left X/Y, right X/Y; unsigned centered stick bytes |
| 4 | D-pad up `01`, right `02`, down `04`, left `08`; A `10`, B `20`, Select `40`, X `80` |
| 5 | Y `01`, Start `02`, LB `04`, RB `08`, L3 `40`, R3 `80`; other bits ignored |
| 6–7 | Left and right trigger levels |
| 8 | FN `01`, Home `08`; other bits ignored |
| 9 | C, Z, M1, M2, M3, M4 in bits 0–5 |
| 10 | `00` for accepted input samples |
| 11–13 | Two packed signed 12-bit motion components |
| 14–17 | Not interpreted by the runtime |
| 18–19 | Suffix `fe 00` |

The input decoder subtracts `80` modulo 256 from each stick byte to produce the
same canonical representation as ordinary HID. Analog trigger levels are used
directly. D-pad directions are independent bits in this format.

Physical X is byte 4 bit 7; Y is byte 5 bit 0. Button checks corrected an earlier
swapped labeling. The observed FN bit is byte 8 mask `01`; byte 9 mask `40` is
ignored and does not represent FN.

### Motion packing

For bytes `b11`, `b12` and `b13`:

```text
first  = sign_extend_12(b11 | ((b12 & 0x0f) << 8))
second = sign_extend_12((b12 >> 4) | (b13 << 4))
```

Both values range from −2048 to 2047. Units, bias and calibration remain
unverified. By default, the virtual encoder uses the second value as inverted
X/pitch and the first as inverted Y/yaw. Inversion follows axis selection.
Scaling saturates to signed 16-bit values.

The third gyro component is zero. Virtual acceleration and calibration values
are fabricated, not physical measurements.

One inspected Android app decodes bytes 14–17 as cursor-related values and a
validity flag. That finding does not prove that every Bluetooth path or firmware
lacks a third component.

## Motion selector and status

After subscribing, the worker observes passively for 250 ms. If activation is
still awaiting observation, it can write the two-byte selector command `ac 01`
once per connection generation and observe for one second.

Qualifying data establishes `ObservedData`. A 20-byte status starting `ac c0`
provides selector status in byte 10. Selector `01`, observed after an attempted
selector write, establishes `ObservedSelector`. No qualifying observation within
the window produces `Unverified`.

The prefix alone does not identify status. Valid input can also start `ac c0`
because the first two bytes are stick positions. The complete data parsers run
before status classification.

A successful write means the transport accepted bytes, not that motion started.
A timed-out D-Bus call can still be followed by a real response or data. The
worker records method errors separately from observed activation.

Cleanup does not restore the selector. My recorded power-cycle test cleared the
NUS stream on firmware 6.7.5.1. If returning to ordinary HID or native input,
power-cycle and reconnect before checking input. This is a recovery observation,
not a persistence guarantee for other firmware.

See [ble_worker.rs](driver/src/ble_worker.rs) for the implementation.

## Bluetooth rumble

Rumble uses exactly 4 bytes written to NUS RX:

```text
11 <left> <right> <duration_ms>
```

The implementation caps each motor level at 250 and uses duration `c8` (200 ms).
While desired feedback remains nonzero, it refreshes the command approximately
every 100 ms. New feedback can cause earlier writes.

```text
11 fa fa c8    maximum implemented levels, 200 ms
11 00 00 c8    stop
```

The stop frame retains a nonzero duration. Repeated nonzero commands extend the
effect. A transport success does not prove that motors moved or stopped.
See [rumble.rs](driver/src/rumble.rs) for the implementation. Physical motor
behavior is not established by the packet format or a successful write.

## Virtual DualSense Edge boundary

The [virtual encoder](driver/src/edge_report.rs) generates a 64-byte report with
ID `01`. FN triggers an experimental PS-only, PS + Cross, PS-only, release
sequence, with minimum durations 100, 60 and 40 ms for the first three phases.
PS (byte 10 mask `01`) and Cross (byte 8 mask `20`) are OR-composed with
physical Home and A. The sequence targets Steam's Guide + A Quick Access
shortcut.
The six extra buttons remain independent. Extra
buttons occupy byte 10: C `02`, Z `04`, M1 `10`, M2 `20`, M3 `40`, M4 `80`.
Gyro X/Y occupy bytes 16–19 as little-endian signed 16-bit values; gyro Z is zero.

Host output is a separate protocol. The runtime recognizes numbered report `02`
at length 48 or 63. Rumble-valid flags are byte 1 bit 0 or byte 39 bit 2. Left
motor level comes from byte 4 and right from byte 3, scaled from `0`–`255` to
`0`–`250`. Unrelated host output is not forwarded as a physical command.

## Limits

These layouts describe the current implementation. Earlier hardware observations
do not establish behavior for this build or its pitch correction. USB reference
captures are not a supported product transport. This document does not authorize
uncharacterized commands or firmware changes.
