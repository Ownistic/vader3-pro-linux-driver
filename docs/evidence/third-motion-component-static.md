# Third motion component over BLE: corrected static record

**Artifact:** `Telink87_For_F3_Ver6.7.5.1.bin`, version `6.7.5.1`, SHA-256
`6cba30540a0c3ba16acad2b62bb4dd64dfccc28add04914c3720c6ae6a53e20a`.
Firmware analysis used saved corrected decompilation, image words decoded
directly, and a scratch Ghidra project under `/tmp`. The separately identified
live GATT inventory/version observation is recorded below. No mode change,
vendor command, firmware write or product edit was made.

**Correction notice.** The first version of this note overclaimed. Specifically:

* It searched only direct literals of `0x8469ec`, missing the established alias
  pointer `0x846a9a` (= `0x8469ec + 0xae`) used by `FUN_000131a8` and
  `FUN_00013bcc`.
* It described `FUN_0000bcb8` as a "second producer/copy" without establishing
  where its copies and derived values go.
* It claimed a complete writer set for `0x847544 + 0x44/0x45/0x46/0x48` while
  omitting the `0x92`-byte bulk clear in `FUN_000143a4`.
* It said mode 2 gates the *enable* of that pair. It gates only the `0x0400`
  seed; the enable store happens first.
* It wrote "no reader of `+0xb2` is a GATT sender" as "no NUS sender reads
  `+0xae..+0xb9`", which is false — `FUN_00010ba4` is a NUS sender and reads
  `+0xae` and `+0xb0`. The correct claim is narrower (see §3).
* It presented decompiler **line numbers** (`L21477`, `L13257`) as if they were
  instruction addresses. They are not; they are removed or relabelled.
* It concluded "no BLE route exists". What the inspected code supports is the
  weaker statement in §4.

---

## 1. The gated pair at `0x847544 + 0x44/0x45/0x46/0x48`

### 1.1 Enable store precedes the mode test (instruction-verified)

Scratch-project disassembly of `FUN_000151b4` (`0x151b4`):

| Address | Instruction | Effect |
| --- | --- | --- |
| `0x151ba` | load byte `[base+0x44]` | read latched flag |
| `0x151bc..0x151be` | `cmp #0`, `tjne` | already set → return |
| `0x151c0..0x151c2` | `r0=1`, store `[base+0x44]` | **`+0x44 = 1` unconditionally** |
| `0x151c4..0x151ca` | load `[base+0x84]`, `cmp #1`, `tjeq` | mode/profile test gates only the seed |
| `0x151ce..0x151d8` | `r1 = 0x80<<3`, store halfword `[base+0x46]`, `[base+0x48]` | seed `0x0400` into both |
| `0x151da..` | second base `0x8475d8` | seeds the feedback block, not the sensor words |

So the state machine is:

| `+0x44` | `+0x46`/`+0x48` | How reached |
| --- | --- | --- |
| `0` | `0`/`0` | initial, or after `FUN_000143a4` / `FUN_00014000` |
| `1` | `0x0400`/`0x0400` | handler ran while `+0x84 == 1` |
| `1` | `0`/`0` | handler ran while `+0x84 != 1` — flag latched, seed skipped |

Because the enable store is unconditional, the third row is reachable. NUS bytes
14-17 therefore have at least two enabled forms: `00 04 40 01` (seeded) and
`00 00 00 01` (enabled, unseeded), plus `00 00 00 00` when disabled. Byte 17 is
`1` in every enabled form and `3` never occurs in inspected code, because
`+0x45` is only ever cleared to `0`.

### 1.2 Inspected writers, including bulk clear

| Writer | Coverage |
| --- | --- |
| `FUN_000143a4` (`0x143a4`) | `memset(0x847544, 0, 0x92)` covers `+0x44`, `+0x45`, `+0x46`, `+0x48` and `+0x84`; then sets `+0x84 = param_2`. Caller `FUN_00010ecc` passes `1` only when `0x846ac8 + 0xc == 2` |
| `FUN_00014000` (`0x14000`) | clears six `0x23` records at `0x8475d8`, then `+0x44=0`, `+0x45=0`, `+0x2c=0`, `+0x46=0`, `+0x48=0` |
| `FUN_000151b4` (`0x151b4`) | `+0x44=1`; `+0x46=+0x48=0x0400` only when `+0x84==1` |

