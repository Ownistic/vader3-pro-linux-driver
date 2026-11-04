# USB motion routing and sensor selection: static trace

Artifact: main image `6.7.5.1`, SHA-256
`6cba30540a0c3ba16acad2b62bb4dd64dfccc28add04914c3720c6ae6a53e20a`.
This is static analysis of the saved corrected TC32 Ghidra project, not a test
of the controller or proof of its installed firmware version.

## Selector initialization

`FUN_0000d4e0` calls `FUN_00008370` before `FUN_0000d4b0`. The former calls
`FUN_000082c8` and stores its return value at `0x00846ac8 + 0x14`, the same
byte subsequently read by the initialization and sampling dispatchers.
The literal `0x8394` was verified directly in the image.

`FUN_000082c8` calls `FUN_0001bafc` and `FUN_0001bbec` in this order:

| First argument to `1bafc` | `1bbec` arguments | Comparison | Selector |
| --- | --- | --- | --- |
| `0xd6` | `0x0f, 1` | return equals `0x6a` | 1 |
| `0xd4` | `0x00, 1` | return equals `0x05` | 2 |
| `0x6e` | `0x0f, 1` | return equals `0x61` | 3 |

The probes are tried in that order. If the first `0x6e` comparison succeeds,
the function repeats it after changing the second `1bafc` argument. In the
saved decompile it returns 3 even if that second comparison fails, after
calling `1bafc` again. If the first three comparisons all fail it returns 0.
The first arguments and register-like indices are not, by themselves, proof
of specific chip models or a confirmed bus transaction. The identity and
meaning of the second `1bafc` argument remain to be resolved.

Ghidra instructions at `0x82d8..0x832e` and `0x833c..0x834e` confirm the
shown call arguments, comparisons, and return assignments; `0x8384..0x838a`
confirm that the result is stored at offset `0x14`. Ghidra's reference
manager found `0xd4ea` as a call to `0x8370` and `0x8384` as a call to
`0x82c8`; indirect and undiscovered references remain possible.

## Two distinct builders and the USB-bound buffer

The [earlier trace](static-subsystems-2026-11-04.md) follows
`FUN_0001899c` through `FUN_0000fb5c` into staging buffer `0x00847204` and
conditionally into `FUN_0000fc18`. That is **not** the only report path and
its ultimate transport remains unresolved.

Separately, `FUN_0001acb0` calls `FUN_0001ab28` when state offset `0x0d` is
1. When the separate mode byte `0x00845e20` is 1, `FUN_0001ab28` calls
`FUN_00019e90` and then `FUN_0001a45c`. The latter builds 32 bytes beginning
`04 FE 66` in buffer `0x008476d8`. When state offset `0xe7` is 1 it instead
populates a special alternate payload after that header; the following table
describes only the normal branch. Literal `0x1a5e4` resolves to sensor-state
base `0x008469ec`, the same base used by the sensor producers.

| Sensor-state offsets | Buffer byte positions, low then high |
| --- | --- |
| `0xae..0xaf` | `18, 20` |
| `0xb0..0xb1` | `26, 27` |
| `0xb2..0xb3` | `29, 30` |
| `0xb4..0xb5` | `11, 12` |
| `0xb6..0xb7` | `13, 14` |
| `0xb8..0xb9` | `15, 16` |

Thus the Windows service's 12-byte motion selection is, in its array order,
the three words at `0xb0`, `0xb2`, `0xae`, followed by those at `0xb4`,
`0xb6`, `0xb8`. That correspondence is static evidence for the serializer;
it does not assign physical axes, sensor types or units.

The same buffer is selected by literal `0x1a428` in `FUN_0001a3a8`.
`FUN_00000360` calls `FUN_000012b0` when state `+0x0d` is 1;
`FUN_000012b0` calls `FUN_0001a43c`, which calls `FUN_0001a3a8` when the
mode byte is 1. Subject to its toggle and readiness conditions, that function
may first call `FUN_00018b0c` on the buffer, then calls
`FUN_00012ba4(buffer, 0x20)`. The helper's write loop repeatedly stores bytes
to memory-mapped address `0x0080011b` and writes its control value to
`0x00800123`; these addresses were read directly from literals `0x12c1c`
and `0x12c14`. A disabled/busy condition can avoid the byte loop. The precise
endpoint number and register semantics require an independently verified
chip register map and descriptor correlation. The optional `FUN_00018b0c`
call may replace the buffer contents before transmission, so this is a
conditional path to the endpoint-shaped FIFO, not proof that every transfer
contains a motion frame or reaches a host.

Read-only `RawRange.java` disassembly verified the header stores at
`0x1a460..0x1a46a`, motion stores at `0x1a4d0..0x1a53a`, the 32-byte call at
`0x1a3cc..0x1a3d0`, and the FIFO-shaped byte loop at `0x12bd0..0x12bdc`.
`RefTo.java` found the direct references `0x1abb4 → 0x1a45c`,
`0x1ad8c → 0x1ab28`, `0x12b2 → 0x1a43c`, `0x1a44c → 0x1a3a8`, and
`0x1a3d0 → 0x12ba4`. These references and the decompiled scheduling conditions
do not exclude indirect callers or concurrent buffer changes.

