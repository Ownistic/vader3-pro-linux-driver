# Secondary firmware: initial static architecture triage

This is static analysis of retained downloaded images, not a hardware test or
proof of installed device revisions. No image bytes or captures are published.

## Dongle image

The extracted `F3_Telink80_For_Dongle_Ver0.4.1.7_release_01.bin` is 22,852
bytes and has SHA-256
`af74da8b80e60aa21f49d46805f89dff9f2d6834a5d2c063fde0097b240a8a60`.
At file offset `0x08` it contains the four-byte `KNLT` marker also present
in the main image. Its little-endian word at `0x18` is `0x5944`, equal to
the file length. That match is an observation, not proof of the word's role,
the image's authenticity, or the installed device's firmware.

The image was imported as a raw binary at address zero in a **separate**
temporary Ghidra project using the existing locally corrected
`Telink_TC32_PC:LE:32:default` language. This is a caller-selected architecture
hypothesis, not automatic detection. Under that hypothesis, the first
instruction decodes as a branch from `0x00` to `0xb4`. Temporary read-only
disassembly from `0xb4` shows coherent startup-shaped operations: register
updates, two clearing loops, processor-mode and stack assignments, and
word-copy loops at `0x14a..0x15c` and `0x15e..0x170`. This supports a TC32
analysis direction; it does not establish the exact chip, load mapping, or
runtime execution. The decoder's address-space and instruction limitations
described in [the research index](research-index.md) still apply.

REA's raw-binary open schema does not offer TC32 as an architecture, so this
specific decode used the existing Ghidra language directly rather than
mislabeling the image as ARM. The temporary project is under
`/tmp/opencode/dongle-static`; it is not a durable research artifact. The
existing REA Binwalk scan and its unvalidated signature candidates remain
documented in the index; neither that scan nor this reset-path inspection
establishes code coverage. The startup copy ranges are traced below; interrupt,
radio and USB paths still need comparison against the main image without
assuming they share a protocol.

### Startup copy ranges

Read-only disassembly in that TC32 hypothesis, with the PC-relative literal
values independently decoded from the original image, gives these ranges. End
addresses are exclusive; the loops copy one four-byte word per iteration.

| Instructions | Source address range | Destination range | Bytes |
| --- | --- | --- | --- |
| `0x14a..0x15c` | `0x5718..0x5774` | `0x00842200..0x0084225c` | `0x5c` |
| `0x15e..0x170` | `0x5774..0x5908` | `0x0084225c..0x008423f0` | `0x194` |

The literal-pool words for those six bounds are at `0x1a0..0x1b4`.
Under the explicitly selected file-offset-zero mapping, the corresponding
source byte ranges lie within the `0x5944`-byte image; the ranges and lengths
are inferred from the loop instructions and literal values, not from an
observed copy on the device. The first loop is conditional: code at
`0x120..0x13e` reads a bit via the `0x008000b8`-based register sequence
using selector `0x7e`. A zero result enters the first loop; a nonzero result
instead takes a halfword transfer before joining the second loop at `0x15e`.
The register's physical meaning and wake/reset implications are not established.

These are mappings for **this dongle image**, not the main image's unresolved
`0x3d64 -> 0x00843d64` loader path. They do not prove the complete startup
mapping, hardware address aliases, a particular controller chip, or radio/USB
behavior. The Ghidra project discarded the temporary disassembly changes.
The interrupt and receive-shaped path are traced below; remaining startup
ranges and radio/USB report paths need independently verified addresses.

### Interrupt and receive-shaped path

Under the selected TC32 decoding, the instruction at image offset `0x10`
branches to `0x200`. That wrapper saves registers, calls `0x230`, restores
state, and executes an interrupt-return-shaped instruction. At `0x230`, the
dispatcher reads a halfword at `0x00800f20`, tests bits `0x02` and `0x01`,
calls `0x430` for the former and `0x5bc` for the latter, and writes the
respective bit back to the same address. Literal pools were decoded directly
from the image. The cited TLSR8258 reference in
[staged-frame transport](staged-frame-transport-static.md) names these RF TX
and RX status bits; this is a cross-model interpretation, not proof of the
dongle chip, physical interrupt behavior, or transport protocol.

The bit-`0x01` handler at `0x5bc` uses an index at `0x008429a0` to select
one of two `0x50`-byte-spaced addresses beginning at `0x00842854`, stores
the selected address at `0x008425ac`, and toggles the index. It calls
`0x5408` with the alternate address, `0x50`, and zero. The callee writes the
address's low 16 bits to `0x00800c08` and additional values to
`0x00800c0a` and `0x00800c0b`; those offsets match the cited reference's RF
receive-DMA register family, but the full address mapping and ownership are
not proven by the static trace.

For the selected buffer, `0x5f8..0x60c` tests bit 0 of the byte at
`buffer + buffer[0] + 3`, and compares `buffer[0]` with
`(buffer[4] & 0x3f) + 11`. Failure increments a halfword at `0x00842714`
and writes 1 to `buffer[0]`. On the checked path, a state- and type-dependent
branch calls either `0x4b8` or `0x29d8`; selected downstream paths are traced
below. The shape of the two checks resembles the main-image
parser described in [staged-frame transport](staged-frame-transport-static.md),
but does not establish that either image transmits or accepts the same frame,
nor the status bit's physical meaning. No checksum, CRC, bounds safety, or
successful receive is inferred.

These observations were checked with read-only temporary disassembly of
`0x10`, `0x200..0x226`, `0x230..0x27e`, `0x5bc..0x644`, and
`0x5408..0x5436`; literal values came from the identified image bytes. Ghidra
discarded temporary changes. Verify the exact chip and register aliases
independently.

### Receive handlers and endpoint-shaped handoffs

Under the same selected decoder, `0x4b8` returns zero unless the low nibble
of received byte `+5` equals 2. On the examined path it calls `0x6dc` with
payload pointer `buffer + 6` and an 8-bit length derived from byte `+4`.
For a length greater than 4, `0x6dc` tests for the `5A A5` prefix: one
branch calls `0x341c` and conditionally passes payload `+2` to `0x1ea0`;
the other copies the payload to `0x00842b10`, records its length at
`0x00842785`, and sets `0x00842774`. Those observations do not prove all
possible input lengths are safe or that the marker has a particular protocol
meaning. `0x4b8` also constructs an object at `0x00842918` and calls `0x2c8`.

The alternate handler `0x29d8` performs state and four-byte field comparisons,
and one branch constructs an object at the same `0x00842918` address. It too
can call `0x2c8`. The latter reaches `0x5448`, which writes a word to
`0x00800f18`, sets a bit at `0x00800f16`, writes `0x85` at `0x00800f00`,
and writes the object's low 16 address bits at `0x00800c0c`. This is an
RF-transmit-shaped handoff by comparison with the cited Telink register map,
**not** the USB endpoint path; transmission remains unobserved. Full branch
conditions and the meaning of the assembled fields require further work.

One downstream path from the non-marker payload is now connected to report
state: `0x2e60` clears the `0x00842774` flag and calls `0x3ff4`; under
additional state and queue conditions, `0x3ff4` calls `0x3f60` or `0x3da8`.
Both inspected routines read `0x00842b10` and update state at `0x00842a9c`.
In particular, `0x3fba..0x3fc8` copies twelve consecutive bytes from payload
offset `0x11` to state offset `0x2d`. Their physical meaning is unverified;
do not call the bytes gyro axes or infer units from their count.

