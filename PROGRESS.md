# Progress

## Current increment

Phase 00: **complete**, independently verified/converged. Initial requirements
saved in `PRD.md` before other project files. Scope/checks/results:
`docs/phases/00-setup.md`. Phase 01 remains incomplete.

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
connection was found and documented below); no live target reports or
real Steam experiment exist. All F1–F5/A1–A11 remain untested. Bluetooth rumble
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

For live Phase 01, obtain owner-authorized, narrowly scoped
read access to the rediscovered target node, then passive input captures. Continue
official-page/signature/extraction work and padctl's native Steam experiment
before selecting runtime architecture. Do not implement a guessed Bluetooth parser.
