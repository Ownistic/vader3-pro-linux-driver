# FN button: static wire contract for the Bluetooth DInput streams

**Artifact:** main image `Telink87_For_F3_Ver6.7.5.1.bin`, 152,068 bytes,
version `6.7.5.1`, SHA-256
`6cba30540a0c3ba16acad2b62bb4dd64dfccc28add04914c3720c6ae6a53e20a`
(bundle `f3_v6.7.5.1.zip`). Re-hashed during this pass; unchanged.

**Repository state:** `a0b40bf3d14e61583cda4592b93cb408b98b3c98`, clean tree.

**Scope.** One question: does the physical FN button reach the button bitmap
that feeds the Bluetooth HID and NUS builders, and at which bit? This note does
not re-analyse motion, transport selection, or the vendor configuration format.
No product code was written. No hardware command, firmware write, or new
disassembler import was performed for this pass. Static work is read-only
against the preserved corrected decompilation
`local/research/scripts/vader3pro-fixed-decompile.c`; literal-pool words were
decoded directly from the hash-identified image with `struct.unpack_from`.

## Short answer

On the tested unit the physical FN button is **logical/config key 24 (`0x18`)**,
which the Windows key table names **Menu**, and it sets **bit 24 of the shared
button bitmap**. There is no table indirection in the normal (non-vendor-mode)
path: the key-history code `0x18` is the bitmap bit index.

| Stream | Offset (0-based) | Mask | Status |
| --- | ---: | ---: | --- |
| NUS 20-byte object | byte **8** | **`0x01`** | **Hardware-confirmed** (isolated FN toggles `byte8 & 0x01` and returns to zero) |
| HID 21-byte input report | byte **11** (report ID `0x01` is byte 0) | **`0x80`** | **Static**, from the same bitmap word; not observed in the AC/NUS capture |
| HID 21-byte input report | byte 12 | `0x01` | **Not FN.** That is bitmap bit 22, config index 22, app name M5 |
| NUS 20-byte object | byte 9 | `0x40` | **Not FN.** Bit 22, neutral during the FN capture |

In firmware terms, FN is `0x01000000` in the 32-bit word at `0x008469ee`
(`0x008469ec + 2`). The NUS builder carries that word's bits 24-30 in
`nus[8] & 0x7f`, so bit 24 is `nus[8] & 0x01`. The HID packer
`FUN_00010a30` writes bit 24 to payload byte 10 (`local_38[10] |= 0x80`),
which is report byte 11 after the `0x01` report ID.

Both target bits are free in the current decoder: `driver/src/input.rs` reads
`report[11]` masks `0x01`-`0x20` and does not read `0x80`, and the NUS parser
ignores `nus[8] & 0x01` (see the capture note). No existing field needs to be
reinterpreted to expose FN.

## Static evidence chain

### 1. FN enters the bitmap as bit 24 from a main-MCU GPIO, not the key frame

`FUN_0000c214` (`0xc214`, decompiled lines 8232-8257):

```c
puVar1 = DAT_0000c274;                 /* 0x00846078 */
if (*DAT_0000c270 != '\0') {           /* 0x0084633e gate */
    *puVar1 = 0;
    if (... *DAT_0000c27c << 0x1c is non-negative) *puVar1 |= 0x08000000;  /* bit 27 */
    if (... *DAT_0000c27c << 0x1f is non-negative) *puVar1 |= 0x01000000;  /* bit 24 */
}
```

Literal-pool values decoded from the image:

| Pool offset | Value | Meaning |
| ---: | ---: | --- |
| `0xc270` | `0x0084633e` | gate byte |
| `0xc274` | `0x00846078` | special-key injection word |
| `0xc27c` | `0x00800598` | GPIO input byte |

The two shifts test GPIO bits 3 and 0, active low: bit 3 low sets bitmap bit 27,
bit 0 low sets bitmap bit 24. Bit 27 is the hardware-observed Home key (Home is
`byte12 0x80` in the HID report and `nus[8] 0x08`). The other assignment, bit 24,
is therefore the second directly wired special key, and the chord and NUS
evidence below identify it as FN.

This function is called from the main loop (`FUN_00008a60` at `0x8a60`, call at
decompiled line 3712) and from two other paths, so the special-key sample is
periodic. It clears `0x00846078` first and re-ORs it on every call.

