# Staged frame to peripheral handoff: static trace

> **Follow-up:** The payload writer described as unresolved below was later
> identified at `0x10960`, reached from the Nordic UART attribute callback at
> `0x8b0c`. See [BLE callback trace](ble-attribute-callback-static.md). The
> RF-shaped receive queue traced here remains a separate path.

Artifact: main image `6.7.5.1`, SHA-256
`6cba30540a0c3ba16acad2b62bb4dd64dfccc28add04914c3720c6ae6a53e20a`.
The analysis below is static. It does not establish which firmware is installed
on the controller or that a physical transmission occurred.

## Producer and arbitration

The non-`5A A5` branch of `FUN_0001896c` calls `FUN_0000fb5c`; that routine
stores the supplied length at `0x008471f9` and copies the supplied data into
`0x00847204`. `FUN_0001899c` builds a sensor-containing buffer and is a
caller of `FUN_0001896c`; its decompiled call loses arguments, so the exact
calling convention should also be checked in instructions before treating
the whole report as verified. `FUN_0001a868` calls `FUN_0001899c` only on one
of its mode branches.

When byte `0x008462d8` is 2, `FUN_0000fc18` assembles a different object at
`0x00845864`. If byte `0x008462da` is zero, it builds a short alternate
frame instead of copying the staged data. Otherwise it first calls
`FUN_0000fb7c`: when that finds an entry, it copies an entry into the output
object; when it does not, `FUN_0000fc18` calls `FUN_0000fbb4` and copies the
staged data from `0x00847204` into the object. It then writes metadata and a
state-selected status byte. These branches are arbitration among possible
sources, not proof that all sensor samples are transmitted.

`FUN_0000fb7c` checks and reads an entry but its saved decompilation does not
show a queue-pop operation. Do not infer dequeue, delivery, acknowledgment, or
losslessness from that function alone.

## Pointer publication and consumer

At the end of the enabled branch, `FUN_0000fc18` writes 1 to `0x008462f0`
and stores the address `0x00845864` in the pointer slot `0x00846318`.
`FUN_0000fcf4` also publishes a different object through the same two
locations in another state. The purpose and consumer of the byte at
`0x008462f0` are not yet established; calling it a verified transmit-ready
flag would be premature.

`FUN_00000f08` loads the pointer from `0x00846318` and calls
`FUN_00001238(pointer, 0xdc)`. That function calls `FUN_0001ce10(2, ...)`
and `FUN_000010e8(pointer, value_from_0x00800740, 0xdc)`. The latter writes
`0xdb` to the halfword address `0x01000f0a`, writes the value loaded from
`0x00800740` at `0x01000f18`, sets bit 2 of a byte at `0x01000f16`, writes
the **low 16 bits** of the pointer at `0x01000c0c`, and writes `0x87` to
`0x01000f00`. These are the exact literal-derived addresses and widths; their
address-space aliases, hardware names and any DMA interpretation require an
authoritative register map.

The saved decompile shows two callers of `FUN_00000f08`: `FUN_00000fc0`
calls it after state-dependent bookkeeping, while `FUN_00000f40` calls it
only under additional flag conditions. `FUN_00000360` calls
`FUN_00000fc0` when state base `0x00846ac8` has `+0x0c == 4` and `+0x0d`
is not 1, subject to its other guard. `FUN_00001120` calls `FUN_00000f40`
when bit 2 is set in a word read from `0x00800f20`. Separately,
`FUN_0001acb0` calls `FUN_0000fc18` in its branch where state `+0x0d` is not
1. The mode values and interrupt conditions do not prove when the producer
and consumer run relative to each other or that the pointer was freshly
published before each consumer call.

This chain reaches a peripheral-register handoff, distinct in code and
addresses from the USB-style `0x0080011b` FIFO in
[USB motion routing](usb-motion-routing-static.md). It does **not** by itself
prove Bluetooth, a particular 2.4 GHz protocol, or a GATT notification. The
external register comparison below supports an RF interpretation. The historical
attribution of `0x00800f20` as a USB receive register in
[queue topology](bluetooth-queue-topology.md) is not established.

## RF register comparison and interrupt path

