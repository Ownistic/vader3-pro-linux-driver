# Virtual pad: measured behaviour

Status: **measured on this machine** with `driver/examples/signal_probe`, which
creates the pad through the real `pad::VirtualPad` API, drives one input at a
time, and asks SDL which joystick slot changed. Every number below is an
observation, not a derivation.

## Why the probe exists

Two properties of the virtual pad cannot be checked from kernel capability
bitmaps alone:

1. Which SDL axis slot each kernel `ABS_*` code lands on. SDL resolves slots
   positionally from the sorted code set, not by name.
2. Which SDL button index each extra produces, which determines the
   `gamecontrollerdb.txt` entry.

Both were previously assumed. Both assumptions were wrong.

## Axis routing

SDL reports 6 axes. Moving one input at a time produced:

| Input driven | Slots that changed |
| --- | --- |
| `left_x` | `a0` |
| `left_y` | `a1` |
| `right_x` | `a2` |
| `right_y` | `a3` |
| `left_trigger` | `a4` |
| `right_trigger` | `a5` |

That is exactly what SDL's default gamepad mapping assumes
(`rightx:a2, righty:a3, lefttrigger:a4, righttrigger:a5`).

**Two defects were found getting there, both user-visible:**

- **Axes did not move at all.** `UI_SET_ABSBIT` enables an axis but leaves its
  range at 0..0, so every value collapses to a single point. A range must be
  declared with `UI_ABS_SETUP` (`0x401c5504`, 28-byte struct). Without this, a
  client sees six axes that are permanently centred.
- **Right stick reported as the right trigger.** Assigning `ABS_*` codes by
  name put right-stick Y on `ABS_RZ`, which SDL reads as `a5` — the right
  trigger slot. The correct assignment is positional, not mnemonic: `RightY`
  uses `ABS_RX` and the triggers use `ABS_RY`/`ABS_RZ`.

## Button routing

SDL reports 21 buttons. Pressing one control at a time produced:

| Control | Index | Control | Index |
| --- | --- | --- | --- |
| A | `b0` | D-pad up | `b11` |
| B | `b1` | D-pad down | `b12` |
| X | `b2` | D-pad left | `b13` |
| Y | `b3` | D-pad right | `b14` |
| LB | `b4` | C | `b15` |
| RB | `b5` | Z | `b16` |
| Select | `b6` | M1 | `b17` |
| Start | `b7` | M2 | `b18` |
| Home | `b8` | M3 | `b19` |
| L3 | `b9` | M4 | `b20` |
| R3 | `b10` | | |

Every control mapped to **exactly one** index — no aliasing, nothing missing.

Note the D-pad ordering: `jbtn14` is **right**, not left. The canonical order in
this project is up, right, down, left, while the kernel numbers
`UP=0x220, DOWN=0x221, LEFT=0x222, RIGHT=0x223`. Listing the codes in numeric
order swapped right and down. A test now pins the intended order.

## Why the extras need a mapping file

SDL ships a default mapping for generic gamepads that binds **b0–b14 only**.
Measured with the virtual pad present:

```
bound   : a=jbtn0 .. dpright=jbtn14
UNBOUND : misc1, paddle1, paddle2, paddle3, paddle4, touchpad
```

So the six extras exist as joystick buttons and are invisible as *gamepad*
controls. That is the difference between "the hardware reports them" and "Steam
can bind them", and it is what `driver/contrib/gamecontrollerdb.txt` fixes.
With that entry applied, all six bind:

```
misc1:b15, touchpad:b16, paddle1:b17, paddle2:b18, paddle3:b19, paddle4:b20
```

`misc1`/`touchpad` are used for C and Z because SDL 2.0's controller model has
no dedicated extra face-button slot; they are distinct bindable inputs, which is
what remapping requires. M1–M4 use `paddle1`–`paddle4`, the slots SDL defines
for rear paddles.

## Reproducing

```sh
cd driver
cargo run --example signal_probe
```

Requires no controller and no root: it creates its own pad and tears it down.

## Limits of this evidence

- Measured with local SDL 2.32. Steam bundles its own SDL, so Steam's reading of
  the same device could differ; the mapping file is the portable fix either way.
- The probe drives `pad::send` directly, so it proves the pad and the mapping.
  It does not prove the whole daemon path under Steam.
- Per-button count queries (`SDL_GameControllerNumButtons`) are unavailable in
  this SDL build, so counts come from the joystick layer.

## Stick axes under the daemon (measured)