Separately, `0x3ae0` can construct a 32-byte `04 FF`-shaped object from a
queue entry under its own conditions and call `0x3274`. The latter tests
readiness and writes the supplied bytes one at a time to `0x0080011b`, then
writes a control value to `0x00800123`. This is endpoint-shaped and resembles
the main image's USB FIFO path, but endpoint identity and descriptor mapping
remain unverified. The selected queue linkage and normalized-state serializer
are traced below; neither proves that a physical USB report was delivered.

Read-only temporary disassembly verified the cited calls, state stores, and
FIFO loop; literal pools were independently read from the image. Temporary
analysis changes were discarded.

### Queue linkage and normalized-state serializer

`0x1ea0` builds a temporary entry with byte `0` equal to the payload length,
byte `1` equal to 2, and the supplied payload beginning at byte `2`. It
rejects lengths greater than 29 and suppresses an entry matching its inspected
queue entry. Otherwise it calls `0x1de4` with `length + 2` bytes. That callee
checks queue occupancy and slot size before copying; notably `0x1ea0` returns
1 after the call without testing its return value, so its return alone is not
proof of insertion when the queue is full or the slot-size check fails.

On the marker branch of `0x6dc`, the supplied payload starts two bytes after
`5A A5`, and the queue descriptor is `0x008423c0`. Under the conditions
already documented for `0x3ae0`, that routine checks the first queued payload
byte at entry `+2`: if it is `0xf0`, it builds an object beginning `04 FF`,
copies the entry payload beginning at `+2` to object `+2`, compares 32 bytes
with a cached object, and attempts the `0x3274` endpoint-shaped write if
changed. A successful writer result causes the queue head to advance. Thus a
conditional **static** route from a `5A A5`-prefixed received payload whose
next byte is `0xf0` to that write is established, subject to validation,
duplicate suppression, queue capacity, scheduling, and readiness. No physical
receipt or host delivery is established; exact frame and report semantics
remain unknown.

`0x3ae0` also calls `0x39f4` on a state-dependent branch. That routine builds
a 32-byte object beginning `04 FE 66` from the state at `0x00842a9c`, then
passes it to `0x3274`. On the previously traced `0x3f60` conversion path,
payload bytes `+0x11..+0x1c` become state bytes `+0x2d..+0x38`. The serializer
places those twelve bytes at object offsets, in source order:
`0x12, 0x14, 0x1a, 0x1b, 0x1d, 0x1e, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
0x10`. This establishes a conditional code-level path into the
endpoint-shaped write, not sensor-axis identities, units, synchronization,
descriptor endpoint identity, or observed hardware behavior. Other state
converters and branches need separate analysis.

Read-only disassembly checked `0x1ea0`, `0x1de4`, `0x3ae0`, `0x39f4`,
and `0x3274`; literal pools and state addresses were decoded from the image.
Selected scheduling and queue initialization are traced below; other converter
branches and descriptor/capture comparisons remain open.

### Scheduler and initialized queue descriptor

The read-only decode identifies a direct call to `0x3ae0` at `0x2ac` inside
`0x28c`. The interrupt dispatcher at `0x230` reads `0x00800648`; its bit-0
branch calls `0x668` and then `0x28c`, before writing to `0x00800623`.
Within `0x28c`, the call to `0x3ae0` requires the byte at `0x008429c7` to
equal 1 and the byte at `0x008423cc` to equal 1. When the first is 1 and
the second is 0, the code instead calls `0x37b8` and `0x3478`. The second
byte's other values return without either branch. The timer-shaped setup below
supports an interrupt-source interpretation, but physical scheduling frequency,
initial hardware state and other indirect callers remain unestablished. The
byte at `0x008429c7` is also tested by the endpoint-shaped writer; its semantic
name remains unknown.

The **dongle** startup's second copy loop at `0x15e..0x170` maps image
offset `0x58d8` to RAM address `0x008423c0`: the source is `0x5774` and the
destination is `0x0084225c`. The 12 image bytes at `0x58d8` decode as a
little-endian stride word `31`, a count halfword `2`, two zero index bytes,
and a storage pointer `0x00842794`. These fields match the accesses in
`0x1de4` and `0x1ea0`; under the selected zero-based image mapping and if the
startup copy runs as decoded, the initial storage spans two 31-byte slots at
`0x00842794..0x008427d1`. The same image-to-RAM arithmetic maps byte
`0x58e4` to the selector at `0x008423cc`, whose initializer is 1. This does
not rule out later overwrites, hardware aliases, or other startup paths, and
does not establish the **main-controller** ring descriptor's initialization.

For this initial dongle descriptor, `0x1de4` rejects a copy length greater
than or equal to the stride. Consequently, although `0x1ea0` accepts a
payload length of 29 before calling `0x1de4`, the resulting 31-byte entry is
rejected by that callee with these initial values; `0x1ea0` still returns 1
because it does not inspect the callee's return. This is an instruction-level
conditional observation, not proof of a runtime failure or vulnerability.

Evidence: read-only temporary Ghidra disassembly of `0x230..0x27e`,
`0x28c..0x2ba`, `0x1ea0..0x1f58`, and `0x1de4..0x1e42`; independent
little-endian reads of the original image's literals and initializer bytes.
A bounded even-offset scan for direct TC32-style long branches found
`0x2ac -> 0x3ae0`; it does not exclude indirect calls or alternate encodings.
The setup and an alternate converter are further traced below. Next trace later
descriptor writes and compare output offsets against descriptors and saved
captures without assuming physical behavior.

### Timer-shaped setup and alternate converter

The `0x4b8` receive handler calls `0x2adc` with argument 1 when the high
nibble of received byte `+5` equals `0x80`, otherwise with 0. In the verified
`0x2adc..0x2b14` instructions, the nonzero branch stores 100 and the zero
branch stores 500 as a halfword at `0x0084228e`. Both enter a shared path
that calls `0x4e24` with zero, computes `r1 = 48000 - 48 * value`, sets
`r2 = 48000`, and calls `0x4d2c` with `r0 = 0`; it then calls `0x4dec`
with zero. These numbers are raw arithmetic values, not established time
units or an observed polling rate.

