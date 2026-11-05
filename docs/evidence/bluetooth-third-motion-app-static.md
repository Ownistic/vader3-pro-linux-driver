# Third motion axis over Bluetooth: official Android app static record

**Artifact:** `local/research/android/flydigi-game-center-7.2.10.5.apk`, version
`7.2.10.5`, SHA-256
`0af8e56fd094b94bed1d4340ab02f995cf118b363e6aa321dc601f336d6ba7a6`.
**Provider:** REA Android (`jadx-headless-mcp` 0.7.1, engine artifact SHA-256
`6e5eacf500b64292bfb73c49797c1958f6ee44646e43e868039ae7feb573ff75`). The APK was
loaded into the provider's own `/tmp` workspace; it was not extracted into the
repository and was not executed.

**Scope.** Read-only static inspection of the APK's Bluetooth gamepad path and its
consumers. No controller was connected or commanded, no firmware was re-opened,
no saved Ghidra project was opened, no OTA characteristic was touched, and no
product file was edited. The firmware findings in
[third-motion-component-static.md](third-motion-component-static.md) and the USB
findings in [usb-third-motion-observation-2026-11-04.md](usb-third-motion-observation-2026-11-04.md)
are treated as prior evidence, not re-derived.

**Artifact integrity:** the APK SHA-256 was identical before and after this pass.
`git status` shows only the pre-existing working-tree changes from the UHID work;
this pass created only this Markdown file. No scratch files were required under
`/tmp/opencode/flydigi-ble-third-app/`.

---

## 1. Short answer

The official Android application implements **one** controller transport for
gamepads: Nordic UART Service (`6e400001/2/3`), with notifications enabled only
on `6e400003` and 20-byte (or shorter) single GATT writes on `6e400002`.

Its input decoder extracts **exactly two motion values** — a signed 12-bit pair
packed into notification bytes **11–13** — and has no third gyro axis anywhere in
the gamepad path. The previously unverified tail bytes **14–17 are not motion**:
they decode as two unsigned 12-bit **absolute cursor coordinates** plus a validity
bit, used to drive the app's floating-window mouse. The firmware's documented
seeded value `00 04 40 01` decodes exactly as cursor `(0x400, 0x400)` + valid —
i.e. screen centre — which closes the open question in
`third-motion-component-static.md` §1.

No app command selects a different motion layout, and no app-side callback,
driver buffer, or characteristic carries the third word (`0x8469ec + 0xb2` /
USB `B2`). **Conclusion for this artifact: there is no Bluetooth third-axis
packet contract to implement.** The third axis remains a USB / raw-six-word-frame
phenomenon as previously recorded.

Confidence: **high for this APK's implemented protocol** (instructions and byte
offsets are explicit). It is **not** a claim that the controller firmware cannot
emit a third axis over some other BLE mechanism; see §8.

---

## 2. Question checklist

| # | Question | Status | Evidence |
|---|---|---|---|
| 1 | Which BLE service/characteristics does the official app use for a gamepad? | answered | §4.1 |
| 2 | Does it enable notifications anywhere else (vendor char, OTA)? | answered for the gamepad path | §4.1, §4.8 |
| 3 | How many motion values does it decode, from which bytes? | answered: two, bytes 11–13 | §5.1 |
| 4 | What are bytes 14–17? | answered: absolute cursor pair + flag | §5.2 |
| 5 | Does any app command request a 3-axis / 6-axis layout? | answered: none found | §6 |
| 6 | Does the app-level or driver-facing consumer use a third axis? | answered: no | §5.3 |
| 7 | MTU / fragmentation for the frames? | answered: no MTU request; 3/4/20-byte single writes; 20-byte notifications | §4.2 |
| 8 | Is this app a reference for the Vader 3 Pro specifically? | partial | §8 |
| 9 | Does the saved C# vendor export add a BLE path? | answered: no NUS/BLE identifiers found | §8 |

---

## 3. Method and entry point

