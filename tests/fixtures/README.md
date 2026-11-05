# HID regression fixtures

The `vader3pro-bluetooth-{xbox,dinput}-descriptor.json` files are sysfs
descriptors captured in the corresponding Bluetooth modes. They are **not
input-report captures**, and do not establish button mapping, motion, feedback,
or Steam support.

`vader3pro-bluetooth-dinput-abcz.json` preserves 14 reports from a coordinated
A/B/C/Z input capture. It records 21-byte report-ID-1 packets even though the
descriptor declares 15 bytes including ID. Associated button meanings follow the
capture sequence, not video or simultaneous-input testing.
Do not label synthetic bytes as recorded input or equate these fixture checks
with working Steam support.

Future sanitized fixtures must document controller/firmware/mode, Bluetooth
transport, descriptor SHA-256, report ID/length, capture procedure, expected state,
and which identifiers were removed. Test each standard/extra control and
simultaneous extras, axes, motion, releases, disconnects, and malformed reports.
Do not infer a Bluetooth packet layout from a USB/receiver fixture.
