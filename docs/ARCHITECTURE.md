# Architecture

Status: **runtime implemented** for Bluetooth DInput input and rumble. Steam
recognition of the six extras remains an open acceptance criterion.

## Data path

```text
Vader 3 Pro ── Bluetooth DInput HID ──> /dev/hidrawN
                                            │
                                    input::parse (21-byte reports)
                                            │
                                     canonical State
                                            │
                              pad::VirtualPad::send  ── uinput ──> evdev gamepad
                                            │                        │
                                            │                    Steam Input
                                            │                        │
                                            │              EV_FF + UI_FF_UPLOAD
                                            │                        │
                                    pad::poll_host <─────────────────┘
                                            │
                                   rumble::bt_frame
                                            │
                            Nordic UART 6e400002 (BlueZ GATT write)
                                            │
                                     physical vibration
```

## Decisions

### Standalone Rust daemon

Chosen because the upstream padctl checkout is not present in this environment
and the required Bluetooth rumble path (a 4-byte vendor frame over a GATT
characteristic) is specific to this device. Zig is not installed; Rust 1.96 is.
The device logic is small enough that a standalone daemon is less work than
extending another project's transport layer.

### uinput rather than UHID for the virtual pad

**Measured, not assumed.** Three HID descriptor variants — a vendor output
report and two Physical Interface Device shapes — all produced
`capabilities/ff: 0` through `/dev/uhid`, because the HID path needs
`hid-universal-pidff` to bind and that driver is an unloaded module here.
uinput declares `FF_RUMBLE` directly and measured `capabilities/ff: 10000 0`,
which is bit 80 = `FF_RUMBLE`.

Rumble is a required feature (F4, A7). A required feature must not depend on an
optional kernel module being loaded, so the pad is created through uinput.
Details and the raw measurements: `docs/evidence/virtual-pad-backend.md`.

### Separate transport parsers

`input::parse` handles only the Bluetooth DInput 21-byte stream and is kept free
of transport knowledge. `device` handles discovery and reading; `pad` handles
host-facing state; `rumble` owns both delivery mechanisms. Rumble delivery is
**not** shared between transports, because the two are genuinely different:

| Transport | Mechanism | Evidence |
| --- | --- | --- |
| Bluetooth | 4-byte `11 <L> <R> <ms>` on the Nordic UART characteristic | hardware-verified sustained vibration |
| USB | 31-byte `05 0F <L> <R>` on the `0xFFA0` interface (`mi_02`) | hardware-verified sustained vibration |

Getting this wrong fails silently: on Bluetooth the descriptor declares zero
output reports, so a hidraw write succeeds and does nothing.

## Safety invariants

- **Neutralise on loss.** A physical read error or shutdown sends a neutral
  state to the virtual pad and a zero rumble command, so no stick is left held
  and no motor keeps running.
- **Scoped matching.** Discovery matches `d7d7:0041` only, so the Legion Go's
  built-in controller and other devices are untouched.
- **No permanent device writes.** The driver never sends configuration,
  LED-blob (`0x33`) or flash-commit commands. `0x33` was analysed and rejected
  because it ends in a flash erase/program; see
  `docs/evidence/led-control-paths.md`.
- **Bounded FF handshake.** The `UI_BEGIN_FF_*`/`UI_END_FF_*` round trip is
  answered promptly; a missed answer blocks the host's `upload_effect()` call
  until it times out.
- **No duplicate suppression of the physical node** yet. The physical controller
  still appears to the system, so PRD A2 is unmet and suppression is
  deliberately deferred until safe cleanup is proven.

## Deviations from the PRD's preferred path

The PRD named `controller → Bluetooth HID → userspace translator → virtual UHID
→ Steam`. The virtual device is uinput rather than UHID, for the measured reason
above. This is a deviation in mechanism, not in shape: it is still a userspace
translator presenting a virtual controller to Steam, with no kernel module, no
OS image change and no privileged daemon.

Python under `tools/` remains development-only research scaffolding.

## Open architecture questions

1. Whether Steam's controller editor presents all six extras as individually
   assignable on a generic 21-button uinput pad (A4, A5). If not, the fallback
   is a DualSense-style identity with a translated report format.
2. Whether the physical node can be suppressed without disturbing the Legion
   Go's built-in controller (A2).
3. Native gyro (F3, A6). The Bluetooth DInput stream carries no motion; gyro
   exists only in vendor mode, which requires `05 EE 01` — analysed but never
   verified over Bluetooth, and the `0x13` transport-mode experiments caused a
   bond reset, so vendor mode is not something to enable casually.
