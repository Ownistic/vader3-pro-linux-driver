# Handoff — 2026-11-03

## Latest: UHID-only cleanup verified and merged locally

I authorized removing the legacy backend and merging into local `main`.
The sole runtime now creates a DualSense Edge through `/dev/uhid`. Legacy uinput
implementation, generic force-feedback probes, SDL mapping and old contrib
scripts were removed. Existing `backend = "uhid"` configs remain readable;
explicit uinput selection is rejected. Current install and migration guidance is
in the [README](../README.md) and [migration guide](uhid-migration.md).

Independent verification, review and scoped convergence passed for driver
manifest `3becafa68e0a7aecb3d62d2e839af6c092dfe2c2d576f4919c51f1d608f40acf`
and release SHA-256
`e8679ac65dda8254839ef1eab620b8e69d1defb2ebef24ae40e5c984c6f84a41`:
88 Rust tests, 35 Python tests, strict all-target Clippy, formatting and release
build pass. Review's BlueZ sender-authentication blocker was corrected and
re-reviewed. Notifications now require the resolved unique owner and authenticated
ownership changes invalidate samples.

My latest hardware feedback was inverted pitch with correct yaw on
the running v4 daemon. Independent `motion.invert_x = true` now corrects pitch
in code; existing yaw inversion is unchanged. This new executable has **not**
been deployed or hardware-verified. The Legion Go's earlier v4 artifact and
Steam ignore setting were not changed by this cleanup. Third gyro component,
measured acceleration/calibration and other live acceptance gaps remain open.

Local integration is complete. Research is committed as `b0ae40a`, the runtime
cleanup as `4145716`, and the feature was merged into `main` at `eb5f0fe`.
The merge tree exactly matched the reviewed feature tree. Both feature tip
`4145716` and prior main `d45e356` are ancestors of local main; the tree was clean
after merging. Nothing was pushed or deployed, and the feature branch remains.
No cleanup or integration action remains; hardware acceptance is still open.
Full criteria and gate history: [cleanup record](phases/02-uhid-only-cleanup.md).

## Latest: Steam shows only the emulated controller

On 2026-11-05 UTC, I confirmed that only **DualSense Edge Wireless
Controller** remained in Steam after adding `04b4/2412` to the root
`InstallConfigStore.controller_blacklist` in Steam's `config.vdf` and restarting
Steam. The physical evdev grab was already active; it did not hide enumeration.
No driver change or kernel unbind was needed. The same v4 daemon/PID and binary
hash remained active. This persistent Steam-only setting must be undone for
native use in Steam after stopping the driver. Post-change input behavior was
not separately retested.

- Reusable setup/undo: [Steam controller visibility](steam-controller-visibility.md).
- Exact backup, hashes and verification: [visibility evidence](evidence/steam-physical-device-hiding-2026-11-05.md).

The background Android BLE investigation finished and saved
[its report](evidence/bluetooth-third-motion-app-static.md). It identifies the
NUS tail as cursor coordinates and finds only two motion values in the inspected
SDK path. It does not establish a third-axis Bluetooth contract or prove global
absence. No new controller commands were authorized from the report. Further
third-axis work is deferred while I focus on documenting visibility.

## Latest: I report working controls; third motion component unresolved

The resumed hardware checks proved that `AC 01` can return a D-Bus `NoReply`
while the controller actually answers selector 1 and streams data. The worker
now verifies observations instead of aborting on that ambiguous method result.
Rust passive and power-cycled movement probes passed. The integrated daemon
exposed gyro in Steam but no ordinary controls: zero HID reports with NUS active.
A focused firmware trace found that AC mode replaces the HID builder and includes
ordinary inputs in NUS bytes 0–10. A new parser/runtime integration and X/Y swap
correction are saved. I confirmed A, sticks, triggers, M1–M3 and corrected
axis selection. Physical X/Y labels and yaw sign then needed correction;
the updated candidate passes 142 tests. After free testing, I reported
everything working correctly except the missing third gyro axis. This confirms
the current mappings in that test, not independent latency/reconnect evidence.
The third emulated component remains deliberately zero: no verified third-value
Bluetooth packet contract has been found. The standard live firmware-version
characteristic reports `6.7.5.1`, matching the analyzed version string but not
proving image identity. Further findings and bounds:
[third-component investigation](evidence/third-motion-component-static.md).
Remaining hardware checks and independent gates are still open.
**Not fully accepted.**

