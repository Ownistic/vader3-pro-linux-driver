# Static subsystem traces — 2026-11-04

Baseline revision `f1f1f265a3da0d7d8fb72be3c5b0e9297c66eb2b` plus the
uncommitted research documents. Main firmware and APK identities are in the
[research index](research-index.md). Findings below are static, not hardware
observations. No target code was executed and no controller was contacted.

## Initialization and scheduling

The saved decompilation of `FUN_000003f4` calls several initialization
functions including `FUN_0000d4e0`, then loops calling `FUN_0000d648`.
`FUN_0000d4e0` calls `FUN_0000d4b0`, which initializes one of the three
sensor-shaped paths according to the selector described below. `FUN_0000d648`
chooses among several state-dependent scheduling paths; for one branch it
calls `FUN_00008a60`, while others call `FUN_0001161c` or `FUN_0001acb0`.
The chain from reset vector to `FUN_000003f4`, clock setup, interrupts and
all mode values remain unverified. These call relationships do not by themselves
establish timing or which physical transport a branch represents.

## Analog control path is not proof of IMU enable

The saved corrected TC32 decompile shows `FUN_0000aac4` calling
`FUN_0000a414` to populate a six-element input array, then calling
`FUN_0000a704` and `FUN_00009b44`. `FUN_0000a414` invokes
`FUN_0000822c` for six configured channels. The exact electrical inputs and
the called sampling primitive require independent verification.

In `FUN_0000a704`, the first four entries participate in paired center-offset,
radius and per-axis scaling operations and eventually produce four control
bytes at output-state offsets `0x32..0x35`. The next two are constrained to
`0..255` and stored at offsets `0x36..0x37`. `FUN_0000a598` records extrema
when its mode flag is set; `FUN_00009df4` and `FUN_0000a128` use stored extrema
to calculate calibration-related values. This is strong structural evidence
of an analog control/calibration path. It is not an identification of the six
physical pins or a complete description of the IMU path.

The branch involving `state+0x11` and `state+0x76` in `FUN_0000a704` is
therefore insufficient evidence that `state+0x76` enables a gyroscope. The
older interpretation in [firmware patch scoping](firmware-patch-scoping.md)
must not be used as a patch recipe. USB motion-sensitive bytes were observed
independently; their source and physical units remain unresolved.

## Motion sample to internal report buffer

The main image's literal pool entry at file offset `0x18b04` contains
`0x008469ec`, the state base used by `FUN_0001899c`. Entry `0x133cc` contains
the same base. `FUN_000131a8` writes six 16-bit values to offsets `0xae`,
`0xb0`, `0xb2`, `0xb4`, `0xb6` and `0xb8` of that state. Its sample path calls
`FUN_00012fac`, which reads 12 bytes via `FUN_00012d14(0x35, ..., 0x0c)` and
scales two groups of three. The surrounding logic can reuse prior samples,
average them, and alter signs depending on mode. The precise sensor register,
axis ordering, physical units and sign conventions are not yet established.

A second path also writes this region: literal entry `0x13ce8` resolves to
`0x00846a9a`, equal to the same base plus `0xae`. `FUN_00013bcc` passes that
address and the next five 16-bit positions to `FUN_00013af0`, which reads 12
bytes through `FUN_000082a0` with arguments `0xd6` and `0x22`, then transforms
and stores six results. Some branches subsequently adjust the first words.
Thus the `FUN_000131a8` trace is not the only producer, and the actual source
depends on initialization and mode. The low-level wrapper's decompiled
signature loses an argument, so the bus address and transaction must be
checked before identifying the sensor. A read-only Ghidra `RawRange.java 13af0
18` inspection confirms that instructions `0x13b04..0x13b0c` load `0xd6`,
`0x22`, the stack buffer and `0x0c` into the four argument registers before
calling `0x82a0`. This does not establish the bus interpretation or sensor model.
`FUN_0000d5e4` selects `FUN_00013bcc` when its state byte at `+0x14` is 1,
`FUN_000131a8` when it is 2, and `FUN_000094d0` when it is 3. The identities
and initialization of those alternatives remain to be traced. `FUN_000094d0`
also obtains two six-byte groups via `FUN_00009464` and stores them into the
same six state words, followed by mode-dependent transformations. Its source
and units remain unknown.
The selector-3 setup `FUN_00009138` checks an identity-shaped byte for
`0x61`, then configures a sequence of registers. `FUN_00009464` obtains a
12-byte block through `FUN_000094cc` and splits it into two groups of three
16-bit words. The exported decompilation of `FUN_000094cc` overlaps the body of
`FUN_000094d0` and contains an unresolved indirect call, so the low-level
access and apparent call structure require disassembly verification. Selector-2
initialization `FUN_00013420` checks for `0x05` and
configures its own register sequence. Neither observation identifies the chips
without validating the access wrappers and correlating external specifications.
Separately, the Windows service's `DInputDataManager.HandleGamePadInfo` labels
information-report byte 14 value 1 as `ST` and value 2 as `QST`. The producer
of that report byte and its relationship to the internal selector are not yet
traced, so these application labels do not identify the sensor in the unit.