For an independent comparison, Telink's public TLSR8258 Zigbee SDK at commit
[`7cf1d6dbac6497505d649bd03342efe81f1a8212`](https://github.com/telink-semi/telink_zigbee_sdk/tree/7cf1d6dbac6497505d649bd03342efe81f1a8212)
defines these offsets in its [DMA](https://github.com/telink-semi/telink_zigbee_sdk/blob/7cf1d6dbac6497505d649bd03342efe81f1a8212/tl_zigbee_sdk/platform/chip_8258/register.h#L1113-L1123)
and [RF](https://github.com/telink-semi/telink_zigbee_sdk/blob/7cf1d6dbac6497505d649bd03342efe81f1a8212/tl_zigbee_sdk/platform/chip_8258/register.h#L1219-L1265)
register definitions. Its [base-address definitions](https://github.com/telink-semi/telink_zigbee_sdk/blob/7cf1d6dbac6497505d649bd03342efe81f1a8212/tl_zigbee_sdk/platform/chip_8258/bsp.h#L32-L36)
use `0x00800000`.

| Offset | TLSR8258 SDK designation | Firmware observation |
| --- | --- | --- |
| `0xf20` | RF interrupt status; masks 1, 2, 4 correspond to RX, TX, RX timeout | `FUN_00001120` reads a halfword at `0x00800f20` and tests and writes back those masks. |
| `0xc0c` | 16-bit RF transmit DMA address | `FUN_000010e8` writes the low 16 bits of the published pointer to literal `0x01000c0c`. |
| `0xf0a`, `0xf16` | RF receive timeout and link-layer control | The same function writes `0xdb` to literal `0x01000f0a` and sets bit 2 at `0x01000f16`. |
| `0x740` | system tick | `FUN_00001238` reads a word from `0x00800740` and supplies it to the register setup. |

These matching offsets and the firmware's channel-shaped setup make an RF
transport handoff a **strong inference**, rather than a confirmed USB path.
The reference is for TLSR8258, not an independently identified chip in this
controller; the firmware uses `0x0100xxxx` literals for some locations while
the reference SDK uses `0x0080xxxx`. The relationship between these prefixes
has not been established. No conclusion follows about BLE versus proprietary
2.4 GHz, the over-the-air frame, GATT reachability, or device behavior.

In the image's `FUN_00001120`, the `0x02` branch calls `FUN_00000ca4`,
which is a return-only instruction in the saved disassembly, then writes 2
back to the status location. The `0x01` branch calls `FUN_00001020`, which
calls `FUN_00001014` and the receive-shaped parser `FUN_00000d54`, before
writing 1 back. The `0x04` branch calls `FUN_00000f40` and writes 4 back;
subject to flags, that function may call `FUN_00000f08`, reusing the published
pointer. The SDK names these masks TX, RX and RX timeout respectively, but
those names remain a cross-model interpretation until the actual silicon and
addressing are verified. The return-only TX branch is not proof that a packet
was transmitted or received. If multiple bits are set, the firmware examines
the saved status value in sequence.

Read-only Ghidra `RawRange.java` confirmed the bit tests and calls at
`0x112a..0x116e`, the return-only instruction at `0x0ca4`, and the conditional
call to `0x0f08` at `0x0f92`. `RefTo.java` found `0x3e2 → 0x1120`,
`0x1144 → 0x0f40`, and `0x1154 → 0x1020`. Literal addresses were decoded from
the image independently of the helper's unreliable annotations. This is not
an exhaustive interrupt-vector or indirect-call analysis.

## Receive, acknowledgment-shaped state, and queue

The `0x01` branch of `FUN_00001120` reaches `FUN_00001020 → FUN_00001014 →
FUN_00000d54`, which examines a buffer at `0x00847278`. The parser checks bit
0 of the byte at offset `buffer[0] + 3` and compares `buffer[0]` with the low
six bits of byte 4 plus 11. For type byte 5 equal to 1, it additionally
checks a connection-state byte and a four-byte value before updating other
state. For type 2 it calls `FUN_00000ca8`. These checks are as decompiled;
the source and length guarantees of the receive buffer remain unverified.
Read-only instructions at `0x0d54..0x0d82` confirm the indexed bit test,
length comparison, type comparisons and call to `0x0ca8`.

`FUN_00000ca8` obtains an acknowledgment-shaped value from the received
frame (or reuses the previous value for a short frame). If it differs from a
stored value, it sets `0x008462d6` to 1. `FUN_00019a50` clears that byte,
checks outgoing queue `0x00845db4`, decrements byte 1 of the current entry
when nonzero, and advances the queue only when the decremented value becomes
zero. `FUN_0001c468` initializes that entry byte to 2. This is a static
connection between received data and queue progression; its precise protocol
semantics and whether a remote peer accepted an application message remain
unknown. Read-only Ghidra references identify a store to `0x008462d6` at
`0x0cca` and its read/clear at `0x19a60`/`0x19aa2`; instructions at
`0x19ab0..0x19aca` show the conditional decrement and queue advance.

For a longer type-2 frame, `FUN_00000ca8` checks for `5A A5` at buffer
offsets 6 and 7. It uses `FUN_00019b40` to compare a candidate with the
previous candidate and can suppress an unchanged one, with a separate
exception path. If accepted by that logic, it passes bytes beginning at offset
8 to `FUN_0001c468` with queue address `0x00845e14`. `FUN_00019a50` reads
that queue and invokes `FUN_00018828`, which chooses a command dispatcher
based on state. Thus `5A A5` is not proven to be output-only, and this queue's
static receive-side producer should not be labeled USB solely because of the
old `0x00800f20` attribution. Queue insertion is not proof that a physical
Bluetooth or dongle packet can reach it.

## Status byte coverage

The two direct literal slots for `0x008462f0` are at `0x0fce4` and `0x0fd74`;
Ghidra reference enumeration found writes at `0x0fc86` and `0x0fd4a`, and no
direct reads. To look for structure-relative aliases, an aligned four-byte
literal scan covered values `0x008462c0..0x008462f0`. One nearby base is
`0x008462dc`, making the status byte offset `0x14`; the inspected use in
`FUN_0000f934` clears five four-byte values at that base, ending at
`0x008462ef`, not the status byte. Other nearby literals identify individual
state bytes. This bounded check does not exclude derived pointers, unaligned
literals, indirect code, or hardware access, so the status byte's consumer
and meaning remain unresolved.

## Comparison with saved Bluetooth and dongle evidence

The historical Bluetooth DInput descriptor is a separate HID-over-GATT
observation: its recorded hash and services are in
[Bluetooth DInput](bluetooth-dinput-mode.md). My recorded four-byte
rumble result is in [Bluetooth rumble](bluetooth-rumble-solved.md), but the
saved `FUN_00010838` body handles input-driven rumble mapping; it does **not**
write the payload consumed by `FUN_00010980`. That producer remains unknown,
and the old attribution has been corrected. This prevents equating the
interrupt-side envelope parser with the observed Nordic UART write path.

The retained dongle image `0.4.1.7` is 22,852 bytes, SHA-256
`af74da8b80e60aa21f49d46805f89dff9f2d6834a5d2c063fde0097b240a8a60`.
An exact byte scan found one `5A A5` occurrence at file offset `0x3506`, and
no `5A A5 BA` occurrence. This is only a byte-pattern comparison: it does not
establish code, frame parsing, protocol compatibility, or whether the occurrence
is meaningful. The dongle image's architecture, load mapping and code paths
remain uncharacterized.

## Payload-writer search and receive DMA candidate

For main image `6.7.5.1` (SHA-256 recorded above), `FUN_00010980` reads a
16-bit length at `0x008472b8`, clears it, and processes bytes beginning at
`0x008472d4`. This consumer does not establish which transport supplies them.
The two addresses occur as little-endian words only at image offsets
`0x1097c`/`0x10a28` and `0x10978`/`0x10a2c` respectively, including unaligned
byte positions. Ghidra finds the consumer's references to `0x10a28` and
`0x10a2c`, but no references to the other two literal slots. Absence of
references is not proof of absence of an indirect writer.

A candidate-base scan for `0x00847200..0x008472d4` identified
`0x008472bc`, four bytes after the length and 24 before the payload. Its
inspected references are not a payload writer: `FUN_00010560` changes its
first two bytes, `FUN_00010eb0` clears its first byte, and
`FUN_00010ce4` compares and updates 14 bytes at offsets 2 through 15 as a
report-change cache. The latter calls `FUN_000102e4` when that cache changes;
it does not write either target field in that inspected path.

Another candidate is the interrupt parser's buffer at `0x00847278`, exactly
`0x40` bytes before the length. The original Ghidra analysis left the routine
at `0x10000` and its callee `0x1cea0` undefined as code. In a **read-only**
headless session, temporarily clearing the analysis classification for those
two ranges and disassembling them showed the following, without saving project
changes:

* At `0x10182..0x10188`, the routine loads literal `0x00847278` from image
  offset `0x1025c`, places `0x40` and `0` in the next argument registers,
  and calls `0x1cea0`. Ghidra's existing call reference from `0x0d59e`
  reaches `0x10000` on the mode-4 branch of `FUN_0000d518`; other callers
  and indirect entries are not exhaustively resolved.
* At `0x1ceae..0x1cec4`, the callee writes the low 16 bits of its first
  argument to `0x00800c08`, writes the second argument shifted right four
  bits to `0x00800c0a` (4 for this call), and writes mode-related bytes to
  `0x00800c0b` and `0x00847820`. The literal words at `0x1cec8..0x1ced4`
  were independently decoded from the image rather than relying on the
  disassembly helper's incorrect PC-relative annotations.
* The independently published [TLSR8258 register header](https://raw.githubusercontent.com/telink-semi/telink_zigbee_sdk/7cf1d6dbac6497505d649bd03342efe81f1a8212/tl_zigbee_sdk/platform/chip_8258/register.h)
  calls offsets `0xc08`, `0xc0a`, and `0xc0b` RF receive DMA address, size,
  and mode. This supports an RF-receive-DMA interpretation of the buffer
  setup; the actual chip and address alias remain unverified.

The 64-byte argument and adjacent layout make a receive DMA buffer a concrete
candidate worth tracing, but do **not** prove the DMA writes the length at the
next address or the later GATT payload. Register length units and boundaries,
mode-specific initialization, later copy/callback paths, and any other DMA or
derived-pointer writers require separate analysis. No instruction-level writer
of the payload length or payload was identified in this bounded investigation.

## Receive queue downstream of the DMA-shaped buffer

Following the already identified type-2 parser downstream gives a more precise
copy and dispatch path, but not a path to `FUN_00010980`:

1. For a qualifying non-suppressed frame, `FUN_00000ca8` passes the bytes at
   receive-buffer offset 8 to `FUN_0001c468` for queue `0x00845e14`.
   `FUN_0001c468` constructs a queue item containing a byte count, a metadata
   byte initialized to 2, and the supplied bytes; it calls `FUN_0001c3ac` to
   insert the item. Its duplicate check and size limit may reject an item.
2. `FUN_00019a50`, called from `FUN_0000fe34` at `0x0fe46`, retrieves an
   item. For a nonzero count it copies from item offset 2 to `0x008477cc`,
   stores the count at `0x0084652a`, calls `FUN_00018828`, and advances the
   queue. The decompiler omits the copy length, but instructions at
   `0x19ad8` and `0x19b0e..0x19b1c` retain the item count in the third
   argument register for the copy call. The destination and count-address
   literals at `0x19b38` and `0x19b3c` were decoded from the image.
3. The dispatcher's literals `0x1892c` and `0x18928` resolve to the same
   `0x008477cc` and `0x0084652a` respectively. In its ordinary dispatch
   path, state byte `0x00846ac8 + 0xe` selects `FUN_000183c8` when equal to
   2, otherwise `FUN_00018138`; short `BA`-prefixed messages have additional
   handling. Instructions at `0x1883a..0x1885e` confirm the state comparison
   and both calls. Both are command dispatchers and have other callers, so
   shared command handlers do not by themselves identify the transport.

The inspected queue copy has a different destination and count address from
`0x008472d4` and `0x008472b8`. No direct handoff from this queue path to the
`FUN_00010980` fields was found in these reviewed instructions. This does not
exclude a deeper dispatcher callee, another interrupt branch, an indirect
callback, DMA aliasing, or an independently supplied Bluetooth stack path.
The next focused step is to examine the actual BLE attribute callback table
and its registered write callbacks, and distinguish them from this RF-shaped
queue and the shared dispatchers.

## Verification and limits

On the saved Ghidra project, read-only `RawRange.java` showed the two
publication stores at `0xfc82..0xfc8a`, the pointer load and call at
`0x0f1a..0x0f20`, and the halfword and control-register stores beginning at
`0x10e8`. `RefTo.java` found calls to `FUN_0000fc18` at `0x0fd10` and
`0x1ad46`, to `FUN_00000f08` at `0x0f92` and `0x0fd8`, and the direct
`0x0f20 → 0x1238 → 0x10e8` chain. Pointer and register literal values were
independently decoded from the image with Python `struct.unpack_from`;
the known-incorrect literal annotations from `RawRange.java` were ignored.
The community TC32 processor, missing indirect targets and overlapping
decompiler functions remain limitations.

Next: resolve the actual chip register map and low-address translation, trace
the receive buffer producer and outstanding structure-relative or indirect
references to `0x008462f0`, identify the actual GATT payload writer, and
analyze the dongle image as code. Do not infer GATT motion reachability from
this static handoff.
