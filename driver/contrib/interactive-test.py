#!/usr/bin/env python3
"""Interactive check: press anything on the controller, see it named.

Starts the daemon itself, then watches both the physical controller and the
virtual pad at once. That combination is deliberate: if the physical side shows a
press but the virtual side does not, the daemon is at fault. If neither shows
anything, the controller is asleep or disconnected — which is a different problem
and must not be blamed on the translator.

Read-only on both nodes. Safe: it never writes to the vendor or OTA
characteristic and never sends 0x13 or 0x33. The daemon it starts does forward
rumble, but nothing here requests any.
"""
import os
import select
import signal
import struct
import subprocess
import sys
import time

BUTTON_NAMES = {
    0x130: 'A', 0x131: 'B', 0x133: 'X', 0x134: 'Y',
    0x136: 'LB', 0x137: 'RB',
    0x13a: 'Select', 0x13b: 'Start', 0x13c: 'Home',
    0x13d: 'L3', 0x13e: 'R3',
    0x220: 'D-PAD UP', 0x221: 'D-PAD DOWN',
    0x222: 'D-PAD LEFT', 0x223: 'D-PAD RIGHT',
    0x2c0: 'C', 0x2c1: 'Z',
    0x2c2: 'M1', 0x2c3: 'M2', 0x2c4: 'M3', 0x2c5: 'M4',
}

ABS_NAMES = {
    0x00: 'LEFT STICK X', 0x01: 'LEFT STICK Y',
    0x02: 'RIGHT STICK X', 0x03: 'RIGHT STICK Y',
    0x04: 'LEFT TRIGGER', 0x05: 'RIGHT TRIGGER',
    0x10: 'D-PAD X', 0x11: 'D-PAD Y',
}

# Which physical report bytes carry which buttons, from docs/PROTOCOL.md.
PHYS_BUTTONS = [
    (9, 0x10, 'A'), (9, 0x20, 'B'), (9, 0x40, 'X'), (9, 0x80, 'Y'),
    (10, 0x01, 'LB'), (10, 0x02, 'RB'),
    (10, 0x10, 'Select'), (10, 0x20, 'Start'),
    (10, 0x40, 'L3'), (10, 0x80, 'R3'),
    (12, 0x80, 'Home'),
    (11, 0x01, 'C'), (11, 0x02, 'Z'),
    (11, 0x04, 'M1'), (11, 0x08, 'M2'),
    (11, 0x10, 'M3'), (11, 0x20, 'M4'),
]


def find_hidraw():
    base = '/sys/class/hidraw'
    for entry in sorted(os.listdir(base)):
        try:
            uevent = open(f'{base}/{entry}/device/uevent').read()
        except OSError:
            continue
        if 'VADER' in uevent.upper():
            return f'/dev/{entry}'
    return None


def find_virtual():
    for entry in sorted(os.listdir('/sys/class/input')):
        if not entry.startswith('event'):
            continue
        try:
            name = open(f'/sys/class/input/{entry}/device/name').read().strip()
        except OSError:
            continue
        if 'translated' in name:
            return f'/dev/input/{entry}'
    return None


def decode_physical(frame):
    """Return the set of button names held in one physical report."""
    held = set()
    for byte_index, mask, name in PHYS_BUTTONS:
        if byte_index < len(frame) and frame[byte_index] & mask:
            held.add(name)
    return held