The gamepad path is not in `com.flydigi.sdk.bluetooth`'s public API surface that
earlier passes examined (`TelinkOtaFactory`, `AbstractDataInteractionController`,
`GamepadInfo`). It is the obfuscated controller `cb.p`, which extends
`AbstractDataInteractionController` and is reached through the `cb.s` singleton
and the app facade `r4.b`. The Android Flutter packages
(`assets/flutter_assets/packages/{controller,device,waspwing}`) contain only
images/translations; the BLE protocol is Java, not Dart.

Entry point inspected: `cb.p.onReceiveDataFromDevice(UUID, byte[])`.

---

## 4. Findings

### 4.1 The gamepad controller is NUS-only

`cb.p.<init>` builds the UUID set (ev_3760f9a9d01f93775c2e65627d7ed1ca0aa63b10e3a1d837c86e22e36eb1b729):

| Field | UUID | Role |
|---|---|---|
| service | `6e400001-b5a3-f393-e0a9-e50e24dcca9e` | NUS service |
| write | `6e400002-b5a3-f393-e0a9-e50e24dcca9e` | command writes |
| read/notify | `6e400003-b5a3-f393-e0a9-e50e24dcca9e` | input notifications |
| descriptor | `00002902-0000-1000-8000-00805f9b34fb` | CCCD |
| extra | `00002a26-…` (firmware revision), `0000180a-…` (Device Information) | version read |

`m0.onServicesDiscovered` (ev_5b0c8481e01147a45a06455aaf073eee559af56f495478f8aa42657a9b5b6f39)
calls `setCharacteristicNotification(readCharacteristic, true)` and writes
`ENABLE_NOTIFICATION_VALUE` to the `0x2902` descriptor of that one
characteristic. `cb.p.initCharacteristic` resolves write/read characteristics from
that service only. No vendor characteristic, and no second service, is used by the
gamepad session.

The vendor `00010203-…-0a0b0c0d2b12` characteristic remains confined to
`com.flydigi.sdk.bluetooth.ota.TelinkOtaFactory` (prior evidence), consistent with
the OTA-only finding; it is not part of the gamepad data path.

### 4.2 Write framing, rate and timeout

`m0.writeToBluetoothDevice` (ev_8c200582c5174e57776fffc9ec171029d58ebe65ec940c5c4cbce33bfe031f44):

* writes `pack.k()` directly, `setWriteType(1)` (write-with-response), to
  `6e400002`;
* keeps at least **30 ms** between writes (`lastTimestamp` guard), retries the
  same write up to 11 times on failure;
* arms `characteristicWriteTimeoutRunnable` **100 ms** after the write (the
  "500 ms" figure is not in this path).

No `requestMtu` appears in the inspected connect/write path. Command payload
lengths produced by the builders are **3, 4 or 20 bytes**, so each command is one
GATT write; there is no 16/32-byte fragmentation in this path. Notifications are
20 bytes in the normal case (`M()` also accepts a 14-byte form, §5.1).

Twenty-byte commands get a trailing checksum: `bb.b.a(byte[])`
(ev_ba67f0b4c5256b477b68d4e448e1b2f606f8a0d05dd264c06291433aaf654df9) writes
`sum(bytes[0 .. len-2]) & 0xff` into the last byte. Four-byte commands are sent
unchecksummed.

### 4.3 Notification dispatch

`cb.p.onReceiveDataFromDevice` (ev_5a0710e36a8ee19d47eac3812c12dfedde9c5dfe243d8f93114290cdfde7ca9c)
branches three ways:

1. **`A5 xx` 20-byte responses** (`value[0] == 0xA5`): `A5 01` → Switch-chip
   version from bytes 6–7 nibbles; `A5 A0` → device UUID string from bytes 2–11;
   `A5 A1` → a two-record loop (records at byte offsets `6 + 5*i`, `i = 0,1`) that
   is not consumed further in the inspected body.
2. **Status packet** `cb.b.q(value)` (ev_ab9545eb247b3258d910cebc44624b03ba45ae0d4207add17e46bc311e3fe2af):
   `length == 20 && value[0] == 0xAC && value[1] == 0xC0`. This updates device
   mode, chip type, battery and firmware version and is not an input report.
3. **Input report** `cb.b.M(value)` (ev_851ae411264e026bc174a18512814db30e72fafaa1c03c5730312e26c54acab0):
   `(length == 20 && value[18] == 0xFE && value[19] == 0x00) || length == 14`.

