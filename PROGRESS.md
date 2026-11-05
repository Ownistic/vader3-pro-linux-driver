# Progress

## Current: UHID-only cleanup and local main integration complete

The owner authorized removing the old backend and merging `feat/uhid-emulation`
into local `main`. The previously uncommitted runtime fixes and research were
preserved in checkpoint `before-uhid-only-cleanup`. Baseline at `cb7895a` plus
those changes: `cargo test --all-targets` passed 145 tests. The saved scope and
acceptance checks are in [the cleanup record](docs/phases/02-uhid-only-cleanup.md).
Implementation now removes the old runtime, generic force-feedback backend,
probes and mapping/scripts. The sole startup path is DualSense Edge UHID. Existing
UHID configs remain readable; explicit uinput selection fails. Implementation
checks now pass 88 Rust tests, 35 Python tests, strict Clippy, formatting and
release build after migration-guide repairs, BlueZ unique-owner authentication
and the owner's independent pitch-inversion correction. Independent verifier,
reviewer and scoped convergence all passed for driver manifest
`3becafa68e0a7aecb3d62d2e839af6c092dfe2c2d576f4919c51f1d608f40acf`
and release `e8679ac65dda8254839ef1eab620b8e69d1defb2ebef24ae40e5c984c6f84a41`.
Local integration is complete: research commit `b0ae40a`, runtime cleanup commit
`4145716`, and merge commit `eb5f0fe`. The merge tree exactly matched the reviewed
feature tree; both feature tip `4145716` and previous main `d45e356` are ancestors
of `main`. The working tree was clean after merging. Nothing was pushed, and the
feature branch was retained. No cleanup/integration work remains.

The Legion Go's last hardware-tested installation was v4; this cleanup did not
replace or restart it. Steam's physical
`04b4/2412` ignore entry was owner-confirmed to leave only the emulated DualSense
Edge visible. Two Bluetooth motion values work; no verified third value exists.
Historical checkpoints below must not be read as the current implementation or
as acceptance evidence for the new cleaned build. Deployment and remaining
hardware acceptance were outside this task.

## UHID work stopped at repeated-blocker limit

The optional backend is saved but **not validated or ready for deployment**.
The first integrated Legion Go run repeatedly failed its AC selector write;
the first inspected journal already exceeded the owner's three-recurrence
limit. No further experiment or repair was attempted. The transient test was
stopped, its Edge HID removed, and the unchanged installed uinput service
restored active/running (PID 51748). The captured journal counted 37 automatic
retries before shutdown; no selector status or motion notification was observed.
The underlying D-Bus error was not retained, so its cause remains unknown.

Tested release binary SHA-256:
`e986bc4014f3cefcba2cd643f87740179a1a0ccdf54b40e69ee35b6cff472fb0`.
Source-manifest SHA-256:
`517bc645e211498d4740eb3af347d678ebfd34388160ae66834bcc2346e232a8`.
`cargo test`: 129 passed; `cargo clippy --all-targets`: no errors, twelve
existing example warnings, none in src; `cargo fmt --check` and release build
passed. Read-only review allowed a first controlled run, not final acceptance.
No old-code regression demonstration or final independent verifier ran.

All final hardware checks (rumble/stop, M1–M4/C bindings, Z behavior,
suppression/reconnect, motion-active latency) remain open. Full details and
the exact next action requiring renewed authority are in
`docs/evidence/uhid-emulation-2026-11-04.md`. Earlier next-action checkpoints
below are historical and do not authorize continuing past the stop condition.

## UHID emulation gate (2026-11-04)

- Active branch: `feat/uhid-emulation`; research checkpoint `0a9e333`, isolated
  spike and decoder checkpoint `ab522e8`. The commit agent packaged the prior
  uncommitted research at the owner's request; the tree was clean at launch.
- Saved spike: `driver/examples/uhid_dualsense_probe.rs`; evidence:
  `docs/evidence/uhid-dualsense-probe.md`. Existing daemon/uinput behavior is
  unchanged. No files under `local/research/` were changed.
