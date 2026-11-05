# FN quick-access mapping

## Scope

I requested that the previously unused circle/FN button open Steam Game Mode's
right-hand Quick Access Menu, which contains Decky. This does not request a
direct Decky-tab selector or a replacement for any existing button.

Baseline: `main` at `a0b40bf`. The Legion Go initially ran the earlier v4 executable
with SHA-256 `616d9e1b5f2e808561ea2213271e0c5db6136525cbce031caffd6a501da4c715`.
The first FN test candidate has been stopped after a reported unwanted A action;
v4 was restored while the correction was prepared. The successful sequenced
candidate now runs as a transient test service. The original v4 executable and
configuration remain preserved; no installed or boot-enabled service was changed.

Allowed changes: recognize FN in the Bluetooth input parsers, carry its state
without changing the six extra buttons, and generate Steam's Guide + A shortcut
(PS + Cross on the virtual DualSense Edge). Update focused tests and current
documentation. The first level-based attempt leaked an A action in Game Mode;
the corrective implementation uses a bounded Guide-before-A sequence while
preserving independently held physical Home and A.

Non-goals: keyboard injection, another virtual device/backend, Steam/Decky
configuration edits, remapping existing controls, USB/dongle support, firmware
changes or unrelated motion changes. Do not commit, push or replace an installed
binary without a separate request; the explicit Git packaging request has now
been received, while installed-binary changes remain unrequested. A temporary, reversible test-service swap may
be needed for the requested hardware check; preserve the current executable and
configuration and explain the swap before starting it.

## Evidence and acceptance

| Criterion | Status | Evidence / remaining work |
| --- | --- | --- |
| Identify FN in the active Bluetooth stream | Pass | I completed FN-only presses followed by one LB marker. The passive capture received 18,152 NUS samples: FN changed byte 8 mask `01`; LB independently changed byte 5 mask `04`. See the [capture record](../evidence/fn-button-capture.md). |
| Identify the ordinary HID mapping | Pass (static) | The same bitmap bit 24 becomes HID byte 11 mask `80`, not byte 12 mask `01`. The earlier optional-control labels were incorrect; see the [static contract](../evidence/fn-button-static.md). No ordinary HID FN capture has been performed. |
| Preserve existing controls | Pass within scope | Emitted-report tests preserve physical A/Home and extras through pulse phases. My selected-game short-tap and hold tests show no unwanted A action from FN; broader controller acceptance is not claimed. |
| Clear FN and its synthetic shortcut on stale input/disconnect | Independent code checks pass | Independent preflight found that candidate `d1790d39` could arm from fabricated neutral state at startup or reconnect. The repaired candidate requires observed HID state before cached input is valid; selector-to-report tests cover a first held FN, actual release, and new press. Reverifier and reviewer pass; hardware lifecycle behavior remains unverified. |
| Pass code checks | Independent code preflight and review pass | Reverification passed 102 Rust and 38 Python tests, formatting, strict Clippy, Cargo check for all targets, release build and diff check. The bounded implementation review found no blockers. Rust analyzer is unavailable; Cargo supplied compiler validation. |
| Open QAM in Game Mode | Pass: sequenced candidate | For release `7ec608e7`, I confirmed both a short FN tap and a two-second hold with a game highlighted opened only the sidebar, with no game launch, repeat or main-menu popup. The first simultaneous candidate remains a recorded failure. See the [runtime record](../evidence/fn-quick-access-runtime.md). |

## Next action

Scoped convergence passed without a blocking gap for release SHA-256
`7ec608e76c8c66a8f97a334b88b24372ea1360be1a85038c970ac34f6ba31c56`,
independently verified and reviewed against driver-input manifest
`1ee4574e63a3534f9b9c872f476111bbd81b3d7ae80f859a5fb41dac052d45d0`
(25 sorted driver paths excluding `target`, each path followed by NUL and its raw
SHA-256 digest). Both coordinated Game Mode checks passed on this exact executable,
which remains active as `vader3p-uhid-test-fn-sequenced.service`. No further hardware
experiment is needed for this scoped check. The explicit request to commit and push
was received after these gates passed; only Git packaging is authorized, not an
installed-service change.

## Closeout

| Gate | Status | Evidence / remaining work |
| --- | --- | --- |
| Independent code verification | Pass | 102 Rust tests, 38 Python tests, formatting, strict Clippy, Cargo check for all targets and release build passed for the recorded source manifest and executable. |
| Bounded implementation review | Pass | Ordered pulse, eligibility, cancellation and raw-control preservation reviewed against the same manifest; no blocking findings. |
| Scoped convergence | Pass | All criteria above are covered by saved implementation, tests and the exact-artifact Game Mode observations. This does not certify the whole PRD. |
| Final documentation checks | Pass | `python3 -m unittest discover -s tests -v`: all 38 passed after closeout updates. `git diff --check` passed. |
| Git commit and push | Requested after gates passed | Commit this scoped feature; the parent session will push `main` to `origin`. No push has yet occurred. |
| Installed-service changes | Not requested | Source changes are committed separately from deployment; only the transient test service is running. |

Minimum pulse durations are scheduling parameters, not measured end-to-end
latency. Ordinary HID FN hardware capture, physical reconnect/lifecycle behavior,
other firmware/Steam versions, and unrelated motion/rumble acceptance remain
explicit limitations rather than completed hardware claims. This feature is
complete within scope; no follow-on phase is authorized.