So the app's input frame is exactly the frame the firmware's NUS builder emits:
20 bytes ending `FE 00` (plus a legacy 14-byte form).

### 4.4 Status packet (`AC C0`)

From the status branch and `cb.b` helpers (each observed):

| Byte | Meaning | Function / evidence |
|---|---|---|
| 0–1 | `AC C0` | `q` ev_ab9545eb… |
| 8 | battery (255 for modes 16/48/50) | `j0` ev_16f9cf4566a6146bba9a060610ce4c787641894a34d4b9f25d6e8b4513e83739 |
| 9 | USB0/USB1 device state bits | `o0`, `c0` |
| 11 | mapping mode (0/1/2/3→1/4) | `m0` ev_d566320f71390fa94c7743364b48703e44918b6b311a07e49384a8d7c6be5593 |
| 13 | device mode | `l0` ev_80051f72b42f3071086ee01606867343e049a2cde85c787c15810be33108642e |
| 14 low nibble | chip type | `k0` ev_149b1bc898b6ddaa844c1efc23e90533b6e785919ecf9e9ceb422fb4bf00c90a |
| 14 high nibble | secondary id | `n0` ev_06eafb5b91e1b4d2add12c039a4f40a7b3306aa3fbf8d645381cb81123745434 |
| 16–17 | firmware version nibbles `a.b.c.d` | `cb.p.t0` ev_b3bb2a337f0bc1a92cac4ef0dfd76dc75e1e2d5faffb068c86b4b3303945c337 |
| 18 | `FD` → flashplay supported | `c` ev_bc98e31689d62f46580a42f7b7c5f6640cccdbf8d2cb7c68b5d3cc7d84d4e41c |

### 4.5 Input frame decoders

All offsets below are notification-array indices.

| Bytes | Decoder | Meaning |
|---|---|---|
| 0,1 | `cb.b.r0` ev_ad242fdaa606b7becb8a2e855996995e23da399964e7396c688d9d361ec05493 | left stick `(v-126.5)/126.5` |
| 2,3 | `cb.b.u0` ev_1807cc6437ae712afd0ed6520905db87983d5a9facd1bdaf61de666f32c8f303 | right stick |
| 6,7 | `cb.b.v0` ev_42e83a351114a8e6ec8b7446c1572662dd5dd9b46b9d7eb1f096ac9b201f417d | wheel pair (if `isSupportLunpan`) |
| 6,7 | `cb.b.s0` ev_7868e9ff869c9f96660ca057303f66661109537585cbcdf91b4e29e1fb47a58c | linear trigger values (if `isSupportLinearButton`) — same bytes, alternate meaning |
| 4,5,8,9 | `cb.b.d` ev_85683eb1ba9cec960c7b5ac4b2438c652cb9eea76b1b1ecfeda2adf286917d69 + key bit tests | buttons/triggers; `d()` gates the key scan |
| **11,12,13** | `cb.b.t0` ev_bba4ce83c66924f3fb12d3fa900ebb08bd2d6b25b2e6fd4ade6648ee29a4113a | **two signed 12-bit motion values** |
| **14,15,16** | `com.android.motionelf.tools.f.a/b` (cursor) | **two unsigned 12-bit absolute cursor coordinates** |
| 17 | `cb.b.U` + `FlyMouseEvent` | bit 0 = cursor/touch valid; other bits passed as click state |
| 18,19 | `cb.b.M` | tail `FE 00` |

### 4.6 The two motion values: exact bit math

`cb.b.t0`:

```
m0 = (b11 & 0xFF) | ((b12 << 8) & 0x0F00);  if (m0 & 0x800) m0 |= ~0xFFF;   // signed 12-bit
m1 = ((b12 >> 4) & 0x0F) | ((b13 << 4) & 0x0FF0); if (m1 & 0x800) m1 |= ~0xFFF;
return Pair(m0, m1);
```

No scaling, no third value, and no bytes outside 11–13 are read. The app's own
callback type is `kotlin.Pair<Integer,Integer>`:
`cb.r.onMotionEventRecognized(Pair<Integer,Integer>)`
(ev_89c03df0636b742bec7cc4ce9021684161d50ca679c161164a305c5a22f346ce). There is
no triple/list 3-axis motion callback in the interface.

