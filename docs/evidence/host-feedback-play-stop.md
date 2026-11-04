# Host feedback: play/stop semantics and the endless-rumble defect

Hardware defect found on 2026-11-03 while installing the driver on a Legion Go
(Bazzite 44, kernel 7.2.11). Firmware 6.7.5.1, controller paired as
`A4:C1:38:30:EA:AB` (`Flydigi VADER3P`), present as `04b4:2412` on bus 5.

## Symptom

The host played a rumble effect for 1.5 seconds. The controller vibrated and kept
vibrating until it was powered off — roughly a minute, and only the power switch
stopped it.

## Two independent causes

Both were in `driver/src/pad.rs`, and both are consequences of modelling upload as
playback.

### 1. `EV_FF` value was ignored

The kernel reports playback as `(EV_FF, effect id, value)`, where `value` is 1 to
start and 0 to stop, and `code` is the effect id. The handler compared `code`
against `FF_RUMBLE` (0x50) and returned the stored magnitudes for everything else:

```rust
match code {
    FF_RUMBLE => self.rumble,          // never matches: code is an effect id
    FF_GAIN => { ... }
    _ => self.rumble,                  // stop lands here and is ignored
}
```

`code` is an effect id in `0..15`, so the `FF_RUMBLE` arm was unreachable and the
stop event took the `_` arm, returning the same non-zero level. The effect could
start and never stop.

Measured event sequence, captured by instrumenting `poll_host`:

```
DIAG control code=1 value=0            UI_FF_UPLOAD
DIAG upload id=0 type=0x50 strong=65535 weak=32768 len=1000
DIAG ff code=0 value=1                 play
DIAG ff code=0 value=0                 stop
DIAG ff code=0 value=0                 stop again, from erase
DIAG control code=2 value=0            UI_FF_ERASE
```

This settles an open question from earlier work: `EV_FF` play and stop events **do**
arrive on the uinput fd read queue, so the daemon can see them. Requiring play
before starting is therefore safe and is not a regression risk.

### 2. Erasing an effect did not notify the delivery loop

Erase cleared the rumble field but returned nothing to the main loop. The delivery
loop only stops when it is handed a zero level, so it kept retransmitting the last
non-zero level on its retry timer indefinitely. Even with the play/stop bug fixed,
erasing a playing effect would have left the motors running.

## Fix

Feedback state moved into `FfState`, which models the kernel's actual protocol:

| Event | Effect on state |
| --- | --- |
| upload | stores magnitudes under the assigned id, does not play |
| `EV_FF` value != 0 | marks that id active, returns its scaled level |
| `EV_FF` value == 0 | stops only if that id is the active one |
| `EV_FF` `FF_GAIN` | stores gain, recomputes the active level |
| erase | forgets the id, and returns a stop level if it was active |

Levels are stored raw (`0..=0xffff`) and scaled at play time, so a gain change does
not rewrite the table, and the firmware ceiling of 250 is applied in one place.

`FfState` is separate from `VirtualPad` so the logic is testable without a uinput
device. Nine regression tests cover it. Three of them fail against the old
behaviour, verified by mutation: reverting the `value == 0` branch and the erase
return makes `stop_event_ends_the_effect`,
`erasing_the_playing_effect_reports_a_stop` and `stop_for_another_effect_is_ignored`
fail.

## Verified on hardware after the fix

| Check | Result |
| --- | --- |
| One 1.5 s effect, then stop | buzzed once and stopped, confirmed by the owner |
| A | `BTN_SOUTH` (304) |
| X | `BTN_NORTH` (307) |
| M1 | `BTN_TRIGGER_HAPPY3` (706) — extra slot 2, which is M1 by the documented order C, Z, M1, M2, M3, M4 |

Replay length is still not honoured as a duration. The firmware applies its own
timeout and the daemon retransmits while an effect is active, so a short effect
plays until the host stops it. This is adequate for Steam, which sends explicit
start and stop, and is the same behaviour that was verified above.
