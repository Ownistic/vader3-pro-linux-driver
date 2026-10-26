# Implementation plan

Requirements: [PRD.md](PRD.md). Current checkpoint: [PROGRESS.md](PROGRESS.md).

The owner requested the PRD first, then setup/scaffolding. Phase 00 is the
completed authorized increment, independently verified and converged. Later phases express the requested roadmap, not
evidence that an architecture has already passed review.

| Phase | Deliverable / exit gate | State |
| --- | --- | --- |
| [00: Setup](docs/phases/00-setup.md) | Requirements, research/evidence structure, safe diagnostics, tests, acquisition checkpoint | Complete; S1–S5 verified/converged |
| [01: Research](docs/phases/01-research.md) | Upstream/license comparison, static Windows evidence, Bluetooth captures, early Steam experiment, architecture decision | Incomplete; source/descriptor triage done, live reports and installer extraction pending |
| 02: Input prototype | Verified physical parser, UHID emulator, ordinary/extra input tests and Steam recognition | Pending phase detail and evidence |
| 03: Steam integration | M1–M4/C/Z independently assignable; no duplicate device, both Steam modes | Pending |
| 04: Motion | Calibrated sensor decoding/orientation and native Steam gyro | Pending |
| 05: Rumble | Bluetooth vibration or evidence establishing the exact remaining limitation | Pending |
| 06: Stabilization | Reconnects, sleep, permissions, service, Bazzite install/update/uninstall | Pending |

## Sequence and decision gates

1. Review current padctl source before choosing standalone Rust or padctl/Zig.
2. Pin source revisions and licenses; acquire 3.4.4.3 outside Git and record hash,
   provenance limits, and signature status. Never execute the installer here.
3. Obtain transport-specific descriptor/report evidence; do not port USB offsets
   directly to Bluetooth.
4. Compare virtual Flydigi, Steam Deck/other supported devices, and Edge/Elite
   against **all six extras plus gyro**, using actual Steam behavior.
5. Record the material architecture choice and review the concrete next phase
   before implementing the runtime. Do not wait for rumble certainty to build
   independently useful, evidence-backed input support.
6. Apply build/check/repair and independent verification at each coherent gate.

See [open questions](docs/OPEN_QUESTIONS.md). Hardware checks that cannot run
remain blocked/untested. Setup passing does not mean Phase 01 or any product
acceptance criterion has passed.
