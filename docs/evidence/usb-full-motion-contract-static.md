# USB full-motion report: static wire contract

Artifact: `Telink87_For_F3_Ver6.7.5.1.bin`, version `6.7.5.1`, SHA-256
`6cba30540a0c3ba16acad2b62bb4dd64dfccc28add04914c3720c6ae6a53e20a`.
Static only: saved corrected decompilation and raw image words, read-only. No
hardware was used for this note; the live facts quoted are attributed to earlier
owner captures. No saved Ghidra project was opened and `local/research` was not
written.

This note answers one question: **what exactly does the firmware put on the USB
wire for the 32-byte vendor report, and what makes it run?** It is the positive
contract; it deliberately makes no claim that this is the only motion path.

---

## 1. The report is report ID 4, 31 bytes, on a vendor collection

The image carries the report descriptor at `0x23fac`:

| Bytes | Item |
| --- | --- |
| `06 A0 FF` | Usage Page `0xFFA0` (vendor) |
| `09 01` | Usage `0x01` |
| `A1 01` | Collection (Application) |
| `09 03` | Usage `0x03` |
| `85 04` | **Report ID 4** |
| `15 80 25 7F` | Logical Min `-128`, Logical Max `127` |
| `35 00 45 FF` | Physical Min `0`, Physical Max `255` |
| `75 08 95 1F` | Report Size 8, Report Count **31** |
| `81 02` | **Input** (Data, Variable, Absolute) |
| `09 05` | Usage `0x05` |
| `85 05` | **Report ID 5** |
| `15 80 25 7F 35 00 45 FF 75 08 95 1F` | same byte shape |
| `91 02` | **Output**, 31 bytes |
| `C0` | End Collection |

A separate descriptor at `0x23768` was attributed to the 21-byte DInput shape
in the static pass. Do not treat that attribution as the current USB gamepad
contract: the live USB gamepad interface has a 125-byte descriptor and emits
**9-byte reports**, while the separate vendor interface matches the 44-byte
descriptor and 32-byte ID-4 report described here. In particular, reusing the
Bluetooth 21-byte parser for the ordinary USB interface is not justified.

**Consequence for the host contract:** each payload byte is declared `-128..127`
to the host, so signedness is per byte; 16-bit quantities are little-endian byte
pairs that the consumer reassembles. The firmware performs no saturation when it
writes these fields (§2), so the declared logical range is not enforced on the
device side.

**Layering, checked against the earlier capture:** the constructor writes exactly
`0x20` = 32 bytes *including* the report ID as byte 0, and the owner's hidraw
capture of the same stream was 32 bytes beginning `04 FE 66`. The two match 1:1 —
there is no host-side report-ID insertion on this interface and no truncation.
That is a size-level confirmation, not a field-level one.

## 2. Byte map of the 32-byte frame

Builder: `FUN_0001a45c` (`0x1a45c`), buffer `0x8476d8` (literal at `0x1a5d8`).
Sources: sensor state `0x8469ec` (literal `0x1a5e4`), device state `0x846ac8`
(literal `0x1a5dc`), input snapshot `0x847740` (literal `0x1a5e0`).

Normal shape (`state+0xe7 != 1`), decompiled lines 23228-23285:

| Byte | Value | Source |
| --- | --- | --- |
| 0 | `0x04` | report ID |
| 1-2 | `FE 66` | constant vendor sub-tag |
| 3 | snapshot `+0x3b` | |
| 4 | snapshot `+0x34` | |
| 5 | snapshot `+0x35` | |
| 6 | snapshot `+0x36` | |
| 7 | snapshot `+0x02` | extra buttons (C, Z, M1-M4 per earlier capture) |
| 8 | snapshot `+0x03` | |
| 9 | snapshot `+0x00` | face/shoulder buttons |
| 10 | snapshot `+0x01` | |
| 11 | low `sensor+0xb4` | motion word 4 |
| 12 | high `sensor+0xb4` | |
| 13 | low `sensor+0xb6` | motion word 5 |
| 14 | high `sensor+0xb6` | |
| 15 | low `sensor+0xb8` | motion word 6 |
| 16 | high `sensor+0xb8` | |
| 17 | low `snapshot+0x04` | stick/axis |
| 18 | **low `sensor+0xae`** | motion word 1, low half |
| 19 | low `snapshot+0x06` | axis |
| 20 | **high `sensor+0xae`** | motion word 1, high half |
| 21 | low `snapshot+0x08` | axis |
| 22 | low `snapshot+0x0a` | axis |
| 23 | snapshot `+0x40` | trigger |
| 24 | snapshot `+0x41` | trigger |
| 25 | `(sensor+0x3b)<<1 \| sensor+0x3a` | 2-bit status, bits 0-1 |
| 26 | low `sensor+0xb0` | motion word 2 |
| 27 | high `sensor+0xb0` | |
| 28 | `state+0x9b` | status byte |
| 29 | low `sensor+0xb2` | motion word 3 |
| 30 | high `sensor+0xb2` | |
| 31 | `0` | constant |

