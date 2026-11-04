# Steam and the six extra buttons: which names exist

The pad exposes 21 buttons: 15 standard plus C, Z and M1–M4. Steam only exposes a
button it can *name*, and SDL2's vocabulary is smaller than 21 minus the one it
reserves.

## Measured on the target

Legion Go (Bazzite 44), driver running, `pad_listen.py` reading the virtual node.

| Press | Kernel code | Joystick index | Mapping name | Steam |
| --- | --- | --- | --- | --- |
| C | `BTN_TRIGGER_HAPPY1` (0x2c0) | b15 | `misc1` | visible as a centre button |
| Z | `BTN_TRIGGER_HAPPY2` (0x2c1) | b16 | `touchpad` | not shown at all |
| M1–M4 | `BTN_TRIGGER_HAPPY3..6` | b17–b20 | `paddle1..4` | visible as paddles |

So both buttons reach the pad correctly. Nothing is lost in the driver, the
capture, or the decode: the loss is in the name `touchpad`.

## Why `touchpad` cannot work

SDL2's `SDL_GameControllerButton` ends at `SDL_CONTROLLER_BUTTON_TOUCHPAD`:

```
SDL_CONTROLLER_BUTTON_MISC1     /* share / mic / capture */
SDL_CONTROLLER_BUTTON_PADDLE1
SDL_CONTROLLER_BUTTON_PADDLE2
SDL_CONTROLLER_BUTTON_PADDLE3
SDL_CONTROLLER_BUTTON_PADDLE4
SDL_CONTROLLER_BUTTON_TOUCHPAD  /* PS4/PS5 touchpad button */
```

There is no `misc2`–`misc6` in SDL2; those were added in SDL3, which Steam does not
use. Counting them: 11 face/shoulder/menu buttons, 4 d-pad directions, then
`misc1`, four paddles and `touchpad` is 21. Our pad has exactly 21 buttons, so
every name is taken — and `touchpad` is the only one Steam does not honour on a
generic device.

The obvious next idea was to *make* the device touchpad-capable by declaring
`ABS_MT_POSITION_X`/`_Y` and `INPUT_PROP_POINTER`, so SDL would report a touchpad
and Steam would honour the touchpad button. That was tested with a throwaway
uinput pad, created through `/dev/uinput` directly because `INPUT_PROP_POINTER`
must be set before `UI_DEV_CREATE`:

```
== plain ==
  probe plain: game_controller=True touchpads=0
== touch ==
  probe touch: game_controller=True touchpads=0
```

**SDL2's evdev backend never reports touchpads.** Touchpad support comes from the
hidapi backend for DualShock/DualSense devices. Declaring the axes changes nothing,
so the route is closed at SDL and never reaches Steam. Do not add `ABS_MT` axes to
the pad on this theory; it would only widen the descriptor for no benefit.

## Could emulating a different controller expose all six?

This was asked directly, and the answer is no for any model Steam supports today.
The ceiling is set by how many *named* extra inputs a device model has, not by the
physical pad:

| Model | Extra inputs beyond the 15 standard | Total |
| --- | --- | --- |
| Xbox Elite Series 2 | 4 paddles + share (`misc1`) | 5 |
| DualSense | mute (`misc1`) + touchpad click | 2 |
| DualShock 4 | touchpad click | 1 |
| Switch Pro | capture (`misc1`) | 1 |

The richest model is the Elite with five, which is what the current mapping already
provides. Two tempting ideas and why they do not help:

- **Pretending to be a DualSense.** It would gain the `touchpad` name, but SDL only
  reports touchpads for hidapi devices, and a hidapi identity means SDL's own PS5
  report parser defines the inputs — a DualSense has no paddles to bind. Five extras
  would become two.
- **SDL3 names.** `misc2`–`misc6` would give enough slots, but Steam ships SDL2, and
  nothing on this side of the boundary changes that.

A device emulated through UHID rather than uinput would be seen by SDL's hidapi
backend and could therefore carry a touchpad *button*, but only while also
inheriting that device model's input set. That trade loses more than it gains.

So the sixth button cannot be a Steam-bindable controller input. If it must be
usable, it has to stop being one: remap it in the controller to a key, or to a
button that already has a slot.

## Consequence

At most **five of the six** extras can be bindable through Steam on a uinput pad:
four paddles plus `misc1`. The current mapping gives those five to C and M1–M4, so
**Z is the one that cannot be bound**.

If Z is needed, the options are, in order of cost:

1. **Swap the casualty.** Give Z `misc1` and C `touchpad`. Still five of six, but a
   different one missing. The mapping line changes in two places.
2. **Remap Z in the controller.** Space Station can make Z emit an input that is
   already exposed, or a key. This is the only way to get all six *through a
   controller interface*, because the limit is Steam's vocabulary, not the driver.
3. **Accept five.** C and M1–M4 cover the usual needs.

The mapping line that produces the current arrangement is in
`driver/contrib/gamecontrollerdb.txt`; the extra bindings are
`misc1:b15,touchpad:b16,paddle1:b17..paddle4:b20`.
