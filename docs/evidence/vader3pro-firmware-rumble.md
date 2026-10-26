# Motor / rumble path in Vader 3 Pro firmware

Static analysis of the official main-MCU image; nothing was sent to hardware.
Companion to `vader3pro-firmware-disassembly.md` (tooling, the `tnegs` p-code
bug, and the general method are documented there).

Artifact: `Telink87_For_F3_Ver6.7.5.1.bin`, SHA-256
`6cba30540a0c3ba16acad2b62bb4dd64dfccc28add04914c3720c6ae6a53e20a`.

Summary: the firmware **does** implement rumble, and the Windows `05 0F <L> <R>`
command reaches it — but through a different parser than the `0xEE` vendor-mode
command, and with four motor slots rather than two. The motors are ordinary
PWM outputs; there is no HID output report and no FF descriptor involved.

## Entry point: `0x0F` is parsed outside the main HID dispatcher

This is the most important structural finding, because it means an
implementation cannot assume "one command parser".

`FUN_00018138` (the dispatcher described in the disassembly doc) handles
`0xE0`, `0xE2`, `0xE3`, `0xEB`, `0xEE`, `0xEF`, `0xF4`, `0xA0`, `0x22`, `0x33`.
**`0x0F` is not among them** — verified by checking that no comparison against
`-0x71` exists in that function.

Rumble is instead handled in `FUN_0001a0cc`, which is reached from
`FUN_0001a148` → `FUN_0001c920`, the frame-arrival handler:

```c
void FUN_0001a0cc(void)
{
  if ((0xb < *DAT_0001a138) && (*DAT_0001a13c == '\x05')) {
    cVar1 = DAT_0001a13c[1];
    if (cVar1 == '\x0f') {                              /* 0x0F */
      FUN_0000b62c(500, DAT_0001a13c[2], DAT_0001a13c[3]);
      FUN_0000b6a4(500, pcVar2[4], pcVar2[5]);
      *(undefined1 *)(DAT_0001a144 + 0xdb) = 1;
    }
    else if (cVar1 == '\x10') { /* ... */ }
    else if (cVar1 != '\x11') { FUN_00018138(DAT_0001a13c + 1, 1); }
  }
  return;
}
```

Read literally, that is:

- Frame must start with `0x05`, and the received length must be **> 11** bytes.
- **bytes 2 and 3 → motor pair A** (`0x0F`'s `<left>`/`<right>` in the Windows
  builder).
- **bytes 4 and 5 → motor pair B** — a second pair that the Windows
  application never populates, because `getTestMotorCommand` allocates
  `perPackageLength + 2` = 15 + 2 bytes and only writes indices 0–3. Indices
  4 and 5 therefore arrive as zero, which the firmware treats as "pair B off".

So the wire command is really **`05 0F A1 A2 B1 B2 …`**, and the 4-byte form
documented from the Windows builder is the special case where the second pair is
zero. Whether B1/B2 drive the same physical motors or a second pair (e.g. trigger
motors on a different model) is not determinable from this image alone; the
code writes them into distinct slots.

Note also that the handler is *length-gated at > 11*, but only bytes 2–5 are
read. No additive-checksum verification was found on this path: the only
sum-and-compare loop in the image (`FUN_00000200`) operates on an outbound
frame that starts `0x5A 0xA5`, not on inbound `0x05` commands. A sender that
omits the checksum byte should still be accepted as long as the length is right.
Do not generalise this to other handlers without checking each one — the `0xEE`
handler also shows no verify, but a shared verify routine was not conclusively
ruled out for the transport layer as a whole.

## From command bytes to PWM duty

Two near-identical setters take `(duration_ms, left_level, right_level)`:

```
FUN_0000b62c(duration, a, b) -> FUN_0000b5d0(0, duration, a, b)   /* pair A */
FUN_0000b6a4(duration, a, b) -> FUN_0000b648(0, duration, a, b)   /* pair B */
```