- Baseline: `cargo test` 109 passed. Spike-specific tests: 3 passed.
  AFT could not provide Rust diagnostics because rust-analyzer is unavailable.
- Probe launched from `ab522e8` with synthetic motion and a 1,200-second
  lifetime. Kernel `7.3.1-1-cachyos` bound `playstation`, created a readable and
  writable hidraw node plus gamepad, motion, touchpad and headset input nodes.
  This is kernel evidence only, not Steam or physical motion evidence.
- Next action: owner inspects Steam for the emulated DualSense Edge and gyro
  options while the probe runs. Do not integrate the backend before this gate.
  AC persistence, physical rumble, buttons, suppression/reconnect and latency
  remain untested for UHID. No AC command was sent by the spike.
- Host correction: Steam is on the owner's Legion Go, accessible using
  `ssh legion`. The local probe finished normally. The same hash-verified
  binary from `ab522e8` is now running for 1,200 seconds from remote
  `/tmp/flydigi-uhid-spike-ab522e8/`; the installed driver is untouched.
  Bazzite kernel `7.2.11-ogc4.1.fc44.x86_64` bound `playstation`, exposed
  hidraw/gamepad/motion nodes and delivered feature requests and output
  reports. Owner Steam UI observation is still required before integration.
- The owner subsequently confirmed **Recognized; gyro available** on the
  Legion Go. Spike gate passed for `ab522e8`. The synthetic probe was stopped,
  exited 0, and its HID node was observed removed. Next: AC persistence check.
  No physical `d7d7:0041` node was found on the Legion Go at this point; ask
  the owner to establish Bluetooth DInput there before the passive baseline.
- Owner connected the physical controller on the Legion Go without a power
  cycle. The diagnostic observed 2,002 candidate notifications in 15 seconds.
  After an owner-confirmed full power cycle/reconnect, the same subscription
  observed zero. Explicit AC 01 then produced AC C0 selector 1 and 1,531 data
  notifications. The enabled stream did not survive that cycle; exact selector
  value while off and broader persistence remain unproven. Artifact hashes,
  commands and acceptance matrix: `docs/evidence/uhid-emulation-2026-11-04.md`.
  Next: implement the explicitly opt-in backend; default uinput must never
  subscribe to motion or send AC. The stream is currently enabled.

## Experimental Bluetooth motion implementation (in progress)

