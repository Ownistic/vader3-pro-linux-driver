# Space Station 3.4.4.3 static analysis: protocol findings

Static inspection only. The Windows installer was never executed and no
proprietary code is copied into this repository. Everything below is a
description of observed behaviour in the shipped binaries, with the exact
artifact identity that produced it.

## Artifacts inspected

Extracted with `innoextract` 1.10-dev (built from upstream master, because the
packaged 1.9 cannot parse Inno Setup 6.3), from
`FlydigiSpaceStation_setup_3.4.4.3_Release.exe`
(SHA-256 `f6e480b75016288797080ba5257aa8cbc46ebc724401ee841680f8c755ca0d50`).

| File | SHA-256 | Role |
| --- | --- | --- |
| `app/LibCommon.dll` | `120a8ba2ed34b8cee7080328c4557426c4354bfe2dea224784283caf2f8bc607` | command builders, config codec, key tables |
| `app/FlydigiSpaceStation.dll` | `a72e3cc035f0660e60d45c5d8e5ddbb1ef44077ee50c47321f43fe32d14d636d` | WPF UI, live-input decode, test view |
| `app/Service/GameController.ServiceHandler.dll` | `574010220bef8dfe678717d52306d158c896daa84f5ae6b552dd2dbf1473e7c3` | HID transport, per-mode data managers, motion decode |
| `app/LibHID.dll` | `027ceae84a667e4400f143b95ff37ead4e9696029f4ddd9e19f8ebd203161f1d` | HID open/read/write wrapper |
| `app/LibBLE.dll` | `216b2fae187ef098fa153c6b8d9e2b95a2943c599d68f3ef911a3ce749efdc47` | BLE SPP client (used only for the BS1 cooler) |

REA evidence: `ev_2894f9caccf660e63f76cdc38ba9b4137eac8aa686b2fe72a3c8c96839e064cf`
(FlydigiSpaceStation.dll identity), `ev_bd8cf46968dd5a1d0e5afda9a1252fddd76188c7957826eae727333014081efe`
(LibCommon members), `ev_4f1da9029a239bf58fd6102ab1133f93f7cb2df0f38f5f754e63fd220b8dae28`
(service native boundaries: IL-only, all protocol logic is managed CIL).
Method bodies were read via ILSpy 11.1.0 on the extracted DLLs; the decompiled
tree is outside Git in `/tmp/opencode/flydigi/decompiled/`.

## Command framing

Two distinct framings exist and must not be conflated.

**Framing A — vendor/config commands**, built in `LibCommon.Command.DongleCommand`
and `XinputCommand`. Fixed-length blobs starting with `0x05`:

```
byte 0     0x05                (packet length prefix)
byte 1     subcommand
byte 2..n  payload
last byte  additive checksum: sum of all preceding bytes, mod 256
```

`DongleCommand` uses `perPackageLength + N` lengths (15 bytes typical for
DInput); `XinputCommand` builds 15-byte packets whose byte 0 is `0xA5` and whose
final byte is the same running sum. Both transports share the checksum rule, so
one checksum routine covers them.

**Framing B — the WPF app ↔ Windows service IPC.** Not a device protocol.
Messages are length-prefixed JSON (`DataTransCommandBuilder`): 2-byte type
(`1` message, `2` heartbeat, `3` event), 2-byte length, UTF-8 body. Live input
reaches the UI as base64 report blobs inside these JSON messages
(`GamepadNativeDataNotice`, actions 18/21). Anything implementing this project
does not need to reproduce framing B.

## Mode switch: DInput → "origin" (vendor) mode

`DongleCommand.getSwitchOriginData(bool state)` produces:

| state | bytes |
| --- | --- |
| enable | `05 EE 01 00 …` (checksum `F4`) |
| disable | `05 EE 00 00 …` (checksum `F3`) |

The service sends this over the DInput HID stream when the UI asks to listen to
"mapping" data (`FDGTransData.SetDataListening` → `DInputDataManager.SendByteArray`).
While it is enabled the device emits 32-byte vendor reports (`0x04 0xFE …`)
instead of, or alongside, the normal 21-byte ID-1 input reports. `0x04 0xFE 0x66`
specifically is the mapping/raw frame the test UI consumes.