This is the same two-word packing the firmware analysis described for
`0x8469ec + 0xae` and `+0xb0` over bytes 11–13.

### 4.7 Bytes 14–17 are cursor coordinates, not motion

This is the key new result.

* Gate: `cb.b.U(data)` (ev_fd53f7628b63dc00d5fb8931f33b737295469f993456ff16dac2334fb503265d)
  = `M(data) && data.length == 20 && (data[17] & 1) == 1`.
* Consumer: `com.android.motionelf.FloatingWindow.u(byte[])`
  (ev_dcb20afd7ce6ba96461cc4c913aa624c1b3ec76f2803a9f8729fdcd53b6c79c6). When
  the gate passes and a floating-window geometry exists, it posts a
  `FlyMouseEvent(tools.f.a(...), tools.f.b(...), bArr[17], true)`.
* Decoders:
  * `com.android.motionelf.tools.f.a`
    (ev_e86427d91958bdb7a8c238c66439895ae57b8e3c73c01b428626d37918e6a5f4):
    `x = (((data[15] & 0x0F) << 8) | (data[14] & 0xFF)) / 2048.0 * realWidth - startX`
  * `com.android.motionelf.tools.f.b`
    (ev_086822fa96b0d34117168185f7ee7ff3d08c234c454649dc5ef546bf2cf5c940):
    `y = (((data[16] << 4) & 0x0FF0) | ((data[15] & 0xF0) >> 4)) / 2048.0 * realHeight - startY`

So bytes 14–16 carry **absolute, normalized cursor position** (0…~2.0, centre
0x400 = 1024 → 0.5), and byte 17 bit 0 is the valid/touch flag. The packing is the
same 24-bit/two-12-bit scheme as the motion pair, but the values are coordinates,
not rates.

**This resolves the firmware document's seeded tail.** A centre cursor
`(0x400, 0x400)` + valid flag packs to bytes `00 04 40 01` — exactly the seeded
value recorded in `third-motion-component-static.md` §1.1 (`+0x46/+0x48 = 0x0400`,
byte 17 = 1). The tail is a touch/cursor pair, and calling it motion (or a hidden
gyro axis) would be wrong.

### 4.8 The app's motion consumers also use only two axes

* The app-level facade `r4.b.onMotionEventRecognized(Pair<Integer,Integer>)`
  (ev_1d5917fe6b0f2ebf4810984863f753e4acd976177b27c9bbe83f888732795b40) is an
  **empty method**; the main session ignores the motion pair.
* The flashplay/driver buffer `cb.e.a(boolean,boolean,byte[],boolean)`
  (ev_2961fe5d40cb230064a3ea4497d1ff8c8be9b7d875d653814cecb729ed5e228d) copies
  only `value[11]`, `value[12]`, `value[13]` for motion (to bytes 5–7, or 11–13 in
  keyboard mode). It never copies bytes 14–17 as motion.
* `com.flydigi.sdk.bluetooth.z` (the BluetoothSocket/SPP controller)
  `onReceiveDataFromDevice` (ev_dae49353cad22fce12c8c3704070ec292cc4d871cd1218f42fd9078dc0a9b617)
  only logs the payload; it parses nothing.

---

## 5. Outbound command surface

All constants come from `cb.e.<clinit>`
(ev_2e20bcf0799c0c544ad5bdc6a8593efa4a772e1a32416064a74a4b5ee1650848); each
builder below was decompiled. "Live" means a static caller chain reaches a
`FloatingWindow`/`ScanFragment` action; "dead" means zero incoming references in
the inspected DEX.

