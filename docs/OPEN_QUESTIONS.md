# Open questions

| Question | Needed evidence | Blocks |
| --- | --- | --- |
| Is padctl an appropriate maintained base? | Pinned Zig transport/UHID/feedback/definition code and license review | Runtime language and repository shape |
| Which virtual identity exposes six extras AND gyro in Steam? | Steam editor experiments in both modes; actual descriptor/control exchanges | Emulator choice |
| How are M1–M4 exposed in DInput at all? | Passive DInput maps M1–M4 to the same bytes/bits as the D-pad (byte 11 `0x04`/`0x08`/`0x10`/`0x20`); extra-button exposure evidently needs vendor mode: identify the vendor-mode switch/initialization command and its report layout | Six independently configurable buttons; PRD core |
| Does vendor mode expose native gyro on Bluetooth? | Passive ID-1 stream shows no motion (whole-controller shake test produced no large byte 1-4 swings); find the vendor command enabling gyro streaming | Native gyro |
| What produced byte 9 low bits `0x01`–`0x07` in session 1 (92–110 s)? | Owner recall or a recreated trigger capture | Complete byte map |
| Does Bluetooth expose raw motion and an initialization path? | Static command trace plus hardware captures | Native gyro |
| Can Bluetooth receive rumble commands? | DInput exposes writable HID-report/vendor/UART GATT objects; command semantics/static trace and bounded hardware experiment still needed | Real vibration |
| Is the candidate installer authenticated? | Current official page linkage and Authenticode verification/trust status | Provenance conclusion |
| What is the project's distribution license? | Owner decision after upstream compatibility assessment | Release packaging, not local setup |

No license is silently selected for the owner. Do not reuse third-party source
until applicable terms and notice requirements have been reviewed.
