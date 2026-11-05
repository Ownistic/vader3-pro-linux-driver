# Implementation plan

Requirements remain in [PRD.md](PRD.md); current checkpoint and historical
development record are in [PROGRESS.md](PROGRESS.md). The latest feature scope
is [FN quick-access mapping](docs/phases/03-fn-quick-access.md), complete within
its scope and verified in Game Mode. No follow-on phase is authorized. The prior
[UHID-only cleanup](docs/phases/02-uhid-only-cleanup.md) is complete.

The standalone Rust daemon now targets Bluetooth DInput and DualSense Edge UHID.
The earlier generic-uinput implementation and its measurements are historical,
not evidence of acceptance of the current build. My v4 observation
confirmed controls except gyro Z and Steam visibility after a separate ignore
setting. Current-build hardware checks for sensor calibration, rumble, reconnect,
latency, and the remaining PRD criteria still need independent evidence. Motion
remains opt-in and incomplete. See [testing](docs/TESTING.md),
[open questions](docs/OPEN_QUESTIONS.md), and [evidence](docs/evidence/).
