# Vader 3 Pro firmware disassembly (TC32)

Static analysis of the official main-MCU image. Nothing was run on hardware and
no firmware was flashed. This document records only what the decompiled code
shows, and separates that from what the Windows application implies.

Artifact: `Telink87_For_F3_Ver6.7.5.1.bin`, 152068 bytes, SHA-256
`6cba30540a0c3ba16acad2b62bb4dd64dfccc28add04914c3720c6ae6a53e20a`
(see `vader3pro-firmware-acquisition.md` for how it was obtained).

## Tooling, and one bug worth recording

The SoC is a Telink TLSR87-class part: a 32-bit TC32 core with a 16-bit program
counter and Thumb-like 16-bit instructions. Stock Ghidra has no TC32 processor
module, so this needed outside help.

- `rgov/Ghidra_TELink_TC32` provides a SLEIGH specification but **no p-code**.
  It disassembles, yet analysis finds zero functions and the decompiler has
  nothing to work with.
- `trust1995/Ghidra_TELink_TC32` (commit `f466dac`) adds p-code semantics and a
  calling convention. This is the one that works: 780 functions, all of which
  decompile cleanly.
- A second community fork, `scriptkitz`, adds partial 32-bit opcode support.

Two local changes were required and are worth knowing about if this is redone:

1. **Address space.** The upstream spec declares `ram_space size=2`, so Ghidra
   clips the 152 KB image at 64 KB. Widening it to `size=4` and renaming the
   language to `Telink_TC32_PC:LE:32:default` makes the whole image
   addressable. File offsets then equal Ghidra addresses one-to-one, which
   makes every address below directly checkable against the binary.
2. **A genuine p-code bug.** `tnegs` (opcode `0x0009`) is implemented as
   *bitwise* NOT (`reg_0_3 = ~reg_3_3`) when the instruction is *arithmetic*
   negation. Anything that sets up `0x80` and then negates it comes out as
   `0x7f`. The bit-expander below writes button masks exactly this way, so the
   first decompilation showed `*param_5 |= 0x7f` where the raw bytes are
   `tmovs r5,#0x80; tnegs r5,r5; tors r5,r6` — i.e. `0x80`. Changing `~` to `-`
   fixes it and all masks become correct. **This is a live bug in an upstream
   fork, not a quirk of this firmware.** Any conclusion drawn from that fork's
   decompiler output about negative constants should be re-checked against the
   disassembly until it is fixed.

Reproduce:

```sh
git clone https://github.com/trust1995/Ghidra_TELink_TC32.git
# widen ram_space to size=4, rename language/spec files, fix tnegs to arithmetic negation
$GHIDRA/support/sleigh Telink_TC32_PC.slaspec Telink_TC32_PC.sla
# install as an extension under $USER/.config/ghidra/<ver>/Extensions/
$GHIDRA/support/analyzeHeadless <projdir> <proj> \
  -import Telink87_For_F3_Ver6.7.5.1.bin \
  -processor "Telink_TC32_PC:LE:32:default" \
  -loader BinaryLoader -loader-baseAddr 0x0
```

## The command dispatcher

`FUN_00018138` is the inbound vendor-command handler. It reads a 1-byte
subcommand from `param_1[0]` and branches over a fixed comparison chain whose
values match, exactly, the subcommand bytes that
`LibCommon.Command.DongleCommand` builds on the Windows side:

| byte | firmware action | Windows builder |
| --- | --- | --- |
| `0xE0` | RGB test | `getTestRGBCommand` |
| `0xE2` | joystick reset, `param_1[1]` selects variant | `getTestJoystickResetCommand` |
| `0xE3` | LED | `getTestLedCommand` |
| `0xEB` | config id / read config | `getReadConfigCommand` |
| `0xEE` | **vendor ("origin") mode flag** | `getSwitchOriginData` |
| `0xEF` | flash / update | `getUpdateCommand` |
| `0xF4` | DInput restart | `getDInputRestartCmd` |
| `0xA0` | trigger params | `SetTriggerParams` |
| `0x22`, `0x33` | `FUN_00017fc0`, `FUN_0001807c` | – |

Branch targets are computed from `addr + 4 + offset*2` (the Thumb-style
pipeline bias), not `addr + 2`; using the wrong bias lands two bytes off and
desynchronises every following instruction.

The `0xEE` case is tiny and does exactly one thing:

```c
else if (cVar1 == -0x12) {              /* 0xEE */
    if (param_1[1] == '\x01') *(undefined1 *)(DAT_000183bc + 0x2a) = 1;
    else                      *(undefined1 *)(DAT_000183bc + 0x2a) = 0;
}
```

`DAT_000183bc` resolves to RAM `0x00846ac8`, the main device state block. So
vendor mode is **one flag byte at `state+0x2a`**, set by `05 EE 01` and cleared
by `05 EE 00`, with no handshake and no reply. The flag is consulted when the
input report is assembled (`FUN_0001a868`): when set, reporting is routed
through the key-mapping path (`FUN_0000094c`); when clear, the plain report
path runs. This matches the Windows behaviour and confirms the command is a
runtime toggle rather than a boot mode.

## Independent confirmation of the DInput byte map

`FUN_00010ce4` builds the normal DInput input report. It copies 8 bytes from
`state+0xe` (the two sticks plus two more bytes), copies 2 bytes from
`state+0x36`, then calls the bit expander `FUN_00010a30`, which takes the
32-bit button flag word and scatters it into four bytes.

The expander's arguments are the *addresses of* payload bytes 8–11, so:

| argument | payload index | wire byte |
| --- | --- | --- |
| `param_5` | 8 | 9 |
| `param_4` | 9 | 10 |
| `param_3` | 10 | 11 |
| `param_2` | 11 | 12 |

Decoding each `if ((int)(param_1 << N) < 0)` to "bit `31-N`" and reading the
mask it writes gives this table. It is derived from the firmware alone:

| flag bit | wire byte | mask | control |
| --- | --- | --- | --- |
| 4 | 9 | `0x10` | A |
| 5 | 9 | `0x20` | B |
| 6 | 10 | `0x10` | Select |
| 7 | 9 | `0x40` | X |
| 8 | 9 | `0x80` | Y |
| 9 | 10 | `0x20` | Start |
| 10 | 10 | `0x01` | LB |
| 11 | 10 | `0x02` | RB |
| 12 | 10 | `0x04` | LT |
| 13 | 10 | `0x08` | RT |
| 14 | 10 | `0x40` | L3 |
| 15 | 10 | `0x80` | R3 |
| 16 | 11 | `0x01` | C |
| 17 | 11 | `0x02` | Z |
| 18 | 11 | `0x04` | M1 |
| 19 | 11 | `0x08` | M2 |
| 20 | 11 | `0x10` | M3 |
| 21 | 11 | `0x20` | M4 |
| 22 | 12 | `0x01` | Menu |
| 23 | 12 | `0x02` | (unused) |
| 24 | 11 | `0x80` | M6 |
| 27 | 12 | `0x80` | HOME |
| 28 | 11 | `0x40` | M5 |

Before that block, the same function writes the D-pad into `param_5`'s low
nibble as a small state machine over bits 0–3, producing `1`/`2`/`3`/`4`/`5`/
`6`/`7` for the eight directions and leaving `0` when no direction bit is set.

**This is the independent proof that the M keys and the D-pad are separate
fields.** Byte 9's low nibble is the hat; M1–M4 are bits 18–21 of the flag word
scattered into byte 11. Every mask in this table agrees with the
owner-coordinated captures recorded in `docs/PROTOCOL.md` — including
`byte 12 0x80 = Home`, which was only ever inferred from a single isolated
press.

One detail that differs from the Windows key table: the flag word places M5 and
M6 at bits 28 and 24, while the config key table places them at indices 22 and
23. Internal ordering is not the config ordering; do not assume they match.

## Why the report is 21 bytes, not 15

This resolves the open discrepancy between the descriptor and the wire.

`FUN_00010ce4` declares `char local_38[20]` and initialises payload indices
`0..0xd` (0–13) only. Indices `0xe..0x13` (14–19) are never written, so they
hold whatever was left there — zero in practice, which is exactly what all
seven captures show for wire bytes 15–20. It then transmits with:

```c
FUN_000102e4(local_38, 0x14);
```

The length argument is `0x14` = 20, so the report on the wire is **20 payload
bytes plus the report ID = 21 bytes**, while the descriptor declares 14 payload
bytes plus ID = 15. The firmware has always sent 21; the descriptor is simply
out of date relative to the payload buffer. There is nothing hidden in the six
trailing bytes — they are uninitialised tail of a 20-byte buffer, and a parser
must neither truncate to the descriptor length nor invent meaning for them.