The same selector also chooses initialization: `FUN_0000d4b0` calls
`FUN_00013cf4`, `FUN_00013420`, or `FUN_00009138` for values 1, 2, or 3.
Literal entries `0xd4dc` and `0xd60c` both resolve to `0x00846ac8`, confirming
that the two functions read the same selector offset in this image. Read-only
Ghidra reference enumeration found three references to `FUN_0000d5e4`:
`FUN_00008a60`, `FUN_0001161c`, and `FUN_0001acb0`. The last calls
`FUN_0001a868` and `FUN_0000fc18` in one scheduled branch, and calls the
sensor selector in another. These are static call sites, not a measured rate
or proof that every indirect caller was found.

For selector 1, `FUN_00013cf4` reads one byte using arguments `0xd6, 0x0f`
and branches on values `0x68`, `0x69`, and `0x6a`, configuring different
register/value pairs and setting an internal type byte. These are chip-ID-like
comparisons, not proof of any particular sensor model without bus and datasheet
corroboration.

`FUN_0001899c` reads those six words and writes their low and high bytes to
internal buffer offsets `0x11..0x1c`. It calls `FUN_0001896c`, which sends
non-`5A A5` buffers to `FUN_0000fb5c`. Literal-pool entries `0xfb78` and
`0xfcf0` both resolve to `0x00847204`: the former is the `FUN_0000fb5c` copy
destination and the latter is a source used by `FUN_0000fc18`. Their length
state entries `0xfb74` and `0xfcec` both resolve to `0x008471f9`. This
connects the sample words to a shared outgoing staging buffer, conditional on
the branch in `FUN_0000fc18`. It does **not** establish the final USB/hidraw
offsets, and it must not be substituted for the separate vendor application's
input-byte map.

Method: inspected the saved decompile and decoded little-endian words at the
named file offsets from the hash-identified image with Python `struct.unpack_from`.
The complete call path, literal-pool interpretations and instructions still
need independent review, especially indirect calls and transport framing.
The helper `RawRange.java` prints a computed literal annotation that appears to
multiply an already byte-scaled Ghidra immediate by four; do not rely on those
annotations. The literal words in this section were read directly from the image.

## Vendor application's virtual-controller motion field

In the Space Station service, `DInputDataManager.CallbackDataReceive` passes
reports beginning `04 FE` to `PS5Driver.SendReportData2PS5`.
`DataUtils.GetPs5Data32New` rearranges the input; the non-K1 branch
`PS5DataConvertService.TransDriverDataformByStandDataK2` assigns a 12-byte
`Motion` array. In array order, the original zero-based input positions are
`26, 27, 29, 30, 18, 20, 11, 12, 13, 14, 15, 16`. The service forwards
that array to a native virtual-controller function. The F3 Pro identifiers are
not K1 identifiers in the application tables, but runtime route and native
library interpretation remain unverified.

This is independent host-side evidence that the application treats those
bytes collectively as motion. It does not prove which words are gyro versus
acceleration, their endianness, units or physical axes. In particular, bytes
18 and 20 need not be a contiguous pair in the original frame. This is not a
Bluetooth motion path.

## Sensor-state words to Nordic UART notification-shaped output

### Sensor register-map comparison

Two sensor-selection branches independently support an angular-rate
interpretation for the two words routed to Nordic UART, conditional on the
actual connected hardware using a corresponding register map:

* Selector 1 calls `0x13af0`, which requests 12 bytes starting at `0x22`.
  Its first three 16-bit input words supply the first three output pointers,
  subject to model-dependent rearrangement, negation, zeroing and later
  processing; `0x13bcc` passes state `+0xae`, `+0xb0`, `+0xb2` as those
  pointers. The [ST LSM6DS3 datasheet](https://content.arduino.cc/assets/st_imu_lsm6ds3_datasheet.pdf)
  describes `0x22..0x27` as three signed angular-rate output words and
  `0x28..0x2d` as acceleration. The firmware's identity-shaped values and
  register layout are consistent with this family, but do not identify the
  physical part or prove the bus transaction.
* Selector 2 calls `0x12fac`, which requests 12 bytes beginning at `0x35`.
  The saved decompile and selected instructions show its first three words
  go through scaling to the first output argument and the latter three to
  the second. `0x131a8` passes its local second group as the first argument
  and local first group as the second, then stores the local first group's
  third and first words at state `+0xae` and `+0xb0` before further
  processing. The [QMI8658A datasheet](https://files.waveshare.com/upload/5/5f/QMI8658A_Datasheet_Rev_A.pdf)
  identifies `0x35..0x3a` as acceleration and `0x3b..0x40` as angular rate.
  Thus, if this is the active compatible sensor path, the Bluetooth pair
  comes from the angular-rate-shaped group, with reordering and scaling.

The independent 2026-11-04 observations showed movement-sensitive packed
fields and larger excursions at faster rotation. Together these observations
and register-map comparisons provide **strong evidence** for two processed
gyro-related values, not a calibrated or universally proven gyro protocol.
The active sensor selector, selector-3 source, hardware identity, exact
firmware version, mode-dependent math, signed truncation, units, axis signs
and third axis remain unresolved. Sensor data may be cached or transformed.
Do not apply raw datasheet scale factors directly to the 12-bit fields.

Method: compared saved decompilation with read-only Ghidra instructions for
`0x12fac`, state stores at `0x1321e..0x1325a`, the `0x13af0` read and
`0x13bcc` pointer setup; checked register ranges against the linked
datasheets. No hardware was contacted during this comparison.

In the hash-identified main image, `FUN_00010ba4` uses the sensor sample
state base `0x008469ec` (literal at image offset `0x10cdc`). Read-only TC32
instructions at `0x10c28..0x10c54` read the first two sample words at state
offsets `0xae` and `0xb0`, packing their low 12 bits into bytes 11–13 of a
20-byte stack buffer. For words `a` and `b`, the bytes are `a & 0xff`,
`((a >> 8) & 0x0f) | ((b & 0x0f) << 4)`, and `(b >> 4) & 0xff`. This does
not establish physical sensor type, axes, sign, scale or sample freshness.
The other four sample words are not mapped by this packing sequence; other
output paths remain unexamined.

At `0x10c70..0x10c74`, the builder passes the buffer and length `0x14` to
`FUN_00010300`. Instructions at `0x10308..0x1031a` gate on the result of
`FUN_00002dd8` being at most 10, then pass handle `0x27`, the buffer and
length to `FUN_000203c0`. The registered attribute entry at image offset
`0x24980`, index/handle `0x27`, has UUID pointer `0x23998` to Nordic UART TX
`6e400003-b5a3-f393-e0a9-e50e24dcca9e`; its declaration at `0x2396c` has
notification property `0x10` and value handle `0x27`. Table registration and
startup copy are documented in the [BLE callback trace](ble-attribute-callback-static.md).

`FUN_000203c0` applies state and capacity-shaped checks. On a qualifying
branch, `0x2046a..0x20488` constructs an object with opcode `0x1b`, the
supplied 16-bit handle and a copy of the payload, then passes it to
`FUN_0001e004`. Opcode and attribute agree with an ATT notification-shaped
path; fragmentation and later transport remain incompletely traced. The
builder ignores the send helper's result, so this does **not** prove a
notification was queued, transmitted, subscribed to, or observed physically.

`FUN_00008818` calls `FUN_00010ba4` directly on one branch
(`0x885e..0x8860`), or through selected branches of `FUN_00010dc0`;
`0x8818..0x885a` includes state, delay and incoming-command checks. The
state-byte meanings, active mode and scheduling remain unresolved. This is a
partial static route for **two** sensor-state words via Nordic UART, not a
proven six-axis Bluetooth DInput HID report. The recorded 21-byte Bluetooth
HID capture and negative chord result do not establish whether this separate
Nordic UART characteristic was subscribed to or carried these bytes.

Method: decoded attribute pointers and declaration bytes directly from the
identified image, then checked the builder, scheduler, send wrapper and packet
construction with read-only Ghidra disassembly; project changes were
discarded. The follow-up below traces one selector and part of the outbound
queue; compare existing NUS captures without conflating HID reports with NUS
notifications.

### Selector and outbound queue follow-up

The literal at `0x8870` identifies the direct branch selector in
`FUN_00008818` as byte `0x008472bc`. `FUN_00010eb0` clears that byte at
`0x10eb2..0x10eb6`; the saved decompile shows `FUN_00010ecc`, called during
the `FUN_00008874` initialization path, invokes that routine. Other startup
branches and later writes remain possible.

The Nordic UART RX callback and `FUN_00010980` path documented in the
[BLE callback trace](ble-attribute-callback-static.md) can reach
`FUN_00010560` for a nonzero copied payload length of at most four. Its
instructions `0x10564..0x1059e` check the first byte for `0xac` and set
`0x008472bc` to the second byte's bit 0 normalized to 0 or 1; they also set
the adjacent byte from bit 4 and call `FUN_00010320`. In the reviewed
`FUN_00008818` path, a nonzero selector reaches `FUN_00010ba4` directly
after the other scheduler gates. This is a static connection, **not** a
recommendation to send a command, proof of persistence, or proof of physical
behavior. The inspected dispatcher permits a one-byte length before this
handler reads byte 1; upstream validation, buffer history and implications
are not established, so this is not a vulnerability conclusion.

The notification-shaped packet builder `FUN_000203c0` passes its object to
`FUN_0001e004`. At `0x1e004..0x1e050` the latter computes an occupancy-shaped
value and can return zero before copying. On the qualifying path,
`0x1e052..0x1e06c` computes a slot using descriptor `0x00843b58` and base
`0x00843d70`, then copies a length derived from the object's second byte;
`0x1e07a..0x1e08a` records length and advances the producer index. An
additional branch calls `0x23490` before finalization. `FUN_000203c0` does
not check the enqueue return at `0x20488` before continuing; a success-shaped
return from it therefore does not prove insertion.

On a separate gated path in `FUN_000028d4`, instructions
`0x29ac..0x2a0e` read the same descriptor and base and write the low 16 bits
of computed slot addresses to `0x00800c2c`, then advance the consumer index.
The handler also inspects `0x00800f20`; the reference register-map comparison
in [staged-frame transport](staged-frame-transport-static.md) supports an
RF-peripheral interpretation, but the chip, DMA interpretation, queue
initialization, optional transform, over-the-air delivery and remote
subscription are not proven. The reference-map addresses do not establish
that every queued object is transmitted.

Method: decoded the literal words from the same image and checked the named
instructions in read-only Ghidra; its project changes were discarded. Next
trace other writers of both scheduler selectors and compare existing NUS
captures. The transform and candidate initializer are examined below.

### Nordic UART subscription and selector-status clues

In the same saved attribute table, TX value handle `0x27` is followed by a
user-description entry at `0x28` and a two-byte client characteristic
configuration descriptor (UUID `0x2902`) at `0x29`, backed by RAM address
`0x00846008`. These are image-table indices, not verified handles on a live
device. The table and notify property make subscription a concrete hypothesis;
they do not prove that a client can subscribe or that the controller emits
notifications in Bluetooth DInput mode.

The `0xac` handler calls `FUN_00010320` after changing its two selector
bytes. Instructions `0x10326..0x1032c` construct a 20-byte status-shaped
object beginning `AC C0`; `0x10364..0x1036c` set zero-based byte 10 to the
normalized nonzero value of selector `0x008472bc`, and `0x103b6..0x103ba`
pass the object to the same Nordic UART send wrapper. The subsequent queue,
transport and subscription gates still apply. This is not an observed response
or proof the command is safe, temporary, persistent, or gyro-enabling; the
handler also modifies an adjacent selector and calls other code. No command
was sent.

For a **future separately authorized hardware test**, first inspect and
subscribe to the discovered TX characteristic and record the initial stream
without writing the `0xac` selector command. Treat an `AC C0` status-shaped
object separately from candidate 20-byte data objects. If a later controlled
selector experiment is justified, first review its remaining side effects,
record the prior state and restoration strategy, and verify a device response
rather than treating transport acceptance as success. Isolated stationary,
single-axis rotation, translation and stick controls would be needed to
distinguish motion from other inputs; two packed values cannot establish a
complete gyro. Ask me before any required physical action or after a
disconnect, and do not run this proposed test as part of static research.

Method: decoded the attribute-table entries and UUID pointers directly from
the hash-identified image; checked the status builder, selector handler and
packed-word instructions with read-only Ghidra. Descriptor-table evidence,
static code and unperformed physical tests are separate.

#### Initial physical notification observation

On 2026-11-04, with repository HEAD `f1f1f26` and the research documents
uncommitted, I connected a Vader 3 Pro in Bluetooth DInput mode.
BlueZ reported Nordic UART TX UUID `6e400003-b5a3-f393-e0a9-e50e24dcca9e`
with `notify` and RX UUID `6e400002-b5a3-f393-e0a9-e50e24dcca9e` with
`write`. The controller firmware version and its identity relative to the
statically examined image are not established.

BlueZ `StartNotify` succeeded; the first 12-second observation at rest
received **zero notifications**, then `StopNotify` was called. A second
120-second subscription likewise received **zero notifications**. During
that window I confirmed holding still, rotating left and right, and
tilting forward and back. The controller remained connected afterward. No
`AC` selector or other controller command was written in either test.
Successful subscription means BlueZ accepted the request, not that the
firmware transmitted. These observations do not rule out an enabled-selector
path, other motion transports, or a different firmware version. A subsequent
state-changing experiment needs a decision after reviewing side effects and a
restoration strategy.

#### Authorized selector-on observation

On 2026-11-04, after the baseline, I explicitly authorized changing
the state. With TX subscribed, a BlueZ `WriteValue` request sent `AC 01` to
Nordic UART RX. A 20-byte `AC C0` status-shaped notification was observed with
zero-based byte 10 equal to 1, supporting that the selector changed; this
conclusion uses the device response, not merely the accepted write. During the
following 90-second window BlueZ delivered 12,039 notifications in total,
including the status object and 12,038 20-byte data objects. Bytes 11–13
included changing values, consistent with the statically traced packed fields.
I confirmed holding still, rotating left/right and tilting forward/back
during the observation. The capture did not label individual movements or
timestamp samples, so it does not establish correlation, physical axes,
sensor type, scale, or gyro.

The observer unsubscribed after the window; it did **not** send `AC 00` or
restore other settings. Selector state and persistence after the test are
unknown; the status response reported 1 during the test. The connected
controller's firmware revision was not independently determined. No raw
capture or identifying status bytes are published. A next test should use
timestamped, separately marked stationary and isolated movements, with my
participation, and assess whether and how to restore prior state.

#### Marked movement phases

On 2026-11-04, following the selector-on experiment and without another
selector write, a new Nordic UART TX subscription produced data during four
phases I confirmed. The listener decoded bytes 11–13 into two 12-bit
values and **hypothetically** interpreted each as two's-complement signed.
It accumulated minimum, maximum and count per monotonic-timestamped phase;
individual packets and identifying fields were not saved to the repository.

| Phase | Notifications | First value range | Second value range |
| --- | ---: | ---: | ---: |
| Stationary on surface | 5,023 | 0 to 0 | -1 to 0 |
| Left/right rotation | 5,713 | -1101 to 685 | -54 to 47 |
| Forward/back tilt | 4,641 | -68 to 31 | -292 to 290 |
| Side-to-side tilt | 3,796 | -94 to 96 | -122 to 169 |

Phase windows were approximately 37.7, 42.8, 34.8 and 28.5 seconds,
respectively; they include instructions, transitions, handling and waiting
for confirmation, not exclusively the requested ten seconds of motion.
The observed movement-dependent ranges support a relationship between the
packed fields and controller motion. They do **not** by themselves establish
gyro rather than another motion-derived signal, precise axis alignment,
sign convention, units, sampling rate, independent axes, or all three gyro
axes. The live firmware version remains unverified. The listener unsubscribed
without changing or restoring the selector. Next use separately marked
stationary orientation holds and controlled rotation versus translation,
then determine signed representation and scale before implementing a driver.

#### Stationary holds, translation and rotation speed

On 2026-11-04 I assisted with a further Nordic UART subscription that used the
same tentative signed-12-bit decoding, without another selector write.
I confirmed each requested phase; phase boundaries were marked
using a monotonic clock and the listener recorded only count and extrema.

| Phase | Notifications | First value range | Second value range |
| --- | ---: | ---: | ---: |
| Level stationary hold | 3,164 | 0 to 0 | -2 to 1 |
| Moving into forward tilt | 2,214 | -34 to 24 | -21 to 189 |
| Hand-held tilted hold | 3,095 | -25 to 17 | -35 to 96 |
| Sideways translation | 4,411 | -245 to 289 | -308 to 168 |
| Slow left/right rotation | 4,196 | -320 to 348 | -172 to 207 |
| Faster left/right rotation | 2,879 | -1889 to 1801 | -581 to 689 |

Windows included the operator's response time and transitions; the tilted
hold was hand-held rather than mechanically fixed, and no per-packet timeline
or orientation measurement was retained. Faster rotation produced a larger
range, consistent with a rate-sensitive signal, but translation and the
tilted hold also varied. Unintended rotation, vibration or handling may
contribute. This does not establish gyro identity, distinguish angular rate
from all acceleration effects, prove scale or direction, or identify a third
axis. Rotation-speed comparisons on other axes and a mechanically supported
tilted hold remain unperformed. The listener unsubscribed; no selector
restoration was attempted.

### Optional outbound transform and candidate ring initializer

When the state byte checked at `0x1e070..0x1e078` is nonzero, the enqueue
path calls `FUN_00023490` on the copied object. Read-only instructions at
`0x23494..0x2349e` increase its second byte by four; `0x234a2..0x234d2`
copy a 32-bit context value into a later context field and increment the
original. The function then calls `FUN_00023200` and `FUN_000230b4` and
copies four bytes from its local result to the end of the object at
`0x234f0..0x23514`. The full algorithms, context initialization, bounds and
hardware behavior are not verified; this is not sufficient to identify a
cipher, security level or wire format.

The paired `FUN_00023528` rejects lengths of four or less, subtracts four
from longer object lengths, increments another context word, invokes the
same two helpers in the opposite order and compares a four-byte result with
the object's trailing four bytes (`0x23530..0x235ce`). On the selected
`FUN_0001de7c` branch, `0x1ded6..0x1def4` calls it and sets a state byte
when its result is nonzero; a zero result continues to the next parser branch.
These are transform/check-shaped operations, not proof of authentication,
correct verification in every path, or successful encrypted radio traffic.

The main image contains a **candidate** outbound ring initializer at image
offset `0x3b58`: a 32-bit stride `0x28`, a 16-bit count `0x10`, two zero
index bytes and word `0x00843d70` at `0x3b60`. These values match the fields
and separately loaded base used by the reviewed queue producer and consumer;
if loaded unchanged at `0x00843b58`, they describe sixteen 40-byte slots
covering `0x00843d70..0x00843fef`. This is distinct from the candidate
receive descriptor at image `0x3d64`. The loader that would map these image
offsets to RAM is not established, nor are subsequent changes or actual slot
capacity. Do not treat the initializer as proof of live device state.

Method: decoded little-endian candidate fields directly from the image;
verified the transform, paired comparison and caller branch with read-only
Ghidra instructions, discarding temporary project changes. Next trace the
context-state initialization and helper operations, and identify the actual
image loader or another source of the runtime descriptor.

### Transform context and helper follow-up

Both the outbound and paired inbound paths use context base `0x00844314`;
the optional-transform gate is byte `+0x2e` (`0x00844342`). The reviewed
initialization-shaped instruction sequence at `0x1e4ac..0x1e4fa` clears
that gate and adjacent state bytes. Its entry conditions and all other
writers have not been traced, so initial runtime state on every boot is not
established. One enabling branch is identified below; its upstream trigger
remains unresolved. The related
`0x1ef38` input handler copies an object and either defers or calls the input
parser depending on the same gate; this does not establish packet delivery.

The outbound context supplied to `FUN_00023490` begins at `0x0084431c`.
Its instructions copy a 16-byte region from that context to addresses
`0x00800550..0x0080055f` before calling the two helpers. The helper literals
at `0x231f8`/`0x23378` and `0x231fc`/`0x2337c` resolve to `0x00800540`
and `0x00800548`. The published [TLSR8258 register definitions](https://github.com/telink-semi/telink_zigbee_sdk/blob/7cf1d6dbac6497505d649bd03342efe81f1a8212/tl_zigbee_sdk/platform/chip_8258/register.h#L551-L562)
label the corresponding offsets AES control, data and key registers. This
strongly supports an AES-peripheral interpretation **if** the controller uses
the same map; the actual chip and runtime configuration remain unverified.
No context contents, keys or raw payloads are published here.

The saved decompile of `FUN_000230b4` shows a 13-byte context-derived input,
16-byte register-output blocks, and XOR of output bytes into the supplied
buffer; the four-byte result is XORed as well. Selected instruction checks
include the 13-byte copy at `0x230ce..0x230d8`, register accesses at
`0x230e2..0x23120` and four-byte XOR at `0x23124..0x23132`. The other helper
`FUN_00023200` also uses the same register-shaped interface and 16-byte
blocks; its selected dataflow is instruction-checked below. These observations
do not establish the precise mode, key derivation, interoperability or security
of the scheme.

Method: decoded literal words from the hash-identified main image, checked
the context gate and selected helper instructions using read-only Ghidra,
and compared offsets against the pinned public SDK header.

### Transform enable branch and second helper

At `0x1e148..0x1e150`, code reads state base `0x00844314` byte `+0x2d`
and branches when it equals 2. That branch sets the state byte to 3, then
instructions `0x1e1ba..0x1e1c8` set byte `+0x2e`—the transform gate—to 1.
The branch passes structure-relative buffers to `0x233fc` at `0x1e1ca..0x1e1e4`.
This identifies an enabling writer, not whether the branch runs in a physical
session. A state-2 writer is traced below; other writers remain possible.

Read-only instructions at `0x233fc..0x2347a` show the setup helper combining
two eight-byte inputs into a local block, calling `0x23380` and `0x1c364`,
then copying 16 derived bytes and two four-byte inputs into the context
supplied by the caller. It clears the first eight bytes of that context. The
source fields' provenance, the precise derivation and ordering or concurrency
relative to the already-set gate remain to be established. No intermediate
contents or keys are recorded.

Selected instructions for `FUN_00023200` confirm the second helper's block
dataflow: `0x23284..0x232a0` feeds the previously identified register-shaped
interface and waits for a status bit; `0x232b4..0x232cc` processes a full
16-byte input block, while `0x23314..0x2332a` handles a shorter block with
zero padding. `0x232da..0x23302` XORs four 32-bit register outputs with a
local block, and `0x23354..0x2335a` stores one resulting register word
through an output pointer. These selected instructions support the saved
decompile's block-processing description, but do not independently verify
every branch, the exact cipher mode, data integrity or hardware operation.

Method: read-only Ghidra disassembly of the enabling branch, `0x233fc` and
`0x23200`, using the hash-identified image; temporary analysis changes were
discarded. The state writer and two input sources are examined below; trace
remaining branches of the helpers and ring loader.

### State-2 writer and setup-input provenance

The instruction sequence at `0x1eecc..0x1eef8` is a state-2 writer. It
compares its first argument, truncated to 16 bits, with the halfword at
`0x00844294 + 0x12`, and rejects a mismatch or `0xffff`. On the qualifying
branch, it copies 16 bytes from its second-argument pointer to context
`0x00844314 + 0x54`, then writes 2 to context byte `+0x2d`. Its callers and
the provenance and validation of that 16-byte input are not yet established.
Another inspected instruction sequence writes 2 at offset `0x2d` of a
**different** base (`0x008440d0`), so it is not evidence for this context.

On a separately gated input-object path, `0x1e990..0x1e9a0` passes context
buffers `+0x48` (eight bytes) and `+0x50` (four bytes) to `FUN_0001cfa0`.
Instructions `0x1cfa0..0x1cfca` fill the requested bytes from successive
calls to `FUN_00002644`, using each returned word in byte steps. The saved
decompile of `FUN_00002644` updates two internal words with arithmetic and
mixes a separate register-shaped value; entropy, seeding and unpredictability
are not established. `0x1e9ae..0x1e9c6` separately copies eight and four
bytes from the currently processed object into context `+0x64` and `+0x6c`.
These are among the buffers passed to the setup routine above, but this
selected path does not prove all are current or that setup subsequently runs.

The same selected path may invoke an indirect callback through the trampoline
at `0x1eeb0`. A bounded byte search
found no exact stored 32-bit `0x1eecc` or tagged `0x1eecd` value in the
image, but that does not exclude direct branches, computed pointers, RAM
registration or external code. The follow-up below identifies a registered
intermediate callback and a conditional direct call to the setter; object
framing, length validation, context setup inputs and later uses remain
incomplete. No content or secrets are published.

Method: verified the setter, source-buffer calls and trampoline with read-only
Ghidra instructions, decoded literal addresses directly from the identified
image and inspected the saved generator decompile. The selected caller and
generator checks are documented below.

### Registered callback and generator initialization

On a qualifying `FUN_00021cc0` initialization branch, instructions
`0x21cd8..0x21cda` pass the image literal `0x00020e99` to
`FUN_0001de48`; `0x1de48..0x1de4a` stores it at
`0x00844294 + 0x6c`. Under the selected TC32 convention its tagged target is
`0x20e98`. The previously traced input-object path reads that same callback
field and invokes `0x1eeb0`, an indirect branch through `r3`, when the field
is nonzero. Later overwrites and alternate initialization remain possible.

Selected instructions at `0x20e98..0x20f34` show that this callback performs
a ten-byte comparison and other state-dependent processing. On one qualifying
branch it passes its preserved first argument and a locally prepared
16-byte buffer to the direct call at `0x20f34` to `0x1eecc`. The preparation
includes a call to `0x2367c` and a bounded copy; its full inputs, other
branches, buffer initialization, length validation and callback return are
not yet traced. Thus the saved image contains a **conditional static chain**
from the registered intermediate callback to the state-2 writer; it does not
prove that a particular incoming object is accepted, that all 16 bytes are
valid, or that setup or notification delivery occurs on hardware.

Read-only instructions at `0x2644..0x2672` confirm that
`FUN_00002644` updates words at `0x00844048` and `0x0084404c` using
multiply/add operations and XORs the result with the word read from
`0x00800740`. The pinned [TLSR8258 register map](https://github.com/telink-semi/telink_zigbee_sdk/blob/7cf1d6dbac6497505d649bd03342efe81f1a8212/tl_zigbee_sdk/platform/chip_8258/register.h#L800-L802)
labels offset `0x740` as a system tick; this is cross-model evidence, not a
confirmed chip identity. During the reviewed initialization path,
`FUN_0001cf0c` waits on a bit at `0x00804408` and reads `0x0080440c`
twice into those two state words (`0x1cf40..0x1cf60`). The peripheral's
identity, readiness semantics, seed quality, other writers and whether the
reads differ remain unverified. Nothing here establishes cryptographic
randomness or security; no seed or context values are recorded.

Method: independently decoded the relevant literal words, checked the
registration, direct call, generator and seed instructions with read-only
Ghidra, and discarded temporary analysis changes. Next trace all branches
and validation in the registered callback and the source at `0x0080440c`,
without collecting or publishing sensitive values.

## Shared settings write path

`FUN_000119d8` selects a state value via `FUN_00011954`, then calls
`FUN_00011984`. That function invokes `FUN_00008284` and `FUN_00008268` with
`0x30000`, the second with length `0x97`. The first reaches `FUN_0001b1a0` →
`FUN_00001774`: the latter emits `0x06`, then `0x20`, then three address bytes
and waits on a status-shaped operation. This resembles a serial-flash erase,
but the peripheral mapping, indirect program target and full persistence
semantics have not been established. Avoid assuming this is a harmless LED
effect or a proven flash write. See the correction in [LED paths](led-control-paths.md).

The controller chord path `FUN_00011c44` writes `0xfe` or zero at offset 9
of the buffer identified by literal `0x11c74`. Direct image reads show that
literal and the settings source literal `0x119ac` both contain `0x0084730c`.
The same function separately sets `0x96` in the state block at `0x00846ac8`
and calls `FUN_00011984`. This establishes a configuration-buffer alias, not a
wire report ID. It does not establish equivalence to the `0xEE` path, which
sets state offset `0x2a`. See the correction in [chord evidence](fe-report-toggle.md).
`FUN_00011ca8` reads 0x97 bytes from address `0x30000` into that same buffer,
then sets state offset `0x96` to 1 only if buffer byte 9 equals **1**; it sets
the flag to 0 otherwise. The apparent mismatch with `FUN_00011c44` writing
`0xfe` when enabling must be resolved at instruction level and through the
read/write wrappers before claiming the chord's state survives a restart.
`FUN_00009d20` reads the same state offset `0x96` (its literal entry `0x9de4`
also resolves to `0x00846ac8`). It selects different normalization arithmetic
when the flag is 1. The four calls in `FUN_0000a704` process the first four
analog channels. This is direct static evidence that the flag affects
control-scaling calculations; it is not evidence that the flag enables motion.
Its exact user-facing meaning and whether it also affects other functions
remain unresolved.

## RGB test command convergence

The four-byte Bluetooth dispatcher's `12 a b c` and the USB dispatcher
`FUN_00018138`'s `E0 a b c` both call `FUN_0000ceb4(a,b,c)`. Space Station's
`LibCommon.Command.DongleCommand.getTestRGBCommand(r,g,b)` builds USB
`05 E0 r g b ...`. The shared callee stores those values into five triples and
sets a flag. Together these are a strong static indication that Bluetooth
`12` sets test RGB values in the same order. Neither invocation's physical
effect, persistence, selected LEDs nor runtime reachability on this unit is
established by this comparison. The USB report ID and Bluetooth payload framing
are different; do not interchange the two command byte sequences.

## Android OTA implementation

REA inspected the base APK without execution. Class inventory
`ev_fa34a710f84aa8b2df264398ce063b4aff1e7d0dd565675a33a769aa5d14b35a`
identifies `com.flydigi.sdk.bluetooth.ota.TelinkOtaFactory` and its methods.
Method `k` looks up a service and characteristic, requests notifications and
returns the same characteristic in both positions (Evidence
`ev_3a85bde53d548a5cbabc8ddc5b6678797e7bea453d5ee4bf1c6f3ba8dbdbaf3f`).
Method `a` constructs indexed, padded chunks and calls two helper methods
before returning the chunk (Evidence
`ev_5cba433e2b541a5c04794a4c10090fdd1d9951d5191a75050e72dd5261bdde91`).
Method `s` emits an integer as two little-endian bytes before optional data
(Evidence `ev_29d734f0f60544e5afc80c3348225d2c59c889cc035c1af935de0ebc9e3a0c55`).
Methods `e` and `l` return the OTA characteristic UUID ending `2b12` and
service UUID ending `1912`, respectively (Evidence
`ev_55e787758c23092a6469ebad7b7cc1b0ce307bfcf735de24381f0596c69ca42c` and
`ev_f109800d882190a3ad57054af27df8a13cbd84112ee2173d7df14bdc61581814`).
The chunk builder places an index and a two-byte result at the ends of the
buffer; methods `w` and `v` write those values little-endian. The decompiler
view of the calculation in `u` contains an unusual boolean/integer expression,
so its exact integrity algorithm needs DEX-level verification before naming it.

These observations identify client-side packet construction only. They do not
establish which firmware image uses this route, bootloader authenticity checks,
successful update behavior, or recovery. The APK's native libraries, dynamic
dispatch and other upgrade factories are not covered by these method views.

## Windows serial updater attribution correction

An exact search of the saved `Station/` C# decompilation for
`OtaSerialPortUpdater` found its declaration and one construction site:
`FmUpgradLCDViewModel.StartScreenUpgrade`, in the `lcdType == "freq"` branch.
That branch searches for a serial device and passes `ScreenBinFullPath` to the
updater. This is evidence for a screen-update path, not evidence that the
Vader 3 Pro main-MCU update uses it. Reflection, omitted binaries and paths
outside the saved `Station/` tree are not excluded.

Within that screen updater, `CmdType` defines a reset opcode of 9. The `8`
written after it by `SendOtaCommand` is a length byte, not the reset opcode.
Its response parser initializes a result field, but the shown state transitions
do not establish that it validates the result. The timer can also call
`OtaStop(2)` after a timeout in state 6. These are observations about this
screen updater only; neither a success indication nor a client-side CRC proves
that a target accepted or verified an image. Trace the controller-specific
updater separately before drawing any bootloader conclusion.

`GamePadSetService.SendUpgradeCmd` sends an IPC request carrying an MCU type.
`FDGTransData.SendUpgradeCmd` selects a DInput or XInput command builder from
the connection mode. `DongleCommand.GetSoftUpgradeCmd` constructs a DInput
`05 F5` command for several MCU types and `05 F6` for others. The WCH-specific
`FmUpgradeComSwViewModel.StartUpdateGamePad` waits for `WchIspHelper`, downloads
a file and calls `ISPFlashDnThread_CH55xUSB`; that path is not automatically
the Telink main-MCU updater. Mapping the Vader 3 Pro device record and its
actual updater selection is still necessary. No updater was executed here.

## Outgoing framing correction

`FUN_0000fc18` inserts the result of `FUN_0000f8bc` into an outgoing buffer.
The latter selects `0xf0`, zero, or a table-derived value, with a possible
`0x80` bit, according to state. The inspected decompilation does not show a
checksum calculation in that function. Its exact field meaning and the whole
frame's integrity scheme remain unknown. This corrects the checksum attribution
in [queue topology](bluetooth-queue-topology.md).

## Checksummed Bluetooth dispatcher

`FUN_00010980` checks a 20-byte frame by summing bytes 0 through 18 and
comparing the low byte to byte 19. It then invokes `FUN_000107a0`. In the
saved decompile, `_` accepts two indices below 8, reads 16 bytes through
`FUN_00014538` using their computed offset, constructs a 20-byte reply and
calls `FUN_00010300`. `R` calls `FUN_00013fc4`, which fills a 900-byte region
with `0xff` and resets several state fields. `S` calls `FUN_00014000`, which
clears and rebuilds several tables from other state. `@` accepts values 0–3
and otherwise stores zero at a state offset; `A` stores one byte in state.
The exact identities and consequences of those state regions remain unknown.

`Q` calls `FUN_00014458` with 16 bytes and an offset computed from frame bytes
1 and 2. That helper maps offset ranges to several different destinations;
it is not established to be a contiguous arbitrary-address write. The `T`
branch calls `FUN_00012148` only when a state byte is 2. That function reaches
the same erase/write-shaped helpers with address `0x32000` and length 900;
the target and persistence require the same independent validation as the
`0x30000` path. After most opcode branches, including `P`, the code calls
`FUN_0001043c`, which constructs an `AA CC` four-byte response using frame
bytes 18 and 19. `_` follows a separate branch. These are static control-flow
observations, not proof a response is transmitted or a command is safe.

## Reset-shaped command caveat

The matched `13 04 00 00` path calls `FUN_00013dc4` and `FUN_00013e60`.
The latter passes `0x308`, `4`, and `1` to helper functions and changes bytes
through literal-derived pointers. The decompile alone does not establish that
those three constants are register addresses, nor does it identify bootloader
entry. The historical description in the handoff and command reference is
therefore too specific. This investigation performed no reset test.
