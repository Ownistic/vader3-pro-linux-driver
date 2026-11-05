# UHID runtime retest — 2026-11-04

## Scope and saved versions

Follow-up on `feat/uhid-emulation`, based on `cb7895a` plus the working-tree
changes listed below. Hardware host: owner's Legion Go / Bazzite (`ssh legion`).
No installed binary or configuration was replaced. Temporary units restore
`vader3p-steam.service` on exit. Stopping that service removes its **virtual**
"translated" controller; such UI disconnects do not establish a physical
Bluetooth disconnect. The earlier owner report did not distinguish these.

Implementation paths: `driver/src/ble_worker.rs`, `nus_input.rs`,
`uhid_runtime.rs`, `edge_report.rs`, `edge.rs`, `config.rs`, `lib.rs`,
`driver/examples/ble_motion_probe.rs`, and `README.md`.

## Activation observations

1. Owner confirmed connected and moving. Passive Python NUS subscription,
   **no AC write**: 1,336 motion-shaped notifications in 10 seconds; no
   observed Bluetooth connection transition.
2. Python control, one `AC 01`, same 500 ms method timeout as Rust:
   `org.freedesktop.DBus.Error.NoReply` after 502.18 ms, **but** one observed
   `AC C0` status with selector byte 1 and 335 data notifications in the
   three-second window; no observed connection transition.
3. Corrected Rust worker, already-active stream, owner moving:
   1,071 data notifications / eight seconds; no selector method error;
   `ObservedData`; first raw range -510..194, second -870..688.
4. Owner fully power-cycled, reconnected in Bluetooth DInput, and started
   moving. Corrected worker sent one AC write: one `NoReply`, **observed
   selector 1 and 1,067 data notifications** / eight seconds, `ObservedData`;
   first range -312..260, second -612..377. Probe exited 0.

Rust probe SHA-256 for observations 3–4:
`bc5439b5041504e57f71ede5e82de6503032666433f26e3f371b2b275bae7b15`.
Remote logs: `/tmp/flydigi-uhid-runtime/{movement,cold-start}-probe-v2.log`.
The installed service was restored after each probe. Restored service state
alone does not prove ordinary inputs work while the AC selector remains set.

The failure was not established to depend on movement. The original worker
treated a method timeout as definite activation failure and unsubscribed
before processing queued device responses. The correction listens passively
first, attempts at most one selector write per subscription, and assesses
actual data/status even after a method error. No observed confirmation leaves
activation unverified and subscribed, not a blind selector retry loop.

## Full daemon: gyro recognition but missing controls

Daemon SHA-256
`5f3ff0e57c55f9c8f7a50e4e6508e48428cd597cab5e2db8cb70a589125738d2`
ran as transient `vader3p-uhid-test-v2.service`, motion and rumble enabled.
Observed about 134 NUS data notifications/s, zero physical HID reports, and
continued virtual reports. Owner confirmed Steam recognized gyro, **but all
ordinary controls were absent**. Owner also observed physical Y rotation
appearing as tilt and tilt appearing as Y rotation: provisional X/Y assignment
was swapped. Two components only; signs and scale remain uncalibrated.

The focused static follow-up is
[NUS input prefix](nus-input-prefix-static.md). For its hash-identified firmware
image, selector 1 chooses a NUS-only builder instead of the normal HID builder.
The NUS prefix contains axes, triggers and all known buttons. That static fact
is consistent with the live counters, not proof of the installed firmware hash.

## Current increment awaiting hardware retest

NUS inputs now share the worker's latest sample/generation with motion.
The UHID runtime uses fresh NUS controls, neutralizes stale controls, and does
not overwrite them with queued HID input. The default uinput path is unchanged.
`[motion] swap_xy = true` maps first raw component to Y and second to X;
`false` retains the earlier provisional assignment. Third component remains 0.
The parser distinguishes complete data framing before status headers: valid
stick coordinates `AC C0` are not rejected as a status message.

The first full-input candidate daemon SHA-256 was
`db1368d855049d3bcc229c9cd0c1b19386f05c78537b3ca34e2a04ceff9afa85`.
On that artifact the owner confirmed A, both stick axes and return to center,
both analog triggers, and distinct M1, M2 and M3 controls. Motion axis selection
was corrected by the swap, but yaw was inverted and pitch correct. Physical X
appeared as triangle and physical Y as square.

