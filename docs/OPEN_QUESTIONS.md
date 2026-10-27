# Open questions

Only **two** questions remain open, and both need a Steam session on the target
machine rather than more code.

## Genuinely open

| Question | Needed evidence | Blocks |
| --- | --- | --- |
| Does Steam present M1-M4/C/Z as six independently assignable controls? | Run Steam with the shipped mapping file and inspect the controller editor | The final acceptance criterion. Everything else is verified. |
| Does the daemon survive a real sleep/wake or reconnect cycle? | Sleep the controller, wake it, confirm re-attach without restarting the service | Phase 06 stabilization |

## Resolved, with how

| Question | Resolution |
| --- | --- |
| Can Bluetooth receive rumble commands? | **Solved.** A 4-byte frame `11 <left> <right> <duration_ms>` on the Nordic UART RX characteristic. The dispatcher requires an exact length of 4, which is why every earlier framing (5, 12, 15, 20 bytes) was silently dropped. Verified by sustained vibration plus a 3-level ordering test. See `docs/evidence/bluetooth-rumble-solved.md`. |
| Which physical M key maps to which byte-11 bit? | **Settled by firmware.** The scatter routine `FUN_00010a30` independently yields M1-M4 = byte 11 `0x04`/`0x08`/`0x10`/`0x20`, matching the captures. No longer table-derived. See `docs/evidence/vader3pro-firmware-disassembly.md`. |
| Is padctl an appropriate maintained base? | **No longer relevant.** Chosen architecture is a standalone Rust daemon: padctl's Bluetooth transport could not be shown to meet the requirements, and standalone avoided inheriting an unclear contract. |
| Which virtual identity exposes six extras AND gyro in Steam? | **Answered for extras, moot for gyro.** `uinput` with 21 buttons plus a `gamecontrollerdb` mapping; `uhid` was rejected because all descriptor variants produced `capabilities/ff = 0`, so rumble would have been impossible. Gyro cannot be exposed because the stream carries none. |
| What is the project's distribution license? | **Decided:** `MIT OR Apache-2.0`, confirmed explicitly before publication. No third-party source was incorporated. |
| What produced byte 9 low bits `0x01`-`0x07` in session 1? | **Explained:** they are the D-pad hat (values 1/3/5/7 = up/right/down/left) from unrequested touches. Separately, the belief that this collided with M1-M4 was wrong. |
| Is the candidate installer authenticated? | **Moot for the release.** The installer was used only for research, never executed, and nothing from it is shipped or required at runtime. |

## Deliberately out of scope

| Item | Why |
| --- | --- |
| Native gyro | The Bluetooth DInput stream carries no motion data at all — a whole-controller shake produced no change in the stick bytes. Gyro exists only in vendor mode, whose switch could not be reached over Bluetooth; the vendor GATT characteristic disconnects the controller on unrecognised frames, and one `0x13` attempt reset its Bluetooth bond. Revisiting needs a recovery plan for a controller that forgets its pairing. |
| LED control | The chunked LED path (`0x33`) ends in flash erase/program and can commit partial state if a sequence is abandoned. The lighter mode command (`0x13`) was rejected over Bluetooth with ATT `0x0e` and dropped the connection. Both are poor risks for a cosmetic feature. |
| Firmware modification | Scoped and then abandoned: rumble turned out to be reachable on stock firmware 6.7.5.1 with no device-side change. |
| XInput mode | It already works with Steam, but it does not expose the extra buttons, which is the entire reason this project exists. |

## Notes

- No license was selected without owner approval; the dual license was confirmed
  explicitly before publication.
- Records in `docs/evidence/` name firmware functions and give byte layouts taken
  from disassembly of proprietary artifacts. Only observed behaviour is described;
  no extracted code or proprietary binary is included in this repository.
