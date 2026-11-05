# UHID-only cleanup and main integration

## Authorization and outcome

I requested removal of the old virtual-controller backend and merging
`feat/uhid-emulation` into local `main`. Make the current DualSense Edge UHID
runtime the only product path. Preserve working Bluetooth input, NUS motion,
feedback, reconnect neutralization and physical evdev grabbing.

I separately require Bluetooth-only product support. USB reference
research and historical evidence remain documentation, not supported backends.

## Baseline and boundaries

- Starting branch: `feat/uhid-emulation`, HEAD
  `cb7895a2b8b32f8ee7c2851833d7de500e944aff`.
- Local `main`: `d45e356`. Preserve its existing commit and all branch research.
- Existing uncommitted runtime fixes and research are part of the merge, not
  disposable changes. Checkpoint: `before-uhid-only-cleanup`.
- Baseline `cargo test --all-targets` in `driver/`: 145 tests passed.
- Remove obsolete uinput modules, generic-pad descriptors/encoders, legacy
  force-feedback machinery, probes, mappings and setup steps when not needed by
  the retained UHID path. Keep useful physical/BLE/UHID diagnostics.
- Default and sole runtime: UHID. Keep existing `[pad] backend = "uhid"`
  configuration readable if practical. Explicit `uinput` selection must fail,
  not silently select a different runtime.
- Runtime, sample config, permissions, service instructions, CI and current
  architecture/testing/developer documentation must agree on `/dev/uhid`.
- Do not erase historical evidence or rewrite original measurements as UHID
  results. Retain PRD requirements and clearly distinguish partial acceptance.
- No new third-axis implementation, firmware writes or USB backend.
- No remote deployment or restart of the Legion Go's running v4 daemon.
- Merge locally after checks. Do not push or delete branches without request.

## Acceptance checks

