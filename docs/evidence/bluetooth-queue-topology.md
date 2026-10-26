# Queue topology: why Bluetooth commands cannot reach the dispatcher

Static analysis of firmware 6.7.5.1 (`Telink87_For_F3_Ver6.7.5.1.bin`,
SHA-256 `6cba3054…53e20a`, 152068 bytes) following the Bluetooth write path.
This resolves the open question left by
[bluetooth-write-paths.md](bluetooth-write-paths.md): why accepted writes on the
Nordic UART service produced no device response.

## The `5A A5` envelope is device-to-host, not host-to-device

This corrects an earlier reading in this project. `5A A5` is what the
**controller emits**, not what it accepts.

`FUN_0000fd7c` builds the envelope and hands it to the outgoing queue:

```c
FUN_0001c340(DAT_0000fe30, 0, 0x1f);       /* clear notify buffer */
puVar4[0] = 0x5a;
puVar4[1] = 0xa5;
puVar4[2] = 0xa5;
puVar4[3] = 0x41;                           /* 'A' */
puVar4[4] = seq++;                          /* rolling sequence */
puVar4[5] = len_lo;  puVar4[6] = len_hi;
/* payload words ..., then */
puVar4[0x11] = 2;
FUN_0001896c(puVar4);                       /* -> OUTGOING queue */
```

`FUN_0001896c` is therefore a **transmit** helper, not a receive parser. Its
`5A A5` test validates frames the firmware itself just built before queueing
them for notification. Reading it as an inbound accept-filter — which is what
this project previously did, and which drove the `5A A5`-wrapped write
experiments — was wrong.

The full outbound chain:

| Function | Role |
| --- | --- |
| `FUN_0000fd7c` | builds `5A A5 A5 41 <seq> <len16> <payload> 02`, enqueues |
| `FUN_0000fb7c` | dequeues, copies `entry+2` into the notify buffer |
| `FUN_0000fc18` | assembles the notification, appends checksum via `FUN_0000f8bc` |
| `FUN_0000fe34` | Bluetooth event pump; calls `FUN_0000fd7c` |

That the one observed `5A A5 00 05 0F 70 70` write produced no notify is
consistent: the controller was not being sent a command, and it had nothing
queued to report.

## Two separate queues

| Queue | Address | Producers | Consumer |
| --- | --- | --- | --- |
| Command (inbound) | `0x00845e14` | `FUN_00000ca8` only | `FUN_00019a50` |
| Notify (outbound) | `0x00845db4` | `FUN_0001896c`, `FUN_0000fd7c` | `FUN_0000fb7c`, `FUN_00019a50` |

`FUN_00019a50` drains both, but in opposite directions: the command queue is
handed to `FUN_00018828` for dispatch, the notify queue to the notification
builder.

## The only producer of inbound commands is the USB frame validator

`FUN_00000ca8` is the sole enqueuer to the command queue, and it has exactly one
caller: `FUN_00000d54`.

```c
FUN_00000d54:                          /* frame arrival */
  if (pkt[3] & 0x80) return;                        /* flag */
  if (pkt[0] != ((pkt[4] & 0x3f) + 0x0b)) return;   /* length consistency */
  if (pkt[5] == 1) { ...session handshake... }
  else if (pkt[5] == 2) FUN_00000ca8(pkt);          /* enqueue command */

FUN_00000ca8:
  FUN_0001c468(DAT_00000d4c, param_1 + 8, uVar4 - 3);   /* -> 0x00845e14 */
```

`FUN_00000d54` is reached from `FUN_00001014`, which is driven by
`FUN_00001020` reading the hardware receive register `0x00800f20`
(`FUN_00001120` tests its bits). That is the **wired USB transport** interrupt
path.

So the command queue's only source is the physical USB interface. A frame
arriving over Bluetooth has no route into it.

## What Bluetooth receive actually dispatches

`FUN_00018828` is the Bluetooth dispatcher, and it reads a buffer
(`0x008477cc`) that the notification path also uses. Its branches:

| Condition | Action |
| --- | --- |
| `len < 9`, `buf[0] == 0xBA` | drive both rumble pairs via `FUN_0000b62c` / `FUN_0000b6a4` |
| `len < 9`, `buf[1..2]` = `AA BB` / `BB AA` | connect handshake, 40 ms confirmation buzz |
| otherwise | `FUN_00018138(buf, 0)` — note: **no `05` skip**, unlike USB's `FUN_00018138(buf + 1, 1)` |

Two consequences, both confirmed by the hardware results already recorded:

1. A `05`-prefixed payload arriving on this path is read as subcommand `0x05`.
   `FUN_00018138` matches only `0xE0`–`0xEE`, so it is silently ignored.
2. Even a correctly framed subcommand would be operating on a **transmit**
   buffer, not a received command buffer.

## Hardware results, re-read against this model

| Frame sent over NUS | Accepted | Disconnect | Response |
| --- | --- | --- | --- |
| `5A A5 00 05 0F 70 70` (20 B) | yes | no | none |
| `AA BB`, `BB AA` (2 B) | yes | no | none (no notify) |
| `BA 70 70 70 70` (5 B) ×14 | yes | no | none |

All three are explained by the topology rather than by a rejected frame: the
writes land somewhere that does not feed the command dispatcher.

## Conclusion

On firmware 6.7.5.1, **the Bluetooth link has no path to the command
dispatcher.** Commands reach `FUN_00018138` only through the wired USB receive
interrupt. This is a structural property of the transport wiring, not a framing
mistake that a different byte sequence could fix.

Therefore, for Bluetooth DInput rumble on this build, the options are:

1. **Host-side translation is impossible without a device-side change**, because
   there is no Bluetooth command channel to translate into.
2. **Firmware modification** is required to add a Bluetooth receive path into
   `FUN_0001c468` / `FUN_00018138`, or to make the `0xBA` short-frame branch
   reachable from a GATT write.
3. **A different controller mode** whose Bluetooth link does have a command
   channel — the Bluetooth XInput descriptor declares six output reports, unlike
   DInput's zero.

Option 3 is worth testing before any firmware work, because it requires no
modification and the descriptor difference is already recorded.

## Caveats

- Reachability is established call-graph-wise. Whether some other writer reaches
  `0x00845e14` through a pointer not matched by a literal-pool scan is not
  excluded; the scan covered direct `FUN_0001c468` calls only.
- `FUN_00018828`'s `buf` (`0x008477cc`) is shared with the notify path, so the
  `0xBA` branch may be intended for internal loopback rather than host input.
- The installed firmware version on the owner's unit is still unconfirmed; this
  analysis covers the downloadable 6.7.5.1 build.