This is a **command, not a mode the device is permanently in**: `state=false`
restores normal DInput reporting. Nothing in this repository has sent either
form to hardware.

## Report layouts

### Normal DInput input report (passive, already verified on hardware)

Report ID `0x01`, 21 bytes on the wire (descriptor declares 15). Field map from
the owner-coordinated captures is in `docs/PROTOCOL.md`; the D-pad is a hat in
byte 9's low nibble and M1–M4 are byte 11 bits, i.e. the two are separate fields.

### Vendor / origin report (32 bytes, unverified on hardware)

Space Station's own converter `XInputDataUtils.FormatDataFromDInput32` maps a
32-byte vendor frame to a 15-byte normalised frame:

| normalised byte | vendor source | meaning |
| --- | --- | --- |
| 0..3 | 17, 19, 21, 22 | stick axes X/Y pairs |
| 4, 5 | 9, 10 | button bytes 1, 2 |
| 6, 7 | 23, 24 | LT, RT analog |
| 8 | 8 | button byte for Menu/Home/Back |
| 9 | 7 | button byte holding C/Z/M1–M6 |
| 10 | 0 | unused |
| 11, 12, 13 | 4, 5, 6 | **gyro X/Y packed 12-bit** |
| 14 | 3 | status |

The key point for the project: in vendor mode byte 7 carries the same
`C, Z, M1..M6` eight-bit group that the service decodes with its
`gamePadKey[2, 8]` table, and bytes 4–6 carry 12-bit angular-rate data that the
passive stream does not contain at all.

The 12-bit unpack, from `KeyMouDataParse.handleMotionData`. **This differs by
both firmware generation and transport** — the earlier note that assumed one
formula was wrong. The 32-byte frame is first renormalised by
`XInputDataUtils.FormatDataFromDInput32` (`normalised[11..13] = vendor[4..6]`),
then:

For **DInput** (the Bluetooth DInput case), unconditionally and without
consulting the origin-motion setting:

```
gyroX = n11 | ((n12 & 0x0F) << 8);
gyroY = (n13 << 4) | ((n12 & 0xF0) >> 4);
sign-extend at 0x800 with -4095; then if value > 0, subtract 1
```

For **XInput**, gated on `IsUseOriginMotionData` and firmware generation:

```
new motion (12-bit): gyroX = b11 | ((b13 & 0xF0) << 4)
                     gyroY = b12 | ((b13 & 0x0F) << 8)
                     sign-extend at 0x800 with -4095
old firmware (10-bit): gyroX = b11 | (((b13 >> 2) & 3) << 8)
                       gyroY = b12 | ((b13 & 3) << 8)
                       sign-extend at 0x200 with -1024
```

Note the asymmetries that matter for a reimplementation:
- Nibble order is **not** the same on both paths: DInput splits X's high bits
  into `b12 & 0x0F` and Y's into `n13<<4 | (n12&0xF0)>>4`, while XInput puts X
  in the high nibble. Copying the wrong branch silently swaps axes and scale.
- There is an unconditional `value--` on positive readings, i.e. a one-LSB
  bias in the vendor code. Reproduce it to match Space Station exactly.
- Vendor mode packs gyro into three bytes; the passive stream has
  **no** motion bytes at all, which is why the shake test moved nothing.
- Whether a Bluetooth DInput unit honours `05 EE 01` and streams 32-byte vendor
  frames is **untested**. All of this is inference from the Windows USB/2.4 GHz
  path until it is tried over Bluetooth.
- Gyro is only consumed when the config's `MotionMapping.MapType == 3`
  (`openMouse` gate); otherwise the service clears its motion flag.

The firmware itself contains a matching 12-bit two-axis packer
(`FUN_0001a6d0`), which proves the device packs motion as two 12-bit values in
three bytes. Its nibble order is its own, so it is evidence of the packing
width, not of the host-side vendor-frame offsets — see
[vader3pro firmware disassembly](vader3pro-firmware-disassembly.md).

## Rumble