No other writer of those four offsets exists in the inspected bodies. Because a
`0x92`-byte clear exists, the "only these writers" statement is scoped to the
five named functions plus that bulk clear, not to the whole image.

### 1.3 Reaching the handler

`FUN_000151b4` has one incoming reference: a computed call at `0x15346` with no
containing function, inside the handler block entered at `0x15318`. That block is
one of four targets in the pointer table at `0x24150`, selected by
`FUN_0001524c` from four-byte records at `0x845ad3` (index byte must be
`<= 0x33`). The record table lives at `0x845a30`, is reset to `0xff` by
`FUN_00013fc4`, and is therefore runtime configuration — the enabling command
byte cannot be read out of the image.

**Status:** the pair is a latched flag plus two seeded constants. Its values are
not established as motion in any state; the seeded value `0x0400` and the
unseeded zero both come from this one handler. No copy from
`0x8469ec + 0xae..0xb8` into these fields exists in the inspected bodies.

## 2. Third word `0x8469ec + 0xb2`: alias readers and the copy path

### 2.1 Alias pointer

`0x8469ec + 0xae = 0x846a9a` appears as its own literal at two sites:

| Literal site | Function | Use |
| --- | --- | --- |
| `0x13ce8` | `FUN_00013bcc` | `puVar4 = 0x846a9a`; calls `FUN_00013af0` on `+0..+5`; calls `FUN_0000bcb8`; then rewrites the six bytes in place |
| `0x133d0` | `FUN_000131a8` | same alias for the alternate producer path |

`FUN_00013bcc` in-place rewrite (decompiled lines 16423-16431): for each of the
three 16-bit words at `0x846a9a + {0,2,4}` it stores `(word << 16) >> 17` and
`word >> 9` back into the same two bytes — a compression of the word pair, and
the third word (`puVar4[2]`, i.e. `+0xb2`) is included. The centring/threshold
path is at decompiled lines 16440 onward. `FUN_00013af0` receives the six byte
addresses `0x846a9a + 0..5`.

### 2.2 `FUN_0000bcb8` copies all six words first

`FUN_0000bcb8` (`0xbcb8`), decompiled lines 8025-8035:

```
0x8469ec+0xae -> 0x846928[0]   0x8469ec+0xb4 -> 0x846920[0]
0x8469ec+0xb0 -> 0x846928[1]   0x8469ec+0xb6 -> 0x846920[1]
0x8469ec+0xb2 -> 0x846928[2]   0x8469ec+0xb8 -> 0x846920[2]
```

It then computes per-axis absolute deltas against baselines at `0x8468f8` and
`0x846900` into `0x846910` and `0x8468f0` (lines 8043-8050), latches a
`bVar1` flag when `|delta| >= 0x33` on the first group or `> 100` on the second,
and on a latch calls `FUN_0000bc00` and re-baselines (lines 8065-8070).
Otherwise it accumulates into six 4-byte cells at `0x846ac8 + 0xa0..0xaf`
(lines 8079-8106), driven by the first word of each group.

**Where the destinations go:** scanning every literal site for
`0x846928`, `0x846920`, `0x846910`, `0x8468f0`, `0x8468f8`, `0x846900` and for
`0x846b68..0x846b7c` (the accumulator cells as absolute addresses) finds **no
reader outside `FUN_0000bcb8` itself**, except `0x8468f8`, which
`FUN_000131a8` also touches. Accessing the accumulators through the `0x846ac8`
base (`+0xa0..+0xaf`) is likewise confined to `FUN_0000bcb8` in the inspected
decompilation.

So this path consumes the third word for internal motion detection and
accumulation but publishes it to no reader that was found. That is the concrete
unresolved gap, not a demonstrated route. Bound: a pointer to those buffers
computed at run time and stored in RAM would not appear as a literal; ROM code
was not inspected.

## 3. The BLE surface actually inspected

* `FUN_000203c0` is the notification primitive identified in the inspected code.
  Its recorded direct callers are `FUN_000102e4` (handle `0x17`) and
  `FUN_00010300` (handle `0x27`); this is not whole-image send-path closure.
* Handle `0x27` callers: `FUN_00010320`, `FUN_000103e4`, `FUN_0001043c`,
  `FUN_0001045c`, `FUN_000104d4`, `FUN_00010604`, `FUN_0001067c` (two sites),
  `FUN_00010ba4`.
