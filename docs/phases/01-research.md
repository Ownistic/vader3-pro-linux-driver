# Phase 01 — research and feasibility

Status: initial investigation, not complete. Requirements: PRD sections 4–7.

## Questions and evidence

1. Pin/review all requested upstream sources and licenses. For padctl, inspect
   transport, definitions/plugins, UHID, feedback, discovery/suppression, and
   deployment code. Explain definitions-only versus backend changes.
2. Acquire official Space Station 3.4.4.3; record SHA-256, source URL, page linkage,
   signature status, tool versions, extraction boundary, and REA Evidence IDs.
3. Trace identification, Bluetooth/DInput/XInput, inputs/extras, sensor enablement
   and parsing, rumble, negotiation, and relevant profiles from handlers to wire
   operations. Cite assembly identity/method/token or file and source revision.
4. Capture target descriptor, bus/VID/PID and report streams per Bluetooth mode.
   Determine Classic HID versus HOGP with BlueZ evidence, not bus ID alone.
5. Compare virtual identities in real Steam Gaming/Desktop Mode, including each
   extra independently and simultaneously, native gyro, feedback, duplicate
   suppression, and recovery. Record Steam/OS/controller versions.
6. Establish rumble feasibility separately from missing Linux support. A failed
   guessed command or absent code path is not proof of hardware impossibility.

## Exit gate

Save sourced findings in `../RESEARCH.md`, a transport-qualified mapping in
`../PROTOCOL.md`, limitations in `../TESTING.md`, and an architecture decision in
`../ARCHITECTURE.md`. Select extension versus standalone and a virtual identity
only with adequate evidence and review of the material choice. Missing hardware
keeps transport/Steam claims unresolved; complete unaffected static research.

## Non-goals and next phase

No unrelated proprietary services, licensing bypass, flashing, or permanent
configuration changes. Do not reproduce proprietary code. Once input evidence
and an integration path exist, define Phase 02's smallest runnable experiment,
including neutralize-on-disconnect and strict target matching.
