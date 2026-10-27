# Vader 3 Pro Linux Driver

> **Status: pre-1.0, under active development.**
>
> Everything here works, but the interface — configuration keys, CLI flags, the
> virtual pad's identity, and the install steps — may still change without a
> migration path until version 1.0. Steam recognition of the six extra buttons is
> not yet verified; see [Status](#status). Expect rough edges and treat every
> version before 1.0 as a preview.

Makes a **Flydigi Vader 3 Pro connected over Bluetooth** work as a complete
controller in **Steam Input** on Linux — including the six extra buttons (M1, M2,
M3, M4, C, Z) and real rumble.

**Userspace, not a kernel module.** No DKMS, no `modprobe`, nothing to rebuild
after a kernel update. It runs as a normal user process.

```
┌──────────────────┐   hidraw    ┌─────────────────┐   uinput   ┌──────────────┐
│ Vader 3 Pro      │───────────▶│ vader3p-steam   │──────────▶│ virtual pad  │──▶ Steam
│ (Bluetooth       │  21-byte    │                 │  EV_KEY    │ 21 buttons   │
│  DInput mode)    │  reports    │                 │  EV_ABS    │ 6 axes       │
└──────────────────┘             │                 │  EV_FF ◀──│ FF_RUMBLE    │
                                 └────────┬────────┘            └──────────────┘
                                          │ 4-byte frame, Nordic UART
                                          ▼
                               rumble on the real controller
```

---

## Why this is needed

In Bluetooth DInput mode the controller already sends everything: all 21 buttons,
including M1–M4 and C/Z as **separate fields** in the report. Two things still
stand between that and a usable Steam controller.

**1. There is no HID output report, so it cannot be rumbled normally.** The
DInput descriptor declares zero output reports. Writing to the hidraw node is
accepted and silently discarded — which is worse than failing, because a write
returning success looks like it worked.

Rumble is delivered instead over a separate GATT characteristic (Nordic UART),
using a 4-byte frame discovered by disassembling the controller's own firmware.

**2. Steam only binds 15 of the 21 buttons.** SDL's default mapping for a generic
gamepad covers `b0`–`b14`. The six extras exist as joystick buttons but are
invisible as *gamepad* controls, so they cannot be assigned. A mapping file is
included that binds them.

This daemon solves the first problem and ships the mapping file for the second.

---

## Steam Input is the point

This project does **not** remap buttons. It has no macro system, no profiles, no
per-game configuration, and deliberately no opinions about what M1 should do.

That is a design decision, and it follows from where the work already exists.
Steam Input is a full remapping engine: per-game layouts, chords, mode shifts,
radial menus, action sets, haptics, and a shareable community of layouts. Building
a second, smaller one inside this daemon would be strictly worse and would fight
Steam for the same job.

So the contract is narrow by intent: **every physical input must arrive in Steam as
its own distinct control.** That is all. Once M1 is a real button rather than a
bit nobody can see, every use for it — a keyboard key, a mode shift, a radial
menu, another controller's button — is Steam's job and already solved.

Two consequences worth knowing, because both look like missing features:

- **The six extras are exposed as paddles and misc buttons, not keyboard keys.**
  `misc1`, `touchpad`, `paddle1`–`paddle4` are the slots SDL has for exactly this
  (a controller that reports more buttons than a standard pad). They arrive as
  ordinary gamepad controls, so Steam Input can bind them like any other button.
- **Rumble is forwarded, not synthesised.** Steam's own haptic settings drive it
  through `FF_RUMBLE` on the virtual pad; the daemon translates those effects to
  the controller's Nordic UART frame. No game needs to know the difference.

---

## Status

**Working, with one important caveat.**

| Area | State |
| --- | --- |
| Reading the controller, all 21 buttons | Verified on hardware |
| Virtual pad exposing 21 separate buttons | Verified |
| Rumble (Bluetooth and USB) | Verified on hardware — felt, with a 3-level ordering test |
| Duplicate physical controller suppressed | Verified |
| Input latency | Measured at **0.03 ms** through the translator |
| **Recognition inside Steam** | **Not yet verified** |
| Native gyro | Not implemented — see [limitations](#known-limitations) |

Everything above is measured at the kernel and SDL level. What has *not* been
confirmed is how Steam itself presents the six extra buttons in its controller
editor. That is the next thing to check, and the mapping file is what should make
it work. Please treat this as beta software.

---

## Requirements

- **Linux** with BlueZ and `/dev/uinput` (tested on Bazzite/Fedora-based systems)
- **Rust 1.75+** to build from source
- **`libdbus` development headers** to build — `libdbus-1-dev` (Debian/Ubuntu),
  `dbus-devel` (Fedora/Bazzite), or `dbus` (Arch). Without them the build stops
  with `Package dbus-1 was not found`.
- A **Flydigi Vader 3 Pro** connected over **Bluetooth in DInput mode**
- Read access to the controller's hidraw node, write access to `/dev/uinput`

> **Not supported:** XInput mode (works already, but hides the extra buttons),
> the 2.4 GHz dongle (works via USB HID, but rumble there needs a different path
> and the goal is Bluetooth), and Windows or macOS.

---

## Install

### 1. Build

```sh
git clone https://github.com/Ownistic/vader3-pro-linux-driver.git
cd vader3-pro-linux-driver/driver
cargo build --release
```

If the build stops on `Package dbus-1 was not found`, install the headers:

```sh
sudo dnf install dbus-devel        # Bazzite / Fedora
sudo apt install libdbus-1-dev    # Debian / Ubuntu
sudo pacman -S dbus               # Arch
```

The binary lands at `driver/target/release/vader3p-steam`. Copy it anywhere on
your `PATH` if you like:

```sh
install -Dm755 target/release/vader3p-steam ~/.local/bin/vader3p-steam
```

### 2. Grant device access

Two nodes are needed. Grant them narrowly — do **not** make them world-writable.

```sh
sudo tee /etc/udev/rules.d/70-vader3p-steam.rules >/dev/null <<'EOF'
# Virtual pad creation
KERNEL=="uinput", TAG+="uaccess"

# Reading the physical controller (Bluetooth DInput identity)
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="d7d7", ATTRS{idProduct}=="0041", TAG+="uaccess"
EOF

sudo udevadm control --reload-rules
sudo udevadm trigger
```

Then unplug and replug the controller (or reconnect it) so the rule applies.

`uaccess` grants the logged-in desktop user access without opening the node to
everyone. Verify:

```sh
getfacl /dev/uinput          # should list your user
ls -l /dev/hidraw*           # find the Vader node, then:
getfacl /dev/hidrawN         # should list your user
```

If the controller is on a different vendor/product pair, substitute the real
values from:

```sh
grep -H "" /sys/class/hidraw/hidraw*/device/uevent | grep -i vader
```

### 3. Configure

```sh
mkdir -p ~/.config/vader3p-steam
cat > ~/.config/vader3p-steam/config.toml <<'EOF'
# Your controller's Bluetooth address. Required for rumble.
# Find it with:  bluetoothctl devices Connected
bluetooth_address = "AA:BB:CC:DD:EE:FF"

virtual_name = "Flydigi Vader 3 Pro (translated)"
rumble = true

# Hide the physical controller so it does not appear twice.
# Reversible: the grab is released when the process exits, even on a crash.
suppress_physical = true

log_level = "info"
EOF
```

### 4. Make the six extra buttons visible to Steam

The environment variable below is **verified** — it is how the mapping was
confirmed against local SDL:

```sh
SDL_GAMECONTROLLERCONFIG="$(grep -v '^#' driver/contrib/gamecontrollerdb.txt | grep -v '^$')" steam
```

That works but is awkward for daily use. The durable option is to put the entry
in Steam's own `gamecontrollerdb.txt`. **This is not verified on the target
machine** — Steam's controller configuration paths differ between versions, so
check which applies to yours rather than assuming:

```sh
# Create the file if absent; it may not exist on a fresh install.
mkdir -p ~/.steam/steam/controller_base
cat driver/contrib/gamecontrollerdb.txt >> ~/.steam/steam/controller_base/gamecontrollerdb.txt
```

Only the single uncommented line in that file is the mapping; the rest are
comments and are ignored by SDL. Re-running the command duplicates the line,
which is harmless but untidy — remove the old copy first if you redo it.

Restart Steam after either change. If Steam still shows the extras as one button,
that is the open question this project has not yet closed; the environment
variable route is the reliable fallback.

---

## Run

Connect the controller in **Bluetooth DInput mode**, then:

```sh
vader3p-steam
```

It uses `~/.config/vader3p-steam/config.toml` by default, or pass `-c <path>`.

Expected output:

```
[info] physical device: /dev/hidraw9 (d7d7:0041, bus 0x5, interface None, matched by id)
[info] transport: bus 5 (Bluetooth, or a device presented on bus 5)
[info] suppressed physical node /dev/input/event22
[info] rumble via Nordic UART characteristic
[info] virtual pad created: Flydigi Vader 3 Pro (translated)
```

Press `Ctrl+C` to stop. The virtual pad disappears and the physical controller is
released automatically.

**How to tell it is working:** press a button. The physical controller should stop
responding to other applications (it is suppressed) and the virtual pad should
take over. If you see the controller twice in a game, `suppress_physical` did not
take effect — check its warning in the log.

### As a background service (optional)

```sh
mkdir -p ~/.config/systemd/user
cat > ~/.config/systemd/user/vader3p-steam.service <<'EOF'
[Unit]
Description=Flydigi Vader 3 Pro Bluetooth translator
After=bluetooth.target

[Service]
ExecStart=%h/.local/bin/vader3p-steam
Restart=on-failure
RestartSec=3

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload
systemctl --user enable --now vader3p-steam
journalctl --user -u vader3p-steam -f      # follow the log
```

Because it restarts on failure, it will also re-attach when you reconnect the
controller.

---

## Verify it works

Three self-contained scripts, in increasing order of what they prove.

### Check that the six extras are individually bindable

```sh
cd driver && cargo run --release --example signal_probe
```

Needs no controller, no root, and no key presses — it creates its own pad, drives
each input, and asks SDL what it saw. Expect every control to map to exactly one
button, ending with:

```
all controls mapped to exactly one button each
```

### Press things and see them named

```sh
cd driver && python3 contrib/interactive-test.py 60
```

Starts the daemon itself, then names every button, stick and trigger you touch.
Two columns — `physical` (raw controller) and `VIRTUAL` (what the pad emits) — so
a missing press is localised instead of guessed at:

- both silent → controller asleep or disconnected
- `physical` fires but `VIRTUAL` does not → translator bug
- both fire → forwarding works

### Measure latency

```sh
cd driver && python3 contrib/latency-test.py 25
```

Reports report rate, interval, and end-to-end button latency. Move a stick and
tap A while it runs.

---

## Configuration reference

All keys are optional; anything missing uses the default shown.

| Key | Default | Meaning |
| --- | --- | --- |
| `bluetooth_address` | none | Controller address. **Required for rumble** — the GATT write needs it. |
| `virtual_name` | `Flydigi Vader 3 Pro (translated)` | Name the kernel sees for the virtual pad. |
| `rumble` | `true` | Forward host force-feedback to the controller. |
| `suppress_physical` | `true` | Grab the physical evdev node so the controller does not appear twice. |
| `vendor_id` | `0xd7d7` | Physical vendor ID. |
| `product_id` | `0x0041` | Physical product ID. |
| `poll_ms` | `4` | Idle housekeeping interval. Not an input latency floor — the loop blocks on `poll(2)`. |
| `log_level` | `info` | `error`, `warn`, `info`, `debug`, or `trace`. |

---

## Troubleshooting

**`no hidraw device with d7d7:0041; is the controller connected?`**
The controller is not attached, or is in the wrong mode. Confirm it is in
Bluetooth DInput mode (it should identify as `Flydigi VADER3P`), not XInput.

**`could not suppress /dev/input/eventN`**
Something else holds that node — often a previous daemon instance, or Steam
itself. Stop it first:

```sh
pkill vader3p-steam
```

Suppression is best-effort by design: if it fails the daemon logs a warning and
continues, because a duplicate controller is cosmetic while no controller would
be functional.

**Rumble does not work**
Most often a missing or wrong `bluetooth_address`. Confirm the daemon logged
`rumble via Nordic UART characteristic`; if it says it fell back to hidraw, the
address is wrong or BlueZ has no matching characteristic. Also confirm volume of
rumble is not being suppressed by the game — many games disable haptics by
default.

**The controller does not appear in Steam at all**
Check the virtual pad exists independently of Steam:

```sh
ls /sys/class/input/event*/device/name | xargs -I{} sh -c 'cat {}' | grep -i translated
```

If it is there, Steam is the problem — restart Steam after installing the mapping
file.

**The six extras appear as one button, or not at all**
The mapping file is not being read. Verify with:

```sh
SDL_GAMECONTROLLERCONFIG="$(grep -v '^#' driver/contrib/gamecontrollerdb.txt | grep -v '^$')" \
  cargo run --release --example sdl_probe
```

`bound` should list `misc1:b15`, `touchpad:b16`, `paddle1..4:b17..b20`.

**Input feels laggy**
Measure rather than guess: `python3 contrib/latency-test.py 25`. The translator
itself adds ~0.03 ms. Occasional 100–200 ms stalls appear in the *controller's own*
reporting rhythm over Bluetooth; they did not affect measured button latency.

---

## How it works

**Reading.** The daemon opens the controller's hidraw node and parses the 21-byte
DInput report. Byte 9 high nibble carries A/B/X/Y; byte 9 low nibble is a D-pad
hat (so it is *not* where M1–M4 live); byte 10 carries shoulders and menu buttons;
byte 11 carries C, Z and M1–M4 as independent bits; byte 12 bit 7 is Home; bytes
1–4 are the sticks. The full map with evidence is in
[docs/PROTOCOL.md](docs/PROTOCOL.md).

The report is **21 bytes while the descriptor declares 15**. That is not a bug in
the controller: the firmware builds a 20-byte payload, writes 14, and sends all
20. Bytes 15–20 are uninitialised padding and are deliberately ignored.

**Writing.** Rumble is a 4-byte frame to the Nordic UART RX characteristic:

```
11 <left> <right> <duration_ms>
```

Three details matter, all confirmed against firmware 6.7.5.1:

- The dispatcher requires a payload length of **exactly 4**. Any other length is
  dropped silently — which is why earlier 5-, 12- and 20-byte attempts did
  nothing at all.
- **Byte 3 is the duration**, bytes 1–2 the levels. Getting these the wrong way
  round gives one very short buzz instead of sustained vibration.
- There is **no start/stop latch**. The duration is a hard firmware timeout, so
  the daemon re-sends every 100 ms to sustain.

Levels clamp at **250**, not 255.

**Latency.** The read loop blocks on `poll(2)` rather than sleeping on a timer.
Sleeping added its full interval to every input — measured at exactly 4 ms p95
against a 0.16 ms transport floor. With event-driven wakeups the translator adds
**0.03 ms** median, about 250× below the controller's own 7.6 ms report interval.
Idle CPU cost is 0.6% of one core.

---

## Known limitations

- **No native gyro.** The Bluetooth DInput stream carries no motion data at all —
  verified by shaking the controller and observing bytes 1–4 stay put (they are
  the sticks). Gyro exists only in a vendor mode switched by `05 EE 01`, which was
  never confirmed over Bluetooth and whose exploration once reset the controller's
  Bluetooth bond. Not attempted further, by choice.
- **Steam recognition of the six extras is unverified.** The mapping file is
  built from measured indices and works with local SDL; Steam bundles its own SDL,
  so it needs confirming there.
- **No LED control.** The chunked LED path (`0x33`) ends in a flash erase/program
  and can commit partial state if a sequence is abandoned, so it is deliberately
  unused. The lighter mode command (`0x13`) was rejected by the device over
  Bluetooth with ATT error `0x0e` and dropped the connection.
- **Suppression hides events, not the device entry.** The physical node still
  exists in `/dev/input`, so a device list can show both controllers — but only
  the virtual one produces input. Hiding the entry entirely would require
  unbinding `hid-generic`, which needs manual repair after a crash.
- **Bluetooth only for the supported path.** USB and the 2.4 GHz dongle use
  different rumble transports; the daemon detects and falls back, but they are not
  the target use case.

---

## Development

```sh
cd driver
cargo test                                        # Rust suite
cargo clippy --lib --bins --tests                 # lint the shipped code
cargo run --release --example signal_probe        # verify pad routing

cd .. && python -m pytest tests/ -q               # offline Python suite
```

The Python tests run offline against recorded fixtures and need no hardware:

```sh
pip install pytest
python -m pytest tests/ -q
```

Diagnostic probes live in `driver/examples/`. They are the instruments that
produced the measurements in this repository, kept so the claims can be re-checked
rather than taken on trust:

| Example | Purpose | Root |
| --- | --- | --- |
| `signal_probe` | Verify every button and axis against SDL | no |
| `sdl_probe` | Show SDL's binding for the virtual pad | no |
| `raw_buttons` | Print the joystick index of each physical press | no |
| `uinput_ff_probe` | Confirm `FF_RUMBLE` is declared | no |
| `ff_handshake_probe` | End-to-end host effect-upload handshake | no |
| `ff_probe` | UHID descriptor variants vs kernel `capabilities/ff` | yes |
| `uhid_caps` | UHID descriptor → kernel capability bitmaps | yes |

> `ff_probe` and `uhid_caps` need root because `/dev/uhid` is mode `0600`. They
> document why the virtual pad uses `uinput` rather than `uhid`: all three UHID
> descriptor variants produced `capabilities/ff = 0`, so Steam could not have
> rumbled the pad at all.

---

## Documentation

This project keeps its evidence separate from its claims. Every hardware fact is
recorded with the revision and artifact that produced it.

| Document | Contents |
| --- | --- |
| [docs/PROTOCOL.md](docs/PROTOCOL.md) | Every byte, frame and command, with evidence class |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Data path and design decisions |
| [docs/TESTING.md](docs/TESTING.md) | Automated checks and hardware acceptance matrix |
| [docs/evidence/](docs/evidence/) | Individual evidence records |
| [docs/RESEARCH.md](docs/RESEARCH.md) | Upstream and prior-art findings |
| [PRD.md](PRD.md) | Requirements and acceptance criteria |

Worth reading if you want the interesting parts:

- [bluetooth-rumble-solved.md](docs/evidence/bluetooth-rumble-solved.md) — why
  every earlier attempt failed, and the 4-byte frame that worked
- [vader3pro-firmware-disassembly.md](docs/evidence/vader3pro-firmware-disassembly.md)
  — how the report layout and rumble path were proven from the firmware itself
- [virtual-pad-signal-routing.md](docs/evidence/virtual-pad-signal-routing.md) —
  three bugs found by measuring, and the latency numbers

---

## Credits and prior art

The controller's own firmware and FlyDigi's Space Station application were
disassembled for protocol research. No code from either is included here; only
observed byte layouts and command semantics are described.

Related work that informed this project: SDL's `SDL_hidapi_flydigi.c`,
`ahungry/vader3`, and the Linux `xpad`/`hid-playstation` drivers. This project
shares no code with them.

Not affiliated with FlyDigi or Valve. "Flydigi", "Vader" and "Steam" are the
property of their respective owners and are used here only to describe what this
software interoperates with.

---

## License

Dual-licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. This matches `license = "MIT OR Apache-2.0"` in
`driver/Cargo.toml` and is the usual Rust convention.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project by you, as defined in the Apache-2.0 license, shall
be dual-licensed as above, without any additional terms or conditions.

### Scope

The licenses above cover the source code and documentation in this repository.
They do **not** grant any rights to FlyDigi's Space Station application or
firmware, which are proprietary and were used only for protocol research. No code
from those artifacts is included here; only observed byte layouts and command
semantics are described, which is why [docs/PROTOCOL.md](docs/PROTOCOL.md)
records evidence rather than shipping extracted material.
