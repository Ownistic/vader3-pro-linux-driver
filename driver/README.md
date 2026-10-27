# vader3p-steam

Userspace translator that makes a **Flydigi Vader 3 Pro over Bluetooth** usable
as a full controller in Steam Input on Linux, including the six extra buttons
(M1–M4, C, Z) and physical rumble.

Status: **runtime implemented; Steam acceptance untested.** Input parsing, the
virtual pad and the host force-feedback path are verified. Whether Steam's
controller editor presents all six extras individually is not yet verified on
the target machine.

## Why this exists

In Bluetooth DInput mode the controller reports 21-byte HID frames containing
M1–M4 and C/Z as separate fields, but:

- DInput's descriptor declares **zero output reports**, so the controller cannot
  be rumbled through the normal HID feedback path.
- The stock FlyDigi profile remaps C/Z/M1–M4 to other buttons, so they do not
  arrive in Steam as independent controls.

This daemon reads the physical reports, presents a virtual gamepad where all 21
buttons are independent, and delivers rumble over the Nordic UART characteristic
— which is where the controller actually accepts it.

## Verified behaviour

| Item | Status | Evidence |
| --- | --- | --- |
| 21-byte DInput report parsing | Verified | owner-coordinated captures, replayed in tests |
| All six extras independent in the report | Verified | firmware scatter table + captures |
| Physical rumble over Bluetooth | Verified on hardware | sustained vibration, 3-level ordering test |
| Physical rumble over USB | Verified on hardware | sustained vibration on `mi_02` `0xFFA0` |
| Virtual pad exposes 21 buttons | Verified | kernel `capabilities/key` = 21 bits |
| Virtual pad declares `FF_RUMBLE` | Verified | kernel `capabilities/ff` = bit 80 |
| Physical node suppressed | Verified | a second reader received 0 events while grabbed |
| Host effect upload handshake | Verified | `EVIOCSFF` completes, magnitudes arrive scaled |
| Steam recognises the pad and the six extras | **Untested** | runs on the Legion Go |
| Native gyro | **Not implemented** | absent from the Bluetooth DInput stream |

## Build

```sh
cd driver
cargo build --release
```

## Run

The controller must already be connected over Bluetooth in DInput mode.

```sh
./target/release/vader3p-steam --config ~/.config/vader3p-steam/config.toml
```

Minimal configuration:

```toml
# Required for Bluetooth rumble: rumble is delivered over a GATT characteristic,
# so the daemon needs the controller's address to find it.
bluetooth_address = "AA:BB:CC:DD:EE:FF"

virtual_name = "Flydigi Vader 3 Pro (translated)"
rumble = true

# Hide the physical controller's input events so it does not register twice.
# Reversible: the kernel drops the grab when the process exits.
suppress_physical = true

log_level = "info"
```

Find the address with `bluetoothctl devices Connected`.

## Permissions

Two device nodes are needed, and both should be granted narrowly:

- `/dev/uinput` — creating the virtual pad.
- `/dev/hidrawN` for the controller — reading physical reports.

A scoped udev rule is the reproducible way to do this. Keep it limited to the
specific vendor/product pair, and do not make nodes world-writable:

```
# /etc/udev/rules.d/70-vader3p-steam.rules
KERNEL=="uinput", TAG+="uaccess"
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="d7d7", ATTRS{idProduct}=="0041", TAG+="uaccess"
```

`uaccess` grants the logged-in seat user access without opening the node to
everyone. Verify with `getfacl /dev/uinput` before running the daemon.

## Rumble specifics

Bluetooth rumble uses a **4-byte** frame on the Nordic UART RX characteristic:

```
11 <left> <right> <duration_ms>
```

Two details are easy to get wrong and both were confirmed against firmware
6.7.5.1:

- **Byte 3 is the duration**, bytes 1–2 are the levels. The dispatcher requires
  a payload length of exactly 4; any other length is dropped silently.
- There is **no start/stop latch**. The duration is a hard firmware timeout, so
  the daemon re-sends every 100 ms to sustain vibration.

Levels clamp at **250**, not 255.

## Known limitations

- **No native gyro.** The Bluetooth DInput stream carries no motion data;
  gyro lives only in vendor mode (`05 EE 01`), which was never verified over
  Bluetooth and whose exploration reset the controller's Bluetooth bond once.
- **Suppression hides events, not the device entry.** `suppress_physical` grabs
  the physical evdev node exclusively, so nothing else receives its input. The
  node still exists, so a device list can show both — but only one produces
  input.
- **Steam recognition of the six extras is unverified.** If Steam collapses them,
  the fallback is a DualSense-style identity with a translated report format.
- **LED control is not exposed.** The `0x33` LED path ends in a flash
  erase/program and can commit partial state on timeout, so it is deliberately
  unused. `0x13` effect selection was rejected by the device over Bluetooth.

## Testing

```sh
cd driver
cargo test              # 74 tests: parser, encoder, ABI layouts, fixture replay
cargo clippy --all-targets
```

Diagnostic probes used to establish the measurements above live in
`driver/examples/`:

| Example | Purpose | Needs root |
| --- | --- | --- |
| `ff_probe` | UHID descriptor variants vs `capabilities/ff` | yes |
| `uinput_ff_probe` | uinput `FF_RUMBLE` declaration | no |
| `ff_handshake_probe` | end-to-end effect upload handshake | no |
| `uhid_caps` | UHID descriptor → kernel capability bitmaps | yes |

## Documentation

- `docs/PROTOCOL.md` — every byte and command, with evidence class
- `docs/ARCHITECTURE.md` — data path and design decisions
- `docs/evidence/` — individual evidence records
- `PRD.md` — requirements and acceptance criteria