**Non-contiguity is real:** motion word 1 (`+0xae`) is split across bytes 18 and
20, because bytes 17/19/21/22 carry the axis snapshot in between. Words 2 and 3
sit at 26-27 and 29-30, with byte 25 (sensor status) and byte 28 (device status)
interleaved. A naive "read words at 26,28,30" parser is wrong.

**Ordinary input is present:** bytes 7, 9 and 10 carry the button bitmap and
bytes 17, 19, 21, 22, 23, 24 carry axes/triggers, which matches the earlier
owner captures (extras at byte 7, control press at byte 9 bit `0x10`).

**No clipping or scaling** happens in this builder: each field is cast to a byte
directly. The six motion words are whatever the sensor pipeline left at
`0x8469ec + 0xae..0xb9`.

### Alternate shape (`state+0xe7 == 1`)

Decompiled lines 23234-23248: bytes 3-31 are zeroed (`FUN_0001c340(.., 0, 0x1d)`)
and bytes 3-10 are filled from four 32-bit counters at `state+0xe8/+0xec/+0xf0/+0xf4`,
truncated to 16 bits each. Bytes 11-31 are zero; bytes 7, 9 and 10 can contain
counter values and must not be interpreted as buttons. There is no separately
established discriminator in the header. A zero tail is a reason to handle the
frame conservatively, not proof of this branch rather than an all-zero normal
tail. Which state raises `+0xe7` was not traced here.

## 3. What schedules the frame

Two independent periodic tasks touch the buffer:

* **Fill:** `FUN_0001acb0` → `FUN_0001ab28` (`0x1ab28`). Gated on
  `state+0x15 == 0`, link-state byte `0x846527 != 1`, and
  (`state+0x0d == 1` or (`state+0xc == 4` and `FUN_00017df4() == 0`)); plus the
  connection byte at `0x846527` being one of `1,3,5,7`. With `+0x0d == 1` (USB
  attached) it calls `FUN_0001ab28`, which in personality `0x845e20 == 1` builds
  the snapshot (`FUN_00019e90`) and then this frame (`FUN_0001a45c`).
* **Send:** `FUN_00000360` → `FUN_000012b0` → `FUN_0001a43c` (`0x1a43c`), which
  calls `FUN_0001a3a8` (`0x1a3a8`) when `0x845e20 == 1`. That function toggles
  `0x8465ba` so it acts on alternate calls, and writes the buffer through
  `FUN_00012ba4(0x8476d8, 0x20)`.

**Send readiness** (`FUN_00012ba4`, `0x12ba4`): it returns without writing when
control `0x800121` bit 7 is set, or when `0x800122 == 0`, or when `0x800123` bit 0
is already clear-to-busy; otherwise it copies `param_2` bytes into the
memory-mapped FIFO at `0x80111b` and arms `0x800123` with `5` (or `9` when a
status byte is non-zero). Endpoint number and register semantics are **not**
assigned here — that needs an authoritative TLSR87 register map plus the live
descriptor, neither of which is in this artifact.

**One buffer, two kinds of frame.** Before the FIFO write, `FUN_0001a3a8` may
call `FUN_00018b0c(0x8476d8, 1)` when `0x845e12 != 0`; that formatter can
overwrite bytes 0-1 with `04 FF` and build a command response in the same buffer.
A 32-byte frame whose bytes 1-2 are `FF ..` is therefore a response, not motion.
The motion shape always begins `04 FE 66`.

**Personality gate:** `0x845e20` is the USB personality byte. Value `1` takes the
path above; value `2` calls `FUN_00019b9c`/`FUN_0001aaf4`/`FUN_0001a190`
instead; other values do nothing here. The 500 Hz `04 FE 66` stream observed by
the owner therefore corresponds to `0x845e20 == 1`.

