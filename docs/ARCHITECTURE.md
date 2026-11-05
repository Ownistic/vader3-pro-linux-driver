# Architecture

The supported product path is Bluetooth DInput to a DualSense Edge UHID device.
Hardware acceptance of the current build remains incomplete; see
[testing](TESTING.md) and [the v4 evidence](evidence/uhid-runtime-retest-2026-11-04.md).

```text
physical Bluetooth HID -> hidraw -> input parser ----+
                                                      +-> canonical state -> Edge reports -> /dev/uhid -> Steam
physical Bluetooth NUS -> input/motion parser --------+                         ^
             ^                                                                  |
             +---------------- Bluetooth NUS rumble <- BLE worker <- host output+
```

Motion is opt-in. With it off, ordinary 21-byte HID input is used. Once NUS
input is observed with motion enabled, it takes priority; stale input and motion
are neutralized rather than retaining a stuck state. The two motion components
are provisionally swapped and both emulated X (pitch) and Y (yaw) inverted by
default. Inversion is applied independently after swapping. The third is zero
and acceleration is fabricated. Units and calibration remain unverified. The BLE
worker checks selector status and notifications within bounded observation
windows; a write alone is not proof of activation.

Discovery restricts candidates to bus 5 and the known target VID/PID or recorded
DInput descriptor. The observed `04b4:2412` device matches by descriptor. Bus 5
does not prove an actual Bluetooth link; forwarded dongles may use it too. The
virtual device outlives physical disconnect; state is neutralized and reattach
reacquires the physical handle and grabs. Rumble uses NUS only.

EVIOCGRAB is best-effort and reversible on exit. It blocks physical evdev
events, not enumeration or hidraw access. The optional manual Steam visibility
workaround is documented [separately](steam-controller-visibility.md); the
daemon does not modify Steam configuration or unbind kernel drivers.

Earlier generic UHID PID and uinput experiments are historical; their measured
latency and force-feedback results do not establish behavior of this Edge path.
Research tools remain independent of the product runtime.