USB investigation then found a real third-component candidate: after a data
cable retry, my roll capture received 3,943 `04 FE 66` reports
in eight seconds; word `+b2` ranged -8,322..8,501 while the other two known
components remained small. No enable command was sent. USB full-motion support
is **not implemented** and this does not add a third Bluetooth value. I
explicitly chose **Bluetooth-only scope**: do not add a USB backend. USB was
unplugged and Bluetooth reconnected; the unchanged v4 daemon logged physical
reattach and resumed NUS input. The focused official Android BLE SDK investigation
completed without live writes; see the latest report link above. Exact USB
descriptors, field positions and limits:
[USB third-component observation](evidence/usb-third-motion-observation-2026-11-04.md).

At my explicit request the old installed uinput service/binary were
disabled and archived on the Legion Go. Do not restore them automatically.
Only transient `vader3p-uhid-test-v4.service` is running, with no time cutoff or
fallback hook; it is not boot-enabled. Current binary SHA-256:
`616d9e1b5f2e808561ea2213271e0c5db6136525cbce031caffd6a501da4c715`.

Exact artifacts, observations, service caveats and next action:
[runtime retest evidence](evidence/uhid-runtime-retest-2026-11-04.md).
Static protocol contract: [NUS input prefix](evidence/nus-input-prefix-static.md).

## Latest: integrated UHID run stopped at retry limit

The optional backend is saved but **not a validated working motion feature**.
First integrated test on the Legion Go repeatedly failed the worker's AC
selector write, with no observed status/data. More than three consecutive
failures were visible, reaching my stop condition. No further repair
or experiment was attempted. The transient test was stopped and its HID
removed; the unchanged installed uinput service was restored active/running.

129 Rust tests passed, all-target clippy has no errors or src warnings (twelve
existing example warnings), formatting and release build passed. Exact tested
source/binary hashes, review scope, complete failure/restoration evidence and
remaining acceptance items: [UHID implementation evidence](evidence/uhid-emulation-2026-11-04.md).
The worker logger omitted the D-Bus error reason. Resuming requires renewed
authority beyond the retry limit; first preserve that error and compare the
Rust request with the successful Python diagnostic. Do not mistake the passed
synthetic Steam gate below for final integrated hardware acceptance.

## UHID spike checkpoint — 2026-11-04

Active branch: `feat/uhid-emulation`. The commit agent saved prior research as
`0a9e333` and the motion decoder plus isolated synthetic UHID spike as
`ab522e8`. The existing daemon and uinput backend are unchanged. Baseline
`cargo test`: 109 passed; spike-specific example tests: 3 passed.

Steam is installed on my Legion Go / Bazzite, reached by `ssh legion`
(account `deck`), not the development host. The hash-verified spike from
`ab522e8` ran there, bound `playstation`, and I confirmed Steam
recognizes it and offers gyro. This passes the initial compatibility gate,
not physical motion, rumble, extra-button or latency validation. The probe
has been stopped. Exact artifact hashes, host evidence and limitations:
[UHID spike evidence](evidence/uhid-dualsense-probe.md).

Next: establish whether the AC-enabled stream survives a controller power
cycle before permitting daemon AC writes. No `d7d7:0041` node was found on
the Legion Go after the spike; I need to connect Bluetooth
DInput there before the passive baseline. No AC command has been sent by
this work. Production integration, all hardware validations, regression
checks and final independent gates remain pending. Do not change
`local/research/` or send `13 04 00 00` / `13 05`.

