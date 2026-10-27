# Progress

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
| Native gyro | **Not implemented** — absent from the Bluetooth DInput stream |

Phases 02 (input prototype) and 05 (rumble) are effectively **complete**. Phase 03
(Steam integration) is implemented but its Steam-side acceptance check has not
been run, so it stays open. Phase 04 (motion) is not started and is blocked by a
protocol limitation rather than by effort. Phase 00 was independently
verified/converged; its record is `docs/phases/00-setup.md`.

Everything recorded below this section is the historical evidence trail from
research, retained because it is the basis for the corrections made later. Where
an earlier note contradicts the table above, the table and the newer evidence
documents supersede it — in particular, the early statement that Bluetooth rumble
was "unresolved" is obsolete.

## Baseline

- Repository: local checkout; branch `main`, no commits.
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