| Bytes | Builder | `cb.p` sender | Reached from | Status |
|---|---|---|---|---|
| `A5 A0 00 00` | `cb.e.l()` | `getUUIDCommand()` | connect when UUID empty | live |
| `A5 01 00 00` | `cb.e.g()` | `o0()` | connect when Switch version empty | live |
| `13 05 <p> 00` | `cb.e.i(int)` ev_8dd0ad51… | `y0(int)` ev_ebd9a5a3… | `cb.s.O` ← `r4.b.g` ← `FloatingWindow` cases 3/12/33 | **live** |
| `13 10/11/12/13 00 00` | `cb.e.u(int)` ev_b28a04b5… | `P0(int)` ev_b8e5cb4a… | `cb.s.a0` ← `r4.b.q/s/t` ← cases 26/10/36 | **live** |
| `41 01/02/03` | `cb.e.e(int)` ev_ff08e0c8… | `F0/E0/N0` | `cb.s.Q/P/Y` ← `r4.b.m/l/n` ← T-mode cases 6/17/7 | **live** |
| `13 02 00 00` | `cb.e` field `f12274d` | `O0()` | `cb.s.Z` ← `r4.b.p` (DFU), `ScanFragment` | live |
| `13 01/04/06`, `14 01` | fields/getters | `I0/K0/J0/M0` | `cb.s.U/W/V/b0` | **dead (0 refs)** |
| `AC 6F 00` | `cb.e.f()/j()` | `D0()/z0()` | `cb.s.N` (0 refs), `cb.s.G` ← `r4.b.h` ← UART-end case 5 | mixed |
| `AC 89 00` | `f12281k` | `L0()` | `cb.s.X` ← `r4.b.o` ← UART-start case 34 | live |
| `AC 30/21/B0/A1` | `cb.e.s(bool,bool)` | `G0(bool)` | `cb.s.S` ← `r4.b.k` ← cases 1/13/39 | live |
| `AC FE 01 00` | `cb.e.d()` | `A0()` | `cb.p.u0` when mapping mode 4 | conditional |
| `2F <b> 00…` (20 B) | `cb.e.h(byte)` | `w0(byte)` | `cb.s.E` ← `r4.b.f` ← LED case 31 | live |
| `11 <a> <b> <c>` | `cb.e.o/n` | `x0/S0` | `cb.s.F`, `cb.s.h0` (h0 dead) | mixed |
| `12 <r><g><b>` | `cb.e.v` | `R0` | `cb.s.g0` | live |
| `10 <i> 01 01` | `cb.e.m(int)` | `Q0(int)` | `cb.s.f0` | dead |
| `52/53`, `54`, `40 <i>`, `51 …` (20 B) | `cb.e.w/x/y/z` | various | test/config helpers (`tools.a.c`, `FloatingWindow.U2`) | live |

**Semantics established from the action names** (`FloatingWindow.onStartCommand`,
ev_c3db606fe7536a8287cef7123f1cb7ea3a971e7190da79230c2aca85c687623f):

* `13 05 00/01/02` = T-mode joystick reset — actions
  `REMOTE_ACTION_T_MODE_RESET_JOYSTICK` (param 0),
  `START_RESET_JOYSTICK_SMART` (param 1), `FINISH_RESET_JOYSTICK_SMART` (param 2).
  This is calibration/reset, **not** a report-layout selector.
* `13 10/11/13` = device mode switch: driver (0), T (1), MI8 developer (3).
* `41 01/02/03` = T-mode delivery: only touch, only GATT, touch + GATT.
* `AC 6F` / `AC 89` / `AC FE 01 00` = UART/driver start-end and mode handshake.

**None of these changes the number of motion axes.** There is no "enable 6-axis",
"select report", or "raw gyro" command in the app's command set. The only
selector-like commands (`41 01/02/03`) choose touch vs GATT delivery of the same
frames; they do not add a field.

---

## 6. Behavioral flow (connect → input)

1. Connect GATT, `discoverServices()`, take `6e400001`, resolve
   `6e400002/6e400003`, enable notifications on `6e400003` via `0x2902`.
2. Send 4-byte status/UUID requests (`A5 A0`, `A5 01`) as needed; receive 20-byte
   `A5 xx` responses.
3. Controller sends 20-byte input frames ending `FE 00` (or 14-byte frames).
4. `cb.p` decodes sticks (0–3), triggers/wheel (4–7), buttons (4–10), motion pair
   (11–13), cursor pair (14–16), flag (17).
