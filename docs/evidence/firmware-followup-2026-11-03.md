# Firmware follow-up: report layout and Bluetooth commands

Static investigation of the downloaded main-MCU image `6.7.5.1`, SHA-256
`6cba30540a0c3ba16acad2b62bb4dd64dfccc28add04914c3720c6ae6a53e20a`,
and the saved Space Station 3.4.4.3 decompilation. Repository baseline:
`f1f1f265a3da0d7d8fb72be3c5b0e9297c66eb2b`. The saved corrected TC32
decompile has SHA-256
`71c4f75ce66e4f67ab57a1e39df48bc1f387be67d5c815637206ececa63748c9`.
No commands were sent to the controller during this investigation. Installed
controller firmware identity is not established by the downloaded image.

## USB vendor report: four stick bytes

**Static application finding:** `XInputDataUtils.FormatDataFromDInput32`
copies input bytes 17, 19, 21 and 22, in that order, into normalized bytes
0 through 3. `KeyMouDataParse.ParseDInputData` passes the result to
`handleJoyStickData`. That function treats the first pair as left-stick X/Y
and the second pair as right-stick X/Y. The offsets include the leading report
byte, consistent with the previously documented application input format.

| Input byte | Application interpretation |
| --- | --- |
| 17 | Left stick X |
| 19 | Left stick Y |
| 21 | Right stick X |
| 22 | Right stick Y |

The application centers each byte around 128 and reverses Y for its own
mapping calculations. This establishes what the application expects, not the
physical direction or calibration of each axis on this particular controller.
The prior captures did not isolate each stick axis. Confirm each axis in a
separate controlled hardware capture before implementing a decoder.

**Correction to the previous trace:** `FUN_0000094c` updates internal input and
key-mapping state. It is not, by itself, the demonstrated byte-by-byte wire
serializer. In the non-`state+0xe == 2` branch, `FUN_0001a868` calls it
conditionally and then calls `FUN_0001899c`; the latter constructs a buffer and
calls `FUN_0001896c`. `FUN_0001a6d0` is used in the other branch. The full
buffer-to-USB-frame relationship remains to be traced, so do not directly
equate those internal buffer offsets with hidraw offsets.

The previous claim that `11:12`, `13:14`, and `15:16` are three gyro axes is
not established. The recorded motion sensitivity is real, but a third pair
near 4096 at rest is not sufficient to identify it as angular rate. Physical
axis, sensor type, endianness, scale and signs require isolated tests and/or a
complete sensor-to-wire trace. The vendor application's two packed motion
values at bytes 4–6 are a separate interpretation; it does not identify all
raw sensor words.

## Bluetooth command findings

These are decompiler-level findings, not observed device responses. The
dispatcher calls several handlers independently, and frames shorter than four
bytes can leave some handler reads dependent on the receive buffer. Use exact
four-byte commands for any future controlled experiment. Do not assume a write
was acted on without observing a response or other device behavior.

| Command | Static behavior | Remaining uncertainty |
| --- | --- | --- |
| `BA 40 ..` | `FUN_000105cc` calls `FUN_00010320`, which constructs a 20-byte `AC C0` status-shaped response and passes it to `FUN_00010300`. | Actual notification, field meanings, and any effect of the delay helper. |
| `A5 01 ..` | `FUN_000105e8` calls `FUN_000103e4`, which builds a 20-byte `A5 01` response containing two state bytes and an additive final byte. | Actual notification and meaning of the two bytes. |
| `AC <flags> ..` | `FUN_00010560` stores bit 0 and bit 4 as separate Boolean state values, then calls the same `AC C0` response builder. | What those flags control and whether settings persist. Do not treat it as a read-only query. |
| `12 a b c` | `FUN_0000ceb4` stores the three supplied values in a state structure and repeats them across five three-byte entries, then sets a flag. The USB `E0` arm calls the same function with the same argument order; Space Station's `DongleCommand.getTestRGBCommand(r,g,b)` constructs that USB command. | Application and firmware together support an RGB-test interpretation, not confirmed physical LED behavior or persistence over Bluetooth. |
| `13 02 ..` | `FUN_0000b4d4` calls `FUN_00010f18`, `FUN_000119d8(8)` and `FUN_0000b39c`. | The mode's meaning and device behavior. |
| `13 10 ..` | `FUN_0000b470` calls `FUN_00010f8c`, `FUN_000119d8(1)` and `FUN_0000b39c`. | The mode's meaning and device behavior. |

`FUN_000119d8` calls `FUN_00011954` to select a mode value and then
`FUN_00011984`, whose decompilation calls erase/write-shaped primitives with
address `0x30000`. The address space and persistence are not yet proven. Thus
`13 02` and `13 10` are not suitable as casual
read-only discovery probes. Their full side effects and recovery remain unknown.
`13 11` follows a similar path with value 2, but that does not prove the meanings
of the other values.

`A5 A1` is also more than a single query reply in the decompilation:
`FUN_0001067c` builds eight 20-byte frames with three indexed entries each,
then one final 20-byte frame with two entries. Their delivery and semantics have
not been verified on hardware. The loop evidence does not prove that every
notification is delivered.

## Next verification

1. Trace the USB framing between `FUN_0001899c` and the captured `04 FE` report,
   and trace sensor source state before assigning raw IMU fields.
2. If the owner is available, capture one stick axis at a time in USB vendor
   mode to verify the application-derived offsets and direction.
3. For Bluetooth status, observe notifications around a single `BA 40 00 00`
   request before trying state-changing commands. Confirm the controller is in
   the intended mode and that the notification stream is subscribed first.
4. Do not infer a Bluetooth motion path from the USB serializer. The previously
   observed Bluetooth DInput stream did not change after the vendor-mode chord.
