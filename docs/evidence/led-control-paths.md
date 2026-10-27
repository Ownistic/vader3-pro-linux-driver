# LED control (`0x33` and `0x13`): reachability and persistence

Static analysis of firmware 6.7.5.1 for the LED-related command paths, prompted
by the question of whether LED effects could be exposed to Steam. No hardware
was touched.

## Summary

| Path | Purpose | Reachable over Bluetooth | Persistent |
| --- | --- | --- | --- |
| `0x13` selector | LED **mode** (effects 1, 2, 8) | in the proven 4-byte dispatcher, untested | sent to hardware, not flash |
| `0x33` | LED **data** blob, chunked | in the 20-byte dispatcher, untested | **yes — commits to flash** |
| `0xFA` sub `0x60`/`0x62` | single brightness bytes | in the 20-byte dispatcher | no |

The important finding is the persistence column: **`0x33` writes to flash**.

## `0x13` — LED mode

Reached from the same 4-byte GATT frame family that carries rumble
([bluetooth-rumble-solved.md](bluetooth-rumble-solved.md)):

```c
FUN_0000b830(buf, 4):
  buf[0] == 0x13:
      buf[1] == 0x02 -> FUN_0000b4d4   -> FUN_000119d8(8)   LED mode 8
      buf[1] == 0x10 -> FUN_0000b470   -> FUN_000119d8(1)   LED mode 1
      buf[1] == 0x11 -> FUN_0000b484   -> FUN_000119d8(2)   LED mode 2
      buf[1] == 0x04 -> state+0x9a = 1
      buf[1] == 0x05 -> transport mode switch
```

`FUN_000119d8` → `FUN_00011954` (mode selector) → `FUN_00011984` (apply):

```c
FUN_00008284(0x30000);
FUN_00008268(0x30000, DAT_000119ac, 0x97);
```

`0x30000` is a peripheral/base address, not a flash slot, so this changes the
live LED effect without persisting anything.

**Frame:** `13 <mode> 00 00` (4 bytes). Modes confirmed in the selector:
`0x01`, `0x02`, `0x05`, `0x06`, `0x07`, `0x08`.

Note this selects a firmware-defined **effect**, not a colour. There is no
RGB entry point here.

## `0x33` — LED data blob

Reached from the 20-byte path, not the 4-byte one. The app builds it as:

```
05 E7 <configId> <numChunks>              getStartLEDCommand
05 33 <10 bytes> A0 <chunkIndex>          one per chunk, chunk 0..n
```

`FUN_0001807c` is the chunk handler:

```c
if (chunkIndex < state+0x52) {                  /* total chunk count */
    state+0x33 = 10;                            /* chunk payload length */
    memcpy(state+0x36, param_1, 10);            /* 10-byte staging buffer */
    state+0x54 = 1;                             /* "chunk ready" */
    if (param_1[10] == 0xA0) {                  /* explicit index */
        nextIndex = param_1[11] + 1;
        state+0x30 = nextIndex;
    } else {
        nextIndex = state+0x30 + 1;             /* sequential */
        state+0x30 = nextIndex;
    }
    if (state+0x52 <= nextIndex) {              /* last chunk */
        state+0x52 = 0;
        state+0x55 = 1;                         /* REQUEST COMMIT */
    }
}
```

The staged chunk is copied into RAM by `FUN_00018b0c`:

```c
if (state+0x54 == 1) {
    FUN_00018954(state+0x2c, state+0x36);       /* chunkIndex, 10 bytes */
    state+0x51 = 1;
    state+0x54 = 0;
}

FUN_00018954 -> FUN_0000f30c(dst, index*10, 10)
             -> dst = 0x00846fdc, length 0x1f3 (499)
```

So chunks land in a **499-byte RAM buffer at `0x846fdc`**, not flash. The
config equivalent uses `FUN_0001893c` → `0x00846be4`, 789 bytes (`0x315`).

## The commit step — and why `0x33` persists

`state+0x55 = 1` is a commit request. `FUN_0000bb14` services it:

```c
if (state+0x20 == 1 || state+0x55 == 1) {
    if (state+0x35 == 0) FUN_0000f4f0();
    else FUN_0000f4b0(state+0x20, state+0x55);
    state+0x20 = 0;
    state+0x55 = 0;
}

FUN_0000f4b0(config, led):
  if (config == 1) { FUN_000120f4(state+0x12, DAT_0000f4ec); FUN_0000d7b4(); }
  if (led    == 1) { FUN_00012068(state+0x12, DAT_0000f4e8); FUN_0000d0b0(); }

FUN_000120f4(slot, src):                 /* config blob */
    FUN_00008284( *(0x008458d4 + slot*4) );            /* erase */
    FUN_00008268( *(0x008458d4 + slot*4), src, 0x316 );/* program 790 bytes */

FUN_00012068(slot, src):                 /* LED blob */
    FUN_00008268( *(0x008458b0 + slot*4), src, 500 );  /* program 500 bytes */
```

`0x008458d4` and `0x008458b0` are slot base-address tables, and `FUN_00008284`
performs an erase before `FUN_00008268` programs. That is a flash write
sequence, not a RAM copy.

**Therefore finishing a `0x33` chunk sequence commits the LED blob to flash.**
The `0x55` flag can also be raised by the watchdog in `FUN_000186f0` after a
timeout, so a sequence that is started and abandoned may still commit whatever
happened to be in the RAM buffer.

## Why this matters for the daemon

`0x13` (mode) is the attractive path for a driver: small, in the already-proven
4-byte family, and non-persistent.

`0x33` is the opposite: chunked over the 20-byte path, and it ends in a flash
erase/program. Two concrete risks:

1. **Abandoned sequences can still commit.** The watchdog raises `0x55` on
   timeout, writing whatever partial state sits in the RAM buffer.
2. **Slot base addresses come from the device's own table.** Nothing in the
   analysis ties a specific chunk count to the 500-byte region safely; a
   sequence longer than the buffer would write past it. `FUN_0000f30c` bounds
   the write to `param_2 <= 0x1f3`, so it stops at the end rather than
   overflowing, but the last chunk would then be silently truncated.

Neither risk exists on the `0x13` path.

## Steam exposure — the honest position

Steam Input drives a **lightbar colour** through the virtual pad's `EV_LED` or
the DualSense/DS4 lightbar report. This device does not accept a colour buffer
in either command above:

- `0x13` picks a firmware effect by number.
- `0x33` uploads an opaque blob whose internal layout (palette indices, effect
  parameters, per-LED triples) has not been decoded here.

So a driver could expose LED *mode* selection, but could not translate a Steam
lightbar colour into it. Making colour work would require decoding the `0x33`
blob layout from the stock `.fdg` profiles, then accepting a flash-write on
every colour change — which is a poor design for a lightbar that updates at
animation rates.

**Recommendation:** expose `0x13` mode selection as an optional feature, do not
expose `0x33`, and tell the user the lightbar will not mirror Steam colour.

## Open questions

- Whether `0x13` actually works over Bluetooth. Same dispatcher as the proven
  `0x11`, so likely, but a single 4-byte write would confirm it.
- The meaning of each LED mode number (1, 2, 5, 6, 7, 8) — none is documented.
- The internal layout of the 499-byte LED blob.
- Whether `0x33` over Bluetooth reaches `FUN_0001807c` at all, given that it
  needs the 20-byte path and that path's checksum gate.