`FUN_0000b5d0` stores the duration as a 16-bit countdown in `state+0x66/0x67`,
stores the raw levels in `state+0x64/0x65`, and mirrors them into the *effective*
level slots `state+0x92/0x93`. It also calls `FUN_00013878(a, b)` to prime the
"current" tracking struct (set `0xFF` marker when either level is non-zero, else
`0`). `FUN_0000b648` is the same shape for pair B, using `state+0x6c/0x6d`,
`state+0x6a/0x6b`, and effective slots `state+0x94/0x95`.

The 500 ms passed by the `0x0F` handler is therefore a **hard timeout**, not an
intensity: the motors stop after 500 ms unless refreshed or re-sent.

`state+0xdb` is set to 1 as a "rumble active" latch.

## Timeout and decay: `FUN_00013970`

Called from the main loop (`FUN_00008a60`, `FUN_0001161c`, `FUN_0001acb0`) every
tick. It decrements both countdowns and recomputes the effective levels through
a curve function:

- While the countdown is non-zero: `effective = curve(level, channel)`.
- When it hits zero: levels are zeroed, `state+0xdb` cleared, and the effective
  slots set to 0.

There is also a special case: when the mode byte `state+0xc == 8`, pair-B
effective levels are forced to 0 unconditionally (`state+0x94 = state+0x95 = 0`),
i.e. in that mode only pair A can rumble.

## The duty curve

`FUN_0000f6a8(level, channel)` converts a 0–255 logical level into a 0–250 PWM
duty. Two things are notable:

- The output stage clamps to **250 (`0xFA`)** in `FUN_0000858c`, so 255 and 250
  are indistinguishable on the wire. The documented "0–255" is really 0–250.
- There are two curve sources. If the device is in the "1"/non-6 mode and
  `state+0x91 == 0`, it uses a per-slot 4-byte profile at
  `state+0x24*4 + channel*4` → `{…, enabled, min, max, gain}` and computes
  `duty = min + level*(max-min)/255 * gain/100`. Otherwise it uses a fixed
  two-entry pair table (`DAT_0000f7c8`) as `min + level*(max-min)/255`.

There is a small non-obvious behaviour around transitions: a 10-tick counter
(`*DAT_0000f7bc` / `*DAT_0000f7c4`) is armed when a non-zero level arrives while
the cached output for that channel was zero. The counters are decremented on
subsequent ticks, but **no code path was found that scales duty by them**, so
their exact effect (soft-start, debounce, or merely a re-arm guard) is not
established. Reproduce only if bit-exact behaviour turns out to matter.

## PWM hardware stage

`FUN_0000858c(channel, duty)` writes the TC32 PWM compare register directly.
The channel argument is not an index — it is the SoC's PWM channel code, and the
code-to-register mapping is a compare chain:

| channel code | compare register | branch |
| --- | --- | --- |
| `0x280` | `0x8007a8` | inverted |
| `0x240` | `0x8007a4` | inverted |
| `0x10` (and any code > `0xF` that is not the above) | `0x80079c` | inverted |
| `0x8` (and any other code ≤ `0xF`) | `0x800798` | inverted for `0x8`, plain otherwise |

Branch selection is `if (channel == 8 || channel > 0xF)` → inverted, else plain.
All four channels this firmware drives (`0x280`, `0x240`, `0x10`, `0x8`) fall in
the inverted branch, so every real motor write is `0xFA - duty`. That is
consistent with active-low drive: a stored `0` must never mean full speed.

This table was re-derived from the raw instruction bytes and literal pool
(`0x860c`, `0x8610`, `0x8614`, `0x8618`) after an earlier draft got it wrong by
conflating the RAM slot addresses with the channel codes. The literal pool uses
a `((site+4) & ~3) + imm8*4` addressing rule here; two different helper scripts
in this session got that scaling wrong and produced nonsense, so the numbers
below are quoted from the verified pass only.