* **Corrected claim:** `FUN_00010ba4` does read `+0xae` and `+0xb0` — it is the
  two-word motion sender. **No listed, inspected handle-`0x27` sender was found
  to read `+0xb2`, `+0xb4`, `+0xb6` or `+0xb8`.**
* `FUN_00010dc0` also passes `+0xaa` and `+0xac` to `FUN_0001601c`; those are
  neighbouring fields, not the six-word sensor block.
* Attribute-table scan: no `value` pointer in `0x846a80-0x846ac0`,
  `0x8471f0-0x847220` or `0x845850-0x845880`. This excludes a *direct* GATT read
  of those buffers; it does not exclude a computed or copied response built by a
  callback into a scratch buffer, nor a notify issued from code outside this
  image.
* The `5A A5 21 18` six-word frame ends in `FUN_000002ec` → `FUN_0001c1f0`,
  which programs `0x80009f` and `0x800c04`. Calling that "not BLE" would be
  unjustified: without the TLSR87 register map it cannot be distinguished from a
  BLE advertising/PDU submission. It is demonstrably not a GATT characteristic
  notification or read.

## 4. What the inspected evidence supports

1. The NUS builder emits exactly **two** motion words (`+0xae`, `+0xb0`) in
   bytes 11-13, plus a latched flag and a constant pair in bytes 14-17.
2. The third word `+0xb2` is produced, is transformed in place by
   `FUN_00013bcc`, and is consumed by `FUN_0000bcb8` (internal delta and
   accumulator, no reader found), by `FUN_0001899c` (staging `0x847204`), by
   `FUN_0001a45c` (`04 FE 66` USB buffer) and by `FUN_00011230` (baseband frame).
3. **No third GATT path is demonstrated in the inspected paths.** Not: "no such
   path exists", and not "the hardware lacks the axis".
4. The raw value at `+0xb2` is not established as physical Z. The earlier
   documents establish axis assignment only for the two words that were
   captured, and the pair's order was inferred from a USB frame order.

## 5. Blocker and next practical investigation

There is no verified third-component packet contract to implement for the
current Bluetooth connection. Keep the working two-component daemon unchanged.
The existing `04 FE 66` USB serializer led to a positive owner-coordinated
roll observation: see [paired USB/Bluetooth observations](usb-third-motion-observation-2026-11-04.md).
The owner explicitly retained **Bluetooth-only product scope**, declining a USB
backend. The USB finding is a reference, not a change of supported transport.

The selected next investigation is the official Android BLE SDK's receive
decoders and command producers, looking for a positive alternate motion
contract. Further callback/copy-aware analysis or an authorized capture of an
official application's BLE interaction may still be needed. Do not probe the
OTA characteristic with speculative commands.

Firmware RAM watchpoints on the copy/accumulator buffers would require
controller debug access that has not been provided. Connecting a vendor host
application alone does not provide that access. Such a trace is not an
immediately available next step, and absence of a read in one runtime window
would not prove that no route exists.

## 7. The second live vendor characteristic (`…0a0b0c0d2b12`)

**Live observation supplied by the parent (not performed here):** a standard
`ReadValue` of firmware revision `0x2a26` on the Legion Go returned `6.7.5.1`,
matching this artifact's version string. A version string is not an image-hash
proof, so artifact identity remains: *string match, hash unverified on device*.

**Inventory:** apart from NUS, the only vendor characteristic is
`00010203-0405-0607-0809-0a0b0c0d2b12` in service `…0a0b0c0d1912`, with
properties read, write-without-response, notify.

### 7.1 Exact attribute-table entry

Attribute table base `0x245d8`, 0x18-byte entries, stride confirmed against the
consumed entries `0x17`, `0x18`, `0x1b`, `0x1f`, `0x24`, `0x27`, `0x29`:

| Field | Value |
| --- | --- |
| Entry index | `0x2c` (table offset `0x249f8`) |
| UUID pointer | `0x23808` → bytes `12 2b 0d 0c 0b 0a 09 08 07 06 05 04 03 02 01 00`, i.e. `00010203-…-0a0b0c0d2b12` |
| Property word | `0x10030000` (same family as the NUS read/write/notify entries) |
| Value pointer | `0x846034` (RAM) |
| Callback slot | `0x2288d` (Thumb bit set; body at `0x2288c`) |
| Second callback slot | `0` |