The owner requested an opt-in Steam-motion prototype on 2026-11-04. The
Nordic UART TX path has been observed on hardware, but only two processed
values have been identified; axes, units, third axis and Steam sensor backend
are unresolved. `driver/src/motion.rs` adds a tested experimental decoder;
it is not connected to the daemon or exposed to Steam. Baseline `cargo test`
passed 107 tests; after the decoder it passed 109. Rust-analyzer was
unavailable to AFT. The owner requires preserving rumble and all existing
features; a UHID experiment that temporarily drops them is not authorized.
`/dev/uhid` was made accessible during this session. Separate uinput sensor
nodes are not sufficient for SDL's common-parent association according to
[padctl issue 81](https://github.com/BANANASJIM/padctl/issues/81); this is
external evidence, not a local Steam verification. Next establish a
Steam-recognized virtual motion backend without regressing the existing
uinput gamepad and rumble, then implement
opt-in transport lifecycle, provisional mapping and disconnect handling.
The historical native-gyro deferral below predates this evidence.

## Current state (pre-release)

The project now has a **working userspace driver**, not only research. Summary;
details and evidence follow below.

| Requirement area | State |
| --- | --- |
| Read physical Bluetooth DInput reports | Working, verified on hardware |
| All 21 buttons incl. M1-M4, C, Z as independent inputs | Working; proven in firmware and by measurement |
| Virtual pad exposes 21 buttons + 6 axes + `FF_RUMBLE` | Working, verified via kernel bitmaps and SDL |
| Rumble (Bluetooth) | **Solved** — 4-byte NUS frame; verified by sustained vibration |
| Rumble (USB DInput) | Verified on hardware (`mi_02`, `0xFFA0`) |
| Duplicate physical controller suppressed | Working (`EVIOCGRAB`, crash-safe) |
| Input latency through translator | Measured **0.03 ms** median |
| Steam recognition of the six extras | **Untested** — needs the mapping file and a Steam run |
| Native gyro | **Deferred to post-1.0** — absent from the Bluetooth DInput stream, and the vendor-mode switch is not reachable on the working channel |

Phases 02 (input prototype) and 05 (rumble) are effectively **complete**. Phase 03
(Steam integration) is implemented but its Steam-side acceptance check has not
been run, so it stays open. Phase 04 (motion) is **deferred to after 1.0**: the
blocker is transport reachability of a single firmware flag, not effort, and
`docs/evidence/gyro-deferred.md` records the three candidate routes so the work
does not restart from scratch. Phase 00 was independently verified/converged; its
record is `docs/phases/00-setup.md`.

Everything recorded below this section is the historical evidence trail from
research, retained because it is the basis for the corrections made later. Where
an earlier note contradicts the table above, the table and the newer evidence
documents supersede it — in particular, the early statement that Bluetooth rumble
was "unresolved" is obsolete.

## Baseline

- Repository: local checkout; branch `main`, no commits at baseline.
- Initially empty working tree except `.git/`; no repository AGENTS/override file.
- Python 3.14.8 available. No baseline tests existed.
- At baseline no physical controller or Steam test had been performed. A later
  owner-confirmed connection supplied metadata only; see the live checkpoint.
- All created project paths are part of this setup task; no preexisting user
  changes were overwritten. No commits or system configuration changes made.

## Saved outputs and implementation checks

- All requested root/docs files, phase records, evidence policy/acquisition record,
  `tools/inventory.py`, offline tests, and a GitHub Actions workflow saved.
- `python3 -m unittest discover -s tests -v`: **17 passed**, Python 3.14.8,
  including the subsequently added recorded-descriptor integrity test.
- CLI help: exit 0. Initial local `--descriptor` inventory: exit 0, no vendor matches,
  no metadata errors; not a hardware or Steam test.
- Scoped `aft_inspect` after edits: 0 errors/warnings; authoritative diagnostics
  for all 4 Python files after the descriptor-fixture addition. GitHub CI itself has not run.
- `git diff --check`: exit 0, but files are untracked on unborn main, so this
  command alone does not validate their content. Independent check pending.

## Research checkpoint

- Initial triage covers all seven requested sources/issue; direct padctl Zig
  inspection pinned to `049953d0e5588266333af3a91e8e783d82cdd544`.
  Details/limitations: `docs/RESEARCH.md`. No upstream source incorporated.
- padctl already has hidraw/DeviceIO, six-extra canonical state, generic UHID,
  native Edge and IMU paths. Existing hidraw may suffice for physical Bluetooth;
  definitions-only native Steam support is not established. No runtime language
  or virtual identity chosen.
- Installer downloaded outside Git, 105741288 bytes, SHA-256
  `f6e480b75016288797080ba5257aa8cbc46ebc724401ee841680f8c755ca0d50`.
- REA PE resource evidence and exported bundle retained under
  `/tmp/opencode/flydigi/`; exact path/ID/limits in
  `docs/evidence/installer-3.4.4.3.md`. No Windows code executed.
- Official page HTTP 429, signature authentication unresolved, static Inno
  extraction/.NET tracing pending (`innoextract`, `osslsigncode`, `dotnet`,
  `ilspycmd` not found). No software/MCP configuration installed or changed.
- Planning specialist could read but could not write due to its session edit
  permissions. Parent is saving the scoped plan; requirements are unchanged.

## Blockers / untested requirements

No default-vendor device was found by the initial sysfs probe (later DInput
connection was found and documented below). The later passive checkpoint has
14 coordinated raw input reports; no real Steam experiment exists. All full
F1–F5/A1–A11 acceptance checks remain untested. Bluetooth rumble
is unresolved, not proven impossible. Phase 01 is incomplete. Project license
selection remains open; no runtime deployment/permissions/service is installed.

## Live hardware checkpoint after initial verification

- Owner connected and confirmed the Xbox-named device is the Vader 3 Pro; mode
  may be XInput, not independently established. Metadata-only observation:
  Bluetooth bus `0005`, `045e:02e0`, descriptor 306 bytes, SHA-256
  `74a5dbcedf6d6a88f9eb6e66fbeb5a255ba800fd084390cbf325e0c4c74492d7`.
- No explicit extra/IMU fields in that descriptor; rumble-style output is
  advertised, but no physical feedback or reports have been tested.
- Matched hidraw node mode 0600, not readable by agent account. No node access,
  grab, writes, or permission changes attempted. No addresses stored in Git.
- Saved sanitized descriptor fixture and `docs/evidence/bluetooth-xbox-mode.md`;
  full redacted inventory retained outside Git. All product criteria untested.
- Owner was asked to switch normally to **Bluetooth DInput** and reconnect for
  comparison. After the owner reported the switch done, inventory still found
  no d7d7 matches and `bluetoothctl devices Connected` returned no devices.
  Owner was asked to pair/reconnect the DInput identity in system settings.
- On reconnection, DInput is visible as Bluetooth **d7d7:0041**, descriptor
  163 bytes, SHA-256
  `227924e94560bea5149d6a75a8bb225143a1e4b0685075dea729a30649bc1f22`.
  BlueZ exposes HID-over-GATT and writable HID-report, vendor and Nordic-UART
  characteristics. No characteristic reads/writes/notifications requested;
  only cached UUID/Flags metadata queried. Node still mode0600/unreadable.
- Both sanitized descriptor fixtures and mode evidence records saved; actual
  controls, sensors, rumble and Steam support remain untested. DInput descriptor
  alone is not proof of the physical button mapping or a rumble limitation.
- Initial independent verifier passed S1–S5 on the 19-file version with manifest
  `836714a3814d0d0f8a1cc29fa48fecf58744e2971bc5c92be371a7d1e8a2cead`.
  Subsequent fixture/evidence changes were reverified as recorded below.

## Final setup gates

- Final 24-file implementation/evidence version, manifest SHA-256
  `193b5220af6bbee398ac4195f762db7e625cde583d8536a79764f8207c3dc248`:
  independent verifier **PASS S1–S5**, convergence **PASS**, no setup blockers.
  Manifest format: sorted `{path, bytes, sha256}` compact key-sorted JSON.
- 17 tests passed on Python 3.14.8, including subtests for both recorded
  descriptors; 4/4 Python diagnostic reports, 0 errors/warnings. Verifier
  independently decoded DInput as report ID1, 112 input payload bits, no
  output/feature main items. This is descriptor validation, not live input.
- Xhigh review omitted as disproportionate for safe metadata-only scaffolding;
  architecture/runtime/permission changes still require their later gates.
- Scribe could inspect but not write under child-session edit permissions;
  parent applied its factual closeout updates. No application code/fixtures
  changed after the final gates, only completion records/initial-probe labeling.
- Setup deliverables packaged in the initial commit; see git log for commit identity. No system configuration or privilege changes made.

## Next action

For live Phase 01, coordinate M1/M2/M3/M4 singles plus C+Z and all four rear
buttons held together/releases, then remaining standard axes/buttons and motion.
Rediscover node and recheck read-only access before each acquisition. Continue
official-page/signature/extraction work and padctl's native Steam experiment
before selecting runtime architecture. Do not implement a guessed Bluetooth parser.

## Phase 01 passive input checkpoint (after setup closeout)

- Owner reconnected DInput and explicitly ran the suggested per-node temporary
  ACL. Agent verified readable=true, writable=false; no agent/root-daemon or
  persistent udev permission changes. The older mode0600 restriction above
  describes the baseline, not current effective access.
- Five-second untouched request yielded zero reports. First 30-second button
  window yielded 14 axis-jitter packets, but owner confirmed missing that window;
  those are not button evidence.
- Coordinated capture stopped after owner's done reply: 14 reports, ID1,
  **21 bytes each**, although the descriptor declares 15 bytes including ID.
  Raw capture flushed incrementally and closed with requested-stop.
- Owner-completed A/B/C/Z sequence gives A=byte9/10, B=byte9/20,
  C=byte11/01, Z=byte11/02 (hex masks, ID included in offsets), each set/clear.
  C/Z differ from A/B in this sample; combinations and M1–M4 remain untested.
- External source `/tmp/opencode/flydigi/passive-dinput-coordinated-dqsg8uzu.jsonl`,
  1802 bytes, SHA-256
  `2ad2b739ef459c92117cc2ea3b50a6710cec5e3bd0b9bec6773761496f6e27fe`.
  Public sanitized fixture and `docs/evidence/passive-dinput-abcz.md` saved.
- No output commands or Steam tests. Phase 01 and all full product acceptance
  criteria remain incomplete; gyro/rumble feasibility unresolved.
- Capture increment checks: `python3 -m unittest discover -s tests -v`:
  **19 passed**, Python 3.14.8. Scoped diagnostics authoritative for 4/4 Python
  files, 0 errors/warnings; `git diff --check` exit 0. Independent verifier
  **PASS**: source hash/size, all 14 packet bytes/times, privacy, descriptor
  discrepancy, sequence and 19 tests confirmed. Checkpoint aggregate SHA-256
  (sorted `path NUL contents NUL`, 11 files):
  `9255a58a569514d89450d89a099d58dbc32bc58c626ff053a55461dd69e3b6ec`.
  Only this factual result was added after verification; Phase 01 not closed.
  Unrelated untracked
  `.cortexkit/` belongs to the tooling/user environment and is left untouched.


## Session 5-7 capture increment: full DInput control map

Owner ran four more coordinated capture batches back-to-back (stop-on-"done",
read-only, no output commands). Tooling: the saved reusable recorder
`tools/capture_reports.py` (argparse: `--vid/--pid/--node/--max-seconds/--max-reports/--out/--correlation-id`).

- Session 4 (LT/RT/Home/C/Z): LT = byte10 `0x04` + byte13 analog ramp
  `0x01`→`0xff` (true analog, 90 sample steps on a slow press); RT = byte10
  `0x08` + byte14; Home = byte12 `0x80` (isolated); C/Z reconfirmed byte11
  `0x01`/`0x02`.
- Session 5 (M1-M4 isolated, owner-confirmed presses): M1/M2/M3/M4 emit byte11
  `0x04`/`0x08`/`0x10`/`0x20` — identical to the D-pad bits. In passive DInput,
  the M keys cannot be distinguished from the D-pad. Session-1 byte10
  `0x10`–`0x80` attribution to M1–M4 was wrong: those were Start/Select/L3/R3.
- Session 6 (isolation): Select = byte10 `0x10`, Start = byte10 `0x20`,
  L3 = byte10 `0x40` (+ byte1/2 stick wiggle), R3 = byte10 `0x80` (+ byte3/4).
- Session 7 (sticks one axis at a time + shake): byte1/2/3/4 = LX/LY/RX/RY.
  LX full-left hold = `0x80` plateau; LX full-right = `0x7f`; LY full-forward =
  `0x80`; right stick symmetric. Idle bytes depend on session (near `0xfd`),
  i.e. values behave like signed, session-recalibrated center. Whole-controller
  shake produced no large byte1-4 swings ⇒ passive ID-1 stream carries no gyro;
  native gyro likely requires a vendor output command (unsent).
- Session-1 byte9 low bits `0x01`–`0x07` remain unassigned (occurred 92–110 s,
  unrequested touches).

Evidence policy: raw JSONLs stay outside Git (`/tmp/opencode/hidraw-capture-{ni7k4w80,_rr2qmvl,6ik4jjni,itqlgkl2,w6m51s37}.jsonl`;
digests in `docs/evidence/passive-dinput-abcz.md` and project memory). Decoded
facts merged into `docs/PROTOCOL.md` ledger and the passive-capture evidence doc.
Phase 01 remains incomplete: no rumble/gyro/Steam/simultaneity/mode-switch
evidence yet. Next: vendor-mode/extra-button research (PRD blocker), rumble
command discovery, then Steam runtime experiments.
