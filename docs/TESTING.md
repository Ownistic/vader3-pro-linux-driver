# Testing and validation

## Offline checks

From `driver/` run `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets`, and
`cargo build --release`. From the repository root run
`python3 -m unittest discover -s tests -v`. Tests do not prove hardware behavior.
The [README](../README.md) describes setup and permissions. CI configuration is
in [ci.yml](../.github/workflows/ci.yml); local runs are not remote CI results.

## Hardware acceptance remains open

The owner reported controls working except gyro Z on the earlier v4 build and
confirmed only the virtual DualSense Edge remained visible in Steam after a
manual physical-device ignore setting. See
[v4 runtime evidence](evidence/uhid-runtime-retest-2026-11-04.md) and
[visibility guide](steam-controller-visibility.md). These are observations of a
particular artifact, not blanket acceptance of later builds. Earlier generic
uinput latency and rumble measurements are historical, not UHID measurements.

Record source revision, binary hash, kernel, Steam version, controller mode and
all observed actions for each future test. A transport write alone does not
prove that the controller responded. Do not send experimental commands without
appropriate authorization.

| PRD area | Evidence still needed for current build |
| --- | --- |
| F1/F2, A1/A3–A5 | Standard and extra controls, independent bindings and combinations in Steam |
| F3, A6 | Sensor orientation, scale, bias/calibration and aiming; third component and acceleration are not measured |
| F4, A7 | Host feedback and physically observed Bluetooth motor start/stop and levels |
| F5, A2/A9/A10 | Physical visibility and suppression, repeated sleep/reconnect/neutralization, unrelated controllers |
| A8/A11 | Per-game configuration, Bazzite permissions, startup, reboot, update and uninstall |
| Latency | Measurement of the current UHID path, including motion active |

These criteria are unresolved, not failures or a claim of full PRD completion.
The historical evidence records retain original measurements and artifact IDs.