A second copy of the same UUID bytes sits at `0x23903`; no table entry's UUID
slot points at it in the range scanned (`0x00–0x3f`), so it is not an active
entry.

### 7.2 What the callback does

The handler at `0x2288c` is a **write parser over an OTA-shaped session state
block at `0x844698`** (base literal at image `0x22b90`; the module's decompiled
companions `FUN_00022604`, `FUN_00022644`, `FUN_00022698`, `FUN_000226a4` all use
the same base). Structure, from instructions only:

* `0x22894` takes the attribute argument (`r5`); `0x228b2..0x228c8` extracts a
  big-endian 16-bit command from payload bytes 9-10 and a big-endian 16-bit
  offset from payload bytes 7-8, storing them at `+0x1a` and `+0x12`.
* `0x228cc..0x228d2` computes `+0x14 = offset + [+0xf]`, a bound used for later
  range checks.
* `0x228ae` arms a `0x40`-byte window (`+0x16 = 0x40`), and `0x228a0` sets a
  busy flag (`+0x4 = 1`).
* `0x2291e..0x2294c` sets a mode byte (`+0x7 = 2`), ORs `+0x40` with 1, writes a
  `-1` sentinel at `+0x38`, clears `+0x54`/`+0x58` and latches a pointer at
  `+0x68`.
* Command dispatch compares the extracted command against stored limits
  (literal `0x22b94` = `0xfeff` is one comparator) and branches into the
  start / continue / finish arms.

That is a chunked transfer controller with an offset, a 64-byte window, a bound
and a completion sentinel — i.e. **an OTA/large-transfer channel**, agreeing with
the existing APK evidence that the only class referencing this UUID is
`com.flydigi.sdk.bluetooth.ota.TelinkOtaFactory`, and with
[the mobile-app protocol record](mobile-app-ble-protocol.md). No update
procedure, image format, signature check or recovery behavior is described here,
and none should be inferred from this note.

### 7.3 Does it carry motion?

**No, on the inspected evidence.**

* The handler dereferences only its argument buffer and the `0x844698` state
  block. It never reads `0x8469ec + 0xae..0xb8`, the alias `0x846a9a`, or any of
  the derived buffers (`0x846928`, `0x846920`, `0x846910`, `0x8468f0`).
* The value buffer `0x846034` appears as a literal nowhere else in the image, so
  nothing else populates it; a read of this characteristic returns whatever RAM
  happens to hold there (undefined from the artifact alone).
* No notify primitive call appears in the inspected handler body. The only
  notification primitive found in this image is `FUN_000203c0`, and this module
  does not call it; the advertised `notify` property is therefore not evidence
  that this characteristic ever notifies.
* The two-word result of §4 stands: the only BLE motion carrier remains NUS
  bytes 11-13.

### 7.4 Bounds of this section

Read-only static work plus the parent's single standard `ReadValue`. **This
analysis did not read or write the `2b12` characteristic, opened no saved Ghidra
project, and touched no live controller.** Unknown: the command opcode set and
its meaning, whether any read value is defined, whether the device notifies on
this characteristic in any state, and whether this module hands off to a
bootloader. The handler body beyond the inspected range (`0x2288c..0x22990`) was
not fully enumerated, and the module has functions outside the saved export.

## 8. Provenance, side effects, and limits

* The firmware image SHA-256 above was re-verified unchanged before and after
  this pass (parent rehash agrees).
* **Disclosed side effect (previous pass):** opening `local/research/ghidra`
  with `-readOnly -noanalysis` to query callers caused Ghidra to refresh its own
  project index metadata under `vader3pro.rep` (`~index.dat`, `~index.bak`,
  database mtimes). Nothing was reverted, because the original bytes are not
  known. Scope of what is provably unchanged: the firmware image (hash), the
  saved decompilation and the scripts. The Ghidra project's internal database
  bytes are not hash-provable from this workspace. This pass did not open that
  project at all.
* Observed: sections 1-3, at the stated instruction addresses and decompiled
  line ranges.
* Inferred: the six words are two three-axis sensors; the `5A A5 21 18` frame is
  a radio submission.
* Unknown: command byte that reaches the handler (runtime config); whether the
  baseband frame leaves as BLE advertising, BLE data PDU or proprietary 2.4 GHz;
  whether any ROM/unmodelled code reads the derived buffers; the installed
  firmware revision; the physical axis identity of `+0xb2`.
