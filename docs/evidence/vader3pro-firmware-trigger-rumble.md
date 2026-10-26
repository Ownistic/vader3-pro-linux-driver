# Trigger rumble / haptic feedback in Vader 3 Pro firmware

Static analysis of the official main-MCU image plus the Space Station 3.4.4.3
managed code. Nothing was sent to hardware.

Artifact: `Telink87_For_F3_Ver6.7.5.1.bin`, SHA-256
`6cba30540a0c3ba16acad2b62bb4dd64dfccc28add04914c3720c6ae6a53e20a`.
Companions: `vader3pro-firmware-disassembly.md` (tooling, `tnegs` bug),
`vader3pro-firmware-rumble.md` (grip motors).

## The short answer

The Vader 3 Pro **is advertised** as having trigger haptics (device ID 80 is in
the app's `TriggerVib` capability list), and the firmware contains a driver for
them. But the command path that would actually *use* them is not wired for this
firmware/transport combination. Three findings, in decreasing confidence:

1. Trigger haptics are **not** grip-motor PWM. They are written to a separate
driver addressed as a device index into a RAM pointer table, through a
function-pointer peripheral transfer — not a PWM compare register. Firmware
code for this exists and runs on a periodic internal refresh.
2. **The Space Station application never sends a trigger-vibration set command on
   DInput.** There is no `else` branch: `SendTmpTriggerCmd` builds and sends
   `A5 30 08` only when `ConnectMode == "XInput"`. Bluetooth DInput is exactly
   the case with no code path.
3. On top of that, **this firmware image has no handler for `A5 30 08`**, and
   `05 A0` (the adaptive-trigger command the app *does* send on DInput) is a
   no-op. So even a hand-crafted command would likely be ignored.

Net: trigger haptics are a documented, physically plausible capability whose
host software support is XInput-only and whose DInput command handler is absent
from firmware 6.7.5.1. That is a stronger negative result than "untested" — but
it is still a static-analysis conclusion, not a hardware observation.

## It is a real, advertised capability for this exact device

`GamePadVersionUtils.TriggerVib` is the capability list:

```csharp
public static int[] TriggerVib = new int[6] { 80, 81, 88, 85, 91, 105 };
```

and `GameHandleListDic` maps those IDs:

| DeviceId | profile | product |
| --- | --- | --- |
| 28 | `f3` | Vader 3 |
| **80** | **`f3p`** | **Vader 3 Pro** |
| 81 | `f3p` | Vader 3 Pro One Piece |
| 88 | `f3p` | Vader 3 Pro EVA |
| 85 / 91 / 105 | `F4` variants | Vader 4 Pro |

So **80 is in `TriggerVib` and is the Vader 3 Pro**. Trigger haptics are
advertised for this device, and `TriggerViewModel.UpdatePageData` gates the
whole trigger page on `IsTriggerVib(...)`:

```csharp
if (GamePadVersionUtils.IsTriggerVib(UniqueData.SelFDGDeviceInfo)) {
    IsShowTriggerBox = true; IsShowMotionBox = false; return;
}
IsShowTriggerBox = false;
```

Note 28 (plain Vader 3) is present in `F3Serial` but **absent** from
`TriggerVib`, so the non-Pro model does not get this page.

## Firmware side: two commands, one driver

Both are in `FUN_00018138`, whose complete set of `tcmp` immediates (byte-verified
across `0x18138..0x183bb`, nested levels included) is:

`E0 E1 E2 E3 E4 E5 E6 E7 E9 EA EC ED EE EB EF F4 F5 A0 22 33 F0 FA F2 02 03
04 01 00 08 05 06 A2 50`

An earlier note in this repo claimed `0x50` was *not* in the dispatcher. That
was **wrong** — it came from a byte-scan bug (wrong compare mask) and was
compounded by the decompiler printing the byte as `'P'`, so a text search for
`0x50` missed the branch entirely. `0xA2` is likewise present, but as a
*subcommand of `0xFA`*, not as a top-level command.

### `0x50` — trigger haptics

`0x50` has subcommands in byte 1:

```c
else if (cVar1 == 'P') {                       /* 0x50 */
  if (param_1[1] == '\x02') {                  /* 0x50 02 */
    *(u8*)(state + 0x25) = 1;                  /* trigger-mode active */
    FUN_0000bba0(param_1[2], 0, base + 0xe4);
  }
  else if (param_1[1] == '\x03') {             /* 0x50 03 */
    *(u8*)(regs + 0xe1) = param_1[2];
    *(u8*)(regs + 0xe2) = param_1[3];
    *(u8*)(state + 0x26) = 1;
    *(u16*)(state + 0xf8) = 0x0BB8;            /* 3000 */
  }
}
```

- `0x50 02 v` — sets trigger mode and writes `v` into the internal driver state.
- `0x50 03 a b` — writes two bytes to the driver's `0xe1`/`0xe2` registers and
  arms a **3000**-count timeout in `state+0xf8`.
- The 3000 counter is decremented once per main-loop tick and fires staged
  actions at 2000 and 1000 remaining. It is a lifetime/expiry counter, not a
  rumble duration.

**A second, larger parent command exists.** `FUN_000183c8` is an alternate
dispatcher keyed on a **`0xA5`** frame (not `0x05`), and it also handles `0x50`,
with more subcommands than the `0x05` path:

| `A5 50` sub | action |
| --- | --- |
| `01` | `FUN_0000b5d0(1,0x14,a,b)` **and** `FUN_0000b648(1,0x14,c,d)` — drives grip pair A *and* pair B together |
| `02` | trigger-mode flag + `FUN_0000bba0` (same as `05 50 02`) |
| `03` | driver `0xe1`/`0xe2` + 3000 timeout (same as `05 50 03`) |
| `04` | `FUN_0000bbdc` (driver reset/init) |
| `05` | sets `state+0x28` and calls `FUN_0000bac8(param_1[3])` |

That `A5 50 01` case is the one that explains the "second motor pair" in the
grip-rumble document: a single command drives both throttles at 20 ms
(`0x14`). It is plausibly the trigger-motor test, which would make pair B the
**trigger** motors rather than a second grip pair. **This is inference, not
proof** — see the open questions.

### The internal driver link

`FUN_0000bba0(device, mode, buf)` is the write path:

```c
if (param_2 == '\x01') {                       /* readback/bookkeeping mode */
    iVar1 = ((uint)*param_3 * 0x100 + param_3[1] + 1) * 0x10000;
    *param_3 = iVar1 >> 0x18; param_3[1] = iVar1 >> 0x10;
} else {
    FUN_0000824c(*(int*)((param_1 & 0xff) * 4 + DAT_0000bbd8) + 0xe1, param_3, 2);
}
```

`DAT_0000bbd8` → `0x008458d4`, a RAM **pointer table** of per-device bases,
indexed by command byte. So the trigger driver is addressed as *device N*,
unlike the grip motors which are single compare registers. `FUN_0000824c` →
`FUN_00008264` → `FUN_00008280` calls through a function pointer at `r3` then
`FUN_0001b1a0()`, i.e. a peripheral transfer, not a PWM write.

Separately, `FUN_0000fd7c` emits a **`5A A5 A5 41`** frame over the internal
link at a fixed interval, carrying a sequence counter (`state+0xe1`), a 16-bit
value, and five 16-bit values, ending in byte `2`. This is the periodic refresh
for that driver — an internal protocol, distinct from anything the host sends.

## Host side: the capability gap that matters

The application builds trigger haptics **only for XInput**:

```csharp
if (val2.ConnectMode == "XInput") {
    byte[] c1 = XinputCommand.GetTriggerVibSetCmd(1, b, filter, array, pwmScal);
    Thread.Sleep(50);
    byte[] c2 = XinputCommand.GetTriggerVibSetCmd(2, b, filter, array, pwmScal);
    XInputDataManager.GetCurrentInstance().SendByteArray(c1);
    XInputDataManager.GetCurrentInstance().SendByteArray(c2);
}
```

There is no `else`. `SendTmpTriggerCmd` therefore does **nothing** on a DInput
connection, including Bluetooth DInput. `GetTriggerVibSetCmd` builds:

```csharp
byte[] array = new byte[10] { 8, TriggerType, BindType, VibFilter, pwmScal,
                              0,0,0,0,0 };   /* + up to 5 param bytes */
return getSendData(CommandDic.SubCmdId["K1CmdID"], array);   /* K1CmdID = 48 */
```

`getSendData` in `XinputCommand` wraps with `data[0] = 0xA5` and appends
`CrcData`, so the intended wire form is
`A5 30 08 <trig> <bind> <filter> <scale> <p..> <crc>`, where `0x30` is
`K1CmdID` and `0x08` is `K1SubCmdSetTriggerVib`.

**However, the firmware has no handler for that subcommand.** The `0xA5`
dispatcher `FUN_000183c8` does branch on `0x30` (to `0x18552`), but its inner
chain compares only `01 04 05 0b 0c 0d` — there is **no `0x08` case anywhere in
the function** (checked by scanning every `tcmp` immediate in
`0x183c8..0x186dd`; the full set of second-level immediates is
`01 02 01 04 05 0b 0c 0d 01 02 01 02 03 04 05 01 01 01`).

So on this firmware image the trigger-vib *set* command appears to be a **no-op
or silently ignored**, despite being defined in `CommandDic` and built by the
app. Two readings are possible and static analysis cannot separate them:

- The handler was added in a later firmware than 6.7.5.1.
- The subcommand is consumed elsewhere (a different dispatcher or a transport
  layer I did not trace) rather than in `FUN_000183c8`.

Either way, **do not assume `A5 30 08` works on this device without testing.**

For DInput the app instead uses `DongleCommand.GetK1TempAutoTrigger`, which
builds the **`05 A0`** adaptive-trigger frame:

```csharp
SetTriggerParams(Params: paramsArr[1..5], ApplyMode: 0,
                 TriggerType: TriggerType, Mode: paramsArr[0]);
/* -> 05 A0 01 <apply> <trigType> <mode> <p0..p4> <additive checksum> */
```

with an additive checksum over bytes 0..10 stored in byte 11.

**That command is a no-op on this firmware.** In `FUN_00018138` the `0xA0` case
branches straight to the function epilogue:

```asm
01819a  a0ab    tcmp r3,#0xa0
01819c  1ac0    tjeq.n 0x000181d4
...
0181d4  106d    tpop { r4,pc }
```

so `05 A0 …` returns immediately without touching state. Two adjacent
findings reinforce that the trigger feature set is not wired for the `05`
transport at all:

- What *looks* like an `0xA0` handler in decompiler output at the end of the
  function is actually the **`0xEB`** case (`-0x15`), where `0xA0` appears only
  as a *subcommand value*: `05 EB A0` sets `state+0x34 = 1`. It is not a
  top-level `0xA0`. Reading the decompiler's `-0x15` as `0xA0` is an easy and
  real mistake.
- The `0xA5` dispatcher does have a genuine `0xA0` case, but it only sets
  `state+0x24 = 1` — a flag, no payload processing.

Combined with the missing `A5 30 08` handler, the picture is consistent:
**trigger haptics and adaptive-trigger payloads are inert on the `05`/DInput
path in firmware 6.7.5.1.**

### An application bug worth recording

In `SendTmpTriggerCmd`'s DInput branch:

```csharp
byte[] a = DongleCommand.GetK1TempAutoTrigger(1, array2);   // 05-framed
byte[] b = XinputCommand.GetK1TempAutoTrigger(2, array2);   // A5-framed!
DInputDataManager.GetCurrentInstance().SendByteArray(a);
DInputDataManager.GetCurrentInstance().SendByteArray(b);
```

The second call uses **`XinputCommand`**, not `DongleCommand`. On a DInput
connection that sends an `A5`-framed command through the DInput manager. Either
the DInput transport tolerates `A5` frames (the firmware does have a `0xA5`
dispatcher, so it might), or the right trigger silently fails. From static
analysis alone I cannot tell which; it looks like a copy-paste defect.

## Config model

The trigger motor settings are part of the device profile and round-trip through
the config blob. `GamepadConfigUtilsV2` reads a **29-byte** `listTriggerMotor`
block from config offsets **154..182**:

| index | field |
| --- | --- |
| 0 | `MainSwitch` (inverted: `== 0` means on) |
| 1–7 | Left `LineGear`: Type, Min, Max, Filter, Vibrlimit, Scale, TimeLimit |
| 8–14 | Left `MicrGear`: same seven fields |
| 15–21 | Right `LineGear` |
| 22–28 | Right `MicrGear` |

"LineGear" and "MicrGear" are two haptic profiles per trigger (a linear/main
mode and a micro/click mode), each with `Type`, `Min`, `Max`, `Filter`,
`Vibrlimit`, `Scale`, `TimeLimit`. The UI names `Min`/`Max` "Amplitude front
length" and "Amplitude center length" — so they are *position* bounds along the
trigger travel, not amplitude in the audio sense.

The stock `device_f3` profile ships:

```
LineGear: Type=1  Min=30  Max=80   Filter=5 Vibrlimit=1 Scale=50 TimeLimit=0
MicrGear: Type=255 Min=40 Max=120  Filter=5 Vibrlimit=0 Scale=50
```

`device_f4` and `device_fp3` ship byte-identical values, so the defaults say
nothing about which hardware actually has the motors.

## Verified vs inferred

Verified by reading code and cross-checking raw instruction bytes:

- `0x50` **is** in `FUN_00018138` (decompiler renders it `'P'`); `0xA2` is too,
  but nested under `0xFA`.
- `0x50 02` sets a trigger-mode flag and writes the driver; `0x50 03` writes two
  driver bytes and arms a 3000-tick counter.
- A separate `0xA5`-framed dispatcher (`FUN_000183c8`) exists and handles `0x30`
  and `0x50`; its `0x30` inner chain handles `01 04 05 0b 0c 0d` but **not
  `0x08`**, so the app's `K1SubCmdSetTriggerVib` has no matching case there.
- `A5 50 01` drives **both** grip motor pairs in one command
  (`FUN_0000b5d0(1,0x14,f[4],f[5])` then `FUN_0000b648(1,0x14,f[6],f[7])`).
- Trigger haptics go through a RAM pointer table + peripheral transfer, not PWM
  compare registers.
- DeviceId 80 = Vader 3 Pro, and 80 ∈ `TriggerVib`.
- The Space Station app has **no DInput path** that sends `A5 30` trigger-vib
  set commands.
- `05 A0` (adaptive trigger) is a **no-op** in `FUN_00018138`: it compares and
  branches directly to the epilogue.
- `05 EB A0` is a real command (sets `state+0x34 = 1`) and is *not* the same as
  `05 A0`. The decompiler renders both `0xA0` and `0xEB` confusingly.

Inferred, not verified:

- That pair B (`state+0x94/0x95`, registers `0x80079c`/`0x8007a4`) is physically
  the trigger motors. `A5 50 01` driving both pairs together is consistent with
  a "test both" action, but no code ties pair B to the trigger orifices.
- That `MicrGear`/`LineGear` map onto the two firmware profiles. The byte layout
  is known; the semantics of `Type` 1 vs 255 are not.
- What the `5A A5 A5 41` internal frame actually commands.
- Whether any firmware build handles `A5 30 08`. This image does not.
- Whether a Bluetooth DInput unit accepts `A5`-framed writes at all. If it does
  not, trigger haptics are unreachable over the transport this project targets,
  and that is the single most important unknown here.
- Whether the 3000-tick counter is time-, loop-, or sample-based.

## Practical implications

- Grip rumble (`05 0F`) and trigger haptics (`0x50` / `A5 30`) are **separate
  subsystems** at every layer: different frame prefixes, different parsers,
  different hardware paths, different host code paths.
- Grip rumble is the one that works on DInput: the app sends `05 0F` and the
  firmware has a real handler for it.
- Trigger haptics are the opposite: the app sends the set command **only** on
  XInput, and the firmware has **no** `A5 30 08` handler. Both halves are
  missing for DInput/Bluetooth.
- Reaching trigger haptics over Bluetooth would require (a) finding or inventing
  a `05`-framed trigger-haptic command, and (b) that command actually being
  implemented in the installed firmware. Neither is established.
- The `0x50` family *is* implemented and reachable via `05 50 02/03`, so if
  trigger haptics can be driven at all on this device, `0x50` is the more
  promising entry point than `A5 30`.

## Open questions

1. Does `05 50 02` / `05 50 03` actually produce trigger vibration? These are
   implemented, unlike `A5 30 08`. This is the highest-value device experiment.
2. Is grip motor pair B (`state+0x94/0x95`) physically the trigger motors? If
   yes, `05 0F`'s second pair already drives them and no separate command is
   needed. This would be the simplest possible answer.
3. What does the periodic `5A A5 A5 41` internal frame command?
4. Do later firmware versions add the `A5 30 08` handler? Comparing another
   version would settle whether the feature was ever exposed on DInput.

## Method notes

Three separate errors were caught during this analysis and are worth recording
so they are not repeated:

1. A byte scan used mask `0x2800` for `tcmp`; the correct mask is `0xA800`. This
   produced a false "`0x50` is not in the dispatcher" conclusion.
2. Literal-pool resolution needs `((site+4) & ~3) + imm8*4`. Two helper scripts
   used different (wrong) scalings and printed nonsense addresses.
3. The decompiler renders `0x50` as `'P'` and `0x5A` as `'Z'`, so searching the
   decompiled text for hex constants misses whole branches.

Every constant above was re-derived from raw bytes after these corrections.
