# Gyro: why it is deferred, and where to resume

> **Historical; partially superseded.** The tests described here cover specific
> Bluetooth DInput reports and commands, not every possible motion route. The
> assertion that all motion depends on `state+0x2a` is not established. See
> [firmware follow-up](firmware-followup-2026-11-03.md) and
> [static subsystem traces](static-subsystems-2026-11-04.md) for the corrected
> serializer, sensor and calibration traces.

Static analysis of firmware 6.7.5.1 (`Telink87_For_F3_Ver6.7.5.1.bin`, SHA-256
`6cba3054…53e20a`). **No hardware was touched for this document.** It records why
motion is out of scope for 1.0 and what the post-1.0 work should start from, so
that effort does not begin by rediscovering the transport.

## Summary

The Bluetooth DInput stream carries no motion data, and the vendor mode that does
carry it cannot be switched on through the channel that is proven to work. Those
are two separate facts with separate causes, and only the first is obvious from
the report bytes.

| Fact | Class | Evidence |
| --- | --- | --- |
| The 21-byte Bluetooth DInput report has no motion fields | Hardware | Shake test; bytes 1–4 are the sticks |
| Motion exists only in vendor mode, gated by `state+0x2a` | Firmware | `FUN_0001a868` report-builder branch |
| Vendor mode is enabled by subcommand `0xEE` in `FUN_00018138` | Firmware | Dispatcher case at the `-0x12` comparison |
| `state+0x2a` has exactly **two** writers in the whole image | Firmware | Symbol sweep over every pool holding `0x00846ac8`; both are `0xEE` arms |
| A second `0xEE` arm exists in `FUN_000183c8`, reachable from `FUN_00019fa4` for frames of 15+ bytes | Firmware | `FUN_00019fa4` length test at `0x19faa` |
| Neither `0xEE` arm has a proven Bluetooth write path | Firmware | See "The second `0xEE` arm" below |
| `FUN_00018138` is **not** reachable from a Bluetooth GATT write | Firmware | Only producer of its queue is the USB validator |
| The proven Bluetooth channel dispatches `FUN_0000b830`, which has no `0xEE` arm | Firmware | 4-byte payload path; arms are `0x10`–`0x13` only |
| Vendor mode has been observed active on real hardware | **Never** | The `0x13 05` test disconnected the controller instead |
| The vendor's own tool switches it over USB only | Windows app | `DInputDataManager : UsbDataManager`; no BLE data manager exists |

The last row is the one that keeps this honest. Everything above it is code, and
code is not behaviour.

## The second `0xEE` arm, and the one route that stays open

There are two dispatchers with a `0xEE` case, not one:

| Dispatcher | Reached from | `0xEE` arm |
| --- | --- | --- |
| `FUN_00018138` | `FUN_00018828` and `FUN_0001a0cc` | sets `state+0x2a` to 1 or 0 |
| `FUN_000183c8` | `FUN_00018828` (when `state+0xe == 2`) and `FUN_00019fa4` | same |

`FUN_00019fa4` is the interesting one. It is the personality dispatcher, and its
gate is only about length:

```c
bVar1 = *length;
if (bVar1 == 0) return;
if (bVar1 != 8) {
    if (bVar1 < 0xf) return;        /* 1..7 and 9..14 are dropped */
    FUN_000183c8(payload, 1);        /* 15+ bytes: full command dispatch */
    return;
}
/* length == 8: the A5 BA / A5 BB rumble pair */
```

So **any frame of 15 bytes or more reaches `FUN_000183c8`, including its `0xEE`
arm.** That is the shortest remaining distance to vendor mode, and it is shorter
than this document previously assumed.

The obstacle is upstream. `FUN_00019fa4` is called from `FUN_0001a148`, which
selects it only when `*0x001a18c == 2`, and `FUN_0001a148` in turn has a single
caller: the frame pump `FUN_0001c920`. That pump drains a peripheral FIFO at
`0x0080011d`/`0x0080011e` into a 68-byte stack buffer — a wired path, not a GATT
write. Consistent with that, the target frame buffer `0x8477cc` (length at
`0x84652a`) has exactly two writers in the image: `FUN_00019a50`, fed by the
software queue `0x00845e14`, and `FUN_0001a148` itself.

**No GATT producer was found for either writer.** That is the whole remaining
question, and it is narrower than "find the BLE stack": it is only whether some
ATT write callback feeds `FUN_0001a148` or the `0x00845e14` queue.

