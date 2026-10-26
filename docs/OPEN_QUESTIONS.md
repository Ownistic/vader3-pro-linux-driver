# Open questions

| Question | Needed evidence | Blocks |
| --- | --- | --- |
| Is padctl an appropriate maintained base? | Pinned Zig transport/UHID/feedback/definition code and license review | Runtime language and repository shape |
| Which virtual identity exposes six extras AND gyro in Steam? | Steam editor experiments in both modes; actual descriptor/control exchanges | Emulator choice |
| What does this hardware send in Bluetooth DInput/XInput? | Both descriptors retained; still need live reports, firmware version and connection trace | Physical parser |
| Does Bluetooth expose raw motion and an initialization path? | Static command trace plus hardware captures | Native gyro |
| Can Bluetooth receive rumble commands? | DInput exposes writable HID-report/vendor/UART GATT objects; command semantics/static trace and bounded hardware experiment still needed | Real vibration |
| Is the candidate installer authenticated? | Current official page linkage and Authenticode verification/trust status | Provenance conclusion |
| What is the project's distribution license? | Owner decision after upstream compatibility assessment | Release packaging, not local setup |

No license is silently selected for the owner. Do not reuse third-party source
until applicable terms and notice requirements have been reviewed.
