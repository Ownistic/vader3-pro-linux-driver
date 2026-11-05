# Nordic UART 20-byte input prefix and HID-builder bypass (static)

**Artifact:** main image `Telink87_For_F3_Ver6.7.5.1.bin`, version `6.7.5.1`,
SHA-256 `6cba30540a0c3ba16acad2b62bb4dd64dfccc28add04914c3720c6ae6a53e20a`
(re-hashed 2026-11-04 during this pass; unchanged).
Repository baseline `f1f1f265a3da0d7d8fb72be3c5b0e9297c66eb2b` plus the
uncommitted research documents.

**Scope.** The original static note answers two questions only; the live X/Y
label correction at the end is separate hardware evidence:

1. What does the 20-byte Nordic UART (NUS) object built by `FUN_00010ba4` place
   in bytes 0-10, given that bytes 11-13 are the already-verified packed sensor
   pair and the object ends `FE 00`?
2. Does the `AC 01` selector change which input builder the Bluetooth scheduler
   runs, i.e. does it stop the normal Bluetooth DInput HID input report?

Sections 1–5 below are **static**. During that analysis no controller was
contacted, no command was sent, and no target code was executed. The installed device's firmware revision
is still not independently established, so this is not a hardware result. All
prior evidence that the live stream carries motion at 11-13 and that the HID
path carries buttons/axes stands as separately documented hardware observation.

**Method.** Read-only: saved corrected decompilation
(`local/research/scripts/vader3pro-fixed-decompile.c`) and read-only Ghidra
12.1.2 disassembly of the same hash-identified image imported into a new
`/tmp` project with the installed `Telink_TC32_32` extension. The saved
`local/research/ghidra` project was **not** opened or modified. Literal-pool
words were decoded directly from the image with `struct.unpack_from`.

> The saved export's decompilation of `FUN_00010ba4` does not show the writes to
> bytes 0-9 (the register holding the output pointer was not recovered). The
> disassembly establishes them; the decompiler agrees on bytes 4-13 and 18-19.
> Treat the byte-0..3 assignment as instruction-established, not
> decompiler-established.

---

## 1. Scheduler interaction: the AC selector switches builders

`FUN_00008818` (`0x8818`) is the per-tick input dispatch used for this
transport. Read-only instructions:

* `0x881a` calls `FUN_0001d2c0`; `0x881e` requires the result `== 8`, otherwise
  return.
* `0x8824` loads literal `0x846ac8`; `0x8826` requires byte `+0x15 == 0`;
  `0x882c..0x883c` are a 16-bit countdown at `+0x19/+0x1a`.
* `0x8840` calls `FUN_00010980` (incoming-frame dispatcher); `0x8844` returns
  when it reports `1`.
* `0x8848` loads literal `0x008472bc` — the byte the NUS `AC` handler writes —
  and `0x884a` reads it; `0x884c` tests it against zero.
* `0x8850` reads mode byte `0x846ac8 + 0xc` when the selector is zero, and
  dispatches `1 -> FUN_00010ce4`, `2 -> FUN_00010dc0`, otherwise nothing.
* When the selector is **nonzero**, control goes to `0x885e` and the only
  callee is `FUN_00010ba4` — the NUS object builder.

`FUN_00010560` (`0x10560`) writes that byte on `AC <b>`: selector =
`b & 1` normalised to 0/1, the adjacent byte `0x008472bd` = `b & 0x10`
normalised to 0/1, then `FUN_00010320` emits the `AC C0` status object whose
byte 10 reports the selector. `FUN_00010eb0` (`0x10eb0`) clears the selector
during the `FUN_00008874` initialization path (already documented).

**Consequence (observed at instruction level, conditional on the gates above):**

| Selector `0x8472bc` | Mode `0x846ac8+0xc` | Builder run this tick |
| --- | --- | --- |
| `0` | `1` | `FUN_00010ce4` — 20-byte HID input payload |
| `0` | `2` | `FUN_00010dc0` — NUS object **and** 17-byte HID-handle payload |
| `0` | other | nothing |
| nonzero | any | `FUN_00010ba4` — NUS object only |

Both HID-report senders reach the report handle only through `FUN_000102e4`
(`0x102e4`, handle `0x17`). Its only call sites are `0x10ce4`+`0x13d` and
`0x10dc0`+`0x5b`. Therefore, while the selector is nonzero and those gates
hold, this image has **no other producer** that would notify the DInput input
report. Bound: the search covers the analyzed image's decompiled call graph;
indirect calls, ROM/library code and unclassified ranges remain possible in
principle, but the handle-`0x17` notify path itself is a single helper.