5. Callbacks: `onJoystickLeft/Right/Wheel`, `onKeyClickRecognized`,
   `onLinearValueRecognized`, `onMotionEventRecognized(Pair)`, and the raw byte[]
   forward to `r4.b` → `FloatingWindow.u`.
6. `FloatingWindow.u` uses bytes 14–17 for the floating mouse and broadcasts the
   raw bytes to the ADB driver; it never interprets a third gyro axis.

---

## 7. What this rules out, and what it does not

**Ruled out for this artifact (bounded):**

* A different gamepad GATT service or vendor data characteristic in the app.
* An app-side 3-axis or 6-axis motion decoder.
* An app command that selects a 3-axis report.
* The reading that bytes 14–17 are motion; they are cursor coordinates.

**Not ruled out / not addressed:**

* Firmware could emit a third axis on a transport or frame type no host app
  consumes (the `5A A5 21 18` six-word frame remains the standing lead; its
  delivery mechanism — advertising/PDU vs proprietary — is still unresolved).
* A different vendor host build could implement a private 3-axis BLE contract
  that this APK does not contain.
* The APK's own UI may not target the Vader 3 Pro at all (see §8), even though its
  NUS decoder matches the firmware exactly.

---

## 8. Boundaries

* **Vader 3 Pro targeting.** No case-insensitive `vader` string was found in any
  of the five DEX files (`strings` over `classes*.dex`), and the bundled
  `assets/product_list.json` lists older mobile products (wasp/apex/wee/cyberfox).
  The APK's `cb.p` is nevertheless a generic vendor NUS gamepad protocol
  implementation with the exact `FE 00` / 11–13 packing of the 6.7.5.1 firmware.
  Treat it as strong protocol evidence, not proof that this app version manages a
  Vader 3 Pro.
* **Saved C# export.** No `gyro`, `6e4000`, `Nordic`, `NUS` or BLE-UART
  identifiers exist in the 720-file saved Space Station decompilation (grep found
  only unrelated `MINUS`/`UART_OTA` serial-port constants). The C# host is
  USB/serial-oriented and contributes no BLE contract.
* **Flutter AOT.** `libapp.so` was not analyzed. Flutter packages are asset-only
  in this APK and the BLE calls are Java, so no Dart protocol path was assumed;
  this remains a coverage gap if a hidden Dart BLE client exists.
* **Static only.** Decompiled Java is reconstruction; no APK code was executed.
  Incoming-reference counts are static and miss reflection/dynamic dispatch.
* **Provider limits.** Class search is case-sensitive substring over top-level
  class names; method-level text search is unavailable. The negative is therefore
  "no third-axis decoder in the inspected path and its consumers", not a formal
  whole-APK absence proof.

---

## 9. Evidence ledger

Observed (all with retained Evidence IDs in the REA session):

* NUS UUID set, single notified characteristic, connect path — §4.1.
* 30 ms pacing / 100 ms ack timer / direct 3–4–20-byte writes — §4.2.
* `A5 xx` / `AC C0` / `FE 00` dispatch and frame gate — §4.3.
* Two-axis motion bit math — §4.5–4.6.
* Cursor decoding of 14–16 and flag 17 — §4.7.
* Empty app motion callback and two-axis driver buffer — §4.8.
* Command constants, builders, reachability — §5.

Inferred:

* The cursor bytes are the touch position for the floating-window mouse (the
  consumer and scaling prove the interpretation; the physical source inside the
  controller is not observed here).
* The `13 05` parameter meanings follow the action names, not a device response.

Unknown:

* Whether any firmware state emits a third axis on NUS, and on which frame type.
* Whether the `5A A5 21 18` six-word frame leaves as BLE advertising/PDU.
* Whether a different official host build implements a 3-axis BLE contract.

---

## 10. Practical consequence

Do not add a third-axis field to the current NUS implementation on the strength of
this app: the vendor host's own parser has no place for it, and the obvious spare
tail bytes are a cursor pair that the app would misread. If I want real
third-axis motion, the established positive contract remains the USB `04 FE 66`
report (bytes 29/30 for `B2`). Any BLE attempt would need new evidence from the
firmware's radio/PDU path or a host build that actually parses three axes — not a
reinterpretation of bytes 14–17.