### 2. The 3-byte key frame cannot supply bit 24

`FUN_0000c4c0` (`0xc4c0`, lines 8400-8439) receives `param_1 + 4` from the
`0x30`/type-6 frame handled by `FUN_00011800` (`0x11800`, lines 13924-13992).
It concatenates exactly **three** bytes:

```c
uVar5 = param_1[0] | (param_1[1] << 8) | (param_1[2] << 16);
uVar5 |= *DAT_0000c528;                /* 0x00846078, bits 24/27 injection */
...
*(char *)(iVar4 + 2) = (char)uVar5;    /* iVar4 = 0x008469ec */
```

So the key frame supplies bitmap bits 0-23 only. Bits 24-31 can only come from
the `0x00846078` injection word. A search of the image for the literal
`0x00846078` finds two literal references, the writer in `FUN_0000c214` and the OR
in `FUN_0000c4c0`. This bounded literal search does not exclude indirect accesses.
In the traced path, bit 24 comes from the GPIO special key, not the key frame.

### 3. The same bitmap feeds the chord history and both report builders

After the merge, `FUN_0000c280` (`0xc280`) applies direction-conflict cleanup
to bits 0-3 (it never touches bit 24), then `FUN_0000c364` (`0xc364`, lines
8328-8397) rebuilds the 10-entry key
history at `0x00846930`: it enumerates set bits of the word at `0x008469ee`
ascending, stores the indices at `+2..+0xb`, and stores the count at `+0xc`.
The history is a **snapshot of held keys**, not a press/release event log.

`FUN_0000b128` (`0xb128`, lines 6832-6859) reads that count and codes and
returns true when at least three entries match any of its three code arguments.
`FUN_0000b32c` (`0xb32c`) calls it with `0x18, 0x12, 0x13`; because each set bit
appears once, all three codes must be held at the same sample. This is the
matcher the earlier FN+M1+M2 hardware test fired. `FUN_0000c59c` (`0xc59c`)
runs the matchers only when keys are held (`0x0084693c != 0`) and the low word
of the tick counter at `0x00846930` equals `0xf0` (line 8517), so the chord is
sampled periodically, not edge-triggered.

### 4. HID and NUS pack the same bit

`FUN_00010ce4` (`0x10ce4`, lines 13162-13224) loads the 32-bit word from
`0x008469ec + 2` and calls:

```c
FUN_00010a30(uVar5, local_38 + 0xb, local_38 + 10, local_38 + 9, local_38 + 8);
```

`FUN_00010a30` (`0x10a30`) tests bit 24 with `param_1 << 7` and executes
`*param_3 |= 0x80` (lines 13062-13064). `param_3` is `local_38 + 10`; the
20-byte payload is sent with `FUN_000102e4(local_38, 0x14)` and the report ID
`0x01` precedes it. Bit 24 is therefore **report byte 11, mask `0x80`**.

`FUN_00010ba4` (`0x10ba4`) builds the 20-byte NUS object from the same word.
The instruction trace recorded in the NUS note (bitmap stores at
`0x10c14..0x10c22`) reads `state+5 & 0x7f` into output byte 8, i.e. bits 24-30.
Bit 24 is the LSB of that byte, so FN is **NUS byte 8, mask `0x01`**.

## Hardware evidence

The coordinated passive Bluetooth capture is recorded separately in
[the capture note](fn-button-capture.md). It observed **18,152 valid 20-byte NUS
samples** over about 136 seconds: 17,739 neutral, 392 with `byte8 & 0x01` (FN
held) and 21 with `byte5 & 0x04` (the independent LB marker). Every FN pulse
returned to neutral. The recording contains **four** FN-level pulses while the
I reported three presses; the discrepancy is retained and this note does not
claim exactly three validated presses. All other button fields were neutral
during the FN pulses, including `NUS byte 9` (bits 16-23). No ordinary HID input
reports occurred while the NUS stream was active, matching the static scheduler
result that the selector moves the build onto the NUS-only branch.

Tested artifacts for that capture: running daemon `616d9e1b5f2e808561ea2213271e0c5db6136525cbce031caffd6a501da4c715`,
isolated probe `183afdc621e72414e2c7dbb96839e4eec0dfc6bce8064d17c5d52c4cc023e519`,
raw capture `21afe2d7410843b3a6f3e666a58e38bc3121cd0f40a60339568c34d8ba983412`.
The capture is private; no raw controller data is added to Git.