## The reachable channel is now fully inventoried

The earlier note that the proven channel "dispatches `FUN_0000b830`" understated
it. `FUN_00010980` branches on length alone:

| Length | Route |
| --- | --- |
| `< 5` | six handlers, then `FUN_0000b830(buf, 4)`. These are the **mobile app's command set**: `0xAC`→`FUN_00010560`, `0xBA 0x40`→`FUN_000105cc`, `0xA5 0x01`→`FUN_000105e8`, `0xA5 0xA0`→`FUN_00010604`; see [mobile-app-ble-protocol.md](mobile-app-ble-protocol.md) |
| `5`–`19` | **nothing** |
| `== 20` | additive checksum over bytes 0–18, then `FUN_000107a0` and `FUN_00010530` |
| other | nothing |

That inventory is not what rules gyro out, though — the symbol sweep is. Every
writer of `state+0x2a` in the image is one of the two `0xEE` arms, so no amount of
work inside the reachable dispatch sets it unless a route to a `0xEE` arm exists.
Inventoring the arms only confirms the sweep.

## The vendor never does this over Bluetooth

The Windows tool switches vendor mode, and it does so over the wire only. The
chain is short and unambiguous:

```csharp
// FDGTransData.SetDataListening
if (val2.ConnectMode.ToLower() == "dinput") {
    byte[] cmd = DongleCommand.getSwitchOriginData(val.isListen);
    DInputDataManager.GetCurrentInstance().SendByteArray(cmd);   // ← transport
}
```

`DInputDataManager` is declared `class DInputDataManager : UsbDataManager`, and the
assembly contains no BLE data manager of any kind — only `UsbDataManager`,
`DInputDataManager`, `XInputDataManager` and `PS5DataManager`. The app also never
references the vendor 128-bit service UUID (`00010203-0405-0607-0809-0a0b0c0d2b12`)
anywhere in 720 decompiled files.

So in the vendor's own implementation, "DInput" means *wired* DInput, and turning
on the origin data that carries motion is a USB operation. That is consistent with
the firmware result above: the two `0xEE` arms exist, but nothing on the GATT side
feeds them.

This reclassifies the problem. It is no longer "we have not found the BLE write
path yet"; it is "the desktop tool does not use one, and the service it would have
to use is referenced by no vendor code we have". The remaining explanations are
that the mobile Flydigi app drives that vendor service over BLE, or that the
firmware exposes no BLE route at all.

## Status: superseded

**Switch mode solves this.** The controller's Nintendo Switch personality exposes

> **Correction, 2026-11-04.** A separate Bluetooth motion path was found and
> observed on hardware afterwards. It is not the HID report analysed below but a
> Nordic UART notification stream, enabled by the `AC` command. Two packed 12-bit
> values respond to rotation; axes, scale and a third component remain unresolved.
> See [static-subsystems-2026-11-04.md](static-subsystems-2026-11-04.md).
> Everything below about the `0xFE` HID report remains accurate.
live gyro and accelerometer through the stock `hid-nintendo` driver over
Bluetooth, plus rumble through the standard force-feedback API, on stock
firmware with no command to send. See [switch-mode.md](switch-mode.md).
The cost is the M3/M4/C/Z buttons, which that personality never transmits.
The vendor-mode analysis below remains valid and is kept as the record of why
that route was closed.

**Update:** a second toggle for the motion report was found afterwards — a
physical button combo, not a host command. See
[fe-report-toggle.md](fe-report-toggle.md). If that combo can be identified, the
motion report is reachable on stock firmware over any transport, and no patch is
required.

## Feasibility: what static analysis can and cannot settle

I confirm this unit has a gyro, having used it from the Windows app
(whether over USB or Bluetooth was not recorded). That removes the earlier "maybe
there is no IMU" uncertainty, so the question is now purely transport:

| Route | Status | Basis |
| --- | --- | --- |
| USB (wired) vendor mode | **reachable** | `FUN_00018138` gets `05`-stripped frames from the wired validator; the same interface already rumbles |
| 2.4 GHz dongle | untested | `state+0xe == 2` skips the report-builder check entirely |
| Bluetooth BLE, any command | **no arm exists** | see below |

The BLE command surface is now fully enumerated, and this is the part worth
stating plainly: there is nothing left to decompile that could change it. Every
host-writable entry point on the NUS characteristic is accounted for:

| Handler | Matches | Effect |
| --- | --- | --- |
| `FUN_00010560` | `AC` | key/led flags |
| `FUN_000105cc` | `BA 40` | calls `FUN_00010320` |
| `FUN_000105e8` | `A5 01` | calls `FUN_000103e4` |
| `FUN_00010604` | `A5 A0` | 13-byte status reply |
| `FUN_000105a4` | unnamed 4-byte constant | two calls, unidentified |
| `FUN_0001067c` | `A5 A1` | 20-byte query reply |
| `FUN_0000b830` | 4-byte, `buf[0]` in `10`–`13` | remap, rumble, table write, mode switches |
| `FUN_000107a0` | 20-byte + checksum | nine opcodes (`_ R S P Q T @ A U`) |

None of these writes `state+0x2a`, and the symbol sweep already showed that only
the two `0xEE` arms ever do. So **no amount of further decompilation makes vendor
mode reachable over BLE**; that conclusion rests on an exhaustive writer search,
not on missing code.

What static analysis **cannot** answer is whether the *Flashplay* personality
(FN+Y) streams motion on the same characteristic — that switch happens inside the
controller on a button combo, not by command, so the published frame format may
differ. That is a runtime question and needs no decompilation.

## Why the shake test settles less than it appears to

A whole-controller shake produced no change in bytes 1–4, which are the sticks.
That proves the *passive* report carries no motion. It does not prove the
controller lacks a gyro, and it does not prove the gyro is unreachable — the
vendor-mode report is a different format emitted under a different flag.

The firmware makes this explicit. `FUN_0001a868` chooses between two builders:

```c
if (*(char *)(state + 0xe) != 0x02) {                 /* not the 2.4 GHz transport */
  if (*(char *)(state + 0x2a) == 0x01 && *(short *)(state + 0x2e) == 0) {
    FUN_0000094c();                                  /* vendor / key-mapping report */
  } else {
    /* plain DInput report: sticks, buttons, no motion */
  }
}
```

So `state+0x2a` is the single switch between "21 useful bytes, no motion" and the
32-byte vendor frame that carries gyro. Every gyro question reduces to: can that
one byte be set over Bluetooth?

## The switch is a plain command with no handshake

In `FUN_00018138`, subcommand `0xEE` (the `-0x12` comparison) does exactly this:

```c
else if (cVar1 == -0x12) {              /* 0xEE */
    if (param_1[1] == 0x01) *(u8 *)(state + 0x2a) = 1;
    else                    *(u8 *)(state + 0x2a) = 0;
}
```

One byte, no reply, no checksum, no persistence. In principle `05 EE 01` is all
that is required. In practice this dispatcher is the one the Bluetooth stack does
not feed — the queue it reads (`0x00845e14`) has a single producer, the wired USB
frame validator `FUN_00000ca8`. That was established in
[bluetooth-queue-topology.md](bluetooth-queue-topology.md) and it is the same
reasoning that misled this project about rumble.

That precedent is the important part of this document. The rumble conclusion was
wrong for a specific, repeatable reason: the investigation enumerated routes into
`FUN_00018138` and `FUN_000183c8` and stopped there, without asking which *other*
functions call the primitive it wanted. Doing that enumeration is what found the
working 4-byte path.

## What the proven channel can and cannot do

The working Bluetooth path is `FUN_00010980` → `FUN_0000b830`, and it gates hard on
payload length: exactly 4 bytes, or exactly 20 with an additive checksum. For the
4-byte case `FUN_0000b830` switches on `buf[0]` with four arms:

| `buf[0]` | Action | Related to motion? |
| --- | --- | --- |
| `0x10` | key remap (`FUN_0000cb94`) | no |
| `0x11` | rumble (`FUN_0000b62c`) | no |
| `0x12` | `FUN_0000ceb4` — writes three bytes into a table | no |
| `0x13` | mode switches, see below | no |

**`0xEE` is not among them, and the match is a flat four-way comparison with no
default arm.** A 4-byte frame cannot set `state+0x2a`. This is checked directly:
the `FUN_00018138` vendor-mode case and the `FUN_0000b830` arm list are different
functions with different comparison sets.

The 20-byte branch is worse for this purpose rather than better. It requires a
valid additive checksum before dispatching, and what it reaches is
`FUN_000107a0` — a byte switch over `_`, `R`, `S`, `P`, `Q`, `T`, `@`, `A`, `U`.
None of those is a vendor-mode toggle either.