This also means the earlier instruction to "preserve the six additional bytes
because their meaning is unknown" was the right call for different reasons than
assumed: they are padding, not data.

## Gyro: the DInput formula differs from the XInput one

The vendor (32-byte, `0x04 0xFE`) frame carries angular-rate data that the
normal report does not. The Windows service decompresses it in
`KeyMouDataParse.handleMotionData`, after `XInputDataUtils.FormatDataFromDInput32`
renormalises the frame:

```
normalised[11] = vendor[4]
normalised[12] = vendor[5]
normalised[13] = vendor[6]
normalised[ 2] = vendor[17]    normalised[ 3] = vendor[19]
normalised[ 4] = vendor[21]    normalised[ 5] = vendor[22]
normalised[ 6] = vendor[9]     normalised[ 7] = vendor[10]
normalised[ 8] = vendor[23]    normalised[ 9] = vendor[24]
normalised[10] = vendor[8]     normalised[14] = vendor[3]
```

For DInput (`isXInput == false`, which is the Bluetooth DInput case) the branch
is unconditional and does **not** consult the origin-motion setting:

```csharp
byte b  = readBuffer[11];   // vendor[4]
byte b3 = readBuffer[12];   // vendor[5]
byte b2 = readBuffer[13];   // vendor[6]
num  = b | ((b3 & 0xF) << 8);          // X
num2 = (b2 << 4) | ((b3 & 0xF0) >> 4); // Y
if ((num  & 0x800) != 0) num  -= 4095f; else if (num  > 0) num--;
if ((num2 & 0x800) != 0) num2 -= 4095f; else if (num2 > 0) num2--;
```

Two corrections to the earlier note in `docs/evidence/space-station-static-analysis.md`:

- The 12-bit X/Y packing used for DInput is **not** the XInput "new motion"
  form. XInput packs `X = b | ((b6 & 0xF0) << 4)` and
  `Y = b2 | ((b6 & 0x0F) << 8)`; DInput splits the nibbles the other way and
  shifts Y's low byte into place. Implementations must use the branch that
  matches the transport.
- There is a `else if (value > 0) value--` calibration quirk after the sign
  check. It subtracts one from every positive reading. It is almost certainly
  an off-by-one in the vendor code, but reproducing it exactly is the only way
  to match Space Station's cursor behaviour.

The firmware contains a matching 12-bit two-axis packer (`FUN_0001a6d0`), which
writes three bytes from two 12-bit values:

```c
puVar6[0x10] = (char)uVar3;
puVar6[0x11] = (char)uVar4;
puVar6[0x12] = (byte)(((uint)uVar4 << 0x14) >> 0x1c)
            | (byte)(((int)((uint)uVar3 << 0x10) >> 0x18 & 0xffffU) << 4);
```

That packer's nibble order is its own (X high bits in the upper nibble of the
third byte, Y high bits in the lower), so it should be treated as evidence that
the device packs 12-bit motion in three bytes — **not** as confirmation of the
exact vendor-frame offset or nibble layout seen host-side. The host-side
mapping above is what the application actually relies on; the firmware side of
the 32-byte vendor frame has not been traced end to end.

## What this establishes, and what it does not

Established by firmware code, independent of the Windows application:

- The `0xEE` vendor-mode command sets one RAM flag and nothing else.
- The DInput report's complete button map, including that M1–M4 are distinct
  bits in byte 11 and the D-pad is a hat in byte 9's low nibble.
- The report is 21 bytes because the firmware sends a 20-byte buffer; the
  15-byte descriptor is stale, and bytes 15–20 are uninitialised padding.

Still not established:

- Whether this unit accepts `05 EE 01` over Bluetooth HID-over-GATT, and what
  it then emits. All of the above is firmware-level truth about the code path;
  it says nothing about whether that path is reachable over this transport.
- The exact firmware-side layout of the 32-byte vendor frame, and therefore
  whether the host-side gyro offsets hold on Bluetooth.
- Rumble. No motor/PWM command handler has been traced yet; the `0x0F`
  subcommand seen in the dispatcher chain is not among the traced handlers.
- The installed firmware version of the physical unit.

## Method notes

- Decompiler output was never trusted where a mask or constant mattered;
  disputed values were re-checked against the raw instruction bytes. The
  `tnegs` bug above is why.
- The decompiled corpus (780 functions, ~31k lines) and the Ghidra projects
  live outside Git in `/tmp/opencode/`. Nothing proprietary is copied into this
  repository; the tables above are derived facts, not extracted code.