**Answer to question 2:** yes — `AC 01` does not merely add a stream. It moves
the scheduler onto a branch that calls only `FUN_00010ba4`, so the normal
Bluetooth DInput HID input payload (`FUN_00010ce4`, and in mode 2 the
`FUN_00010dc0` companion payload) stops being built and therefore cannot be
notified. That is a static branch fact, not a hardware result; it is consistent
with the observed "134 NUS notifications/s and zero physical HID reports".

Two nuances worth keeping:

* The NUS object is **not unique to the selector**. In mode 2 the selector-zero
  path (`FUN_00010dc0`) also calls `FUN_00010ba4`. Do not infer selector state
  from NUS presence alone.
* `0x8472bd` (the `0x10` bit of the same `AC` byte) is written but its only
  documented reader is the status object; it does not gate the scheduler branch.

---

## 2. NUS 20-byte object layout

Builder: `FUN_00010ba4` (`0x10ba4`), output length `0x14`, sent through
`FUN_00010300` -> `FUN_000203c0(0x27, buf, 0x14)` to the Nordic UART TX
characteristic `6e400003-...` (handle `0x27`). The send is additionally gated by
`FUN_00002dd8()` returning `< 11` (`0x10308..0x1031a`) — a two-ring space-shaped
value, identity still unresolved, so "the object was built" does not prove it
was queued (unchanged from the earlier trace).

State base used by the builder: `0x008469ec` (literal at image `0x10cdc`);
second base for bytes 14-17: `0x00847544` (literal at image `0x10ce0`).
All offsets below are byte offsets from those bases.

| NUS byte | Source | Derivation |
| ---: | --- | --- |
| 0 | `0x8469ec + 0x0e` | low byte of the 16-bit axis word 0 |
| 1 | `0x8469ec + 0x10` | low byte of axis word 1 |
| 2 | `0x8469ec + 0x12` | low byte of axis word 2 |
| 3 | `0x8469ec + 0x14` | low byte of axis word 3 |
| 4 | `0x8469ec + 0x02` | button/hat bitmap bits 0-7 |
| 5 | `0x8469ec + 0x03` | bitmap bits 8-15 |
| 6 | `0x8469ec + 0x36` | left-analog channel, raw `0..255` |
| 7 | `0x8469ec + 0x37` | right-analog channel, raw `0..255` |
| 8 | `0x8469ec + 0x05` | bitmap bits 24-30 (`& 0x7f`; bit 31 not carried) |
| 9 | `0x8469ec + 0x04` | bitmap bits 16-23 |
| 10 | constant | `0x00` in every reviewed path |
| 11 | `0x8469ec + 0xae/+0xaf` | first packed 12-bit sensor pair (already verified) |
| 12 | packed | `(a & 0xff)` / nibble crossing (already verified) |
| 13 | `0x8469ec + 0xb0/+0xb1` | `b >> 4` (already verified) |
| 14 | `0x847544 + 0x46` | low byte of gated halfword A, else `0` |
| 15 | gated packing | `((a>>8)&0xf) \| ((b&0xf)<<4)`, else `0` |
| 16 | `0x847544 + 0x48` | `b >> 4`, else `0` |
| 17 | `0x847544 + 0x45` | `3` if that byte is `1`, else `1`; `0` when `+0x44 != 1` |
| 18 | constant | `0xfe` |
| 19 | constant | `0x00` |

Instruction anchors for the prefix: axis stores `0x10c04..0x10c12`; bitmap
bytes `0x10c14..0x10c22`; byte 10 `0x10c24..0x10c26`; packing `0x10c28..0x10c54`;
gate `0x10c56..0x10c5e`; zero fill and `FE 00` `0x10c60..0x10c6e`; gated branch
`0x10c80..0x10cb6`; send `0x10c70..0x10c74`.

### 2.1 Axes (bytes 0-3) are the same axes as HID bytes 1-4

`FUN_00010ce4` copies eight bytes from `0x8469ec + 0x0e` and then, at
`0x10d30..0x10d46`, does `byte - 0x80` on the bytes at copied offsets
`0,2,4,6` and stores them as HID payload bytes 0-3. So:

* NUS axis byte = the raw low byte of the same 16-bit axis word, **unsigned,
  offset-binary** (rest near `0x80`, full scale `0x00..0xff`).
* HID payload axis = `nus_axis - 0x80` (mod 256), i.e. the signed 8-bit value
  the existing `driver/src/input.rs` stores.

