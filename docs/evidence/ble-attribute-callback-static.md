# BLE attribute registration and payload callback

**Artifact:** main image `Telink87_For_F3_Ver6.7.5.1.bin`, SHA-256
`6cba30540a0c3ba16acad2b62bb4dd64dfccc28add04914c3720c6ae6a53e20a`.
These are static findings from the downloaded image, not proof of the firmware
installed on the controller or a new physical test.

## Registration and attribute entry

The initialized-data source begins at image offset `0x245a4` and is copied
to RAM starting at `0x00845200`, as represented in the image's startup data
at `0x194` and `0x198`. The attribute-table-shaped data begins at image offset
`0x245d8`, corresponding to RAM `0x00845234`. `FUN_00008874` calls
`FUN_00008b2c`, which supplies `0x00845234` to `FUN_0001f400`; the latter
stores that pointer at `0x00844470`. This establishes registration of the
table-shaped data in this static initialization path. Other initialization
branches and changes to the table remain possible. Read-only instructions at
`0x152..0x164` show the initialization copy loop; `0x8b48..0x8b4a` loads
the table address and calls `0x1f400`, and `0x1f400..0x1f406` stores the
nonzero argument. Literal values were decoded from the image independently of
the Ghidra helper's known-incorrect literal annotations.

The 24-byte entry at image `0x24938` is index 36 (`0x24`) relative to that
table. Its UUID pointer targets `0x2394c`, containing the Nordic UART RX
UUID `6e400002-b5a3-f393-e0a9-e50e24dcca9e`; its callback-shaped field at
`0x24948` contains `0x00008b0d`. The low-bit-tagged pointer corresponds to
code at `0x8b0c` under the analysis's TC32 calling convention. The observed
attribute handle `0x0024` and UUID agree with the saved Bluetooth enumeration
in [mobile application evidence](mobile-app-ble-protocol.md); that agreement
does not prove this specific image was installed during the recording.

