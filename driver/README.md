# vader3p-steam (crate)

The Rust daemon. **Start with the [project README](../README.md)** for
installation, configuration and troubleshooting — this file covers only the
internals a developer needs.

## Layout

| File | Responsibility |
| --- | --- |
| `src/main.rs` | CLI, signal handling, exit codes |
| `src/config.rs` | TOML configuration and defaults |
| `src/device.rs` | hidraw discovery, report reading, `poll(2)` wait |
| `src/input.rs` | 21-byte DInput report → canonical `State` |
| `src/encode.rs` | canonical state → virtual-pad axis values |
| `src/pad.rs` | the virtual pad (`uinput`), 21 buttons + 6 axes + `FF_RUMBLE` |
| `src/ff.rs` | host force-feedback upload handshake |
| `src/rumble.rs` | rumble translation: Nordic UART frame, USB fallback |
| `src/suppress.rs` | `EVIOCGRAB` so the physical pad is not seen twice |
| `src/lib.rs` | the translator loop that joins the above |
| `src/uhid.rs` | UHID implementation — **diagnostics only**, not used by the daemon |

`src/uhid.rs` is retained because it is the instrument that proved the virtual pad
must use `uinput`: all three UHID descriptor variants produced
`capabilities/ff = 0`, so Steam could not have rumbled a UHID pad. See
[`docs/evidence/virtual-pad-backend.md`](../docs/evidence/virtual-pad-backend.md).
It is exercised only by the `ff_probe` and `uhid_caps` examples.

## Design decisions worth knowing before changing things

**`uinput`, not `uhid`.** Measured, not assumed — see above.

**Axis codes are positional, not mnemonic.** SDL assigns joystick axis slots by
the numeric order of kernel `ABS_*` codes, and its default gamepad mapping reads
`a2` as right-x, `a3` as right-y, `a4` as left trigger. So right-stick Y must use
`ABS_RX` and the triggers `ABS_RY`/`ABS_RZ`. Assigning by name makes SDL read stick
motion as trigger presses. `AXIS_ASSIGNMENT` in `pad.rs` is the single source of
truth, with a test that the codes stay ascending.

**Axis ranges must be declared.** `UI_SET_ABSBIT` alone leaves a range of 0..0, so
every value collapses to one point and the axis never moves. `UI_ABS_SETUP` is
required.

**The read loop blocks on `poll(2)`.** Sleeping for a fixed interval added that
interval to every input — measured at exactly 4 ms p95 against a 0.16 ms transport
floor. Blocking wait brought it to 0.03 ms at 0.6% idle CPU.

**`read_state` drains the queue and returns the newest state.** Returning the first
parseable report leaves the rest buffered, so the pad falls further behind during
a burst.

**Rumble needs an exact 4-byte payload.** The firmware dispatcher drops any other
length silently, which is why every earlier framing appeared to do nothing. Byte 3
is the duration, bytes 1–2 the levels, and there is no start/stop latch — the
daemon re-sends every 100 ms.

## Testing

```sh
cargo test                                  # 97 tests
cargo clippy --lib --bins --tests -- -D warnings
cargo fmt --check
```

Shipped code (`src/`, `tests/`) must be warning-free and formatted. The examples
under `examples/` are exempt: they resolve SDL symbols through `dlopen` and use
`transmute` deliberately.

Tests worth reading, because each pins a bug that shipped once:

- `pad::tests::axis_assignment_matches_sdl_slot_order` — right stick routed to the
  trigger slot
- `pad::tests::dpad_codes_follow_canonical_order_not_numeric_order` — D-pad
  right/left swapped with down
- `pad::tests::abs_setup_struct_matches_the_kernel_abi` — the 28-byte struct
- `device::tests::newest_state_returns_the_last_report_not_the_first` — queue drift
- `descriptor::tests::descriptor_declares_21_buttons`

## Examples

Diagnostic probes that produced the measurements in this repository. They are kept
so the claims can be re-checked rather than trusted.

```sh
cargo run --release --example signal_probe    # verify all 21 buttons + 6 axes
```

| Example | Purpose | Root |
| --- | --- | --- |
| `signal_probe` | Drive every input, read back which SDL slot changed | no |
| `sdl_probe` | Show SDL's binding for the virtual pad | no |
| `raw_buttons` | Raw joystick index per physical press | no |
| `uinput_ff_probe` | Confirm `FF_RUMBLE` is declared | no |
| `ff_handshake_probe` | End-to-end effect-upload handshake | no |
| `ff_path_probe` | Where FF events arrive | no |
| `ff_event_arrival` | Whether `EV_UINPUT` reaches the fd | no |
| `ff_probe` | UHID descriptor variants vs `capabilities/ff` | yes |
| `uhid_caps` | UHID descriptor → kernel capability bitmaps | yes |
| `uhid_smoke` | Minimal UHID device creation | yes |

The last three need root because `/dev/uhid` is mode `0600`.

## User-facing scripts

In `contrib/`, and documented in the main README:

- `interactive-test.py` — press things, see them named (both streams side by side)
- `latency-test.py` — report rate and end-to-end button latency
- `test-run.sh` — build, test, and probe in one non-destructive run
- `gamecontrollerdb.txt` — SDL mapping that binds the six extras

## Documentation

- [`../docs/PROTOCOL.md`](../docs/PROTOCOL.md) — every byte, with evidence class
- [`../docs/ARCHITECTURE.md`](../docs/ARCHITECTURE.md) — data path
- [`../docs/evidence/`](../docs/evidence/) — individual evidence records
