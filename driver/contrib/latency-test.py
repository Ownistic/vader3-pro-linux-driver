#!/usr/bin/env python3
"""Measure report rate and end-to-end latency through the daemon.

Three numbers decide whether this is usable:

1. How often the controller actually sends a report (the ceiling).
2. How often the daemon produces a virtual-pad event (what a game sees).
3. How long a change takes to appear on the virtual pad (what a player feels).

Latency is measured by correlating a button edge: the moment the A button appears
in a physical report is compared with the moment the virtual pad emits it. That is
the only signal present in both streams with an unambiguous identity.

Read-only on both nodes.
"""
import os
import select
import signal
import statistics
import struct
import subprocess
import sys
import time

HIDRAW = None
for _entry in sorted(os.listdir('/sys/class/hidraw')):
    try:
        _uevent = open(f'/sys/class/hidraw/{_entry}/device/uevent').read()
    except OSError:
        continue
    if 'VADER' in _uevent.upper():
        HIDRAW = f'/dev/{_entry}'
        break

VIRTUAL = None
for _entry in sorted(os.listdir('/sys/class/input')):
    if not _entry.startswith('event'):
        continue
    try:
        _name = open(f'/sys/class/input/{_entry}/device/name').read().strip()
    except OSError:
        continue
    if 'translated' in _name:
        VIRTUAL = f'/dev/input/{_entry}'
        break


def percentile(values, pct):
    if not values:
        return None
    ordered = sorted(values)
    index = min(len(ordered) - 1, int(len(ordered) * pct / 100))
    return ordered[index]


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


def start_daemon():
    """Start the daemon if it is not already running, and return the handle.

    Self-contained on purpose: an earlier version of this script assumed a daemon
    was already up and reported "virtual pad: None" when it was not, which reads
    as a broken device rather than a missing prerequisite.
    """
    existing = subprocess.run(
        ['pgrep', '-f', 'vader3p-steam'], capture_output=True, text=True
    ).stdout.strip()
    if existing:
        print('daemon     : already running, using it')
        return None

    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    binary = os.path.join(root, 'target/release/vader3p-steam')
    if not os.path.exists(binary):
        print(f'missing {binary} — run: cargo build --release')
        return False

    address = ''
    out = subprocess.run(['bluetoothctl', 'devices'], capture_output=True, text=True).stdout
    for line in out.splitlines():
        if 'vader' in line.lower():
            address = line.split()[1]
            break

    cfg = '/tmp/opencode/vader3p-latency.toml'
    with open(cfg, 'w') as fh:
        fh.write('log_level = "warn"\n')
        fh.write('rumble = true\n')
        fh.write('suppress_physical = true\n')
        if address:
            fh.write(f'bluetooth_address = "{address}"\n')

    proc = subprocess.Popen(
        [binary, '-c', cfg], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
    )
    print('daemon     : started')
    for _ in range(50):
        time.sleep(0.1)
        if find_virtual():
            break
    return proc


def main():
    duration = float(sys.argv[1]) if len(sys.argv) > 1 else 20.0
    if not HIDRAW:
        print('controller not found on hidraw')
        print('connect it in Bluetooth DInput mode, then run this again')
        return 1

    daemon = start_daemon()
    if daemon is False:
        return 1

    virtual = find_virtual()
    if not virtual:
        print('virtual pad did not appear')
        if daemon:
            daemon.terminate()
        return 1
    VIRTUAL = virtual

    print(f'controller : {HIDRAW}')
    print(f'virtual pad: {VIRTUAL}')
    print()
    print('=' * 64)
    print('  Move a stick continuously, and tap A every second or so.')
    print(f'  Measuring for {duration:.0f} seconds.')
    print('=' * 64)
    print()

    phys_fd = os.open(HIDRAW, os.O_RDONLY | os.O_NONBLOCK)
    virt_fd = os.open(VIRTUAL, os.O_RDONLY | os.O_NONBLOCK)
    _daemon_to_clean = daemon

    start = time.monotonic()
    phys_times = []
    virt_times = []
    # (time, pressed?) for the A button on each side.
    phys_a_edges = []
    virt_a_edges = []
    pending_a = None
    latencies = []
    prev_a_phys = 0
    prev_a_virt = 0
    axis_values = []

    try:
        while time.monotonic() - start < duration:
            ready, _, _ = select.select([phys_fd, virt_fd], [], [], 0.05)
            for fd in ready:
                if fd == phys_fd:
                    try:
                        frame = os.read(phys_fd, 256)
                    except BlockingIOError:
                        continue
                    if not frame:
                        continue
                    now = time.monotonic()
                    phys_times.append(now)
                    if len(frame) > 9:
                        a = 1 if frame[9] & 0x10 else 0
                        if a != prev_a_phys:
                            prev_a_phys = a
                            phys_a_edges.append((now, a))
                            if a == 1:
                                pending_a = now
                        # Left stick X as reported by the device.
                        if len(frame) > 1:
                            axis_values.append(frame[1])
                else:
                    try:
                        data = os.read(virt_fd, 24 * 256)
                    except BlockingIOError:
                        continue
                    for off in range(0, len(data) - 23, 24):
                        _s, _u, kind, code, value = struct.unpack_from(
                            '<qqHHi', data, off
                        )
                        now = time.monotonic()
                        virt_times.append(now)
                        if kind == 1 and code == 0x130:      # BTN_SOUTH (A)
                            if value != prev_a_virt:
                                prev_a_virt = value
                                virt_a_edges.append((now, value))
                                if value == 1 and pending_a is not None:
                                    latencies.append((now - pending_a) * 1000.0)
                                    pending_a = None
    finally:
        os.close(phys_fd)
        os.close(virt_fd)
        if daemon:
            daemon.send_signal(signal.SIGTERM)
            try:
                daemon.wait(timeout=3)
            except subprocess.TimeoutExpired:
                daemon.kill()

    elapsed = time.monotonic() - start

    print('--- throughput ---')
    print(f'physical reports : {len(phys_times):5d}  ({len(phys_times) / elapsed:6.1f} /s)')
    print(f'virtual events   : {len(virt_times):5d}  ({len(virt_times) / elapsed:6.1f} /s)')

    if len(phys_times) > 1:
        gaps = [
            (phys_times[i + 1] - phys_times[i]) * 1000.0
            for i in range(len(phys_times) - 1)
        ]
        print()
        print('--- physical report interval ---')
        print(f'median : {statistics.median(gaps):6.2f} ms')
        print(f'p95    : {percentile(gaps, 95):6.2f} ms')
        print(f'max    : {max(gaps):6.2f} ms')

    if axis_values:
        print()
        print('--- left stick X as the device reports it ---')
        print(f'samples: {len(axis_values)}, distinct values: {len(set(axis_values))}')
        print(f'range  : {min(axis_values)}..{max(axis_values)}')

    print()
    print('--- button latency (physical report -> virtual pad event) ---')
    print(f'A presses paired: {len(latencies)}')
    if latencies:
        print(f'median : {statistics.median(latencies):6.2f} ms')
        print(f'p95    : {percentile(latencies, 95):6.2f} ms')
        print(f'max    : {max(latencies):6.2f} ms')
        print(f'min    : {min(latencies):6.2f} ms')
    else:
        print('no paired presses — tap A a few times next run')

    print()
    print('note: a Bluetooth DInput controller is edge-triggered, so the report')
    print('rate follows how much you move, not a fixed polling clock.')
    return 0


if __name__ == '__main__':
    sys.exit(main())