**Current research index:** [research-index.md](evidence/research-index.md).
The 2026-11-04 static pass adds an analog calibration trace, Android OTA
evidence, and corrections to bootloader and LED persistence claims. Research is
in progress; the index records incomplete coverage and next static steps.
The F3-specific Windows updater's observed chunk protocol and limitations are
in [f3-windows-updater-static.md](evidence/f3-windows-updater-static.md).
The sensor probe selector and the separate `04 FE 66` USB-bound builder are now
traced through an endpoint-shaped FIFO in
[USB motion routing](evidence/usb-motion-routing-static.md), including the
response formatter's prioritized substitution and input gate. Next resolve its
input paths, synchronization and register/descriptor mapping, and the distinct outgoing
staging buffer's peripheral identity; identify axes without assuming sensor type.
[Staged-frame transport](evidence/staged-frame-transport-static.md) traces that
buffer through pointer publication and peripheral-register setup. Comparison
with the TLSR8258 SDK supports an RF interpretation, but the actual chip,
address aliases and protocol remain unverified; it does not prove a GATT
notification. Continue
the unreviewed code and secondary-firmware inventory in the index.
The receive/queue trace and a bounded search for the staged status byte are
in [staged-frame transport](evidence/staged-frame-transport-static.md). The
`FUN_00010980` payload writer is `0x10960`, reached from the Nordic UART
attribute callback `0x8b0c`; see [BLE callback trace](evidence/ble-attribute-callback-static.md).
The older attribution to `FUN_00010838` was incorrect. A bounded pointer-base
search found an RF-receive-DMA-shaped setup for the nearby `0x00847278`
buffer, but no proven write to the payload fields; see the same transport
document for instruction addresses and the next trace. The inspected receive
queue copies into `0x008477cc` and dispatches via shared command handlers,
not directly into the `FUN_00010980` fields. The ATT write selection and
callback branch are now traced in the BLE callback document. Its dispatcher
entry and opcode jump table are identified, as is an MTU-exchange-shaped state
calculation; upstream packet validation remains open. A separate [static
trace](evidence/static-subsystems-2026-11-04.md#sensor-state-words-to-nordic-uart-notification-shaped-output)
connects two sensor-state words to a Nordic UART notification-shaped builder;
one selector is cleared by initialization and set by a short `0xac` input;
the packet enters a capacity-gated queue whose consumer publishes slot
addresses to a peripheral-shaped register. The optional transform appends a
four-byte computed value, and a paired input path compares four bytes; a
16-slot outbound descriptor initializer is only a candidate until its loader
is identified. Context helper register offsets match a published Telink AES
map. A state-2 branch sets the gate and calls context setup; a conditional
state-2 writer and selected setup-input sources are now traced. One registered
intermediate callback conditionally calls that writer; input validation,
seed-source identity, chip and mode remain unverified. Other selector writers, actual delivery
and the remaining sample words remain unresolved.
The saved TX attribute has a client-configuration descriptor; an `AC C0`
status-shaped notification can report the selector in byte 10. See the
[subscription and status clues](evidence/static-subsystems-2026-11-04.md#nordic-uart-subscription-and-selector-status-clues)
and physical follow-up: on 2026-11-04, two accepted TX subscriptions saw no
notifications at rest or during movement I confirmed. No selector command
was sent in those baselines. In a subsequent `AC 01` test I authorized,
an `AC C0` status reported selector 1 and 12,038 data notifications followed;
bytes 11–13 changed, but unlabeled movement does not identify gyro axes. The
selector was not reset after the test; persistence and live firmware version
remain unknown. See the same evidence document for the hardware observations.
Marked subsequent movement phases showed the first packed value varying most
in left/right rotation and the second most in forward/back tilt, under a
tentative signed-12-bit interpretation; motion type and physical axes remain
unverified. See [marked phases](evidence/static-subsystems-2026-11-04.md#marked-movement-phases).
Follow-up [holds, translation and rotation-speed comparison](evidence/static-subsystems-2026-11-04.md#stationary-holds-translation-and-rotation-speed)
showed larger ranges during faster rotation but also variation during
translation and hand-held tilt; the signal's physical interpretation remains
unresolved. [Two sensor register-map comparisons](evidence/static-subsystems-2026-11-04.md#sensor-register-map-comparison)
strongly support processed gyro-related values on the ST-shaped and QST-shaped
branches; the active hardware branch, scale, signs and third axis are unknown.
A direct caller at `0x20cd6` and the packet helper at `0x20898`
are now traced in the BLE callback document; the registered upstream callback
is invoked through an indirect path. Its packet pointer is derived from an
earlier argument under documented state gates. A wider trace identifies its
consumer in the main-loop ring (`0x008440d0 + 0x4c`) and an RF-DMA-shaped
ring producer. The registered callback at `0x370c` can set the ring entry's
selection flag to `0x40` and return a retention result; its other branches and
the type-3 helper's downstream validation remain partial. The helper copies a
fixed 24-byte slice into state. The image has a candidate eight-entry,
64-byte-stride ring descriptor. Published Telink retention-layout documentation
supports the matching image/RAM offsets, but this firmware's loader and complete
incoming validation remain unresolved. A wider descriptor-reference scan found
an additional slot writer but no proven stride/count initializer or overwrite
in the reviewed paths; aliases remain open. See the BLE callback document.
The [secondary firmware triage](evidence/secondary-firmware-static.md) now
records dongle startup and conditional receive-to-endpoint-shaped paths under a
selected TC32 hypothesis, including a `04 FE 66` serializer, marker-queue
linkage, copied queue initializer and timer-0-shaped interrupt setup; exact chip,
cadence, later overwrites and protocol remain unverified. The decoded startup
zeroes the twelve report-source bytes; an alternate converter can leave prior
values in place, while its exceptional helper clears the scheduler selector.
Queue/cache gates leave stale endpoint serialization unproven. The helper's
conditional startup pointers and command sequences are strongly consistent with
SPI-NOR read and erase/program. The readback path checks erased/default fields
and derives a value from four bytes, but full integrity validation, actual flash
identity and persistence are unverified. A post-read initialization routine
clears the twelve report-source bytes, and a separate counter-gated path can
set the scheduler selector to 1; restoration from saved byte `+0x12` is not
yet established. A configuration-adjacent alias was traced to a distinct
queue and byte-copy path, not a verified restore. Two producers and selected
callers for that queue, plus the startup branch skipping the callback copy,
are traced. Register-fed branches copy input to the shared buffer in a path
consistent with USB endpoint 5/6. Two statically selected configurations
declare endpoint `0x05` with a 64-byte maximum packet; neither declares
`0x06`. The saved Windows F3 SI-upgrade route delegates EXE URLs to an
external updater, while its ZIP branch is gated to another component type;
the retained SI ZIP is locally consistent with its extracted image, whose
high-entropy data include repeated 1,024-byte-spaced blocks and repeated
32-byte XOR-difference patterns across several distances. This does not
identify an encoding; its internal format and target CPU remain unresolved.
A cross-model
Telink handbook describes endpoints 5/6 as OUT and a
configurable cache maximum. A USB-reset-shaped branch clears computed endpoint
control offsets `0x120..0x127`, but not the maximum register. A broader
computed-base scan identified a generic writer and checked selected callers;
other writes, actual count bounds, endpoint-6 reachability, and runtime
callback values remain unknown.
A separate incoming record
path compares a CRC-shaped result against its own bytes `+0x12..+0x13`; it is
not the configuration object.
The SI image's
architecture is still unresolved. No controller action or flashing is part of
the current investigation.

**Follow-up:** See [firmware follow-up](evidence/firmware-followup-2026-11-03.md)
for application-derived USB stick offsets, corrected serializer and sensor
interpretations, and static Bluetooth command findings. The revision in that
follow-up is the baseline for the new investigation; the historical state below
describes the earlier session.

The bootloader signature and recovery claims below are hypotheses, not proven
facts. The cited serial updater belongs to an LCD screen-upgrade flow, not an
established Vader 3 Pro main-MCU path. See the correction in
[bootloader verification](evidence/bootloader-verification.md).
The historical statements below that Bluetooth *cannot* emit motion or that
its entire command surface is known exceed the evidence. The documented tests
exclude only the paths and conditions they exercised; indirect routes and
uncharacterized handlers remain open.
The chord's `0xfe` write targets a settings buffer, not an established USB
report buffer; the flag is distinct from the `0xEE` flag. See
[chord evidence](evidence/fe-report-toggle.md).

State of the project at the end of the session that took gyro from "deferred" to
"answered, with one option left untested".

Repository: `git@github.com:Ownistic/vader3-pro-linux-driver.git`, branch `main`,
HEAD `bd0be0b`. Working tree clean. `cargo test`: **107 passed**.

## What is delivered and working

The userspace driver is installed and running on the Legion Go as a systemd user
service, with an SDL mapping drop-in so the mapping survives reboots.

| Capability | State |
| --- | --- |
| Sticks, triggers, 15 standard buttons | correct in Steam |
| M1–M4, C | bind as paddles and `misc1` |
| Z | reaches the pad; no Steam slot exists for it |
| Rumble | play/stop fixed; sustained rumble via NUS |
| Reconnects | self-healing across `hidraw8`/`hidraw9` |
| Axes | conventional `ABS_*` layout, verified in Steam |

Nothing in the driver changed this session. All work here was research.

## What this session established

Ordered by what a future session is most likely to need.

**1. Switch mode gives gyro over Bluetooth on stock firmware.** Pairing the
controller as a Pro Controller (`057E:2009`) brings up `hid-nintendo`, which
exposes a second evdev node with `ABS_X/Y/Z` (accelerometer) and `ABS_RX/RY/RZ`
(gyro) at roughly 330 Hz, plus rumble through the standard force-feedback API.
No driver is involved. Cost: M3, M4, C and Z are never transmitted in this
personality, and M1/M2 alias L3/R3.
Evidence: `docs/evidence/switch-mode.md`.

**2. The vendor report carries extras *and* motion together, over USB.** Sending
`05 EE 01` on the `mi_02` interface switches the stream to 32-byte report `0xFE`
at exactly 500 Hz. Bytes 11–16 are three angular-rate pairs (still ≈ 14–80 counts
of spread; rotating ≈ 5 000–6 800), bytes 29–30 a wide-range pair, byte 7 the
extras (C `0x01`, Z `0x02`, M1–M4 `0x04`–`0x20`), byte 9 the standard buttons.
Evidence: `docs/evidence/vendor-report-usb.md`.

**3. Bluetooth cannot emit that report, by three independent routes.**
`0xEE` exists in two dispatchers but has no Bluetooth producer; the physical chord
`FN + M1 + M2` does set the same flag and acknowledges with a rumble, yet the
Bluetooth stream stays report `0x01` at 21 bytes across two captures (289 and 275
reports, both all-`0x01`). Order does not matter. The Bluetooth builder is
separate from the wired one.
Evidence: `docs/evidence/fe-report-toggle.md`.

**4. The full Bluetooth command surface is now documented**, including the
structural fact that frames of **5–19 bytes reach no handler at all** — which
explains several historical silent failures.
Evidence: `docs/evidence/ble-command-reference.md`.

**5. The bootloader almost certainly checks CRC only.** No 128- or 256-byte
incompressible block exists anywhere in the image, and the updater transmits only
length plus CRC32, so a signature could not be validated. This is an inference
from the interface contract, not an observation of the verifying code.
Evidence: `docs/evidence/bootloader-verification.md`.

## Negative results — do not repeat these

- **Bluetooth DInput motion.** Tried and closed by three routes: vendor command
  (`0xEE`), GATT output path, and the physical chord. All three either cannot
  reach the builder or do not change the Bluetooth report.
- **`A5 A0` is not a battery query.** The `0x50` byte is a *fallback* written when
  the real source is invalid, so it reads 80 % forever. Do not expose it.
- **The `U` opcode is not an arbitrary write.** `0x8472e8` is the key-remap table.
- **The Switch descriptor is not in any shipped image.** It is assembled at
  runtime, which is why a Switch-mode patch is expensive.
- **SDL cannot honour a touchpad mapping on a uinput device.** Six extras cannot
  all be bindable; one slot is unreachable by design.

## The one promising lead left

`13 04 00 00` is matched by `FUN_000105a4` against the literal at flash `0x23f90`,
and calls `FUN_00013dc4()` — a full peripheral teardown with a busy-wait and RF
shutdown — plus `FUN_00013e60()`, which writes registers `0x308`, `4`, `1`.

**It is plausibly the entry point into the firmware-update mode**, which would be
the recovery path that made flashing risky. It is also plausibly just a reset.

Untested. Testing it while wired is the cheap experiment: if the device
re-enumerates as something new, that is a finding; if it simply resets, it costs a
reconnect. Over Bluetooth it would drop the link.

## Next candidates, cheapest first

1. **Six uncharacterised reachable handlers** — `12`, `13 02`, `13 10`, `AC`,
   `BA 40`, `A5 01`. These are where dormant capabilities would live: LED control,
   deadzones, calibration, polling rate. All are reachable over the channel the
   driver already uses.
2. **Derive the vendor report axis layout from `FUN_0000094c`.** The stores are
   authoritative and need no hardware. Captures narrowed it to bytes 19, 21, 22
   (stick-like) and 23, 24 (trigger-like) but could not fix the pairing, so no
   decode code was written against it.
3. **`13 04 00 00`**, as above, with the wiring caveat.

## Environment notes that cost time

- **`local/research/` is gitignored and holds everything**: firmware images,
  decompiled Space Station C#, the Ghidra project, the Android APK and extracted
  assets, captures, and the analysis scripts. It survives reboots; `/tmp` does not.
- The Ghidra TC32 processor module lives **outside** the repository and its
  `tnegs` patch reverts on system-level changes. Re-apply it after any reinstall,
  then verify against function `0x10a30` (780 functions should decompile).
- The flat decompile used for all analysis is
  `local/research/scripts/vader3pro-fixed-decompile.c` — this is the file quoted
  throughout `docs/evidence/`.
- **hidraw ACLs reset on every reconnect.** Expect to ask me for
  `sudo chmod a+r /dev/hidrawN` after each power cycle.
- The controller's node moves between `hidraw8` and `hidraw9` across reconnects;
  resolve it by walking `/sys/bus/hid/devices` rather than assuming a number.
- **Capture timing races.** Short windows finish before I start. Use
  long windows (3+ minutes) and analyse by activity detection.
- A transient **digitizer personality** appeared once (`04B4:2412`, 196-byte
  touch-screen descriptor, three finger contacts). It reports only on touch, so
  captures against it are silent. A reconnect restores the gamepad descriptor.
  Cause not isolated.

## My preferences worth honoring

- English only, in replies, code, docs and commits.
- Hardware experiments are authorized, including commands that may disconnect.
  When I must act — reconnect, grant access, press a button, confirm a
  felt result — **stop and ask with the `question` tool** rather than guessing.
- Never present a successful write as proof the device acted. Only an observed
  response counts.
- Button and control tests must be **one control at a time**, because I
  cannot see most console output. Sequences of many presses get confused.
