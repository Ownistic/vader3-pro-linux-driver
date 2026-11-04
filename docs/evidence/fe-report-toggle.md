# The motion report has a controller-side toggle

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

## What is not yet known

- Which physical buttons correspond to logical codes `0x12`, `0x13`, `0x14`, `0x15`
  and `0x18`. Candidates are documented in the key-mapping tables, but the mapping
  has not been resolved.
- Whether the toggle persists across a power cycle or only for the session.
- Whether the same combo works while connected over Bluetooth (the handler is
  transport-independent, but that is reasoning, not an observation).

## How to settle it

Two routes, cheapest first:

1. **Read the key-code table.** The controller's own remap tables and the app's key
   enumerations map logical codes to physical buttons; `0x12`–`0x18` are close
   enough together to be an FN-layer row.
2. **Try combos while capturing the raw stream.** With the controller wired, press
   candidate three-key combos and watch for the report id changing to `0xFE`. The
   rumble pulse marks a hit even without reading the stream.

Either way this is testable with no flashing.
