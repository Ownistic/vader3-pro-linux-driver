# UHID emulation implementation and validation

## Scope and tested baseline

Branch `feat/uhid-emulation`; starting HEAD `d45e356`. Existing research and
decoder work was preserved and committed as `0a9e333` and `ab522e8` at
my request. Baseline `cargo test`: 109 passed. The isolated spike has
three passing example tests. Rust-analyzer is unavailable locally; Cargo is
the authoritative Rust check.

Allowed: isolated spike, opt-in `[pad] backend = "uhid"`, motion-shaped NUS
input and HID output rumble, regression tests, README and evidence/handoff.
Default uinput behavior and existing config keys must remain unchanged.
No changes under `local/research/`, no flashing, and no `13 04 00 00` or
`13 05` commands. My actions and felt/UI results require confirmation.

## Compatibility gate

I confirmed **Recognized; gyro available** for the synthetic
DualSense Edge on the Legion Go running Bazzite. See
[spike evidence](uhid-dualsense-probe.md) for revision, binary/source hashes,
host kernel and observed feature/output traffic. The synthetic probe has been
stopped. This proves the Steam compatibility gate, not physical sensor accuracy
or final daemon behavior.

## AC power-cycle gate

Host: Legion Go via SSH alias `legion`, account `deck`, kernel
`7.2.11-ogc4.1.fc44.x86_64`. I confirmed the Vader was connected there
without being power-cycled. BlueZ named the connected/resolved device
`Flydigi VADER3P`. The standalone diagnostic scopes discovery to that exact
name and refuses ambiguous NUS devices; no addresses or complete notifications
are recorded.

Diagnostic artifact: `driver/examples/nus_selector_probe.py`, uncommitted on
top of `ab522e8` during these tests. SHA-256 before the power cycle:
`7a3a92f5fb20364d3cd2042b2c834c2ca5df392c141361abc7c30d5cac15d81d`.
Remote location: `/tmp/flydigi-uhid-spike-ab522e8/nus_selector_probe.py`.

1. `python3 .../nus_selector_probe.py --seconds 15 --query-selector`:
   connected/resolved; StartNotify accepted; **2,002** candidate 20-byte
   notifications ending `FE 00`. The `BA 40 00 00` query's write was accepted,
   but **no AC C0 status** was observed. Do not infer selector value from the
   query write or treat it as a proven status query on this live firmware.
2. I confirmed fully powering off for about five seconds, turning on
   in the same Bluetooth DInput mode, and reconnecting to the Legion Go.
3. `python3 .../nus_selector_probe.py --seconds 15` (strictly passive):
   connected/resolved; StartNotify accepted; **zero notifications**.
4. Updated diagnostic SHA-256:
   `a16d94338dcb75daf82ee012de4d2efc03c3bb5d893f70f315a87a9895ab3378`.
   `python3 .../nus_selector_probe.py --seconds 15 --enable-stream` waits three
   passive seconds, then sends the previously verified **AC 01**. Observed
   **one AC C0 response, byte 10 = 1**, and **1,531 candidate data
   notifications** (1,532 20-byte notifications total). The response and data,
   not transport acceptance, support re-enabling the stream.

Conclusion: the AC-enabled stream did **not remain active across this power
cycle**, and explicit re-enabling restored it. The selector was not directly
read while off; no claim is made about every stored setting, all firmware
versions or all boot paths. This supports an explicitly opt-in daemon AC path,
never the default uinput path. The stream is currently enabled; diagnostics
unsubscribed without a selector-off write. Firmware version remains unknown.
No physical axis, sign, scale, sensor sample rate or third component is proven.

## Acceptance record

| Criterion | Result | Evidence / remaining work |
| --- | --- | --- |
| Steam supported controller and gyro UI | Spike passed; final backend pending | My confirmation for `ab522e8` synthetic spike |
| Standard-path rumble and autonomous stop | Pending | Requires my felt confirmation through final HID output path |
| M1–M4 and C separate; Z documented | Pending | Test one control at a time in Steam |
| Suppression and reconnect without daemon restart | Pending | Final daemon hardware test |
| Motion-active latency versus 130 reports/s, 0.03 ms median | Pending | Same measurement boundary and physical input activity required |
| Cargo tests, clippy for src, fmt | Implementation checks passed; independent gate pending | 129 tests; all-target clippy has 12 legacy example warnings, none in src; fmt passed |
| New tests fail against old implementation | Pending | Record concrete regression test evidence |
| README, evidence, handoff | In progress | Final configuration and limitations not yet implemented |
| Independent verification/review and clean reviewable commits | Pending | Snapshot commits are not final certification |

## Integrated implementation and first hardware run — stopped

