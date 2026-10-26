# Open questions

| Question | Needed evidence | Blocks |
| --- | --- | --- |
| Is padctl an appropriate maintained base? | Pinned Zig transport/UHID/feedback/definition code and license review | Runtime language and repository shape |
| Which virtual identity exposes six extras AND gyro in Steam? | Steam editor experiments in both modes; actual descriptor/control exchanges | Emulator choice |
| Which physical M key maps to which byte-11 bit? | Byte 11 bits `0x04`/`0x08`/`0x10`/`0x20` are M1–M4 and are distinct from the D-pad (byte 9 low-nibble hat); the per-key assignment follows FlyDigi's table but was never owner-confirmed. An isolated recapture naming each key would settle it | Cosmetic remap order; not a blocker |
| Does vendor/origin mode expose anything DInput lacks? | DInput already exposes all six extras, sticks, triggers and C/Z. Vendor mode (5-byte `05 EE 01` = `getSwitchOriginData`) additionally carries gyro in bytes 11–13 of a 32-byte report; no hardware test yet | Native gyro |
| Does vendor mode expose native gyro on Bluetooth? | Passive ID-1 stream shows no motion (whole-controller shake test produced no large byte 1–4 swings). Space Station decodes gyro from vendor-mode report bytes 11–13 as 12-bit packed X/Y; whether a Bluetooth DInput unit streams it on request is untested | Native gyro |
| What produced byte 9 low bits `0x01`–`0x07` in session 1 (92–110 s)? | Owner recall or a recreated trigger capture | Complete byte map |
| Does Bluetooth expose raw motion and an initialization path? | Static command trace plus hardware captures | Native gyro |
| Can Bluetooth receive rumble commands? | DInput exposes writable HID-report/vendor/UART GATT objects; command semantics/static trace and bounded hardware experiment still needed | Real vibration |
| Is the candidate installer authenticated? | Current official page linkage and Authenticode verification/trust status | Provenance conclusion |
| What is the project's distribution license? | Owner decision after upstream compatibility assessment | Release packaging, not local setup |

No license is silently selected for the owner. Do not reuse third-party source
until applicable terms and notice requirements have been reviewed.
