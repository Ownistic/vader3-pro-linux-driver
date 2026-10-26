# HID regression fixtures

The `vader3pro-bluetooth-{xbox,dinput}-descriptor.json` files are real sysfs
descriptors captured before/after the owner switched Bluetooth modes. They are
**not input-report captures**, and do not establish button mapping,
motion, feedback, or Steam support. See
[Xbox-mode evidence](../../docs/evidence/bluetooth-xbox-mode.md) and
[DInput evidence](../../docs/evidence/bluetooth-dinput-mode.md).

`vader3pro-bluetooth-dinput-abcz.json` preserves all 14 reports from the first
owner-coordinated A/B/C/Z input capture. It records real 21-byte report-ID-1
packets even though the descriptor declares 15 bytes including ID. Associated
button meanings follow the requested/owner-completed sequence, not video or
simultaneous-input testing. See [capture evidence](../../docs/evidence/passive-dinput-abcz.md).
Do not label synthetic bytes as recorded input or equate these fixture checks
with working Steam support.

Future sanitized fixtures must document controller/firmware/mode, Bluetooth
transport, descriptor SHA-256, report ID/length, capture procedure, expected state,
and which identifiers were removed. Test each standard/extra control and
simultaneous extras, axes, motion, releases, disconnects, and malformed reports.
Do not infer a Bluetooth packet layout from a USB/receiver fixture.
