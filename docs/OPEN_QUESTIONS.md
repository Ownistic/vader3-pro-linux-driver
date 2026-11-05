# Open questions

The [PRD](../PRD.md) retains the full acceptance requirements. The earlier v4
observations do not automatically validate the current build. See
[testing](TESTING.md) and [v4 evidence](evidence/uhid-runtime-retest-2026-11-04.md).

- Do all standard and extra controls remain independently assignable in Steam
  on the current build, including combinations and release?
- What are the motion values' physical axes, units, scale and bias? The third
  Bluetooth value is unknown and currently zero; acceleration is fabricated.
- Do real motor start/stop and levels work end to end through Edge host feedback
  and Bluetooth NUS on the current build?
- Do sleep, reconnect, suppression, permissions, service lifecycle and latency
  satisfy the PRD on the target system?

Earlier generic-pad experiments and measurements are preserved in
[evidence](evidence/); they do not establish UHID performance. LED control,
firmware modification, XInput mode and USB/dongle product support are out of
scope. See [architecture](ARCHITECTURE.md) for the current data path.
