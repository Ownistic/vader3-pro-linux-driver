# Bluetooth DInput write paths: what is reachable and what is not

This record covers active Bluetooth experiments on the Vader 3 Pro
(`d7d7:0041`, HID-over-GATT) on 2026-10-26, plus the firmware analysis used to
choose the frames. It corrects several earlier conclusions, two of which were
wrong claims made in this project and are marked as superseded.

All hardware statements below come from observed device behaviour. A write
returning success is never treated as evidence that the device acted on it.

## The controller is edge-triggered

Twenty seconds of repeated A presses produced **two** frames. A resting
controller that is not being touched produces **zero** frames.

```
01 ff ff 00 fe 00 00 00 00 10 00 00 00 00 00 00 …   A pressed
01 ff ff 00 fe 00 00 00 00 00 00 00 00 00 00 00 …   A released
```

`byte 9 = 0x10` for A, matching the earlier mapping. Every previous "empty
capture" in this project was probably normal idle silence, not a fault.

Dataset: 2 frames over 20 s, `/dev/hidraw9`, no writes issued.

## Bluetooth DInput declares no output report

Parsing both recorded descriptors:

| Descriptor | Input reports | Output reports | Feature reports |
| --- | --- | --- | --- |
| Bluetooth DInput (163 B) | 1 (14 B payload) | **0** | 0 |
| Bluetooth XInput (306 B) | 1, 2, 4 | 6 | 0 |

The DInput report map is 112 bits = 14 payload bytes, and the wire frames are
21 bytes (20-byte builder buffer + report ID). That mismatch is already
explained by the firmware builder in
[vader3pro-firmware-disassembly.md](vader3pro-firmware-disassembly.md).

## hidraw acceptance on the Bluetooth node is meaningless

The BT device exposes no output report, so a write to `/dev/hidraw9` has no
report to map onto. Tested four "report IDs", three of which the device never
declares:

| First byte | Declared? | Write result |
| --- | --- | --- |
| `0x05` | no | accepted, 21 B |
| `0xEE` | no | accepted, 21 B |
| `0x7F` | no | accepted, 21 B |
| `0xFF` | no | accepted, 21 B |

Every value was accepted. When acceptance is unconditional, a successful write
carries no information. This invalidates the framing of earlier Bluetooth write
experiments that reported "write accepted" as progress.

## BlueZ refuses HID-service writes locally, not on the air

Writes to characteristics inside HID service `0x1812` fail with
`org.bluez.Error.NotAuthorized`. This is produced by **bluetoothd**, not by the
device, and not by D-Bus permissions.

Established by elimination and by capture:

| Test | Result |
| --- | --- |
| `char001a` ReadValue | succeeds (`ay 1 0`) |
| `char001a` WriteValue as normal user | `NotAuthorized` |
| `char001a` WriteValue as **root** | `NotAuthorized` |
| `char001a` WriteValue with `hid-generic` **unbound** | `NotAuthorized` |
| NUS `char0023` write (service `6e400001`) | accepted |
| Vendor `char002b` write (service `1A12`) | accepted |
| `btmon` capture during the refused write | **no ATT Write on the wire** |

The `btmon` result is decisive: bluetoothd refused the operation before it
reached the controller. The device was never asked. So no conclusion about
device-side rumble support can be drawn from these refusals.

The controller itself declares `char001a` as `read`, `write`,
`write-without-response`. Whether it services those writes is unknown, because
BlueZ will not transmit them.

## Every write to the vendor `1A12` characteristic disconnects the controller

Observed repeatedly, with several framings:

| Frame sent | Length | Outcome |
| --- | --- | --- |
| `05 0F FA FA` + padding | 12 B | disconnected |
| `BA 70 70 70 70` | 5 B | disconnected |
| `5A A5 00 05 0F 70 70` + padding | 20 B | disconnected |

The link itself is **not** unstable: a 45-second window with no writes at all
recorded zero disconnects, sampled once per second. The disconnects are caused
by the writes.

This is consistent with the characteristic being an OTA/update endpoint that
resets the link on a frame it does not recognise. It is not a usable command
channel with any framing tried here.

## The `OTA` label belongs to the Nordic UART service

The firmware attribute table around `0x239b8`:

```
0x239b8  02 13 00 4a 2a 00 00 00 03 28 00 00 4f 54 41 00   ...J*....(..OTA.
0x239c8  9e ca dc 24 0e e5 a9 e0 93 f3 a3 b5 01 00 40 6e   ...$..........@n
```

`03 28` is a service declaration, followed by the UTF-8 name `OTA\0` at
`0x239c4`, then the Nordic UART service UUID `6e400001` at `0x239c8`.

So the OTA service **is** the Nordic UART service, not the vendor `1A12`
service. Writes to NUS are accepted and do not disconnect — the only Bluetooth
channel that survived a command frame.

Dataset: NUS `char0023` accepted 12-byte, 5-byte and 20-byte frames with no
disconnect.

## The Bluetooth receive path does not strip the `05` prefix

Two dispatch sites differ, and the difference explains the silence.

USB / DInput frame arrival (`FUN_0001a0cc`) skips the prefix:

```c
if (len > 0xb && buf[0] == 0x05) {
    if (buf[1] == 0x0f) { FUN_0000b62c(500, buf[2], buf[3]); ... }   /* rumble */
    else if (buf[1] != 0x11) FUN_00018138(buf + 1, 1);               /* skips 05 */
}
```