Saved implementation: `driver/src/edge_report.rs` (21 distinct encoded controls,
two provisional rate-shaped fields), `edge.rs` (UHID control plane),
`ble_worker.rs` (bounded latest-sample/feedback worker), `uhid_runtime.rs`
(optional runtime), config/entry-point dispatch, and README configuration.
The existing `pad.rs` is unchanged; uinput remains the default. This is a
**partial, non-certified implementation**, not a working motion feature.

An independent read-only reviewer found no blocking issue for a first
controlled hardware run. Two reported minor issues were corrected before the
run: selector write errors now trigger cleanup/retry, and checked UHID polling
uses `ppoll` for sub-millisecond deadlines without changing uinput polling.

Tested source: `ab522e8` plus the uncommitted implementation. SHA-256 of the
compact, sorted JSON map of paths to SHA-256 for `driver/src/*.rs`,
`driver/tests/*.rs`, `driver/Cargo.toml`, and `driver/Cargo.lock`:
`517bc645e211498d4740eb3af347d678ebfd34388160ae66834bcc2346e232a8`.
The manifest is saved at `/tmp/opencode/flydigi-uhid-runtime-manifest.json`.
Release binary SHA-256, verified locally and on the Legion Go:
`e986bc4014f3cefcba2cd643f87740179a1a0ccdf54b40e69ee35b6cff472fb0`.

Implementation checks on that source:

```text
cargo test                  129 passed, 0 failed
cargo clippy --all-targets   0 errors; 12 warnings, all in existing examples
cargo fmt --check           passed (exit 0)
cargo build --release       passed
```

The earlier `cargo clippy --lib --bins --tests -- -D warnings` passed before
the two review corrections; the subsequent all-target check showed no source
warnings. AFT still could not obtain Rust diagnostics. The twelve example
warnings are seven transmute annotations, three type-complexity warnings, an
unused import in `signal_probe.rs`, and an unused variable in `sdl_probe.rs`.
No old-code regression demonstration or final independent verifier ran.

I reconnected the physical controller after it disconnected during
implementation. The readable physical node was `/dev/hidraw8`, ID
`04b4:2412` on bus 5, matching the known alternate personality. The installed
`vader3p-steam.service` was stopped temporarily, not replaced. The test binary
and generated opt-in config lived only in `/tmp/flydigi-uhid-runtime/`, with
`[pad] backend="uhid"`, `[motion] enabled=true, scale=1, stale_ms=100`, rumble
and suppression enabled, and the connected controller's address kept private.
Transient unit: `vader3p-uhid-test.service`, PID **51097**.

Observed daemon logs:

```text
physical device: /dev/hidraw8 (04b4:2412, bus 0x5, interface None, matched by id)
suppressed physical node /dev/input/event16
experimental UHID virtual pad created: Flydigi Vader 3 Pro (UHID translated)
UHID output #1 len=63 id=02 bytes[1..=5]=[00, 00, 00, 00, 00] byte[39]=02
UHID output #4 len=48 id=02 bytes[1..=5]=[00, 00, 00, 00, 00] byte[39]=00
BLE worker: selector write failed; retrying discovery
BLE worker: cleanup; notifications=0, motion=0, selector=None; selector not restored
```

The first inspected log already contained more than three consecutive
selector-write failures. My repeated-blocker stop condition was
therefore reached: **no more experiments or repairs were attempted**. The
test was stopped and the original installed service restarted. The saved
complete journal counted 37 automatic retries before shutdown; periodic
counters reported zero motion notifications. The error logger does not retain
the underlying D-Bus error, so the reason is **unknown**, not proven to be a
permissions, firmware or protocol problem. The preceding Python diagnostic's
AC write and observed response do not prove this Rust path works.

Failure journal: remote `/tmp/flydigi-uhid-runtime/failed-run.log`.
Cleanup verification: test unit inactive, zero Edge HID devices remaining,
original `vader3p-steam.service` active/running, PID **51748**. Its logs show
physical `/dev/hidraw8`, exclusive grab of `/dev/input/event16`, Nordic UART
rumble selection, and the original translated virtual pad created. These
observations establish restoration/startup, not a newly felt rumble test.

No final button, rumble, gyro, suppression, reconnect or latency test by me
was performed on the integrated backend. Physical axes, signs, units and a
third component remain unresolved; bytes 14–17 were not decoded or guessed.
Z is encoded as microphone/mute but its Steam bindability remains unproven.

If I authorize resuming beyond the stop limit, the next concrete
action is to preserve/report the worker's actual D-Bus write error and compare
its selector request with the successful scoped Python diagnostic. Then rerun
the controlled hardware ladder, regression demonstration and final gates.
