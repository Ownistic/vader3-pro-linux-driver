# The motion report has a controller-side toggle

> **Correction:** `FUN_00011c44` writes `0xfe` at offset 9 of the same buffer
> that `FUN_00011984` passes to the settings-write path, not a buffer proven to
> be a USB report. Literal-pool entries `0x11c74` and `0x119ac` both resolve to
> `0x0084730c`. The claim below that the chord selects report `0xFE` on USB is
> unverified; no wired chord capture is documented here. The `0x96` flag and
> the `0x2a` flag set by `0xEE` are distinct fields and their equivalence is
> not established. The recorded Bluetooth chord and report observations remain
> observations, but rumble alone does not prove which state write occurred.
> `FUN_00009d20` reads the `0x96` flag when normalizing four analog channels;
> this supports an effect on control scaling, not a demonstrated motion toggle.
> See [static subsystem traces](static-subsystems-2026-11-04.md). A separate
> static path connects two sensor-state words to a Nordic UART
> notification-shaped builder; the recorded HID captures do not establish
> whether that separate characteristic delivered those values.

While deriving the vendor report layout, the builder that emits report id `0xFE`
turned out to have a caller outside the vendor-command path. That matters more
than the layout.

## What enables the report

```c
/* FUN_00011c44 */
void FUN_00011c44(char on) {
    if (on == 1) {
        *(char *)(state + 0x96) = 1;
        *(char *)(report_buf + 9) = 0xfe;     /* report id 0xFE */
    } else {
        *(char *)(state + 0x96) = 0;
        *(char *)(report_buf + 9) = 0;        /* back to report id 0x00 */
    }
    FUN_00011984();
}
```

`FUN_00011c44` has exactly **two** callers, both inside `FUN_0000c59c`, which is the
key-combo processor: it builds a list of currently pressed logical keys and tests
for combinations.

```c
if (FUN_0000b32c(buf) != 0) {          /* combo A */
    ...
    FUN_0000b62c(0x1e, 0xff, 0xff);    /* rumble pulse as feedback */
    FUN_00011c44(1);                   /* enable report 0xFE */
}
if (FUN_0000b33c(buf) != 0) {          /* combo B */
    ...
    FUN_0000b62c(0x1e, 0xff, 0xff);    /* rumble pulse as feedback */
    FUN_00011c44(0);                   /* disable report 0xFE */
}
```

The matchers are thin wrappers:

| Wrapper | Test |
| --- | --- |
| `FUN_0000b32c` | `FUN_0000b128(buf, 0x18, 0x12, 0x13)` |
| `FUN_0000b33c` | `FUN_0000b128(buf, 0x18, 0x14, 0x15)` |

and `FUN_0000b128` counts how many entries of the key list equal any of the three
codes, returning true when **three or more** match. So each combo is three logical
key codes pressed together.

## Why this changes the picture

The earlier conclusion was that only the `0xEE` vendor command selects the motion
report, and that no host command reaches it over Bluetooth — which made a firmware
patch the only route.

This shows a **second, independent way to select it: a physical button combo on the
controller itself.** A combo needs no transport at all, so if the codes can be
identified, the motion report becomes reachable over any link, on stock firmware,
with no patch.

The rumble pulse in both branches is also useful as an oracle: when the right combo
is pressed, the controller buzzes briefly.

## Tested on hardware: the chord fires, the Bluetooth stream does not change

Owner tested both chords on the controller, connected over **Bluetooth DInput**:

| Chord | Rumble acknowledgement | Stream changed? |
| --- | --- | --- |
| FN + M1 + M2 | **yes** (plus a second pulse a few seconds later) | **no** |
| FN + M3 + M4 | yes | **no** |

Both pulses confirm both matchers fired, so the key-code decode is correct.
A 60-second Bluetooth capture spanning a plain A press, stick movement, the chord,
and A plus stick again contains **289 reports, all report id `0x01` at 21 bytes**.
The motion report was never emitted over Bluetooth.

A second capture repeated the test in the explicit **disable-then-enable** order
(FN+M3+M4, pause, FN+M1+M2, then stick), in case an already-set flag made the
enable a no-op. Result identical: **275 reports, all `id=0x01`, all 21 bytes**.
Order does not matter, and the Bluetooth stream never changes.

So the chord sets the same flag the vendor command sets, and the flag changes the
report the firmware builds — but the **Bluetooth notification path still sends the
ordinary 21-byte report**. `FUN_00011c44` writes `0xFE` into a report buffer that
only the wired path transmits, matching the earlier finding that Bluetooth and USB
have separate dispatch paths.

**Conclusion: the chord is not a route to motion over Bluetooth.** It is still a
useful discovery — it enables the motion report on USB without sending a command,
and it confirms which logical keys the back buttons are.

## Side observation: a transient digitizer personality

After the chords, the controller re-enumerated over Bluetooth with a **196-byte
digitizer descriptor** — Usage Page `0x0D`, touch screen, three finger contacts,
12-bit absolute X/Y — under `04B4:2412`. A plain reconnect restored the 163-byte
gamepad descriptor. A digitizer only reports on touch, which explains a window of
silent captures during this test. The cause was not isolated; it may be a mode the
chords or an earlier FN+Y sequence selected.

## What is not yet known

- Which physical buttons correspond to logical codes `0x12`, `0x13`, `0x14`, `0x15`
  and `0x18`. Candidates are documented in the key-mapping tables, but the mapping
  has not been resolved.
- Whether the toggle persists across a power cycle or only for the session.
- Whether the same combo works while connected over Bluetooth (the handler is
  transport-independent, but that is reasoning, not an observation).

## The key codes decode to the back buttons

The application's own key-index table in `LibCommon.Config/Constant.cs` groups the
logical codes:

```csharp
public static int[] CommonKey   = { 4, 5, 7, 8, 0, 2, 3, 1, 12, 13, 10, 11, 6, 9, 14, 15 };
public static int[] FrontExtKey = { 16, 17 };
public static int[] BackExtKey  = { 18, 19, 20, 21, 22, 23 };   /* 0x12–0x17 */
public static int[] FuncKey     = { 24, 27, 28 };              /* 0x18, 0x1b, 0x1c */
```

`BackExtKey` is **six consecutive codes, `0x12`–`0x17`** — these are the back
buttons M1–M6. `FuncKey` holds `0x18`, `0x1b`, `0x1c`, one of which is FN.

Substituting into the matchers:

| Combo | Codes | Likely buttons | Effect |
| --- | --- | --- | --- |
| `FUN_0000b32c` | `0x18`, `0x12`, `0x13` | FN + M1 + M2 | **enable** report `0xFE` |
| `FUN_0000b33c` | `0x18`, `0x14`, `0x15` | FN + M3 + M4 | **disable** report `0xFE` |

So the toggle is a three-button chord on the controller's own back buttons. This is
a prediction from static analysis, not an observation: it assumes `0x18` is FN and
that `BackExtKey` order matches M1→M6, `0x12`→`0x17`.

## How to confirm

With the controller wired and the raw stream captured, hold **FN + M1 + M2**. Two
independent signals mark a hit:

1. the report id changes from `00`/`01` to **`0xFE`**, and
2. the firmware fires `FUN_0000b62c(0x1e, 0xff, 0xff)` — a short, strong rumble.

Then **FN + M3 + M4** should turn it back off.
