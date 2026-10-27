# Reachability: can a Bluetooth write reach the rumble handler?

Static answer to the question left open by
[firmware-patch-scoping.md](firmware-patch-scoping.md): is `FUN_00019fa4`
reachable from a GATT write, or does Option A require a different route?

Analysis of 6.7.5.1. No hardware was touched for this document.

## Result

**Both Bluetooth command handlers are reachable from a radio write.** Which one
runs depends on a runtime transport-mode field, not on the frame contents.

```
radio write buffer (BLE RX FIFO at 0x80011d, length at 0x800115)
  -> FUN_0001c920                copies the frame out of the FIFO
  -> FUN_0001a148(buf)           single argument; length read from 0x84652a
      switch (*(0x845e20))       runtime transport mode
        mode 1 -> FUN_0001a0cc    requires buf[0]==0x05 && len > 11
                     buf[1]==0x0F -> FUN_0000b62c(500, b[2], b[3])   RUMBLE
        mode 2 -> FUN_00019fa4    requires len==8 or len>=15
                     buf[0]==0xA5 && buf[1]==0x12 -> RUMBLE
  -> else FUN_00018138(buf, 0)    general command dispatcher
```

## What sets the transport mode

`0x00845e20` has exactly two writers:

| Writer | Value | Caller |
| --- | --- | --- |
| `FUN_0001a618(x)` | 1 or 2 | `FUN_0000b590` (1), `FUN_0000b550` (2) |
| `FUN_0001a5f8(x)` | copies `state+0xe`, defaults 1 | `FUN_0000b7d4` |

`FUN_0000b550` sets `state+0xe = 2` and calls `FUN_0001a618(2)`. It is triggered
by `state+0x21`, which the main dispatcher sets on subcommand **`0xED`**
(`cVar1 == -0x13` in `FUN_00018138`). `FUN_0000b590` is the mirror image,
triggered by `state+0x22`, set by subcommand **`0xEC`** (`cVar1 == -0x14`) —
oddly, given both handlers are reachable at all.

So the mode is written by a command that itself arrives over the transport being
selected. Mode 1 is the default (`FUN_0001a5f8` clamps anything else to 1).

## The FIFO is the BLE controller block

The copy in `FUN_0001c920` reads:

| Address | Use |
| --- | --- |
| `0x0080011d` | source data (30 bytes after `0x8000ff`) |
| `0x00800115` | length (`pbVar5[-0x24]`) |
| `0x00800139` | status/freshness flag |
| `0x00800102` | mode byte |
| `0x00800103` | interrupt flag |

Only eight functions touch `0x00800100`–`0x00800140`:
`FUN_00012c24`, `FUN_0001c530`, `FUN_0001c6b0`, `FUN_0001c808`, `FUN_0001c894`,
`FUN_0001c8ec`, `FUN_0001c920`, `FUN_0001caac` — a single self-contained
driver, and `FUN_0001c920` is the sole reader of the buffer and length. This is
a hardware packet interface with a data port, a length register and a freshness
flag, not a GATT-level characteristic value.

**Caveat that matters for this project:** whether this block belongs to the
Bluetooth controller or to the 2.4 GHz proprietary link is **not established**.
`FUN_0000d648` calls `FUN_000128d4` (→ `FUN_0001c920`) from several branches of
its transport switch (`state+0xd == 1`, and `state+0xc` in 1, 2, 3, 4, 6), which
suggests a poll shared by more than one transport rather than a Bluetooth-only
path. If this is the 2.4 GHz link, then the Bluetooth command path is not here
at all and the conclusion below is even stronger.

## Correction to an earlier reading

An earlier note in this project described these paths as GATT-level write
handlers. They are not. `FUN_0001c920` is driven at the radio packet level: the
`5, 2, 3, 6, 0xff` byte sequence in `FUN_0001227c` is written into the same
register window the copy loop reads, which is consistent with an RF test or
descriptor setup routine, not a HID report write.

This matters for what a patch would have to intercept: **radio frames, not GATT
writes.** A GATT write arrives through the BLE stack's attribute handling; if
that layer never forwards the payload into this FIFO path, then no frame format
can reach either handler.

## Why the hardware tests produced nothing

Every NUS frame tried so far was aimed at these handlers, and none worked:

| Frame | Length | Handler it would fit | Result |
| --- | --- | --- | --- |
| `BA 70 70 70 70` | 5 | `FUN_00018828` short branch | no effect |
| `A5 12 70 70` + pad | 15 | `FUN_00019fa4` (mode 2) | no effect |
| `05 0F 70 70` + pad | 12 | `FUN_0001a0cc` (mode 1) | no effect |
| `5A A5 …` | 20 | `FUN_000183c8` | no effect |

Under this model the explanation is not framing but path: the frames reach the
GATT attribute table and stop there, never entering the FIFO the handlers read.
That is consistent with the earlier finding that the inbound **command** queue
(`0x00845e14`) is fed only by the wired USB validator — the Bluetooth stack
simply does not forward written payloads into either consumer.

## Consequence for the patch

Option A as scoped — inserting a rumble branch into an already-called function —
**does not stand up**, because the problem is upstream of those functions. A
patch that only edits `FUN_00018828` or `FUN_00019fa4` would add code to a path
the GATT write never enters.

A workable patch has to do one of:

1. **Hook the GATT write handler** in the BLE attribute layer and forward the
   payload into a rumble call. Requires locating that handler in the Telink BLE
   stack, which is only partly mapped here.
2. **Add an output report** to the DInput descriptor so the host has a HID
   channel, then service it. Structurally harder: fixed-size descriptor at
   `0x23764`, attribute table at `0x23808`.
3. **Redirect the notification path**, since `FUN_0000fd7c` already builds and
   queues frames for transmission — the reverse direction demonstrably works.

Option 3 is the most promising lead: the device already has a working
device→host frame builder, so the symmetry suggests there is a paired
host→device entry point that has not been found yet.

## Still unresolved

- Which function in the BLE attribute layer receives a GATT write payload. That
  is the hook point for Option 1 and it is not identified.
- Whether `0x00800100`–`0x00800140` is the Bluetooth controller block or the
  2.4 GHz proprietary RF block. This determines whether any of the analysis
  above applies to the Bluetooth link at all.
- Whether the `0xED` / `0xEC` transport commands can be delivered at all on a
  Bluetooth-only connection.
- Whether the RX FIFO is ever fed from GATT data on this firmware.