Method: `/opt/ghidra/support/analyzeHeadless` opened the existing project with
`-readOnly -noanalysis` and ran the saved `RawRange.java` and `RefTo.java`
helpers. The literal annotations printed by `RawRange.java` are known to be
incorrect and were ignored; literal values above were independently decoded
from the image with Python `struct.unpack_from`. The TC32 module's instruction
semantics and overlapping function boundaries remain limitations.

## Arbitration before the FIFO write

`FUN_0001a3a8` uses a byte at `0x008465ba` as a toggle: when it is nonzero,
the function clears it and returns; when zero, it sets it and evaluates the
send paths. For the 32-byte path, bit 0 of the byte at `0x00800123` must be
clear. If it is clear, the function calls `FUN_00018b0c(0x008476d8, 1)` only
when the byte at `0x00845e12` is nonzero. It then calls `FUN_0001a1e8` and
`FUN_00012ba4(0x008476d8, 0x20)` regardless of the formatter's return value.
Consequently a formatter return of 0 does not itself suppress the write. The
later helper still has its own readiness checks and may not execute its FIFO
loop. These are code conditions, not measured polling frequency.

The gate at `0x00845e12` is not simply a pending-response counter.
`FUN_0001a148` sets it to 0 before copying incoming data and dispatching on a
mode byte, then sets it to 1 afterward. In mode 1, `FUN_0001a0cc` checks that
the incoming length exceeds 11 and starts with `0x05`; except for special
cases it passes the following bytes to `FUN_00018138`. Mode 2 dispatches through
`FUN_00019fa4`. `FUN_0001c920` has two static calls to `FUN_0001a148` after
reading bytes from two peripheral input paths. The decompiler loses an argument
at those call sites; the transfer lengths and exact input endpoint identities
require instruction and register-map verification. The gate is shared with
the alternate-mode `FUN_00001528`; no claim is made that it can only change
in the identified routines or that this sequence is race-free.

`FUN_00018b0c` reads the state block at `0x00846ac8` and checks pending fields
in this priority order: `+0x1d`, `+0x45`, `+0x34`, `+0x24`, `+0x25`,
`+0x26`, `+0x4b`, then, after processing `+0x1f` and `+0x54` preparation
flags, `+0x43`, `+0x46`, `+0x51`, `+0x29`, and `+0x50`. The last four
include additional comparisons or bounds, so a nonzero field does not alone
prove that branch constructs a response. Most selected one-shot fields are
decremented; chunked responses advance an index. These are state fields, not a
proven FIFO queue, and only one selected branch is handled per invocation.

For the USB-style argument 1, response branches write `04 FF` at buffer bytes
0 and 1, replacing the normal `04 FE 66` header. For example, dispatcher
`FUN_00018138` sets `+0x1d` on command `EC`, `+0x34` on `EB A0`, and
`+0x24` on `FA A0`; the formatter checks these in the priority order above.
The response format and command relationships are static observations only,
not permission to send them. Branches overwrite different portions of the
32-byte buffer rather than universally clearing all bytes. When no eligible
branch constructs a response, the formatter returns 0 without overwriting a
response header, although some preparation state can still change. The
previous buffer contents and interleaving of producer and sender are not
established, so neither fresh motion nor a fresh response is guaranteed for
every attempted write.

The other observed call is `FUN_00019a50 → FUN_00018b0c(buffer, 0)` when a
different outgoing queue is empty and state `+0x0e` is not 2. That argument
selects a `5A A5` header and can call `FUN_0001896c` instead of relying on the
USB-style caller's later FIFO write. This is a separate framed path; its
transport and Bluetooth reachability remain unresolved.

Verification: read-only Ghidra `RawRange.java` shows the toggle and gate tests
at `0x1a3aa..0x1a3c4`, the formatter call with argument 1 at `0x1a404..0x1a40a`,
and the subsequent call to `0x12ba4` at `0x1a3c8..0x1a3d0`. Instructions at
`0x18b20..0x18b2e` confirm the first priority field's check and decrement;
`0x1a150..0x1a17a` confirm the gate's clear, mode dispatch and set sequence.
Read-only `RefTo.java` reported the two formatter callers at `0x19a9a` and
`0x1a40a`, and two `FUN_0001a148` calls at `0x1c9b8` and `0x1c9fe`. The rest
of the priority table is derived from the saved decompilation and has not been
individually instruction-verified. Literal aliases were decoded directly from
the image, not from the known-incorrect `RawRange.java` annotations.

## Next investigations

- Resolve the two `FUN_0001c920` input paths and their lost argument, validate
  the remaining response-priority branches in instructions, and determine the
  producer/sender synchronization and stale-buffer behavior.
- Correlate the FIFO and control addresses with an authoritative TLSR87 register
  map and the saved USB descriptors before assigning an endpoint number.
- Identify selector source chips using verified bus primitives and specifications;
  determine axis type, scale and orientation with isolated future hardware tests.
- Trace the separate `0x00847204` staging buffer and `FUN_0000fc18` to its
  transport without assuming it is the USB `04 FE 66` route.
