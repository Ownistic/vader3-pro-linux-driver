# Bluetooth command reference for the Vader 3 Pro

Consolidated from the firmware (6.7.5.1) and from captures of the mobile
application and of the driver's own traffic. Every entry is marked either
**verified on hardware** or **static only**.

Transport: NUS `6e400002` (write) / `6e400003` (notify). All values are unescaped
bytes; no `05` prefix (that prefix is a USB-only framing byte).

## How a write is routed

`FUN_00010980` dispatches on payload length:

| Length | Handler | Notes |
| --- | --- | --- |
| 4 | six short handlers, then `FUN_0000b830` | the main command surface |
| 20 | `FUN_000107a0` | checksummed opcodes `_ R S P Q T @ A U` |
| < 5 | short handlers only | `A5` family and `AC` |
| 5–19 | **nothing** | silently dropped |
| > 20 | nothing | |

That table explains several earlier silent failures: a 5-, 12- or 15-byte attempt
never reached any handler.

## Four-byte commands

`FUN_0000b830`, dispatched on `buf[0]`:

| Command | Handler | Effect | Status |
| --- | --- | --- | --- |
| `10 <n> <b> <c>` | `FUN_0000cb94(n,b,c,5)` | remap table write, `n < 6` | static |
| `11 <dur> <lo> <hi>` | `FUN_0000b62c(dur,lo,hi)` | rumble; levels clamp at 250 | **verified** |
| `12 <a> <b> <c>` | `FUN_0000ceb4(a,b,c)` | uncharacterised | static |
| `13 02 ..` | `FUN_0000b4d4()` | uncharacterised | static |
| `13 04 ..` | sets `state+0x9a = 1` | see the reset warning below | static |
| `13 05 01` | `FUN_0000b500()` | transport/mode switch | **verified: bond reset** |
| `13 05 02` | `FUN_0000b53c()` | transport/mode switch | static |
| `13 10 ..` | `FUN_0000b470()` | uncharacterised | static |
| `13 11 ..` | `FUN_0000b484()` | sent by the app before switching to Flashplay | **verified by capture** |

`11 00 00 00` stops rumble. Sustained rumble requires periodic retransmission —
the firmware enforces a 500 ms timeout.

## Warning: `13 04 00 00` reaches a reset sequence

Two dispatchers treat these bytes, and the shorter path is not the flag setter.
`FUN_000105a4` memcmp's the payload against the literal at flash `0x23f90`:

```
13 04 00 00      <- matched literal
a5 a0 00 00      <- the next literal (status query)
```

On a match it calls `FUN_00013dc4()`, which walks the whole peripheral down —
`FUN_000015ac(0x3b,0)`, `FUN_0000d300()`, clock and GPIO teardown, a busy-wait on a
status bit, then an RF shutdown — and `FUN_00013e60()`, which writes registers
`0x308`, `4` and `1` directly.

That is a reset/shutdown-shaped sequence, and it is plausibly the entry point into
the firmware-update mode that the vendor's updater expects. **It has not been
tested.** If it does reboot into a bootloader, it may be the recovery path that
made flashing risky; if it is a hard power-off, it costs a reconnect. Either way it
should be treated as destructive until proven otherwise.

## Short `A5` family

Reached before the four-byte dispatcher, so these are matched on their own:

| Frame | Handler | Effect | Status |
| --- | --- | --- | --- |
| `A5 A0 00 00` | `FUN_00010604` | 13-byte status reply | **verified** |
| `A5 A1 ..` | `FUN_0001067c` | 20-byte query reply | static |
| `A5 01 ..` | `FUN_000105e8` | `FUN_000103e4(0)` | static |
| `AC <flags>` | `FUN_00010560` | key/LED flag bits | static |
| `BA 40 ..` | `FUN_000105cc` | single call | static |

### The status reply is not battery

`A5 A0 00 00` returns:

```
A5 A0 17 | <state+0x9b> | <8 bytes from state+0xd2> | <checksum>
```

It is tempting to read `<state+0x9b>` as a battery percentage — the observed value
was `0x50`, i.e. 80 %. It is not. The status copier writes that byte from a source
value only when the source is valid, and otherwise assigns `0x50` as a **fallback**:

```c
if ((byte)(cVar10 - 1U) < 0xfe)   *(char *)(state + 0x9b) = cVar10;
else if (rate_flag == 1)          *(char *)(state + 0x9b) = 0x1c;
else                              *(char *)(state + 0x9b) = 0x50;   /* default */
```

A constant 80 across every capture is the default, not a charge level. Do not
expose it as battery in the driver. The adjacent byte `state+0x9c` is a charging
flag and is likewise only meaningful when the source is present.

## Twenty-byte checksummed opcodes

`FUN_000107a0`, dispatched on the first byte, with a checksum in the last two bytes:

| Opcode | Handler | Effect |
| --- | --- | --- |
| `_` | `FUN_000104d4(b1,b2)` | uncharacterised |
| `R` | `FUN_00013fc4()` | uncharacterised |
| `S` | `FUN_00014000()` | uncharacterised |
| `P` | — | falls through: the test is `!= 'P'` |
| `Q` | `FUN_00014458(p+3, offset, 0x10)` | 16-byte write at a computed offset |
| `T` | `FUN_00012148()` if `state+0xc == 2` | gated |
| `@` | `FUN_00014424(b1)` | uncharacterised |
| `A` | `FUN_0001444c(b1)` | uncharacterised |
| `U` | `memcpy(0x8472e8, p+3, 16)` | key-remap table write |

`U` looked like an arbitrary write; `0x8472e8` is the 5-byte-stride key-remap
table that `FUN_00010838` walks, so it is a partial remap, not a forwarder.

## What is still uncharacterised

- `12`, `13 02`, `13 10`, and the `AC`/`BA`/`A5 01` handlers: reachable, effect
  unknown. These are the most likely place for dormant capabilities.
- Whether `13 04 00 00` enters a bootloader or simply resets.
- `state+0x9a`, the flag `13 04` sets in the other dispatcher.