Static equivalence, no scaling or clamping is applied by either builder to these
bytes; the 16-bit words they come from are produced by `FUN_0000a704`
(`0xa704`, stores at `0xe..0x15`) and are not re-scaled here.

### 2.2 Button/hat bitmap (bytes 4, 5, 9, 8)

Bytes 4/5/9/8 are the 32-bit word at `0x8469ec + 0x02` split little-endian, with
the halves re-ordered and the top bit dropped:

```
bitmap = nus[4] | (nus[5] << 8) | (nus[9] << 16) | ((nus[8] & 0x7f) << 24)
```

The same word is the input to `FUN_00010a30` (`0x10a30`), the HID button packer
called from `FUN_00010ce4`. That gives the bit meanings in NUS coordinates:

| Bitmap bit | NUS position | Meaning (per HID report layout) |
| ---: | --- | --- |
| 0 | byte 4 `0x01` | D-pad up |
| 1 | byte 4 `0x02` | D-pad right |
| 2 | byte 4 `0x04` | D-pad down |
| 3 | byte 4 `0x08` | D-pad left |
| 4 | byte 4 `0x10` | A |
| 5 | byte 4 `0x20` | B |
| 6 | byte 4 `0x40` | Select |
| 7 | byte 4 `0x80` | X on the installed controller; original static label inference Y was reversed |
| 8 | byte 5 `0x01` | Y on the installed controller; original static label inference X was reversed |
| 9 | byte 5 `0x02` | Start |
| 10 | byte 5 `0x04` | LB |
| 11 | byte 5 `0x08` | RB |
| 12 | byte 5 `0x10` | LT digital |
| 13 | byte 5 `0x20` | RT digital |
| 14 | byte 5 `0x40` | L3 |
| 15 | byte 5 `0x80` | R3 |
| 16 | byte 9 `0x01` | C |
| 17 | byte 9 `0x02` | Z |
| 18 | byte 9 `0x04` | M1 |
| 19 | byte 9 `0x08` | M2 |
| 20 | byte 9 `0x10` | M3 |
| 21 | byte 9 `0x20` | M4 |
| 22 | byte 9 `0x40` | unknown |
| 23 | byte 9 `0x80` | unknown |
| 24 | byte 8 `0x01` | unknown |
| 25, 26 | — | not tested by the HID packer |
| 27 | byte 8 `0x08` | Home |
| 28 | byte 8 `0x10` | unknown |
| 29, 30 | byte 8 `0x20`, `0x40` | not tested by the HID packer |
| 31 | — | dropped by the builder's `& 0x7f` |

The 15 standard controls, the 6 extras and the four D-pad directions are
therefore all present in the NUS prefix (`0x10a30` maps bits 4-28 to HID
payload bytes 8-11; compare the documented 21-byte report map in
`driver/src/input.rs`). Bits 22, 23, 24 and 28 are carried but not decoded by
that packer, so either they are unused or they are controls the current project
does not know; this note does not guess.

**Important encoding difference.** The NUS stream carries the *pre-encoding*
bitmap: one bit per D-pad direction, so diagonals are simply two bits set. The
HID report carries the *packed* hat value (1, 3, 5, 7 for up/right/down/left and
2, 4, 6, 8 for diagonals) produced by `FUN_00010a30`. A decoder must not feed
`nus[4] & 0x0f` straight into a HID hat field.

### 2.3 Analog triggers (bytes 6, 7)

`0x8469ec + 0x36` / `+0x37` are the two channels that `FUN_0000a704` clamps to
`0..255` (`0xa968..0xaa20`) from analog channels 4 and 5, and they are the same
bytes `FUN_00010ce4` places at HID payload bytes 12/13, which the documented
report reads as left/right trigger level (host bytes 13/14). So NUS byte 6 =
left trigger raw level, byte 7 = right trigger raw level, `0..255`, no centring,
no gating. The corresponding "pressed" flags remain bits 12/13 of the bitmap.

### 2.4 Bytes 14-17 (out of scope, recorded for completeness)

Gated by `0x847544 + 0x44 == 1`; when false all four bytes are zero. When true,
bytes 14-16 are a second 12-bit pair with the same packing as 11-13, sourced
from halfwords at `0x847544 + 0x46` and `+0x48`, and byte 17 is `1` or `3`
according to `0x847544 + 0x45`. These are not ordinary buttons: their sources
are in the BLE/feedback state block, not the input state. The values' meaning,
and whether `+0x44`/`+0x45` are set in a normal DInput session, are unresolved.

### 2.5 Edge case: calibration/diagnostic flag