## The `0x13` lead, and why it is not a shortcut

`0x13` on the reachable channel selects transport/report sub-modes:

| Frame | Path | Effect |
| --- | --- | --- |
| `13 02 00 00` | `FUN_0000b4d4` → `FUN_000119d8(8)` | LED effect 8 |
| `13 04 00 00` | `state+0x9a = 1` | sets a flag |
| `13 05 01 00` | `FUN_0000b500` | zeroes 8 bytes at `state+0x9c`, `state+0x76 = 1` |
| `13 05 02 00` | `FUN_0000b53c` | `state+0x76 = 0` |
| `13 10 00 00` | `FUN_0000b470` → `FUN_000119d8(1)` | LED effect 1 |
| `13 11 00 00` | `FUN_0000b484` → `FUN_000119d8(2)` | LED effect 2 |

Two of these touch motion-adjacent state. `FUN_0000b500` sets `state+0x76` and
zeroes eight bytes at `state+0x9c`; `state+0x76` gates the sensor-processing
routine `FUN_0000a27c`. Separately, `FUN_000119d8` sets a sensor mode byte through
`FUN_00011954`. That is a plausible-looking route, and it
is **why the `0x13` experiment was run** — not as a random poke.

It did not work. The device returned ATT error `0x0e` and dropped the link, and the
controller came back under a different address needing re-pairing. Whether that
was caused by `0x13` specifically or by the surrounding sequence was never
isolated, and the earlier claim that this path was "safe and non-persistent" was
withdrawn.

**Do not resume here without a recovery plan.** A controller that forgets its
pairing needs physical access to re-pair, which is a poor failure mode for a
feature nobody has confirmed is even present on this model.

## Where the post-1.0 work should actually start

Three approaches, in rough order of cost. The first settles the question rather
than advancing it, and costs nothing if an APK is already at hand.

**1. Analyse the mobile Flydigi app.** This is now the cheapest decisive step, and
it replaces "find the BLE stack" as the recommendation. The vendor's desktop tool
is USB-only for this feature, so if any vendor software enables origin mode over
BLE it is the phone app, and it would have to write to the vendor service
`00010203-…` that the desktop tool never touches. That answers the question by
existence: either the app sends a vendor-mode frame over BLE — and its bytes give
the channel and format directly — or no vendor software does, which makes this
firmware-patch territory. Requires an APK; none is in `local/research/`.

**2. Find whether any GATT write reaches `FUN_0001a148` or the `0x00845e14`
queue.** Still the right static question, but lower priority than it was, and it
now needs the Telink ATT table format to be resolved first. The firmware is
stripped: no SDK version string survives (`Telink Remote` is the only identifiable
name), so the ATT layer has to be recovered structurally.

**2b. Log into the app and open `Config management`.** The mobile BLE channel is
now fully characterised ([mobile-app-ble-protocol.md](mobile-app-ble-protocol.md)):
the app drives the controller over NUS with `A5`/`AC`/`BA` frames, and four of
those map to known firmware handlers. The one screen that was never reached is
where motion is enabled, and it needs a Flydigi account. Capturing that screen is
the cheapest remaining route to the missing command.

**3. Revisit `0x13 05` with recovery arranged.** Highest risk, and it should only
be attempted once the first two are exhausted, since a bond reset makes the device
harder to work with rather than easier.

## What would count as success

Not "the vendor-mode flag reads back as 1" — that is a write being accepted, which
is not evidence. The observable would be the input stream changing shape: 32-byte
`04 FE` frames appearing on the same characteristic, with some field inside them
varying when the controller is rotated and staying still when it is not. The exact
offset is not asserted here — the earlier "bytes 11–13" figure came from host-side
analysis and has not been checked against the firmware builder. Anything less is
unverified.

## What is still unknown

- Whether `05 EE 01` is accepted over Bluetooth at all — the flag has never been
  observed set on hardware.
- The exact byte layout of the vendor frame. `FUN_0000094c` is the builder, but its
  sensor fields were not decoded for this document, so the "gyro in bytes 11–13"
  figure is host-side inference and remains unconfirmed.
- Whether the vendor-mode frame is emitted on the same GATT characteristic as the
  DInput reports.
- Whether `state+0xe == 2` (the 2.4 GHz branch that skips the check entirely)
  behaves differently, which the dongle path might answer more cheaply.
- Whether this unit even has a gyro. The product is advertised with one; no motion
  field has been observed on it in any capture.