| Criterion | Required evidence |
| --- | --- |
| UHID is the only implemented backend | Inspect runtime, module exports, CLI/config behavior; no reachable uinput creation/force-feedback backend |
| Bluetooth input/motion/feedback preserved | Retained and updated decoder, report, feedback, neutralization and lifecycle unit tests |
| Configuration migration is explicit | Tests for default UHID, existing UHID config and rejection of explicit uinput config |
| Bluetooth notification trust | Unique BlueZ-owner filtering and callback authentication; spoofed broadcast/unicast rejected; authenticated ownership changes invalidate samples |
| Pitch correction requested during cleanup | Independent `motion.invert_x` reverses pitch without changing yaw; signs apply after swapping; document no hardware retest |
| Install/use documentation is coherent | Review active guides, service/permission examples, retained diagnostic scripts and internal links |
| Static build quality | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets`, `cargo build --release` |
| Research tooling retained | `python3 -m unittest discover -s tests -v`; any affected contrib tests/checks |
| Independent gates | Verifier, risk-based reviewer, scoped convergence; fix returned blockers |
| Git integration | Coherent commits, clean working tree, local main contains cleaned branch |

My earlier hardware observations apply to the recorded v4 executable,
not automatically to the new build. The third Bluetooth motion component remains
zero; sensor units/calibration and outstanding hardware gates remain limitations.
This cleanup is not a declaration that all original PRD acceptance checks pass.

## Checkpoint

Implementation removed the old backend modules, generic probes and obsolete
contrib mapping/scripts. The direct Edge runtime is now the sole startup path.
`[pad] backend = "uhid"` remains a compatibility setting; `uinput` is rejected.
Discovery retains the recorded `04b4:2412` descriptor match, limited to bus 5,
with the documented caveat that bus 5 alone does not prove Bluetooth transport.

Implementation checks passed: formatting, strict all-target Clippy, 85 Rust
tests, release build, 32 Python tests, retained Python diagnostic compilation and
`git diff --check`. The smaller Rust suite reflects removal of legacy modules,
not a claim that their behavior was ported. Release SHA-256:
`2a4f49b4b47ccc31c765ebae3039b69e521ec2aa19609ab638da8ec29d8e6f13`.
AFT reported partial diagnostics because rust-analyzer was unavailable; compiler
and Clippy checks supply the authoritative build evidence.

No cleaned build has been installed or tested on hardware. Next: independent
verification, then risk-based review, scoped convergence and Git integration.

### Gate corrections and my direction

- Initial independent verification passed all code checks but blocked the missing
  upgrade path for existing services and SDL mapping overrides. Added
  [migration instructions](../uhid-migration.md), an early README link and
  restored Rust-derived documentation guards. Re-verification passed with 35
  Python tests and the unchanged initial cleanup binary.
- Review then blocked unauthenticated D-Bus notifications. The worker now binds
  methods and matches to BlueZ's resolved unique owner, checks sender headers in
  callbacks, and authenticates ownership-change invalidation. Three hardware-free
  security tests cover spoofed broadcast/unicast and ownership changes.
- I reported inverted pitch but correct yaw on the running v4 daemon.
  Added independent `motion.invert_x = true` by default. Yaw inversion remains
  unchanged. Tests cover emitted report values, independent signs, swapping,
  zero third component and extreme scale. This correction is not hardware-verified.
- `poll_ms` is now explicitly compatibility-only. The README also documents the
  previously observed power-cycle/reconnect recovery when leaving NUS motion;
  no unverified selector-restoration command was added.

Latest implementation and independent checks: 88 Rust tests, 35 Python tests,
strict Clippy, formatting, release build and diff hygiene passed. Latest release
SHA-256: `e8679ac65dda8254839ef1eab620b8e69d1defb2ebef24ae40e5c984c6f84a41`.

## Final acceptance evidence

Independent verification, follow-up security review and scoped convergence all
passed for the same saved version. The driver manifest SHA-256 is
`3becafa68e0a7aecb3d62d2e839af6c092dfe2c2d576f4919c51f1d608f40acf`.
It covers 22 existing tracked/untracked driver files, excluding build output,
Python caches and deleted paths. It hashes sorted lines of
`<file SHA-256>  <repository-relative path>\n`.

| Criterion | Result | Command/action and observed evidence | Remaining work |
| --- | --- | --- | --- |
| Sole UHID backend | Pass | Inspected direct `EdgeRuntime` startup, module exports and deleted legacy implementation/probes/mapping | None for cleanup |
| Retained Bluetooth input, motion, feedback and lifecycle | Pass, offline scope | `cargo test --all-targets` in `driver/`: 88 passed, including HID fixture replay, NUS inputs, stale neutralization and feedback/session tests | Current-build hardware acceptance remains separate |
| Configuration migration | Pass | Config file-loading tests accept existing UHID selection and reject uinput; CLI invalid-config check returned exit 1 before device access | None |
| BlueZ notification trust | Pass | Reviewed strict unique-owner matches and actual callback checks; three constructed-message tests reject foreign broadcast/unicast and authenticate owner invalidation | No live-bus exploit test was performed |
| Independent pitch inversion | Pass, code-level | Emitted-report regression checks X/Y signs after swapping, unchanged yaw, extreme scale, saturation and zero third component | Confirm physical pitch direction when deploying |
| Installation, migration and current guides | Pass | Reviewed README, architecture, testing, visibility and migration guides; independent scan found no missing local links | No host installation change was performed |
| Formatting | Pass | `cargo fmt --check` in `driver/` | None |
| Strict lint | Pass | `cargo clippy --all-targets -- -D warnings` in `driver/` | None |
| Release build | Pass | `cargo build --release` in `driver/`; SHA-256 recorded above | Not deployed |
| Retained Python tooling and documentation guards | Pass | `python3 -m unittest discover -s tests -v`: 35 passed | None |
| Diff hygiene | Pass | `git diff --check`: exit 0 | None |
| Independent review and convergence | Pass, scoped | Verifier PASS, reviewer PASS with B1 resolved, scoped convergence PASS for matching source and release identities | Does not certify full PRD acceptance |
| Git integration | Pass | Merge `eb5f0fe`; feature tip `4145716` and previous main `d45e356` are ancestors of main; merge tree equals feature tree; working tree clean after merge | None for local cleanup scope |

## Completed local integration

- Research and Steam visibility documentation: `b0ae40a`.
- Verified UHID-only runtime, tests and active documentation: `4145716`.
- Local merge: `eb5f0fe2a68831e7e2d43ec025e5b835598095ce`.
- Feature tip: `414571605d3239ae8b5a029f16b69ece26ae14a0`.
- `git diff --exit-code feat/uhid-emulation main` returned exit 0 at the merge.
- `git merge-base --is-ancestor feat/uhid-emulation main` and
  `git merge-base --is-ancestor d45e356 main` both returned exit 0.
- `git status --porcelain` was empty after merging. The reviewed driver files
  and release identity were unchanged.

The authorized cleanup and local merge are complete. Nothing was pushed or
deployed, and the feature branch was retained. Current-build hardware acceptance
and full PRD acceptance remain open, not waived.