That capture directly establishes **FN = NUS byte 8 `0x01` = bitmap bit 24**.
It does not by itself observe the HID byte, because no HID report was sent in
that mode. The HID byte 11 `0x80` claim is static, anchored to the same bitmap
word and to the instruction-level `FUN_00010a30` mask.

## Why not "flag 22" / HID byte 12 `0x01`

The earlier HID table labelled bit 22 "Menu", bit 24 "M6" and bit 28 "M5". That
naming is inconsistent with the application's canonical 32-key order, which is
`16 C, 17 Z, 18 M1, 19 M2, 20 M3, 21 M4, 22 M5, 23 M6, 24 Menu, 27 HOME,
28 BACK` (`docs/evidence/space-station-static-analysis.md`, key-table section).
The mask column of the old HID table is instruction-derived and remains correct;
the labels for bits at and above 22 are not.

Three independent facts put FN at bit 24, not bit 22:

1. `FUN_0000c214` sets only bit 24 (and bit 27 for Home) from the GPIO sample.
   It never sets bit 22.
2. The 3-byte key frame supplies bits 0-23, but FN is a main-MCU GPIO key and
   is not part of that frame.
3. The NUS capture lit only `byte8 & 0x01` and left `byte9` (bits 16-23)
   neutral, so bit 22 was not set while FN was held.

Because one bitmap word feeds both builders, a bit that is clear in the NUS
object is also clear in the HID payload. FN therefore produces HID byte 11
`0x80`, not HID byte 12 `0x01`.

## Press/hold behaviour

- FN is **level state**, not a release-only event. It is present in the bitmap
  while held and absent after release.
- NUS streams continuously; HID sends a report only when one of the 14 payload
  bytes changes (`FUN_00010ce4` compares against the previous payload and calls
  the sender only when different). A host sees one press report and one release
  report per FN tap.
- The FN chord matcher needs all three codes held at the sampled instant
  (`0x18`, `0x12`, `0x13`); it is not a "combined keypress mask" and it is not
  triggered on release.
- Firing a chord does not clear FN from the bitmap. The mode-change handlers act
  on other state and leave the input word intact.

## Observed / inferred / unknown

**Observed (static, same image hash).** The GPIO-to-bit24 injection in
`FUN_0000c214`; the two literal references to `0x00846078`; the 3-byte frame width in
`FUN_0000c4c0`; the held-key history build in `FUN_0000c364`; the matcher in
`FUN_0000b128`; the periodic `0xf0` sample gate in `FUN_0000c59c`; the
bit-24-to-HID mask in `FUN_00010a30`; the NUS byte-8 packing in `FUN_00010ba4`.

**Observed (hardware, coordinated capture).** Isolated FN toggles NUS
`byte8 & 0x01` and returns to zero across 18,152 NUS samples; other button
fields neutral; no HID report while the NUS path was active; four pulses
observed against the three presses I reported.

**Inferred.** HID report byte 11 `0x80` for FN, from the shared bitmap and the
static packer. That the gate byte `0x0084633e` is satisfied in normal DInput:
the isolated-FN capture shows the injection path active, but the gate's full
semantics were not traced.

**Unknown / not closed here.** A direct HID-mode capture with FN held and the AC
selector zero (this would turn the HID byte inference into an observation).
Whether a host-issued vendor-mode flag (`0x2a`) remap changes bit 24; that path
writes a different word and was not traced. The installed unit's firmware
revision is not independently proven, though the observed NUS bit matches this
image's static path.

## Checklist

| Question | Status |
| --- | --- |
| 1. FN logical code and scan mapping | **Answered.** Code 24 (`0x18`), app name Menu; GPIO `0x800598` bit 0 into bitmap bit 24; NUS confirms. |
| 2. Does FN survive local handling to the bitmap/HID/NUS? | **Answered for NUS (observed).** HID inferred from the shared word; no HID in the AC capture. |
| 3. Exact HID/NUS byte and mask | **Answered.** NUS byte 8 `0x01`; HID byte 11 `0x80`. HID byte 12 `0x01` is bit 22/M5, not FN. |
| 4. Release-only, combined mask, or press-hold? | **Answered.** Held level state; chord matcher samples simultaneous holds; no release-only path. |