For that argument, `0x4d2c` sets bit 0 at `0x00800640`, writes `r1` to
`0x00800630`, `r2` to `0x00800624`, writes 1 to `0x00800623`, and clears
bits 1 and 2 at `0x00800620`; `0x4dec(0)` then sets bit 0 at `0x00800620`.
The independently published TLSR8258 SDK names those offsets respectively
IRQ mask, timer-0 tick, timer-0 capture, timer status, and timer control;
its IRQ source at offset `0x648` identifies bit 0 as timer 0. See the pinned
[register definitions](https://github.com/telink-semi/telink_zigbee_sdk/blob/7cf1d6dbac6497505d649bd03342efe81f1a8212/tl_zigbee_sdk/platform/chip_8258/register.h#L722-L766).
The setup, dispatcher test and status write are therefore strongly consistent
with a timer-0-driven path under that cross-model interpretation. Actual
silicon, register aliases, clock frequency and delivered interrupt cadence are
not independently established.

The alternate state converter `0x3da8`, called by `0x3ff4` on its selected
state branch, loads payload at `0x00842b10` and state at `0x00842a9c`.
Instructions `0x3dcc..0x3e06` copy payload bytes `+2..+5` to state `+0..+3`
and assemble four little-endian halfwords from payload `+6..+0xd` into state
`+8..+0xf`. Instructions `0x3e08..0x3e4a` copy payload `+0xe..+0x12` to
state `+0x24, +0x25, +0x1c, +0x1d, +0x1e`, and payload `+0x13..+0x1c` to
state `+4..+7, +0x10..+0x13, +0x26..+0x27`. The remaining inspected
instructions build bitfields in state `+0x28..+0x2b`. Unlike the earlier
`0x3f60` path, no direct store to state `+0x2d..+0x38` appears in the
disassembled `0x3da8` body. Initialization and selector gates are analyzed
below. No button, stick or sensor semantics are inferred from the shapes alone.

Read-only Ghidra instruction checks covered `0x2adc..0x2b14`,
`0x4d2c..0x4dd8`, `0x4dec..0x4e1e`, and `0x3da8..0x3f38`; literal pools
were decoded from the identified image. Next inspect descriptor writes and
state initialization across mode changes, timer clock configuration and
callers, and report/descriptor correspondence. Temporary disassembly was
discarded; no hardware was contacted.

### Initialization and retained report-source state

Under the selected zero-based image mapping and TC32 decoder, the startup
instructions at `0xe6..0xf4` zero successive words starting at `0x008423f0`
while the address is below `0x00842b85`. The two literal values are at image
offsets `0x188` and `0x18c`. This range includes all twelve report-source
bytes at `0x00842ac9..0x00842ad4` (state base `0x00842a9c` plus
`0x2d..0x38`). The later two startup copy loops target ranges ending at
`0x008423f0`, so those particular copies do not overwrite the twelve bytes.
An earlier startup fill writes `0xffffffff` over a broader range before the
zeroing loop. Thus the decoded startup sequence initializes these bytes to
zero if that path runs as decoded; it is not a physical observation or proof
that every reset or retention path runs it.

In the inspected non-marker path, `0x6dc` copies the payload to `0x00842b10`
and stores its first byte at state offset `0x2c`. Subject to the other gates
at `0x3ff4`, selector 1 calls `0x3da8`, while selectors 2 or 3 call `0x3f60`.
The latter writes the twelve bytes; the complete decoded `0x3da8` body does
not directly write them, although its branch to `0x3d98` has additional
side effects described below. Therefore the inspected
converter itself can leave the previous twelve-byte state in place when the
selector changes from 2/3 to 1. This is a **conditional static retention
possibility**, not proof that such a sequence occurs or a guarantee no other
writer clears the state between conversions.

The `0x3ae0` normal-output branch checks state offset `0x2c` and proceeds for
selectors 2 or 3, not 1. A separate queue path can write selector 3 before
rejoining the normal-output logic, but its queue, cache, readiness and
scheduling conditions must also hold. Consequently, converter-level retention
alone does not prove stale bytes reach the endpoint, much less that the
physical controller reports stale motion. The mode-change routine `0x3c64`
was also inspected: it writes a separate selector and on one branch builds or
copies a queue-shaped object; no direct clear of the twelve bytes appears in
its decoded body. Calls and aliases remain to be traced.

Evidence: Ghidra read-only instructions at `0xc6..0x104`, `0x14a..0x170`,
`0x6dc..0x716`, `0x3ff4..0x404a`, `0x3ae0..0x3be4`, and
`0x3c64..0x3ce0`; startup and state literal pools were independently decoded
from the identified image. A literal-word inventory of addresses
`0x00842a80..0x00842af0` finds multiple aliases, which were not all traced;
this is not a whole-program proof of writers or freshness. The `0x3d98`
callee and additional output gates are traced below; compare output
descriptors and recorded captures without treating static state as hardware
evidence.

### Converter helper, scheduler selector and retained-output gates

The exceptional `0x3da8` branch at `0x3f32..0x3f38` calls `0x3d98` when
the selector at `0x008423cc` is nonzero and an associated byte exceeds 100.
`0x3d98` calls `0x3c40(0)` and then `0x3d64`. The first call **clears
`0x008423cc`**, the separate byte that `0x28c` requires to equal 1 before
calling `0x3ae0`. Its `0x3c0c` callee copies that selector to offset `0x12`
of an object at `0x0084295c`, calls `0x4b1c` with `0x1f000`, and passes
that address, length `0x14`, and the object to an indirect function pointer
loaded from `0x00842214`. The startup initializer and storage-shaped
transaction are traced below; neither proves a physical flash write,
persistence, or a safe hardware command.

The `0x3d64` call performs selector-`0x0b` transactions through `0x908` and
`0x948`, with a `0xc84` counter-wait-shaped call in between. The inspected
instructions do not directly store the twelve report-source bytes, but their
peripheral effects, the indirect function above, and possible concurrent
writers remain unverified. If the exceptional branch runs, it disables the
inspected `0x28c -> 0x3ae0` route until some other path restores the selector;
the converter can also take branches that bypass `0x3d98`.

The queue-to-output instructions narrow, but do not prove reachability of, a
retained-value sequence. With scheduler selector 1, report selector 1,
retained source bytes, a qualifying `0xf0` queue head, and no intervening
refresh, `0x3ae0` can compare its constructed `04 FF` object with the cached
32 bytes. If they match, `0x3bce..0x3bd6` sets report selector 3 and rejoins
the normal-output path, which can call `0x39f4` and attempt `0x3274`. A
changed object instead attempts the `04 FF` write first and takes different
branches depending on its result. `0x341c`, called before enqueue, separately
compares and updates a prior payload and returns a change indication; queue
insertion and output therefore require more than just the marker and selector.
The cache, queue history, scheduler and readiness constraints have not been
shown jointly reachable from initialization, and indirect/concurrent writes
remain open. **Stale serialization is still a conditional code-path
hypothesis, not a demonstrated full execution or physical-device result.**

Read-only Ghidra checks covered `0x3d98..0x3da4`, `0x3c40..0x3c5e`,
`0x3c0c..0x3c2e`, `0x3d64..0x3d92`, `0x908..0x934`, `0x948..0x978`,
`0xc84..0xc92`, `0x4b1c..0x4b30`, `0x341c..0x346e`, and the previously
documented converter and output paths. Independent literal resolution found
eleven aligned literal-pool instances of `0x008423cc`; these are candidates,
not a complete alias or write inventory. Selector restoration and cache/queue
initialization remain open.

### Dongle storage-shaped transaction

The conditional startup copy `0x14a..0x15c` maps image offset `0x572c` to
`0x00842214`; that word is `0x00004ae1`. The indirect trampoline at `0x3c3c`
branches through the loaded register. Under the selected TC32 decode, the
low-bit-tagged value corresponds to the code at `0x4ae0`, but the conditional
copy can be skipped on the alternate startup branch and later pointer writes
or aliases have not been excluded. Thus this is the **conditional initializer**,
not a proof of the runtime target in every state.

Decoded instructions at `0x4ae0..0x4b1a` split the given address and data
length at 256-byte boundaries and call `0xb20` with command `0x02`, address,
data pointer and chunk length. Separately, `0x4b1c..0x4b30` calls `0xb20`
with command `0x20`, the supplied address, and no data bytes. On the traced
`0x3c0c` route the address is `0x1f000`, and the later indirect call is given
the 20-byte object at `0x0084295c`.

`0xb20` calls `0xa00` with `0x06`, then with the requested command; the
address-bearing branch calls `0x9c0` to send three address bytes in high-to-low
order. It writes any data bytes via `0x0080000c`, polling bit `0x10` at
`0x0080000d`, then calls `0xa2c`, which sends `0x05` and polls a returned
status bit with a finite counter. The pinned TLSR8258 SDK names offsets
[`0x0c` and `0x0d` MSPI data/control and bit 4 busy](https://github.com/telink-semi/telink_zigbee_sdk/blob/7cf1d6dbac6497505d649bd03342efe81f1a8212/tl_zigbee_sdk/platform/chip_8258/register.h#L122-L130).
The sequence is strongly consistent with a NOR-flash write-enable, sector-erase
and page-program flow: common SPI-NOR commands `0x06`, `0x20`, `0x02`, and
`0x05` have those meanings in the [W25Q example](https://docs.buspirate.com/docs/devices/spi-flash-chips/).
This is an inference from a different documented device, not identification
of the dongle's flash, physical geometry, or successful persistence.

The `0xa2c` poll is bounded, and `0xb20` does not branch on its return before
restoring the saved byte and returning. No readback or error propagation was
found in the inspected chain; no conclusion follows about other verification
paths. The `0x20` and `0x02` sequences are **state-changing candidates** and
must not be used as discovery probes. No command was sent. Read-only Ghidra
checks covered `0x3c0c..0x3c3c`, `0x4ae0..0x4b30`, `0xb20..0xb8e`,
`0x9c0..0xa60`, and the startup copy; literal values came directly from the
identified image. The adjacent readback path and bounded pointer-reference
search are documented below; selector restoration and unresolved SI and
Bluetooth paths remain open.

### Readback and configuration checks

The same conditional startup copy that initializes `0x00842214` also maps
image word `0x4ac5` at offset `0x5728` to `0x00842210`. Under the selected
TC32 decode this tagged value corresponds to `0x4ac4`. That routine calls
`0xa70` with command `0x03`, address, destination and length; `0xa70` sends
the command and three-byte address and reads the requested bytes through the
same MSPI-shaped registers. This is consistent with a common SPI-NOR read,
not proof of flash identity or a successful physical read. The alternate
startup branch skips the copy; runtime pointer contents there remain unknown.

At `0x2cdc..0x2cee`, the firmware loads the pointer at `0x00842210` and
indirectly invokes it with address `0x1f000`, length `0x14`, and destination
`0x0084295c`—the same object and address used by the previously traced
storage-shaped write. It then tests the object's first word against
`0xffffffff`. On that value, `0x2dac` calls `0x1f5c` to populate four bytes,
then invokes the pointer at `0x00842214` with address `0x1f000`, length 4,
and the object; it clears object byte `+0x11` before proceeding. The source
and quality of the four generated bytes are not established. This branch is a
state-changing candidate and was not executed on hardware.

On another branch, object byte `+0x11 == 1` causes the first word to be
stored at `0x0084258c`. Otherwise, `0x2d02..0x2d4c` computes a 16-bit,
CRC-shaped bitwise transform over the first four object bytes, using seed
`0xffff` and low 16-bit polynomial value `0x8408`, then transforms and stores
the resulting value at `0x0084258c`. In the inspected branch it is **derived
and stored**, not compared against a stored checksum. Additional field checks
replace object byte `+0x0c` if it equals `0xff` and replace bytes `+0x0d`
and `+0x10` when they are zero or `0xff`. These observed checks are not proof
of full-object integrity validation, authentication, or the absence of such
checks elsewhere. Restoration from object byte `+0x12` remains unproven;
the additional selector traces below identify a separate re-enable path.

The aligned image contains three literal-pool occurrences of `0x00842214`;
bounded PC-relative scans found use sites at `0x222c`, `0x22e0`, `0x2db4`,
and `0x3c20`. The inspected `0x2db4` and `0x3c20` paths load and call the
pointer rather than directly overwrite it. The longer `0x22xx` paths, derived
pointers, conditional startup alternatives, DMA, and indirect writes have not
been exhaustively checked, so no absence-of-overwrite claim is made.

Evidence: read-only temporary Ghidra disassembly of `0x2cdc..0x2dc8`,
`0x4ac4..0x4adc`, `0xa70..0xb10`, and `0x1f5c..0x1f88`, together with
independent literal and initializer reads from the identified dongle image.
Temporary changes were discarded.

### Selector re-enable, state reset and callback aliases

`0x1f8c` calls the `0x2cdc` configuration-read routine and then `0x2e40`.
The latter calls `0x2dec`, which calls `0x3cf8` at `0x2e30`. In the decoded
`0x3cf8..0x3d54` routine, scheduler selector `0x008423cc == 1` sets report
selector `state + 0x2c` to 3; selector 0 sets it to `0xa5`; other values
skip those two stores. All three routes then zero the twelve report-source
bytes at state offsets `+0x2d..+0x38`. This is a previously unaccounted-for
reset after the inspected configuration-read call, not proof that all mode
changes or all paths clear those bytes. It narrows the retained-state
hypothesis above: any proposed sequence must account for this reset when the
`0x2e40` initialization route runs.

Separately, `0x3f60` checks the scheduler selector and a counter; when the
selector is not 1 and the counter is greater than 100, it calls `0x3f50`.
That helper calls `0x3c40(1)`, which sets `0x008423cc` to 1 and invokes the
previously documented storage-shaped path, then calls `0x3d64`. This is a
conditional **code path to re-enable** the selector; it is not evidence
that the device reached it, successfully persisted a value, or restored the
value from object byte `+0x12`.

The inspected `0x2cdc` body reads the 20-byte object and handles several
other offsets, but does not directly copy its byte `+0x12` to the scheduler
selector. Nor did the bounded literal-reference inspection establish such a
copy elsewhere. The startup copy separately initializes the selector to 1.
The object-byte restoration path therefore remains **unresolved**, rather
than inferred from the write path or the independent `0x3f50` setter.

At callback-reference sites `0x222c` and `0x22e0`, the longer `0x21c4`
routine retains or loads the address `0x00842214` and uses the pointed-to
word for indirect calls; the inspected instructions do not directly store a
new callback value there. These calls are additional possible consumers, not
proof of their hardware effects. The scan was bounded to aligned literal
words and corresponding PC-relative loads; derived aliases, dynamically
written pointers and the alternate startup branch remain unexamined.

Evidence: read-only TC32 disassembly at `0x1f8c..0x1fcc`,
`0x2e40..0x2e46`, `0x2dec..0x2e30`, `0x3cf8..0x3d54`, `0x3f50..0x3fd8`,
`0x3c40..0x3c5e`, and the selected `0x21c4` pointer use sites. Independently
decoded literal pools confirm the selector and object addresses; an even-offset
direct-branch scan found `0x2e30 -> 0x3cf8`, not a complete caller inventory.
Temporary analysis changes were discarded. Next trace indirect aliases and the
actual `+0x12` restoration, then investigate SI format and Bluetooth motion.

### Configuration aliases and separate record integrity

For this image, the 20-byte configuration object starts at `0x0084295c`,
so byte `+0x12` is `0x0084296e`. An independent aligned-word inventory of
image literals in `0x00842940..0x00842980` found aliases including
`0x00842958` (four bytes before the object), `0x0084295c`, `0x00842970`
(object `+0x14`) and `0x00842974`; it found no literal equal to the target
byte itself. A bounded scan of TC32-shaped PC-relative loads identified
their use sites; it is not whole-program data-flow analysis.

The decoded `0x2134` sites at `0x2158` and `0x2188` use the word at
`0x00842958`, not a byte at object offset `+0x12`; `0x23c6` similarly loads
that preceding word for a call to `0x4b1c`. The `0x2c90` sites at `0x2c9e`
and `0x2cb4` write to `0x00842970`, two bytes *after* the target, before
other buffer operations. The already traced `0x2cdc` uses the object base
for readback but does not directly load `+0x12`, and `0x3c0c` writes that
byte from the scheduler selector. These inspected instructions do not establish
a reverse copy into `0x008423cc`; indirect callers and derived pointers remain
possible. The saved-selector restoration question is still unresolved.

An apparently similar offset in `0x21c4` is **a different object**. Its
instructions at `0x21d2` derive `r4 = input argument + 4`; subsequent
iterations advance that pointer by `0x14`. At `0x225e..0x2272`, the code
calls `0x2078` over the first `0x12` bytes of each such record, forms a
little-endian halfword from record bytes `+0x12` and `+0x13`, and branches on
whether it equals the result. The helper at `0x2078` implements a bitwise
CRC-shaped calculation with initial low halfword `0xffff` and polynomial
low halfword `0xa001`. This is a **record integrity comparison**, unlike the
derived value in `0x2cdc`; it is not evidence that the saved configuration
object's byte `+0x12` is a checksum or restores the selector. Source,
authentication, full error behavior, and physical device use remain unknown.

Evidence: read-only disassembly of `0x2134..0x2190`, `0x21c4..0x2272`,
`0x23c6..0x23ca`, `0x2c90..0x2cca`, and `0x2078..0x20b6`; literal values
were decoded independently from the identified image. No hardware was
contacted. Next trace callers passing object-derived pointers and any indirect
copy to `0x008423cc`, then pursue the still-open SI and Bluetooth paths.

### Configuration-adjacent queue and indirect-copy trace

The direct-branch candidates to `0x2c90` at `0x33fe` and `0x3410` were
verified in read-only disassembly of `0x33ec..0x3414`. That caller checks
queue descriptor `0x00842368`: if empty it calls `0x2c90(0, 0)`; otherwise
it obtains an entry, passes entry `+2` as the source and entry byte 0 as the
length. The decoded dongle startup copy maps image offset `0x5880` to this
queue descriptor; the image initializer gives stride 31, count 2, zero
indices, and storage pointer `0x008427d8`. This is a **different queue** from
the `0x008423c0` queue used by the earlier receive-to-`04 FF` path. Runtime
descriptor changes and indirect callers remain possible.

`0x2c90` uses `0x00842970` as a length byte. When the length is zero it
sets that byte to 3 and constructs three bytes at `0x008429a8`. On its
nonzero branch it stores the length, modifies a byte derived from the supplied
source and length, and calls `0x4ee0` with destination `0x008429a8`, the
supplied source and length. The `0x4ee0 -> 0x4eac` instructions implement a
direction-dependent byte copy. No reverse copy from configuration byte
`0x0084296e` to scheduler selector `0x008423cc` occurs in these inspected
instructions. The queue descriptor and source are writable, so this is not a
proof that malformed pointers, arbitrary lengths, aliases or other callers
cannot reach those addresses; their bounds and producer paths were not fully
verified.

The storage-read caller `0x2cdc` passes configuration object `0x0084295c` as
the **destination** to the indirect trampoline at `0x2de8`; the exceptional
fallback passes the same object as a source to the storage-write callback.
The decoded trampoline is an indirect branch through its supplied register,
so its runtime target and side effects cannot be bounded solely by the call
site. Separately, the known `0x3c40` direct callers at `0x3d9c` and `0x3f54`
pass constants 0 and 1 respectively; they are not a demonstrated load of
configuration byte `+0x12`. A bounded even-offset scan of direct TC32-style
long branches found these callers but cannot rule out indirect calls, other
encodings or alternate execution mappings. **Restoration from the saved byte
remains unresolved.**

Evidence: read-only Ghidra instructions at `0x33ec..0x3414`,
`0x2c90..0x2cca`, `0x4ee0..0x4eee`, `0x4eac..0x4edc`, `0x2de8`, and the
previously cited `0x2cdc` and `0x3c40` paths; literal pools and the candidate
queue initializer were independently read from the identified image.
Temporary changes were discarded. Next trace dynamic queue producers and the
alternate callback initialization, and investigate SI and Bluetooth motion
without treating negative bounded scans as exhaustive.

### Second-queue producers and alternate startup branch

Two instruction-verified producer paths target queue `0x00842368` through
`0x1ea0`:

* `0x34d0` checks whether its byte-sized argument is nonzero; if so it passes
  source `0x00842a08` and the byte length at `0x00842781` to `0x1ea0`.
  The code at `0x34f4` constructs a ten-byte buffer at that source beginning
  `5A A5 BA`, with further bytes from arguments and state. A bounded direct
  long-branch scan found callers of `0x34d0` at `0x361a` and `0x407c`, and
  callers of `0x34f4` at `0x368c` and `0x40da`; the full caller contexts,
  source semantics and possible indirect invocations remain unverified.
* `0x3478` increments a halfword while another halfword is nonzero; after
  the incremented value exceeds `0x31`, it resets the counter and conditionally
  decrements the other halfword. If the queue is empty, it calls `0x1ea0`
  with length byte `0x0084278c` and buffer `0x008429f4`. On one branch,
  `0x34f4` sets that length to ten and copies the constructed buffer there.
  `0x02b6` is a decoded direct caller of `0x3478`; the real timing and the
  meaning of the halfwords are not established. An insertion attempt is not
  proof of queue insertion: `0x1ea0` has the previously documented unchecked
  insertion-result limitation.

On the alternate startup route at `0x13e..0x148`, when the tested register
bit is nonzero, the code reads a halfword from `0x0084223c`, writes it to
`0x0080063e`, and branches to `0x15e`. Thus it skips the `0x14a..0x15c`
copy that would initialize callback pointers at `0x00842210` and
`0x00842214`, but still reaches the next startup copy. The skipped range is
not explicitly filled with replacement callback pointers on this branch. The
source halfword's meaning, prior RAM contents, other initialization, and the
callbacks' resulting runtime values remain unknown; calling this a retained
or valid callback state would exceed the evidence.

Evidence: read-only decoded instructions `0x3478..0x34b8`,
`0x34d0..0x34e4`, `0x34f4..0x3590`, `0x1ea0`, and `0x12c..0x16a`;
literal pools were decoded independently from the identified dongle image.
The branch scan covered even offsets using the selected TC32 long-call
encoding and is not exhaustive. No hardware was contacted. Next inspect
the producer caller contexts, possible callback writers and the SI and
Bluetooth paths.

### Producer callers and nearby callback-region stores

Under the selected TC32 decode, `0x35b0` reads input buffer `0x00842af0`
and proceeds only when its first byte is 5. For selected second-byte values
`0xa0..0xdf`, it constructs a 20-byte `5A A5`-prefixed buffer at
`0x00842a08`, copies 15 input bytes and zeroes its last two bytes, then
passes 1 to `0x34d0` at `0x361a`. Other branches share that call with 0,
which does not attempt insertion. For second byte `0x0f`, `0x3680..0x3690`
passes five input bytes to `0x34f4` and then forwards that helper's return
value to `0x34d0`. The helper's return and queue state therefore matter;
the call-site address alone does not prove insertion.

`0x405c` reads a length byte at `0x00842786` and the same input-buffer base.
For a nonzero length greater than `0x0e`, subject to its additional branches,
it constructs a `5A A5`-prefixed buffer at `0x00842a08`, sets a length
derived from the input length, and passes 1 to `0x34d0` at `0x407c`. The
shorter path passes 0. One special branch calls `0x34f4` at `0x40da` and
forwards its result to the same `0x34d0` call. Upstream ownership, complete
input validation, and all exceptional branches were not traced, so these
are conditional code paths, not demonstrated received commands or device
actions.

A bounded aligned-literal inventory in `0x00842200..0x00842218` and
PC-relative-load scan found nearby stores worth distinguishing from the
storage pointers at `0x00842210` and `0x00842214`. Instructions
`0x48a8..0x48ac` write only `0x00842200..0x00842202`, and
`0x48b4..0x48b8` write only `0x00842204..0x00842206`.
`0x0996..0x0998` stores a byte at `0x0084220c`; `0x4f8e..0x4f9e`
read and write byte `0x00842224` via base `0x00842218`. None is a direct
write to either storage pointer. The previously documented conditional
startup copy does cover both pointers. Other computed addresses,
indirect calls, DMA, alternate image mappings, and longer data flow were not
excluded; alternate-branch callback initialization remains unresolved.

Evidence: read-only Ghidra disassembly `0x35b0..0x3690`,
`0x405c..0x40da`, `0x0996..0x0998`, `0x48a8..0x48b8` and
`0x4f8e..0x4f9e`; associated literal-pool values were independently decoded
from the identified image. Candidate scanning did not prove all writes or
callers. Temporary analysis changes were discarded; no hardware was contacted.
Next trace the input buffer's producer and unbounded callback aliases, or
continue the open SI and Bluetooth analyses.

### Input-buffer producer and receive validation limits

The decoded routine at `0x415c` stores the low byte of its second argument
at `0x00842786`, copies that many bytes from its first argument into
`0x00842af0` via `0x4ee0`, then dispatches to `0x405c` for scheduler
selector 0 or `0x35b0` for selector 1. Other selector values skip both
consumers. There is no explicit length comparison in the inspected
`0x415c..0x4186` body before its copy; this is not proof of an overflow or
of an attacker-controlled length.

Two instruction-verified calls at `0x46ae` and `0x46e4` pass a stack
buffer and an eight-bit count to `0x415c`. The containing decoded routine
begins at `0x4640` and reserves `0x40` stack bytes. On the first branch it
reads the count from `0x00800115`, clears that register, repeatedly reads
`0x0080011d` into the stack buffer and then calls `0x415c`. On the second
branch it similarly reads and clears `0x00800116` and copies bytes read
from `0x0080011e`. The inspected loops compare their index against the
register-derived count but do not include an explicit comparison to `0x40`
before the stack stores; the zero-count paths still call `0x415c` with a
zero count. Upstream hardware limits, other entry conditions, and the
destination buffer's actual capacity remain unverified. A short copy can
also leave bytes outside its copied range unchanged; downstream checks must
not be assumed to see a fully refreshed input object.

The [published TLSR8258 register header](https://raw.githubusercontent.com/telink-semi/telink_zigbee_sdk/7cf1d6dbac6497505d649bd03342efe81f1a8212/tl_zigbee_sdk/platform/chip_8258/register.h)
labels offsets `0x115`/`0x116` USB endpoint 5/6 pointer registers,
`0x11d`/`0x11e` their data registers, and `0x139` USB endpoint interrupt
status with bits 5/6 for those endpoints. The checked instructions test those
bits and use the corresponding register pairs, strongly consistent with a
USB-endpoint input path. This cross-model comparison does not establish the
actual chip, endpoint descriptors, host behavior, maximum count, or a
physical transfer. The exact descriptor-to-handler mapping and any
hardware-enforced bound need independent verification.

Evidence: read-only Ghidra disassembly `0x4640..0x46ec`,
`0x415c..0x4186` and the earlier `0x4ee0 -> 0x4eac` byte-copy trace;
literal pool values were independently decoded from the identified dongle
image. An even-offset TC32-style direct-call scan found callers `0x46ae`
and `0x46e4`, not a complete indirect-caller inventory. Temporary changes
were discarded. Next trace endpoint configuration and descriptors for count
bounds and mapping, while pursuing SI architecture and Bluetooth motion.

### Dongle configuration descriptors and input mapping

Two structurally parseable USB configuration descriptor sequences occur in
the identified dongle image. Their declared total lengths and endpoint
descriptors were parsed directly from image bytes, without publishing the
descriptor payloads:

| Image offset | Total | Interfaces and endpoint declarations |
| --- | ---: | --- |
| `0x55d0` | 116 | Four HID-class interfaces: `0x81`, `0x82`, `0x83`, `0x05`, `0x84`; all interrupt, each declaring maximum packet size 64. Endpoint `0x05` belongs to interface 2. |
| `0x57bc` | 49 | One vendor-class interface: `0x81` and `0x05`, both interrupt and each declaring maximum packet size 64. |

The previously traced startup copy maps image offset `0x57bc` to RAM
`0x008422a4`. In decoded code, `0x30d4` returns image address `0x55d0`
when selector `0x008423cc == 1`, otherwise `0x008422a4`. `0x30f0`
returns 116 for selector 1, otherwise reads the little-endian total length
at the RAM descriptor. At `0x4260..0x42d4`, a descriptor-type-2-shaped
dispatch selects those two functions for pointer and length and clamps the
response length against another request field. This supports configuration
selection in static code, not proof that a physical host received either
descriptor or that mutable RAM has not changed.

Both configurations declare **OUT endpoint `0x05`**, matching the endpoint-5
pointer/data/status offsets in the previously documented receive branch
under the published TLSR8258 register-map comparison. Neither parsed
configuration declares endpoint `0x06`, despite the handler's separate
endpoint-6-shaped branch. Other descriptors, alternate mappings,
conditional registration and hardware-specific interpretations remain
possible; the selected configuration's advertised endpoints alone do not
prove the second branch is unreachable.

The advertised 64-byte maximum packet size is the same numerical size as
the handler's 64-byte stack allocation. This is a **descriptor-level bound
declaration**, not evidence that the count register is capped at 64, that
the host or hardware honors it, or that a multi-transaction FIFO cannot
exceed it. The inspected handler does not itself clamp the count. Endpoint
configuration writes, actual FIFO capacity, USB core behavior and destination
capacity remain open; no vulnerability or safety conclusion follows from
the descriptor alone.

Evidence: parse of descriptor headers and complete declared-length walks at
`0x55d0` and `0x57bc`; read-only instruction checks `0x30d4..0x30e2`,
`0x30f0..0x3104`, `0x4260..0x42d4`, `0x4640..0x46ec`; startup mapping
is documented above. Literal addresses were independently decoded. The
bounded byte search and two parsed sequences are not an exhaustive descriptor
or endpoint-configuration inventory. Next trace endpoint initialization,
USB core documentation and possible alternate descriptors; no hardware was
contacted.

### Endpoint initialization and USB-core comparison

The decoded initialization route `0x2dec` calls `0x4794`; that helper calls
`0x4e5c` with `0xa0` to clear those bits in `0x00800104`, then writes `0xff`
to `0x0080010e`. The separately
documented TLSR8258 header identifies offset `0x104`, used by `0x4e5c`, as
the control-endpoint interrupt-mode register. The precise effects of the
initialization sequence on endpoint 5/6 allocation or the maximum register
have not been established. In the receive path, the instructions after the
two respective copies write value 1 to `0x00800125` and `0x00800126`; the
reference header identifies these offsets as endpoint 5 and 6 control
registers. These are observed register-address-shaped writes, not proof
of a completed transfer or endpoint activation on physical hardware.

Telink's [Platform SDK handbook, full-speed USB hardware section](https://doc.telink-semi.cn/doc/en/software/res/sdk/driver/platform_sdk_handbook_en/)
describes an 8+256-byte USB-RAM family including B80 and B85, with endpoints
5 and 6 as OUT endpoints. It says endpoint cache allocation shares 256
bytes and its maximum is controlled by a max register with default 64 bytes.
This is **external cross-model documentation**: the dongle filename and
matching offsets do not establish its chip, configured allocation, reset
state, register-width semantics, or whether the register-derived count in
this firmware is always limited to 64. It supports endpoint-6 hardware
plausibility but does not establish that a host can reach it with either
selected descriptor or via another configuration.

An aligned-literal and PC-relative-load inventory across candidate register
addresses `0x00800100..0x0080013f` identified the above control writes but
no direct literal reference to the max register `0x0080013e`. This bounded
scan does not exclude addresses formed arithmetically, writes through
callbacks, other code mappings, hardware or boot-time defaults. Neither it
nor the handbook establishes the runtime maximum or the destination buffer
capacity; no safety or vulnerability conclusion is justified.

Evidence: read-only disassembly `0x2dec..0x2dfe`, `0x4794..0x47a2`,
`0x4e5c` and `0x46a8..0x46ec`; literal pools were resolved against the
identified dongle image. The reference handbook and register header are
external, not a verified hardware identity. Temporary analysis changes were
discarded; no hardware was contacted. Next trace computed register writes,
other descriptor registration and callback paths, while continuing the SI
and Bluetooth investigations.

### Computed endpoint-control reset loop

On the selected TC32 decode, `0x46ee` reads `0x00800648` and tests bit
`0x00020000`. When set, `0x46fa..0x4704` stores 1 to `0x0084282b` and
writes the tested bit back to the status-shaped word. The following loop at
`0x4706..0x4726` runs with index `0..7`: it forms
`0x00800000 | (0x120 + (index & 7))`, writes zero to that byte, and also
writes zero to `0x00842b68 + index`. Under the cited TLSR8258 register map,
the tested bit corresponds to USB reset and the computed addresses are the
endpoint 8, then endpoint 1–7, control-register offsets. This is a
**USB-reset-shaped control clear**, not a verified physical reset or a
demonstration that any endpoint has been enabled.

The exact computed register range is `0x00800120..0x00800127`; it does not
include the reference maximum-size register at `0x0080013e` or the endpoint
buffer-address offsets `0x128..0x12f`. Thus this verified computed-write path
does not resolve the receive-count limit or endpoint allocation. The separate
RAM array's meaning and all other possible computed or indirect writes remain
unknown. A bounded raw scan for the identical two-instruction sequence that
constructs `0x00800000` found only `0x470c`; other instruction sequences,
register allocations, callers and external initialization are not excluded.

Evidence: read-only disassembly `0x46ee..0x4726`; literal pools resolve
`0x46ee` to `0x00800648`, `0x46fc` to `0x0084282b`, and `0x4708` to
`0x00842b68` in the identified dongle image. The interrupt and register
labels are cross-model comparisons with the previously linked Telink header,
not verified chip identity. Ghidra discarded temporary analysis changes.
Next trace other computed register bases and descriptor/endpoint setup;
physical behavior and runtime count bounds remain unverified.

### Additional computed-address writer candidates

A broader raw scan considered every even image offset for a `mov` of
`0x80` into any low register immediately followed by a shift-shaped
instruction, using masks `0xf8ff` and `0xfc00`. Ten candidates matched;
read-only decoding showed that three use shifts other than 16. The remaining
shift-by-16 candidates include the reset loop above and these distinct paths:

* `0x1ffa` forms `0x00800000` as a **value** ORed into a word already read
  from the register at `0x00800620`; it does not use that value as the
  store address.
* `0x49c8` is a generic byte read-modify-write helper whose destination is
  its first argument ORed with `0x00800000`; in principle its destination
  depends on callers. A bounded direct-call scan found calls at `0x48e8`
  and `0x48f6`. Their instruction-verified argument literals are `0x1378`
  and `0x137c`, producing `0x00801378` and `0x0080137c`, not USB
  maximum-size register `0x0080013e`. Indirect callers or other call
  encodings were not excluded.
* The startup route at `0x2e02..0x2e16` calls `0x4be8` with `(4,0)` and
  `(2,0)`, then `0x4cb0` with `(6,1)`. Following the inspected arithmetic
  and literal bases, these particular calls write bytes at `0x0080054a`,
  `0x00800549`, `0x00800506`, and `0x00800501`, not the USB endpoint
  allocation or maximum offsets. Other arguments, callers, and alternate
  helper branches were not exhaustively assessed.

This widens the earlier identical-byte search; it **does not** prove that
all possible computed writes have been found. Instructions separated by
other operations, different constant constructions, arithmetic aliases,
indirect calls and external code remain possible. Consequently endpoint
allocation and the runtime receive-count bound are still unresolved.

Evidence: read-only disassembly `0x1fdc..0x2000`, `0x48c0..0x48f6`,
`0x49c8..0x49fa`, `0x2e02..0x2e16`, `0x4be8..0x4ca2` and
`0x4cb0..0x4cf4`; independently decoded literal pools include `0x1378`,
`0x137c`, `0x506`, and `0x501`. The image-wide masks and direct-call scan
are candidate generators, not whole-program reachability analysis. Ghidra
discarded temporary changes. Next trace indirect callers and other address
constructions, or continue the open Bluetooth and SI research.

## SI image

The extracted SI image is 296,400 bytes, SHA-256
`e6dc2f68540d3a31b39b6250492702b2566a6a97da7d04fce6895ba040722c14`.
A bounded static byte scan found no `KNLT` marker and no four-byte-aligned
pair consisting of a candidate Cortex-M stack pointer in `0x20000000`–
`0x20080000` and a low-bit-set reset value in `0x08000000`–`0x081fffff`.
These narrow negatives do not identify the architecture, encoding, load
address, encryption, or compression. The earlier REA firmware-region scan
found no recognized regions; it likewise does not prove their absence.
Next inspect its updater/container format and cross-reference the vendor
application's SI update flow before selecting a decoder.

### Saved Windows application's SI updater route

In the saved Space Station 3.4.4.3 decompilation, the F3 device configuration
sets `SIFirmwareVersion` to `F3SI`. `MainWindowViewModel` routes an F3-series
`si` upgrade to `ShowFmFPUpgrade`; that method queries firmware information
using the SI version key, the literal API argument `ch573`, and the reported
extension-CPU version, then assigns `cpuType = 4`. The same `ch573` argument
is also used in that method for other component types; it is not proof that
the SI image is CH573 machine code or even a raw executable.

In the inspected `FmUpgradeFPViewModel.StartUpdateGamePad` path, a URL whose
last decoded path extension is exactly `.exe` is passed with that type to
`FmUtils.UpdateFirmware`. The latter downloads the file, starts a process
using the saved path, and sends an upgrade command. Its separate ZIP path is
gated on `CpuType == 3`, while this SI route sets 4; consequently this saved
code does not establish a ZIP-to-SI-image decoder or direct transfer of the
retained SI ZIP. The retained 3.0.2 archive URL is a ZIP, whereas the
saved application's API response at the time of an SI upgrade is not
available. Do not assume the archived URL was ever returned to this route,
or that this version of the UI successfully handled it.

This is **static application evidence**: no updater, executable, device
command or download was run during this investigation. The downloaded
external updater's identity, executable code, actual command transport,
image format, integrity checks and target CPU remain unresolved. A next
static step is to identify a locally retained SI updater executable or a
separately saved API response, then inspect the actual artifact with REA;
if unavailable, continue comparing the SI image against independently
documented container formats without equating a plausible decoder with proof.

Sources: saved `LibCommon.Config.GameHandleListDic` entry 28,
`MainWindowViewModel` methods selecting `si` and `ShowFmFPUpgrade`,
`FmUpgradeFPViewModel.StartUpdateGamePad`, `FmUtils.UpdateFirmware`, and
`UrlUtils.GetUriExt`/`IsExeUrl`/`IsZipUrl`. The original Station assembly is
not currently available at its historical extraction path; the saved
decompilation's prior recorded assembly hash is
`a72e3cc035f0660e60d45c5d8e5ddbb1ef44077ee50c47321f43fe32d14d636d`,
as documented in [Space Station static analysis](space-station-static-analysis.md).
The decompilation is not independent runtime evidence and was not revalidated
against the unavailable original assembly in this step.

### Archive and image structure checks

Python's standard `zipfile` reader opened the retained `f3si_v3.0.2.zip`
(SHA-256 `7b76dea5f8a092d063c744ec768f17f0ad748ae4c36aab7f35f128190365d291`)
and read its single DEFLATE-compressed member without a CRC error. The expanded
296,400 bytes hash to the SI image SHA-256 above. This verifies consistency
between the local ZIP and extracted file, not publisher authenticity or the
meaning of the member. The outer ZIP does not identify the member's internal
format or target architecture.

At offset zero the member does not have the gzip identification bytes
specified in [RFC 1952 section 2.3.1](https://www.rfc-editor.org/rfc/rfc1952#section-2.3.1),
nor the checked ZIP local-header, ELF, DOS/PE or ESP image prefixes. These
are prefix checks only: nested, shifted, encrypted, compressed or otherwise
wrapped formats remain possible. The image length is divisible by 16, which
alone has no diagnostic value. Entropy measured independently in successive
32-KiB blocks was approximately 7.975–7.992 bits per byte for each full
block; the shorter final block measured 7.863. High byte entropy is not
proof of encryption, compression, randomness, or authentication.

The data are not featureless under a bounded alignment test: among 37,050
aligned eight-byte blocks there are 87 repeated block values (90 duplicate
occurrences beyond the first), and 78 equal pairs at an exact 1,024-byte
separation. Every such pair's first offset is divisible by 32; 75 start in
`0x30000..0x3ffff`, three in `0x10000..0x1ffff`. At 16-byte alignment,
18,525 blocks contain one duplicated value; 32-byte alignment found no
duplicate among 9,262 complete blocks. These observations suggest structure
worth comparing across image versions; they do **not** identify a cipher,
key, compression scheme, page format, or executable. The search covered
only the stated alignments and spacing, not all substrings or transformations.

A filename search of unignored `local/research` entries found no `.exe`;
two ignored directory entries were excluded, so this is not an exhaustive
inventory and does not establish that an SI updater is unavailable elsewhere.
The original Windows assembly and external updater remain unverified. Next
compare another retained SI version or the updater's known format if an
actual artifact or historical response is identified; investigate the repeated
offset structure without publishing or executing proprietary payloads.

### Comparison of repeated-block neighborhoods

For each of the 78 SI pairs above, the first eight bytes match but **none**
of the following 24 byte positions match between the two 32-byte-aligned
windows. Taking a bytewise XOR of those following 24 positions yields 18
distinct difference patterns; one pattern occurs 60 times. Across all 9,230
complete 32-byte-aligned window pairs at distance 1,024, the same dominant
nonzero 32-byte XOR-difference pattern occurs 60 times. Those 60 pairs start
between `0x3e200` and `0x3f3e0` inclusive. No full 32-byte pair at that
distance is identical.

Applying the same comparison to the independently identified main image
(`6cba30540a0c3ba16acad2b62bb4dd64dfccc28add04914c3720c6ae6a53e20a`)
produced 4,720 pairs: 460 identical 32-byte pairs and a most frequent
*nonzero* XOR-difference pattern occurring twice. The dongle image
(`af74da8b80e60aa21f49d46805f89dff9f2d6834a5d2c063fde0097b240a8a60`)
produced 682 pairs, with no identical pair and no repeated 32-byte XOR
pattern. The files have different sizes and known content; this comparison
does not establish a common baseline or statistical independence.

For the SI image alone, the most common complete 32-byte XOR-difference
pattern appeared 92, 85, 81, 77, 69, 60, 56 and 48 times for respective
distances 32, 64, 128, 256, 512, 1,024, 2,048 and 4,096 bytes; none of the
modal patterns was all zero. This is more specific evidence of repeated
relationships than the earlier entropy measurement, but it does not prove
how those relationships arose. In particular it does not establish an XOR
encoding, encryption mode, plaintext, page framing, key or processor, and
there has been no decryption or execution.

Method: read each identified local image unchanged with Python; compare
complete 32-byte-aligned windows at the stated byte offsets using bytewise
equality and XOR, and count exact patterns with `collections.Counter`. For
the first paragraph, the earlier 8-byte-aligned filter selected the 78 pairs;
all happened to begin on 32-byte boundaries. Only the chosen alignments,
distances and artifacts were tested. Next compare another independently
identified SI version or a saved updater's decoding implementation, and
retain these measurements as hypotheses rather than decode instructions.