`FUN_0000bafc` (`0xbafc`) returns true when `0x846ac8 + 0x76 != 0` (the flag
already documented in `FUN_0000a704`). When it is true, `FUN_00010ba4`
(`0x10be6`, branch at `0x10d98`) re-copies the axis block but forces bytes
**4-9 to zero** and byte 10 to zero, leaving axes, 11-17 and `FE 00` intact. A
decoder must therefore tolerate an all-zero button/trigger prefix and should not
treat it as "all buttons released" without knowing that flag. The same flag also
zeroes the HID axes/trigger bytes in `FUN_00010ce4` (bytes 12/13) — reviewed in
that function's decompilation.

---

## 3. What a clean-room implementation can do with this

For a device known to be in Bluetooth DInput (mode 1) and with the AC selector
nonzero, the NUS 20-byte object alone carries every non-motion control the HID
report carries:

* 4 axes: `nus[0..3]`, unsigned offset-binary, `0x80` centre; the signed value
  used by the documented HID decoder is `nus[i].wrapping_sub(0x80)`.
* full 32-bit button/hat bitmap reassembled as in 2.2, including the D-pad as
  four independent direction bits and the six extras.
* left/right trigger analog levels `nus[6]`, `nus[7]`, plus their digital flags
  at bitmap bits 12/13.
* the two motion values at 11-13 already covered by the earlier documents.
* byte 10 is a constant zero and bytes 18/19 are the `FE 00` trailer; they carry
  no input.

This is consistent with the runtime observation that the same session showed
motion in Steam while the physical HID input report was absent: the stream that
remains contains the buttons/sticks/triggers too, they were simply not being
decoded.

## 4. Observed vs inferred vs unknown

**Observed (instruction/decompile level, same image hash).** The scheduler
branch table in section 1; the selector byte `0x8472bc` written by `AC` and
cleared at init; the two HID-sender call sites; the byte-0..10 sources, order,
masks and the `FE 00` trailer of the NUS object; the `-0x80` HID axis
conversion; the `FUN_00010a30` bitmap-to-HID mapping; the `0..255` trigger
clamp; the `bafc` zeroing path; the `0x847544` gate for bytes 14-17.

**Inferred.** That the 32-bit word at `0x8469ec + 0x02` is the complete button
state in every mode (proven for the mode-1 HID report by the byte-for-byte
agreement with the coordinated captures, not for every mode). That the NUS
prefix is what the vendor application or another host consumer would use for
buttons — the firmware only proves the bytes are there.

**Unknown / not closed here.** Which physical controls bits 22, 23, 24, 28 are;
the meaning of bytes 14-17; whether `0x847544 + 0x44` is set on a live unit;
whether the live device runs this exact revision; the queue gate
`FUN_00002dd8` identity; the layer that supplies the HID report ID `0x01` on the
host side (the builder writes a 20-byte payload beginning with an axis, while
the host capture shows a 21-byte report with ID `0x01`; the report reference
descriptor at image `0x24aa4` is `01 01`, so the ID is consistent, but where it
is prepended is not proven); and any claim about physical axis sign, scale or
sensor identity, which remain hardware questions.

## 5. Bounds of the search

Only `FUN_00008818`, `FUN_00010560`, `FUN_000106xx`-adjacent NUS paths already
documented, `FUN_00010ba4`, `FUN_00010ce4`, `FUN_00010dc0`, `FUN_00010a30`,
`FUN_0000a704`, `FUN_00009d20`, `FUN_0000bafc`, `FUN_00010300`, `FUN_000102e4`
and the GATT table entries `0x10-0x2a` were inspected. The call-graph bounds
are the saved corrected export plus read-only disassembly of the listed
functions; indirect calls, ROM/library code, other scheduler callers and
revisions other than `6.7.5.1` were not covered. No hardware behavior is
claimed. The analyzed image's SHA-256 was unchanged after the pass.

## Live correction: physical X and Y labels

In a subsequent owner-coordinated passive capture during the candidate daemon
run with SHA-256 prefix `db1368d855049d3b`, physical X held for two seconds
produced 268 data notifications; all had byte 4 masked by `0xf0` equal to `0x80`
and byte 5 masked by `0x03` equal to zero. Physical Y held for two seconds
produced 269 data notifications; all had byte 4 equal to zero and byte 5 masked
by `0x03` equal to `0x01`. The owner observed X as Triangle and Y as Square
with the initial implementation. The original static inference of those two
physical labels was reversed. This does not establish a firmware change or the
installed firmware revision; it corrects the mapping for the observed device.