## 4. A vendor-mode flag; no command required in the observed USB session

`FUN_00018138` (`0x18138`), decompiled lines 20870-20877:

```
else if (cVar1 == -0x12) {            // subcommand 0xEE
    if (param_1[1] == 1) state[0x2a] = 1;
    else                 state[0x2a] = 0;
}
```

with `state` = `0x846ac8` (literal `0x183bc`). The output report is report ID 5,
so the frame the owner used — **`05 EE 01`** — is exactly `report ID 5`, payload
`EE 01`, which sets `0x846ac8 + 0x2a = 1`; `05 EE 00` clears it. This is a
non-flashing, non-destructive configuration bit and it is not in the prohibited
`13 04` / `13 05` family.

The current passive USB session already produced fresh `04 FE 66` reports
without this command. The flag assignment alone is not proof that `EE 01`
universally enables fresh motion; the gate below remains relevant. Do not send
it merely because this section previously called it an enable command.

**Caveat on the fill gate (unresolved polarity).** `FUN_0001ab28` skips the
rebuild when `state+0x2a == 1` **and** the counter `state+0x2e == 0`; the rebuild
otherwise runs. The counter `+0x2e` is also used elsewhere as an "idle" test
(`FUN_00017df4`, `FUN_000186f0`). The most consistent reading is that `+0x2a`
selects vendor mode and `+0x2e` is a streaming/refresh counter that is non-zero
while active, so the rebuild branch runs during streaming — but the artifact
alone does not prove the steady-state value of `+0x2e`. **This is the one point a
single live observation settles, and it does not require sending anything:** with
the vendor stream already running, a capture that shows fresh bytes 11-16
changing while the controller moves proves the rebuild branch is live.

## 5. Clean-room contract (what a host implementation may rely on)

1. Open the interface that carries the vendor collection (Usage Page `0xFFA0`,
   Usage `0x01`); earlier capture puts it on `mi_02`. Confirm the interface index
   from passive enumeration, not from this note.
2. Expect 32-byte input reports with `report ID 4` as byte 0.
3. Treat bytes 1-2 as the frame tag: `FE 66` = device-generated motion/input
   frame, `FF xx` = command response (parse the response, not motion).
4. When the tag is `FE 66`, bytes 11-16 are three little-endian 16-bit words
   (words 4, 5, 6 of the sensor block); byte 18 + byte 20 are word 1 low/high;
   bytes 26-27 are word 2; bytes 29-30 are word 3. Do not read words at
   contiguous 26/28/30.
5. All other bytes are input/status as mapped in §2; byte 31 is always 0.
6. Treat a fully zero tail at bytes 11-31 conservatively: counter-mode frames
   have this shape and their apparent button fields may be counters. Do not
   claim an unambiguous normal/debug discriminator has been established.
7. Do not assume a rate: the send path alternates on a toggle, so the effective
   report rate is a scheduling property, not a protocol constant. The owner's
   earlier capture measured 500 Hz in this personality.

**Not established by static analysis, and required before exposing a third axis:**
whether `sensor+0xb2` is physically the missing axis, its sign and scale, and its
relation to the two words already decoded. That is a labelling question this
artifact cannot answer — the firmware writes the word without naming it.

## 6. Observed / inferred / unknown

* **Observed (static):** descriptor bytes and report IDs; the byte map and its
  sources; the two frame shapes; the scheduling gates; the FIFO readiness bits;
  the `0xEE` flag handler; the personality byte values.
* **Observed (prior owner captures, quoted not re-measured):** 32-byte `04 FE 66`
  stream at 500 Hz in DInput personality; buttons at byte 7 and byte 9; sticks at
  bytes 17, 19, 21, 22; the `05 EE 01` command switching the stream.
* **Inferred:** that `+0x2e` is a streaming counter, so the rebuild branch runs
  during streaming; that the vendor collection sits on `mi_02`.
* **Unknown:** endpoint number and its register map; the live interface index;
  what raises `state+0xe7`; whether `+0xb2` is the third physical axis; whether a
  `04 FF` response can be interleaved with the motion stream at 500 Hz.

## 7. Verification this note does not claim

No controller was connected, no report was written, no personality was changed.
The `05 EE 01` bytes above are documented from the image and from the owner's
earlier capture; they were not sent by this analysis. Passive enumeration plus
one motion-only capture in the current personality is sufficient to confirm the
byte map without changing any device state.