Passive two-second captures with one physical button held, without an AC write
or daemon restart, proved X sets NUS byte 4 mask `0x80` (268 notifications) and
Y sets byte 5 mask `0x01` (269 notifications). This corrects the static report's
inferred X/Y labels; the static bit positions themselves were correct.

Current candidate daemon SHA-256:
`616d9e1b5f2e808561ea2213271e0c5db6136525cbce031caffd6a501da4c715`.
Probe SHA-256:
`482cabb7ac756a5968b3284360a0f3b621fec5150c76a7df6c3ac84308d7c16c`.
It corrects the NUS X/Y labels and adds `[motion] invert_y = true`, applied
after axis selection with wide signed arithmetic before saturation. X is not
inverted. After free testing, the owner reported everything working correctly
except the missing third gyro axis. This is owner feedback on the current
artifact, not separate proof of unmeasured latency or physical reconnect.

| Check | Status | Evidence / remaining work |
| --- | --- | --- |
| Rust tests | pass | `cargo test`: 142 passed; existing unrelated example warnings |
| Strict scoped Clippy | pass | `cargo clippy --lib --bins --tests --example ble_motion_probe -- -D warnings` |
| Formatting | pass | `cargo fmt --check` |
| Release artifacts | pass | `cargo build --release --bin vader3p-steam --example ble_motion_probe` |
| AFT diagnostics | unresolved | Rust analyzer unavailable; compiler-backed checks above passed |
| Cold-start activation | pass, earlier worker artifact | Observations 3–4; new full-input candidate not yet tested |
| Steam gyro recognition | pass, earlier daemon artifact | Owner confirmation above, not full-input acceptance |
| NUS ordinary controls and axis assignment | owner-reported pass | Targeted A/stick/trigger/M1–M3 checks on prior candidate; owner reported current free test correct except third motion axis |
| Third motion component | unresolved | Current report deliberately uses 0; no verified third-value Bluetooth contract found; see third-motion-component-static.md |
| Independent verification/review | unresolved | Required after implementation settles |
| Rumble / extra bindings / reconnect / latency / suppression | unresolved | Must be observed; output writes alone prove none of these |

No final acceptance or deployment is claimed. Do not send `13 04 00 00` or
`13 05`; do not modify `local/research/`.

## Current Legion Go state: owner-directed free test

The v3 controller vanished because its transient unit reached the deliberate
20-minute limit, not because a device disconnect was established. Journal:
`Service reached runtime time limit`; cleanup counted 160,030 notifications.
Owner subsequently confirmed the physical controller remained connected and
the Legion Go was awake. M4, C and Z checks were interrupted and remain open.

The owner requested free testing instead of per-input questions, and explicitly
requested removing the old "Flydigi Vader 3 Pro (translated)" driver from the
Legion Go. Accordingly:

- Disabled/stopped the installed `vader3p-steam.service`, removed its enabled
  target link, and moved its unit and binary out of active installation.
- Preserved a recovery archive at
  `/home/deck/.local/state/vader3p-steam/retired-uinput-20261104-u_ejfiub/`.
  Original paths were `/home/deck/.config/systemd/user/vader3p-steam.service`
  and `/home/deck/.local/bin/vader3p-steam`. Configuration and shared device
  access rules were not changed.
- Verified old unit `LoadState=not-found`, `ActiveState=inactive`.
- Started hash-verified current candidate from
  `/tmp/flydigi-uhid-runtime/vader3p-steam-v4`, using the unchanged temporary
  `config.toml`, as `vader3p-uhid-test-v4.service`.
- New unit is active, `RuntimeMaxSec=infinity`, and has **no old-driver
  restoration hook**. It is a transient test instance, not boot-enabled
  installation; reboot or explicitly stopping it ends this instance.

Next action: resolve the third-component transport decision with the owner;
the current working test daemon is unchanged. Firmware inspection supports the
two-word NUS packing, but does not prove that no other Bluetooth route exists.
The other live vendor characteristic is corroborated as an OTA endpoint, not a
verified motion route; it was not read or written. Only the standard firmware
revision characteristic was read (`6.7.5.1`).

Still open: a verified third-value transport, specific Z-button behavior if
needed, felt-rumble/automatic-stop evidence beyond the broad free-test report,
physical reconnect with the same virtual device, suppression, motion-active
latency, independent implementation verification/review, and final packaging.
Do not silently restore the retired uinput service contrary to the owner's
removal request.