`DongleCommand.getTestMotorCommand(int left, int right)`:

```
05 0F <left> <right> 00 00 00 00 00 00 00 00 <checksum>
```

Left/right are 0–255 levels. The UI calls this with `0xFF, 0xFF` for a full test
and with `0, 0` to stop, and also derives it from trigger pressure in the test
view. This is an output command in the same framing as the mode switch, so it is
the natural rumble candidate — but no Bluetooth delivery path has been proven.

## Key tables (authoritative for this firmware family)

`LibCommon.Common.Constant` / `GamePadKey` define the canonical 32-key order:

| index | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| +0 | Up | Right | Down | Left | A | B | Select | X |
| +8 | Y | Start | LB | RB | LT | RT | THL (L3) | THR (R3) |
| +16 | C | Z | M1 | M2 | M3 | M4 | M5 | M6 |
| +24 | Menu | – | – | HOME | BACK | – | – | – |

Joystick derived directions occupy indices 161–168 (`lup`…`llu`, `rup`…`rlu`)
and 160 is the joystick zero. This table is what produced the byte-11 bit order
`0x04`/`0x08`/`0x10`/`0x20` for M1–M4 and is the strongest available evidence
for that assignment, since the owner could not confirm press order.

## Device configuration format (`.fdg`)

Default device configs ship as JSON (with trailing commas, so a strict parser
needs lenient handling). `app/default/device_f3/config.fdg` is the Vader 3 Pro
family profile: `Config.Version` `0.2`, `Config.PackageLength` `76`, and a
`KeyMapping` array of 32 entries in the key order above.

Each `KeyMapping` entry carries `KeyId`, `Key`, `MapKey`, `IsMap`, `MapType`,
`MapData` and Turbo fields. `IsMap == true` means the physical key is remapped
to `MapKey`; the shipped default profile contains
`C→THUMBL`, `Z→THUMBR`, `M1→UP`, `M2→DOWN`, `M3→RIGHT`, `M4→LEFT`.

**That default mapping is the likely source of the earlier confusion.** The
profile does not say the M keys are indistinguishable from the D-pad in the
report; it says the Vader 3 Pro's stock configuration *forwards* them to D-pad
outputs. The report-level fields remain separate (proved above), so a profile
that maps M1–M4 to themselves would expose them as independent buttons.

Serialised to the device by `GamepadConfigUtilsV3.ConvertByteByGPConfig` as a
`PackageLength`-byte payload with a stable field order (version, length, LED,
96-byte key map at offset 13, joystick, trigger, motion, motor, trigger-motor,
lunpan, auto-trigger, random, revision, macro, name, extra joystick data, macro
repeat). The 96-byte key map is 32 entries × 3 bytes:
`{targetKeyId, turboType, turboCount}` where `0xFF` in `targetKeyId` means "no
change". This is the payload a Linux tool would have to build to make M1–M4
independent; it is read back with `GamepadConfigUtilsV3.ConvertGPConfigByByte`.

## Bluetooth relevance (unresolved)

Space Station is a Windows application whose transport is raw HID
(`LibHID.HIDClient`, matching on VID/PID and optionally `mi_02` interface).
`FDGDevice` lists vendor IDs `0x04B4`/`0x04B5` with PIDs `0x2410`–`0x2415`, and
`0x045E`/`0x028E` for the Xbox-named identity. **`d7d7` does not appear anywhere
in these binaries**, and `LibBLE` (a WCH BLE-SPP wrapper) is only used for the
BS1 cooler, not for controllers. So Space Station's controller support was never
demonstrated on the `d7d7:0041` Bluetooth identity this project targets; the
command formats are the transferable part, the transport is not.

## What is still unverified

- Whether `05 EE 01` works over Bluetooth HID-over-GATT, and what report the
  device then emits.
- Whether rumble `05 0F …` produces physical vibration on Bluetooth.
- Whether the 32-byte vendor frame on Bluetooth uses the same offsets as the
  Windows USB/2.4 GHz path.
- The exact `config.fdg` write sequence over Bluetooth, including whether the
  config can be applied without switching to XInput.
- Steam Input recognition in any of these modes.
