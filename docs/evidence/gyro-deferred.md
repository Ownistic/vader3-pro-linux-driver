# Gyro: why it is deferred, and where to resume

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
| `FUN_00018138` is **not** reachable from a Bluetooth GATT write | Firmware | Only producer of its queue is the USB validator |
| The proven Bluetooth channel dispatches `FUN_0000b830`, which has no `0xEE` arm | Firmware | 4-byte payload path; arms are `0x10`–`0x13` only |
| Vendor mode has been observed active on real hardware | **Never** | The `0x13 05` test disconnected the controller instead |

The last row is the one that keeps this honest. Everything above it is code, and
code is not behaviour.

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

Three approaches, in rough order of cost. The first is the only one that needs no
new firmware knowledge.

**1. Find the GATT write handler in the Telink BLE stack.** This is the same
missing function that
[bluetooth-reachability.md](bluetooth-reachability.md) names as the hook point for
a firmware patch. It is required for gyro for a simpler reason: it is the only
place where a host-written payload could be forwarded to `FUN_00018138`. Locating
it answers whether `05 EE 01` over NUS is possible at all, with no device risk —
it is static work. This is the recommended first step.

**2. Test whether the 20-byte path's checksum gate can carry `0xEF`-style
commands.** `FUN_000107a0` handles nine opcodes but is reached only after the
checksum passes, and only one 20-byte frame was ever tried (a `5A A5` envelope,
which belongs to the outbound direction and was never going to work). A correctly
checksummed frame aimed at `FUN_000107a0` has not been sent. Lower value for gyro,
but cheap and untested.

**3. Revisit `0x13 05` with recovery arranged.** Highest risk, and it should only
be attempted once the first two are exhausted, since a bond reset makes the device
harder to work with rather than easier.

## What would count as success

Not "the vendor-mode flag reads back as 1" — that is a write being accepted, which
is not evidence. The observable would be the input stream changing shape: 32-byte
`04 FE` frames appearing on the same characteristic, with bytes 4–6 varying when
the controller is rotated and staying still when it is not. Anything less is
unverified.

## What is still unknown

- Whether this unit even has a gyro. The product is advertised with one; no
  motion field has been observed on it in any capture.
- Whether `05 EE 01` is accepted over Bluetooth at all — the flag has never been
  observed set on hardware.
- Whether the vendor-mode frame is emitted on the same GATT characteristic as the
  DInput reports.
- Whether `state+0xe == 2` (the 2.4 GHz branch that skips the check entirely)
  behaves differently, which the dongle path might answer more cheaply.