The signal probe covers the pad in isolation. A separate interactive run through
the full daemon confirmed the sticks end-to-end: moving both sticks for 25 s
produced these raw `EV_ABS` samples on the virtual pad's event node.

| Axis | Samples | Range observed |
| --- | --- | --- |
| `LEFT STICK X` | 766 | 0..255 |
| `LEFT STICK Y` | 818 | 0..255 |
| `RIGHT STICK X` | 734 | 0..255 |
| `RIGHT STICK Y` | 766 | 0..255 |

Full travel in both directions, so no scaling or clamping problem exists in the
daemon path either.

### The reporting bug this exposed

The interactive probe initially reported no stick movement at all, and the
obvious reading — "sticks are broken" — was wrong. Its filter compared each
sample to the *immediately previous* one with a 90-count threshold. A hand-moved
stick changes only a few counts between reports, so no single step ever cleared
the threshold and nothing was printed. Triggers appeared to work only because
they jump `0 -> 117 -> 207` in a single report.

The fix compares against the last **reported** value, so gradual movement
accumulates. Thresholds are now 24 counts for sticks and 32 for triggers, which
produces several reports for a full sweep without one line per sample.

Two lessons recorded deliberately:

- **Absence of output is not evidence of absence of signal.** A reader with no
  activity is indistinguishable from a filter that rejects everything, so a probe
  must report its own sample counts — which is what made the difference here.
- The unit tests in `tests/test_interactive_probe.py` include one that asserts a
  *per-sample* comparison finds nothing, so the original defect is pinned rather
  than merely fixed.

## Latency, measured

Measured with `driver/contrib/latency-test.py` during a continuous right-stick
sweep with periodic A taps, 25 s window, over Bluetooth DInput.

| Metric | Value |
| --- | --- |
| Physical report rate | 128 /s |
| Virtual event rate | 324 /s |
| Report interval median | 7.61 ms |
| Report interval p95 | 8.95 ms |
| Button latency median (before) | 2.67 ms |
| Button latency p95 (before) | 4.00 ms |
| Button latency max (before) | 4.10 ms |
| Button latency min (before) | 0.16 ms |
| **Button latency median (after)** | **0.03 ms** |
| **Button latency p95 (after)** | **0.10 ms** |
| Button latency max (after) | 0.11 ms |
| Button latency min (after) | 0.02 ms |

### The 4 ms figure was the sleep, not the device

The distribution is the giveaway: `min 0.16 ms` but `p95 4.00` and `max 4.10`.
Those round numbers are `poll_ms = 4` quantised. The loop slept for a fixed
interval instead of waiting for the fd, so every input paid the full interval in
the worst case. The minimum is what the transport can actually do.

Replacing the sleep with `poll(2)` on the hidraw fd removes the floor: the kernel
wakes the loop the moment a report arrives, and the timeout only bounds
housekeeping. Idle CPU cost after the change measured **0.6% of one core**
(3 ticks over 5 s), so blocking wait costs nothing when nothing happens.

Re-measured after the change, same procedure, 37 paired presses:

| | before | after |
| --- | --- | --- |
| median | 2.67 ms | **0.03 ms** |
| p95 | 4.00 ms | **0.10 ms** |
| max | 4.10 ms | **0.11 ms** |

The quantisation is gone: p95 and max no longer sit on 4 ms, and the spread is
now tight (0.02–0.11 ms) rather than a flat 4 ms plateau. That is the signature
of a wait that ends on the event instead of on a timer.

Translator overhead is therefore about **0.03 ms**, roughly 250× below the
device's own 7.6 ms report interval. Input latency is now dominated entirely by
the controller and the Bluetooth link, not by this software.

### Reading these numbers correctly

- **128 reports/s is the device's rate, not a cap.** The controller is
  edge-triggered: it reports when something changes. A hand sweep produced 34
  distinct left-stick-X values across 3217 reports, so reports arrive far faster
  than the axis changes — the rate reflects motion, not a polling clock.
- **Virtual events exceed physical reports** because each report expands into
  several evdev events (axes plus a `SYN_REPORT`), which is expected.
- **Report-interval outliers (217 ms, then 134 ms) are stalls in the *device's*
  reporting**, not the translator. They appear roughly once per 25 s, coincide
  with the radio link, and do not affect the measured button latency — which
  stayed at 0.11 ms max across the same run. They are worth watching only if
  they show up as a visible stick freeze during play.
- **255 distinct left-stick-X values** were seen in the 3272 reports of the
  post-fix run, i.e. the full 8-bit range. No resolution is lost in translation.