Bluetooth frame arrival (`FUN_00018828`) does not:

```c
if (len < 9) {
    if (buf[0] != 0xBA) goto joined;
    ... FUN_0000b62c(500, b1, b2);   /* pair A */
    ... FUN_0000b6a4(500, b3, b4);   /* pair B */
}
joined:
    if (mode == 2) FUN_000183c8(buf, 0, len);
    else           FUN_00018138(buf, 0);     /* 05 NOT skipped */
```

`FUN_00018138` matches only `0xE0`–`0xEE`. `0x0F` is absent from it, and the
rumble handler is separate (see
[vader3pro-firmware-rumble.md](vader3pro-firmware-rumble.md)). A `05`-prefixed
frame arriving over Bluetooth is therefore read as subcommand `0x05`, matches
nothing, and is silently ignored.

The enqueue path also constrains framing. `FUN_0001896c` accepts only frames
beginning `5A A5` and queues 20 bytes:

```c
if (msg[0] == 'Z' && msg[1] == 0xA5) FUN_0001c468(queue, msg, 0x14);
else                                  FUN_0000fb5c(msg, 0x1f);
```

The short-frame branch is a connect handshake, not a rumble command:

| Frame | Condition | Effect |
| --- | --- | --- |
| `x AA BB` | state 0 | state ← 1, 40 ms buzz at `0x40` |
| `x BB AA` | state 1 | state ← 2, 40 ms buzz at `0x40` |
| `x BA …` | any | drive both rumble pairs |

## Hardware results over NUS

Frames were sent to the NUS RX characteristic with notify subscribed on NUS TX.

| Frame | Writes | Accepted | Disconnect | Vibration |
| --- | --- | --- | --- | --- |
| `5A A5 00 05 0F 70 70` + pad (20 B) | 1 | yes | no | **none** |
| `AA BB`, `BB AA` (2 B) | 1 each | yes | no | **none** (no notify either) |
| `BA 70 70 70 70` (5 B) | 14 | yes | no | **none** |

NUS TX produced no notification in any case.

Conclusion that the evidence supports: NUS writes are accepted at the GATT
layer and do not disturb the link, but **no command tried so far produced an
observable device response**. Whether NUS reaches the `FUN_00018828` dispatcher
at all is unresolved; the frames may be consumed by the OTA state machine
before reaching it, or NUS may require an OTA session to be opened first.

**Resolved** by [bluetooth-queue-topology.md](bluetooth-queue-topology.md): the
`5A A5` envelope is device-to-host, `FUN_0001896c` is a transmit helper, and the
inbound command queue's only producer is the wired USB frame validator. Bluetooth
has no path into the command dispatcher on this build.

## Superseded claims

Two statements made earlier in this project are withdrawn:

1. *"`05 EE 01` activated vendor mode over Bluetooth."* Wrong. The report
   content change was stick-axis jitter (`byte 1`, left-stick X, idles near
   `0xfd`), and the frame count was the owner deliberately nudging the
   controller to keep it awake. Vendor mode was never entered. The M1-in-byte-7
   signature of vendor mode was never observed over Bluetooth.

3. *"`FUN_0001896c` accepts only `5A A5` frames, so commands must be sent with
   that envelope over Bluetooth."* Wrong direction. `FUN_0001896c` is the
   firmware's own **transmit** helper: `FUN_0000fd7c` builds
   `5A A5 A5 41 <seq> <len16> <payload> 02` and queues it for notification. The
   wrapped-frame experiments were built on this mistaken reading. See
   [bluetooth-queue-topology.md](bluetooth-queue-topology.md).

2. *"Bluetooth has no working output path, and the descriptor's lack of an
   output report makes rumble doubtful."* The descriptor fact is correct and
   now firmly established, but the conclusion drawn from it was premature: the
   NUS/OTA channel **does** accept host writes over Bluetooth. What fails is not
   delivery but reaching a command handler with a format it recognises.

## Open questions

- ~~Does NUS RX reach `FUN_00018828`, and if not, which handler consumes it?~~
  Resolved: see [bluetooth-queue-topology.md](bluetooth-queue-topology.md).
- Does an OTA session need to be opened before NUS accepts command frames?
  Moot for commanding the device, since NUS writes do not reach the dispatcher.
- Would the HID report characteristic service writes if BlueZ transmitted them?
- Is the installed firmware the same build analysed here (6.7.5.1)?
- Does the Bluetooth **XInput** personality (six declared output reports) expose a
  command channel that DInput does not?

## Reproduction

Scripts used, all outside the repository:

| Script | Purpose |
| --- | --- |
| `m1_probe.py`, `a_probe.py` | button/mode capture with vendor-mode attempts |
| `disconnect_timing.py` | link sampling around a write |
| `att_trace_probe.py` | `btmon` capture proving no ATT write on the wire |
| `hog_unbind_probe.py` | driver unbind + write + rebind |
| `ba_rumble_probe.py`, `nus_ba.py` | `0xBA` short-frame bursts |
| `ota_handshake_probe.py` | `AA BB` / `BB AA` handshake with notify |
| `wrapped_rumble_probe.py`, `nus_probe.py` | `5A A5` wrapped frames |