Verified against the literal pool at `0x860c`/`0x8610`/`0x8614`/`0x8618` rather
than from decompiler output, because an earlier draft of this table was wrong:
it had conflated the RAM slot addresses with the channel codes.

The stored value is `(duty & 0x3FFF) << 2 | (0xFA << 18)`. The `0xFA << 18` term
is a constant period/top field; the compare value occupies bits 2–15.

`FUN_00008454(channel, value)` is the init variant: it programs the pin mux via
`FUN_0001b394(channel, func)`, stores the value, then clears the channel's
enable bit in `0x8007b0` using masks `0xf7`/`0xef`/`0xbf`/`0x7f`.
`FUN_000083fc` sets enable bits in `0x800780`; `FUN_00008428` clears them.
`FUN_0000851c` performs bring-up, gated on `state+0xc5` being 0 or 1:

```
FUN_00008454(0x240, 0);      /* off */
FUN_00008454(0x280, 0);      /* off */
FUN_00008454(0x10,  1000);   /* timebase */
FUN_00008454(0x8,   1000);
```

The init variant does not shift the value the way the setter does; it ORs the
`0xFA << 18` top field into the raw value, which is harmless for the 0/1000
values used here.

Bring-up also seeds the four RAM slots with their channel codes, which is what
ties a slot to a register later:

```
0x84660a = 0x240    0x846608 = 0x280
0x846606 = 0x10     0x846604 = 0x8
```

Slot → code → register ← duty, from `FUN_000137f4` and `FUN_00013970` (both
agree):

| slot | code | register | duty byte | pair |
| --- | --- | --- | --- | --- |
| `0x846608` | `0x280` | `0x8007a8` | `state+0x92` | A left |
| `0x84660a` | `0x240` | `0x8007a4` | `state+0x93` | A right |
| `0x846604` | `0x8` | `0x800798` | `state+0x94` | B left |
| `0x846606` | `0x10` | `0x80079c` | `state+0x95` | B right |

## What this means for a Linux implementation

- Rumble is reachable with the **same `05`-framed output write** used for the
  vendor-mode switch. No HID output report, no FF report ID, no descriptor
  support is needed on the host side.
- Send **at least 12 bytes** (the handler requires length > 11), i.e.
  `05 0F L R 00 00` plus padding. The four-byte form from the Windows builder
  (`05 0F L R` padded to 17) satisfies the length gate because the transport
  pads the report.
- Levels are 0–250, not 0–255.
- **The 500 ms timeout is the command's own duration.** Continuous rumble means
  re-sending periodically or relying on the host's own repeat; there is no
  "start until stopped" command on this path. `05 0F 00 00` stops immediately.
- Nothing here proves the Bluetooth transport delivers `05`-framed output
  writes to the firmware. That remains the open question, and it is the same
  question as for `05 EE 01`.

## Verified vs inferred

Verified by reading code, cross-checked against raw instruction bytes:

- `0x0F` is not in `FUN_00018138`; it is handled in `FUN_0001a0cc`.
- Frame gate is `frame[0] == 0x05` and length > 11.
- Bytes 2/3 → pair A, bytes 4/5 → pair B.
- 500 ms countdown, then zero.
- Duty clamp 250; register encoding `(duty & 0x3FFF) << 2 | (0xFA << 18)`.
- Slot → code → register mapping in the table above, re-derived from the literal
  pool after an earlier draft got it wrong.

Inferred, not verified:

- Whether pair B drives physical motors on *this* model. The code paths exist;
  no evidence ties them to hardware in this image.
- Whether the `0x91`-gated per-slot curve is active on a stock Vader 3 Pro. It
  depends on a runtime flag whose writer was not traced.
- All of it over Bluetooth. Nothing here exercises the transport.
- That the checksum is genuinely unvalidated — checked for this path by finding
  no verify loop; a transport-level shared verifier was not conclusively
  excluded.
