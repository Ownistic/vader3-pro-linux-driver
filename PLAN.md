# Implementation plan

Requirements: [PRD.md](PRD.md). Current checkpoint: [PROGRESS.md](PROGRESS.md).

Current checkpoint: [PROGRESS.md](PROGRESS.md). Release documentation:
[README.md](README.md).

The plan below is the original roadmap, kept so the reasoning is traceable. The
work diverged from it in one important way: the runtime was written as a
**standalone Rust daemon** rather than on top of padctl, because padctl's Bluetooth
transport could not be shown to satisfy the requirements and a standalone
implementation needed no upstream contract to negotiate.

| Phase | Deliverable / exit gate | State |
| --- | --- | --- |
| [00: Setup](docs/phases/00-setup.md) | Requirements, research/evidence structure, safe diagnostics, tests, acquisition checkpoint | **Complete**, S1–S5 verified/converged |
| [01: Research](docs/phases/01-research.md) | Upstream/license comparison, static Windows/firmware evidence, Bluetooth captures, architecture decision | **Substantially complete.** Protocol fully mapped, firmware disassembled, rumble solved. Steam-side experiment remains. |
| 02: Input prototype | Verified physical parser, virtual pad, ordinary/extra input tests | **Complete.** 21 buttons + 6 axes verified; latency measured |
| 03: Steam integration | M1–M4/C/Z independently assignable; no duplicate device | **Implemented, unverified in Steam.** Mapping file shipped; needs a Steam run |
| 04: Motion | Calibrated sensor decoding/orientation and native Steam gyro | **Blocked.** No motion data exists in the Bluetooth DInput stream |
| 05: Rumble | Bluetooth vibration or evidence establishing the exact limitation | **Complete.** Solved via a 4-byte Nordic UART frame; no limitation remains |
| 06: Stabilization | Reconnects, sleep, permissions, service, install/uninstall | **Partially complete.** Permissions, systemd unit and docs done; reconnect behaviour and Bazzite install flow untested |

## Sequence and decision gates

1. ~~Review padctl source before choosing standalone Rust or padctl/Zig.~~
   Resolved: standalone Rust. padctl's Bluetooth path was not demonstrable.
2. Pin source revisions and licenses; acquire the installer outside Git and record
   its hash and provenance. The installer was never executed here.
3. Obtain transport-specific descriptor/report evidence; do not port USB offsets
   to Bluetooth. Done — the two descriptors differ, and Bluetooth declares no
   output reports at all.
4. Compare virtual device identities against all six extras plus gyro using actual
   Steam behaviour. Extras verified via SDL; **Steam itself still untested**.
5. Record the architecture choice before implementing the runtime. Done.
6. Apply build/check/repair and independent verification at each gate.

## What remains before a 1.0

1. **Verify the six extras inside Steam.** The single open acceptance criterion.
   The mapping file is built from measured indices, but Steam bundles its own SDL.
2. **Confirm reconnect handling on the Legion Go** — controller sleep, resume, and
   daemon restart after a Bluetooth dropout.
3. **Document the gyro limitation as final** unless vendor mode is revisited. The
   `05 EE 01` path was never confirmed over Bluetooth and reset the bond once
   during exploration, so it is out of scope by decision rather than by oversight.

See [open questions](docs/OPEN_QUESTIONS.md).