The layout also matches the independently published [Telink B80 SDK
description](https://doc.telink-semi.cn/doc/en/software/res/sdk/ble/b80_ble_single_connection_en/b80_ble_single_connection_en/)
of `attribute_t`, whose fields include UUID and value pointers followed by
write and read callbacks. This is corroboration, not proof of the controller's
precise chip or SDK version.

## Callback and payload writer

The saved Ghidra project had classified both callback code at `0x8b0c` and
the writer at `0x10960` as data, so the prior corrected-decompile export
omitted them. A read-only headless Ghidra session temporarily cleared the
classification in each range and disassembled the instructions; the project
changes were discarded afterward.

* `0x8b0c..0x8b28` takes an incoming pointer, passes pointer + 9 and
  `(byte at pointer + 2 minus 3) modulo 256` to `0x10960`, then calls
  `FUN_00010980` and returns zero. The incoming packet structure and upstream
  stack checks are not independently established by this callback alone.
* `0x10960..0x10976` copies the supplied number of bytes to `0x008472d4`
  through `FUN_0001c354`, then stores that length as a halfword at
  `0x008472b8`. The literal words at `0x10978` and `0x1097c` were decoded
  directly from the image. `FUN_00010980` reads and clears this length and
  dispatches the copied bytes. Thus `0x10960` is the previously unidentified
  producer, and the table entry links it to the Nordic UART write callback.
* No length bound is visible in the callback or this short writer. That is not
  evidence of an exploitable condition: packet-length validation, buffer
  capacity, calling conventions, and synchronization must be traced upstream.

This establishes an instruction-level connection for the saved image; it does
not identify a GATT motion notification, prove BLE behavior for every mode, or
establish the physical device's installed revision. The existing owner-reported
rumble response remains separate hardware evidence.

## Attribute consumer and write-branch validation

Read-only instruction inspection extends the registered table trace. The code
at `0x1fad4..0x1fafc` checks a nonzero byte from packet offset 7 against the
table's first halfword, selects `table + byte * 0x18`, and tests bit 1 of the
selected entry's byte at offset 2. If the test passes, it proceeds through the
branch at `0x1fdd4`; the complete entry into this block and all preceding packet
checks have not yet been reconstructed. The examined Nordic UART RX entry has
that bit set and a nonzero field at entry offset `0x10`.

At `0x1fde6..0x1fe04` the code assembles the four-byte field at selected
entry offset `0x10`. If nonzero, it passes the original packet pointer in
`r7` as `r0` and calls the indirect trampoline at `0x203a4`, whose instruction
branches through `r3`. The table's field is `0x8b0d`, linking this path to
the previously traced `0x8b0c` callback. A zero field instead branches to
`0x201a8`; in that fallback, instructions at `0x201a8..0x201da` compare the
16-bit packet field at offsets 2–3 with 2 and compare `(field - 3) & 0xff`
with the 32-bit selected-entry field at offset 4 before the normal copy. A
related branch at `0x1fe90..0x1febe` performs the same style of check. These
fallback comparisons are **not** evidence that the callback branch passes
through them.

The opcode byte at packet offset 6 is compared with `0x12` at `0x1fde0`;
that branch builds a response-shaped object containing `0x13` at `0x20272`
before returning to the callback-or-fallback selection. Error handling at
`0x1fe10..0x1fe1a` distinguishes byte `0x52`. These values are consistent
with ATT write request/response and write command, respectively, but complete
upstream opcode routing and actual transmission have not been verified here.
The callback's zero return and error-result branches are static observations.

No explicit size comparison appears between the callback selection at
`0x1fde6` and the indirect call at `0x1fe04`. This narrow observation does
not establish that oversized or malformed packets reach the branch: upstream
framing, negotiated MTU, high handle byte, transport buffers, and all callers
still require analysis. Do not infer a vulnerability or a validated safe
maximum from this trace.

## Dispatcher entry, opcode table, and MTU-shaped state

The dispatcher entry at `0x1f774` was classified as data in the saved
analysis. In a read-only Ghidra session, temporarily clearing that
classification and disassembling `0x1f774..0x1f7c9` showed that it retains
its second argument as the packet pointer, loads the registered table pointer,
and returns if that pointer or a response-buffer-shaped pointer is zero. It
reads packet byte 6, rejects opcodes above `0x52`, and uses the byte as a
four-byte index into the table whose base literal is at `0x1fac4`. The base
literal resolves to image offset `0x24370`. Image words at `0x243b8` and
`0x244b8` (indices `0x12` and `0x52`) both target `0x1fad4`, the write
selection path described above. This confirms the static opcode routing for
both branches, but does not prove that a received radio packet reaches the
dispatcher. The saved Ghidra reference enumeration found no direct caller
for `0x1f774`; the subsequent raw-instruction scan below found one in
previously misclassified code. Other callers remain a coverage gap.

The same table's indices `0x02` and `0x03` lead to `0x1fa52`, which checks
the 16-bit packet field at offsets 2–3 against 3 before reaching the
`0x1fcaa` branch. Instructions at `0x1fcaa..0x1fcc6` read a 16-bit value
from packet offsets 7–8 and write a negotiated-size-shaped value to state
offset `0x22`: 23 when the incoming value is at most 22; otherwise the lesser
of the incoming value and the local halfword at state offset `0x20`. This is
consistent with ATT MTU exchange, but the local maximum's initialization and
the full packet framing have not been traced. In particular, this negotiation
calculation is **not** evidence that every incoming write is checked against
the resulting value before the callback.

The dispatcher's packet is passed unchanged to the write callback; that
callback reads byte 2 and derives its copy length modulo 256 as documented
above. No caller provenance, minimum packet size, full 16-bit length handling,
or callback buffer capacity was established in this bounded trace. Do not
equate the RF-shaped receive buffer with this packet or infer a vulnerability.
The temporary classification changes were discarded; the image and saved
Ghidra project were not changed.

## Upstream packet handler and framing helper

A halfword-aligned scan of the image for the TC32 direct-call encoding found
one candidate targeting `0x1f774`, at `0x20cd6`. Read-only Ghidra
disassembly after temporarily clearing code classification confirms the call.
It belongs to the previously unclassified routine beginning at `0x20c48`:

1. The routine passes its incoming packet pointer to `0x20898` and returns
   if that helper returns zero. It reads the returned buffer's 16-bit field at
   offsets 4–5 and routes value 4 to the ATT-shaped path; values 5 and 6 take
   other branches. In the value-4 path, the byte at offset 6 selects between
   this dispatcher and a different callback path; the call at `0x20cd6`
   passes the returned pointer unchanged as argument 2 to `0x1f774`.
2. The helper at `0x20898` examines the low two bits of packet byte 0. For
   value 3 it returns the input pointer directly. For value 2 it also returns
   the input directly when byte 1 equals the 16-bit field at offsets 2–3
   plus 4; otherwise it has a separate buffered path. The value-1 path
   requires a previously active buffer and compares a computed accumulated
   length with a state halfword minus 5 before a continuation copy. These are
   instruction-level branches consistent with fragment handling, not a
   complete proof of L2CAP or ATT framing invariants.
3. Initialization `FUN_00008874` passes the low-bit-tagged pointer `0x20c49`
   to `FUN_00020880`, which stores it at `0x0084412c`. A separate previously
   unclassified block at `0x1ea0a..0x1ea16` loads that same pointer, checks
   it for zero, and passes `r6` as argument 2 through the `0x1eeb0` indirect
   trampoline. The literal words were decoded from image offsets `0x895c`,
   `0x20888`, and `0x1ebb0`; the trampoline's opcode is an indirect branch
   through `r3`. Its predecessor conditions, packet origin, and all other
   consumers of the registration require further tracing.

The helper's continuation-buffer comparison does not establish a universal
minimum length or MTU bound: the value-3 and complete-value-2 branches return
directly, and upstream packet production and memory sizes remain unknown. The
scan covered aligned direct-call encodings, not tail calls, indirect calls, or
all possible instruction interpretations. It does not establish exploitability
or a safe callback payload maximum.

## Predecessor conditions and packet-pointer origin

The previously unclassified routine beginning at `0x1e854` receives a pointer
in its first argument, retains it in `r4`, and sets `r6` to that pointer plus
4 at `0x1e880`. On one verified route to the indirect callback at `0x1ea0a`:

* byte 5 of the original argument is nonzero (`0x1e85e..0x1e862`);
* byte `0x2e` of state base `0x00844314` is zero (`0x1e87a..0x1e884`);
* byte `0x71` of that state is zero (`0x1e890..0x1e898`); and
* the low two bits of the byte at the derived pointer are neither 0 nor 3
  (`0x1e88a..0x1e88e`, `0x1ea00..0x1ea08`).

On that route, a nonzero registered callback at `0x0084412c` is called with
`0x40` as argument 1 and the derived pointer as argument 2. This proves the
offset relationship for this route, not the physical origin, buffer capacity,
CRC status, or complete length validation of the original argument. Other
state-dependent routes and packet types were not exhaustively traced.

A separate wrapper at `0x1eeb4` checks byte `0x2f` of `0x00844314`; if zero,
it calls `0x1e854` at `0x1eec0` without modifying the first argument. It
returns zero afterward. Initialization `FUN_0001ddc4`, itself called by
`FUN_00008874`, stores tagged pointer `0x1eeb5` at `0x0084411c`. Image
literals at `0x1de28` and `0x1de2c` establish that registration; the
wrapper's state-base literal is at `0x1eec8`. A bounded aligned direct-call
scan found no call to the wrapper, and a full-byte literal scan found its
tagged pointer at `0x1de28`. The indirect consumer of `0x0084411c` and the
original buffer producer remain unresolved; the presence of registration
alone does not prove this route runs on the controller.

These observations were checked by read-only temporary disassembly of
`0x1e854..0x1e920`, `0x1ea00..0x1ea16` and `0x1eeb4..0x1eec6`; Ghidra
discarded those temporary classification changes. No hardware was contacted.

## Registered-wrapper consumer search (superseded by trace below)

The wrapper pointer is stored at `0x0084411c`; an aligned literal scan found
its explicit address only at image offset `0x1de2c`. A scan for neighboring
structure bases found several references to `0x00844108`, which would require
a further offset of `0x14` to address the wrapper slot, and to `0x00844100`,
which would require offset `0x1c`. A subsequent halfword scan identified
candidate PC-relative loads of those literal slots, including `0x2fb6`,
`0x1dc14`, `0x1dc42`, `0x1eb7e`, `0x1ec58`, and `0x1ee18`.

Read-only instruction checks of these examples show loads of the **base
contents** and calls through trampolines, not a load from base plus `0x14`.
For example, `0x1eb7e..0x1eb86` uses the pointer at `0x00844108` with
arguments `8`, a state pointer, and `0x0c`, while `0x1dc14..0x1dc1c` uses
the same pointer with different arguments. These are neighboring callback
uses, not evidence that the registered wrapper at `0x0084411c` was invoked.
A bounded scan for immediate word loads at offset `0x14` in the next 128
bytes after the identified `0x00844108` literal loads found no candidates;
register-derived addresses, longer-range data flow, other bases, indirect
calls, unclassified instructions, and external or ROM code were not ruled out.

That bounded inspection did **not** identify the wrapper consumer or the
original packet producer. The established `original argument + 4` relation
and route-specific gates above remain the furthest verified provenance. Next,
trace `0x0084411c` using register/data-flow analysis beyond the bounded
windows and inspect the boot/ROM interface or runtime-supplied callbacks if
the saved image cannot account for the read. Do not infer CRC, MTU, receive
buffer ownership, or exploitability from this negative search.

## Wider data flow: ring consumer and RF-shaped producer

The wider search identified the wrapper consumer missed above. The base of a
separate state structure is `0x008440d0`, so its field at `+0x4c` is exactly
the registered wrapper slot `0x0084411c`. Read-only Ghidra disassembly of
`0x2a8c..0x2b0a` verifies the following:

* The main-loop routine indexes a ring using control at `0x00843d64` and
  storage at `0x00843b64`. It computes the entry pointer in `r0` using the
  read index, ring mask, and entry stride.
* If entry byte `+2` is nonzero, it loads the function pointer at
  `0x008440d0 + 0x4c` at `0x2af8`, checks for zero, and calls it through the
  trampoline at `0x2acc`, preserving the entry pointer as argument 1. If
  byte `+2` is zero, a different pointer at `0x00846600` is selected instead.
  Thus the callback selected on the nonzero branch is the registered wrapper,
  subject to registration and the state gates documented above.
* `FUN_00002e10` computes a slot from the same ring control and storage and
  writes its low 16 address bits to `0x00800c08`. `FUN_000026c0` advances
  the same ring's write index and programs the next slot to that register.
  The independently cited TLSR8258 register map in
  [staged-frame-transport-static.md](staged-frame-transport-static.md)
  identifies offset `0xc08` as RF receive DMA address; the firmware's chip
  and the full address interpretation remain unverified. This supports an
  RF-receive-DMA-shaped producer, not a physical BLE observation.
* `FUN_000028d8` reads `0x00800f20` and calls `FUN_000026c0` when its low
  status bit is set (`0x28ea..0x28f0`, `0x297e`). Within the receive handler,
  `0x2708..0x2710` compares the entry's first word with byte `+5` plus 13;
  `0x2782..0x2788` tests bit 0 of a byte at entry + first word + 3. A failed
  check takes the rejection path that can roll back the write index and
  restore the slot address (`0x2712..0x2716`, `0x2730..0x273c`). Do not
  label the bit CRC status without a matching confirmed hardware definition.

The handler clears entry byte `+2` before invoking other state-dependent
processing. The flag-setting path is now identified below. The two checks do
not prove ATT minimum length,
MTU bounds, integrity semantics, or physical controller behavior. The startup
code registers the wrapper and programs the receive-shaped ring in the saved
image; no separate bootloader or ROM implementation has been identified, so
there is no basis to attribute this now-identified callback read to ROM.
Temporary Ghidra changes were discarded. `RawRange.java`'s printed literal
comments were independently resolved from the binary because its PC-relative
display can be wrong; the instruction mnemonics and branches were used, not
those comments.

## Ring-entry selection flag

`FUN_0001ddc4` registers tagged pointer `0x370d` at `0x0084410c` using the
literal words at `0x1de18` and `0x1de1c`. On the state-8 branch with the
substate byte at `0x00844050 + 0xe` equal to 4 or 6, `FUN_000026c0` checks
the callback pointer at `0x008440d0 + 0x3c` (the same `0x0084410c`), and
at `0x2842..0x2846` passes the ring-entry pointer in `r0` through the
`0x289c` indirect trampoline. The low byte of its return becomes the
retention result (`0x284a..0x284e`); the receive handler rolls back the
ring write index when that result is zero.

The callback body at `0x370c` was previously classified as data. Temporary
read-only Ghidra disassembly shows it retains argument 1 in `r5`, reads entry
bytes `+4` and `+5`, and has state- and packet-dependent branches. On a
verified route, it stores `0x40` at `[r5 + 2]` at `0x37a6..0x37a8`; for
packet types whose low two bits of entry byte `+4` are not 3 it then sets
return value 1 at `0x37b4` and returns through the common epilogue. This
connects a retained, nonzero-flag entry to the main-loop wrapper selection
above, provided intervening state and queue conditions hold. A separate branch
for low bits equal to 3 can call `0x1ef38` before joining the return path;
that callee is traced below, with its downstream effects still partial.

The flag-setting route is not unconditional. For example, in the disassembly
at `0x377e..0x378a`, bit 3 of entry byte `+4` is compared with byte `+6` of
state `0x00844294`; unequal values enter `0x378c`, while equality branches to
`0x381a`, which has additional conditions before it can reach the flag store.
Other state checks, zero-length handling, early returns, and indirect callback
replacement can alter the result. The preceding length-shaped and status-bit
checks remain separate observations; this path does not establish complete ATT
length, MTU, CRC, or security validation. Static control flow does not prove
that a physical controller traverses the path.

Ghidra instruction output was checked for the registration, call and return,
and temporary disassembly of `0x370c..0x3864`. Literal-pool values were read
directly from the identified firmware image because the `RawRange.java`
literal comments can be incorrect. Temporary analysis changes were discarded.

## Type-3 helper and remaining callback branches

Temporary read-only Ghidra disassembly of `0x1ef38..0x1ef68` shows that the
type-3 helper calls the byte-copy routine `FUN_00003b04` with source entry
`+4`, destination `0x00844314 + 0x30` (`0x00844344`), and a fixed length
of `0x18`. It then tests state byte `0x00844314 + 0x2e`. If nonzero, it
copies the word at `0x00844294 + 0x50` to `+0x54`, stores 1 at `+0x60`,
and stores 1 at `0x00844050 + 6`. If zero, it passes the copied 24-byte
buffer to `FUN_0001de7c` instead. The literal words at `0x1ef6c`, `0x1ef70`,
and `0x1ef74` identify the three state bases. The existing decompilation of
`FUN_0001de7c` shows state-dependent processing of fields in that copy,
including branches on byte `+2` and comparisons of 16-bit fields; its
complete downstream behavior is not instruction-verified here.

At `0x384c..0x3852`, `0x370c` calls this helper for selected byte-`+5`
values in the low-two-bits-equal-3 path and then branches to `0x37b4`, which
sets the outer return value to 1. The caller does not branch on the helper's
return value on this path. The 24-byte copy has no local length comparison
visible inside `0x1ef38`; that observation is not proof of an out-of-bounds
read because the size and earlier validation of the ring slot are not fully
established.

The caller `FUN_000026c0` can invoke the registered `0x370c` callback when
its substate is 4 **or** 6, but the callback's `0x3752..0x3768` branch
returns zero unless substate `0x00844050 + 0xe` is 6 and byte `+9` is zero.
Thus the documented flag-setting path requires substate 6. On that path,
`0x3796..0x37a4` can skip the flag store when entry byte `+5` is zero and
state byte `0x00844314 + 0x70` is zero. The equality branch at
`0x381a..0x382c` also skips the flag store when entry byte `+5` is zero or
state `0x00844294 + 1` is nonzero. These are bounded instruction observations,
not a complete enumeration of all dynamic callback replacements or packet
validation, nor evidence of physical device behavior. Temporary analysis
changes were discarded.

## Type-3 parser and candidate ring initialization

Read-only instructions at `FUN_0001de7c` confirm that it examines byte `+2`
of its input and has branches for `0`, `1`, and `0x18`. For `0x18`, it
assembles a 16-bit value from bytes `+5` and `+6`, subtracts a state halfword,
and uses a signed comparison after a 16-bit shift. On the accepted branch it
sets state bytes and combines input bytes `+3` and `+4` to select one of
several state values. The saved decompilation additionally shows distinct
field copies and state flags for `0` and `1`. These operations are consistent
with control/state processing; their protocol meanings and the full downstream
behavior, including `FUN_00023528`, remain unresolved. The signed comparison
is not an ATT payload-length check.

The image contains a **candidate** ring descriptor at file offset `0x3d64`:
the first word is `0x40`, the following count halfword is 8, and the next
pointer-shaped word is `0x00843b64`. Those values fit the independently
traced runtime descriptor at `0x00843d64`: 8 slots at a 64-byte stride cover
the 512 bytes from `0x00843b64` through `0x00843d63`, immediately preceding
the descriptor. The startup metadata includes words `0x00843b4c`,
`0x00844858`, and `0x00003b4c`, suggesting an image-to-RAM association with
matching low addresses. However, the examined startup copy at
`0x152..0x164` initializes a different range beginning at `0x00845200`;
the code or loader that initializes the `0x00843d64` descriptor has not been
verified. The 64-byte stride and eight entries are therefore image-layout
evidence and a strong candidate, not a proven runtime capacity or a bound on
DMA writes.

Separately, `FUN_0001d25c` calls `FUN_0001d170`, which reads the runtime
word at `0x00843d64`. Read-only instructions at `0x1d19c..0x1d1b6` show
that values above 64 produce two state halfwords from `(word - 24)`, capped
at 251; values at most 64 take the 27-byte fallback at `0x1d1c6`. This
documents a size-dependent initialization, but it neither proves the
descriptor's actual value at runtime nor enforces an incoming packet bound.
In particular, do not turn the candidate stride into proof that the fixed
24-byte copy is safe under every runtime configuration.

## Loader and retention-layout cross-check

The [Telink single-connection SDK documentation, “Space Allocation of
Versions Prior to SDK V3.4.2.4”](https://doc.telink-semi.cn/doc/en/software/res/sdk/ble/tc_ble_en/tc_ble_single_connection_en/)
describes an older B85/B87 layout in which retention data is stored in flash
at offset `_rstored_` and mapped to SRAM at `0x840000 + _rstored_`; it says
that vector, RAM code, and retention data are copied on power-on or wake from
deep sleep. This is consistent with the candidate image/RAM correspondence
`0x3d64 -> 0x00843d64` and its location within the first 16 KiB of SRAM.
It is **external reference behavior**, not identification of the actual
controller IC, SDK revision, loader, or this firmware's power-on execution.

In the saved image, a full-byte search for the three candidate range words
found `0x00843b4c` at image offsets `0x1ac` and `0x8030`, `0x00844858`
only at `0x1b0`, and `0x00003b4c` only at `0x1b4`. The use at `0x8030`
belongs to `FUN_00008000`, whose inspected code stores flash-address-shaped
values to `0x00843b4c` and the following word based on a detected value; it
is not a copy of the ring descriptor. A halfword-aligned scan for PC-relative
literal loads targeting `0x1ac`, `0x1b0`, and `0x1b4` in the preceding
startup region found none. This is a bounded encoding scan, not proof that the
words are unused or that an external loader consumes them.

The independently inspected `0x152..0x164` startup loop copies from
`0x245a4` to a range beginning at `0x00845200`, not `0x00843d64`. A
[published TLSR8258 Zigbee SDK startup file at commit
`7cf1d6dbac6497505d649bd03342efe81f1a8212`](https://github.com/telink-semi/telink_zigbee_sdk/blob/7cf1d6dbac6497505d649bd03342efe81f1a8212/tl_zigbee_sdk/platform/boot/8258/cstartup_8258.S)
contains an analogous general data-copy loop, but this is a different SDK
artifact and does not verify the candidate ring initialization. No separate
controller loader or ROM image has been identified. The descriptor remains
**candidate initialized data**, not an observed runtime capacity; a next
static step is to identify the image's actual load mechanism or a matching
link map/startup implementation and trace any descriptor overwrites.

### Descriptor-reference follow-up

An aligned image scan for literal values from `0x00843d40` through
`0x00843d64` found only the five known literals equal to `0x00843d64`, at
image offsets `0x2860`, `0x2cb8`, `0x2e64`, `0x1d1d8`, and `0x1db00`.
A halfword-aligned PC-relative-load scan yielded instruction candidates at
`0x26c6`, `0x2aac`, `0x2e12`, `0x1ce00`, `0x1d19c`, `0x1d1be`, and
`0x1d8bc`; `0x1ce00` lies in data in the saved analysis and is not an
established instruction. This is an encoding-based candidate list, not a
complete pointer or data-flow analysis.

Previously unclassified code beginning at `0x1d78c` was temporarily
disassembled read-only. At `0x1d8bc..0x1d8ce`, it reads the descriptor's
count, write index, and stride, computes a slot address using the base at
image literal `0x1db04` (`0x00843b64`), and retains that address in `fp`.
It clears a **word at slot + 4** at `0x1d8e4..0x1d8e6`; this is a slot
write, not a write to the descriptor's stride or count. The other traced
ring users update indices at descriptor offsets `+6` and `+7`, while
`FUN_0001d170` reads the stride for its size-dependent initialization. The
reviewed direct-reference paths do not establish a stride or count overwrite,
but longer-range aliases, indirect code, DMA, and external loader behavior
remain unexamined. The Ghidra project discarded temporary classification
changes.

The [Telink SDK documentation's newer memory-layout section](https://doc.telink-semi.cn/doc/en/software/res/sdk/ble/tc_ble_en/tc_ble_single_connection_en/)
also describes retention-data copies and a cache immediately following the
aligned retention area. This is consistent with the firmware's distinct
candidate descriptor and cache-shaped ranges, but no matching link map or
actual loader was identified. The image-to-RAM mapping and runtime values
therefore remain unverified; do not treat this follow-up as proof that the
descriptor cannot be overwritten or that all slots are 64 bytes at runtime.

## Next investigation

Verify the loader or initialization of the candidate ring descriptor, trace
the remaining `FUN_0001de7c` branches and `FUN_00023528`, subsequent packet
validation and incoming MTU enforcement, and other indirect callers. Then
trace the notification-side
registration and send functions for motion. The RF-shaped
receive queue discussed in [staged-frame transport](staged-frame-transport-static.md)
remains a separate path unless additional code connects it.