def main():
    duration = int(sys.argv[1]) if len(sys.argv) > 1 else 45

    hidraw = find_hidraw()
    if not hidraw:
        print('Controller not found on hidraw.')
        print('Connect it in Bluetooth DInput mode, then run this again.')
        return 1
    print(f'controller : {hidraw}')

    # Start the daemon so the virtual pad exists. If one is already running, use
    # it instead of fighting over /dev/uinput.
    existing = subprocess.run(
        ['pgrep', '-f', 'vader3p-steam'], capture_output=True, text=True
    ).stdout.strip()
    daemon = None
    if existing:
        print('daemon     : already running, using it')
    else:
        root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        binary = os.path.join(root, 'target/release/vader3p-steam')
        if not os.path.exists(binary):
            print(f'missing {binary} — run: cargo build --release')
            return 1
        cfg = '/tmp/opencode/vader3p-interactive.toml'
        address = ''
        out = subprocess.run(
            ['bluetoothctl', 'devices'], capture_output=True, text=True
        ).stdout
        for line in out.splitlines():
            if 'vader' in line.lower():
                address = line.split()[1]
                break
        with open(cfg, 'w') as fh:
            fh.write('log_level = "warn"\n')
            fh.write('rumble = true\n')
            fh.write('suppress_physical = true\n')
            if address:
                fh.write(f'bluetooth_address = "{address}"\n')
        daemon = subprocess.Popen(
            [binary, '-c', cfg],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        print('daemon     : started')
        time.sleep(2.0)

    virtual = find_virtual()
    if not virtual:
        print('Virtual pad did not appear.')
        if daemon:
            daemon.terminate()
        return 1
    print(f'virtual pad: {virtual}')

    phys_fd = os.open(hidraw, os.O_RDONLY | os.O_NONBLOCK)
    virt_fd = os.open(virtual, os.O_RDONLY | os.O_NONBLOCK)

    print()
    print('=' * 62)
    print(f'  Press anything for the next {duration} seconds.')
    print('  Buttons, sticks, triggers, D-pad — all of it is reported.')
    print('=' * 62)
    print()

    start = time.time()
    phys_held = set()
    virt_held = set()
    axis_last = {}
    seen_controls = set()
    phys_reports = 0
    virt_events = 0

    try:
        while time.time() - start < duration:
            remaining = duration - (time.time() - start)
            ready, _, _ = select.select([phys_fd, virt_fd], [], [], 0.2)
            elapsed = time.time() - start

            for fd in ready:
                if fd == phys_fd:
                    try:
                        frame = os.read(phys_fd, 256)
                    except BlockingIOError:
                        continue
                    if not frame:
                        continue
                    phys_reports += 1
                    now = decode_physical(frame)
                    for name in sorted(now - phys_held):
                        print(f'{elapsed:5.1f}s  physical  {name} down')
                    for name in sorted(phys_held - now):
                        print(f'{elapsed:5.1f}s  physical  {name} up')
                    phys_held = now
                    seen_controls |= now
                else:
                    try:
                        data = os.read(virt_fd, 24 * 128)
                    except BlockingIOError:
                        continue
                    for off in range(0, len(data) - 23, 24):
                        _s, _u, kind, code, value = struct.unpack_from(
                            '<qqHHi', data, off
                        )
                        virt_events += 1
                        if kind == 1:  # EV_KEY
                            name = BUTTON_NAMES.get(code, f'key {code:#x}')
                            if value == 1:
                                print(f'{elapsed:5.1f}s  VIRTUAL   {name} down')
                                virt_held.add(name)
                                seen_controls.add(name)
                            elif value == 0:
                                print(f'{elapsed:5.1f}s  VIRTUAL   {name} up')
                                virt_held.discard(name)
                        elif kind == 3:  # EV_ABS
                            name = ABS_NAMES.get(code, f'abs {code:#x}')
                            # Compare against the last *reported* value, not the
                            # previous sample. A stick moved by hand changes a
                            # few counts per report, so a per-sample threshold
                            # never fires and stick motion goes unreported.
                            # Accumulating deviation is what makes it visible.
                            last_reported = axis_last.get(code)
                            if last_reported is None:
                                # First sample establishes the baseline silently;
                                # it is usually the resting centre.
                                axis_last[code] = value
                                continue
                            delta = value - last_reported
                            # Thresholds are a fraction of the 0..255 range so
                            # the output stays readable during a fast sweep
                            # without hiding a deliberate move. A stick moved
                            # slowly changes only a few counts per report, so
                            # the comparison must be cumulative — see
                            # tests/test_interactive_probe.py.
                            threshold = 32 if name.endswith('TRIGGER') else 24
                            if abs(delta) <= threshold:
                                continue
                            axis_last[code] = value
                            direction = '+' if delta > 0 else '-'
                            print(f'{elapsed:5.1f}s  VIRTUAL   {name} {direction} (now {value})')
    finally:
        try:
            os.close(phys_fd)
            os.close(virt_fd)
        except OSError:
            pass
        if daemon:
            daemon.send_signal(signal.SIGTERM)
            try:
                daemon.wait(timeout=3)
            except subprocess.TimeoutExpired:
                daemon.kill()

    print()
    print('=' * 62)
    print(f'  physical reports: {phys_reports}')
    print(f'  virtual events  : {virt_events}')
    print()
    if phys_reports == 0 and virt_events == 0:
        print('  NOTHING ARRIVED. The controller was asleep or disconnected.')
        print('  Press a button or two, then run this again.')
    elif phys_reports > 0 and virt_events == 0:
        print('  CONTROLLER REACHED THE DAEMON BUT THE VIRTUAL PAD STAYED SILENT.')
        print('  That is a translator bug, not a controller problem.')
    else:
        print('  Both sides active — the translator is forwarding.')
        expected = set(BUTTON_NAMES.values()) - {'D-PAD X', 'D-PAD Y'}
        buttons_seen = {c for c in seen_controls if not c.startswith(('key ', 'abs '))}
        missing = {'A', 'B', 'X', 'Y', 'LB', 'RB', 'Select', 'Start', 'Home',
                   'L3', 'R3', 'C', 'Z', 'M1', 'M2', 'M3', 'M4'} - buttons_seen
        if missing:
            print(f'  Not pressed during this run: {", ".join(sorted(missing))}')
        else:
            print('  Every named button was exercised.')
    print('=' * 62)
    return 0


if __name__ == '__main__':
    sys.exit(main())
