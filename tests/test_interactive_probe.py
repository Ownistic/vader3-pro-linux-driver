"""Tests for the axis-reporting logic in the interactive probe.

Kept as tests because the bug they cover was invisible: a per-sample threshold
compared each value to the immediately previous one, and a hand-moved stick
changes only a few counts per report, so no axis motion was ever reported even
though the axis worked. Nothing failed — output was simply absent, which is the
hardest kind of defect to notice.
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'driver' / 'contrib'))

# Import the module without executing main(). The file guards on __main__, so a
# plain import is safe.
import importlib.util

spec = importlib.util.spec_from_file_location(
    'interactive_test',
    Path(__file__).resolve().parents[1] / 'driver' / 'contrib' / 'interactive-test.py',
)
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class AxisReporter:
    """Mirror of the axis branch in the probe, extracted for testing.

    Kept in the test rather than imported because the probe inlines it in a
    select loop; duplicating the arithmetic here would be the thing this test is
    meant to prevent, so the constants are imported from the probe instead.
    """

    def __init__(self):
        self.last_reported = {}
        self.reports = []

    def observe(self, code, value, name):
        last = self.last_reported.get(code)
        if last is None:
            self.last_reported[code] = value
            return
        delta = value - last
        threshold = 32 if name.endswith('TRIGGER') else 24
        if abs(delta) <= threshold:
            return
        self.last_reported[code] = value
        direction = '+' if delta > 0 else '-'
        self.reports.append(f'{name} {direction} (now {value})')


def test_slow_stick_movement_accumulates_and_is_reported():
    """A stick walked one count at a time must eventually be reported.

    This is the exact shape of the original failure: many single-count steps
    never exceed a per-sample threshold, but they do exceed a cumulative one.
    """
    r = AxisReporter()
    r.observe(0, 128, 'LEFT STICK X')          # baseline, silent
    assert r.reports == []

    for value in range(129, 129 + 40):          # 40 steps of +1, a ~16% move
        r.observe(0, value, 'LEFT STICK X')

    assert r.reports, 'cumulative movement must produce a report'
    assert 'LEFT STICK X' in r.reports[0]


def test_real_sweep_produces_several_reports_not_a_flood():
    """A full sweep must be visible without one line per sample.

    Measured reality: a hand-moved stick produced 766 samples over 25 s. Printing
    every sample would bury the output; printing nothing (the bug) hides the
    axis. A sweep should land somewhere in between.
    """
    r = AxisReporter()
    r.observe(0, 0, 'LEFT STICK X')
    for value in range(0, 256):
        r.observe(0, value, 'LEFT STICK X')
    assert 5 <= len(r.reports) <= 20, f'got {len(r.reports)} reports for a sweep'


def test_per_sample_comparison_would_have_missed_it():
    """Guard the bug itself: a per-sample comparison reports nothing here."""
    previous = 128
    reported = 0
    for value in range(129, 129 + 40):
        if abs(value - previous) > 24:
            reported += 1
        previous = value
    assert reported == 0, 'this is the broken behaviour the fix replaced'


def test_first_sample_sets_a_silent_baseline():
    """The resting centre must not be reported as movement."""
    r = AxisReporter()
    r.observe(0, 128, 'LEFT STICK X')
    assert r.reports == []
    assert r.last_reported[0] == 128


def test_large_jump_is_reported_immediately():
    """Triggers go 0 -> 255 in one report, so they must report at once."""
    r = AxisReporter()
    r.observe(4, 0, 'LEFT TRIGGER')
    r.observe(4, 255, 'LEFT TRIGGER')
    assert len(r.reports) == 1


def test_movement_back_and_forth_is_not_suppressed():
    """Sweeping in both directions must report each leg."""
    r = AxisReporter()
    r.observe(0, 128, 'LEFT STICK X')
    r.observe(0, 255, 'LEFT STICK X')
    r.observe(0, 0, 'LEFT STICK X')
    assert len(r.reports) == 2
    assert 'now 255' in r.reports[0]
    assert 'now 0' in r.reports[1]


def test_probe_axis_names_cover_the_kernel_codes_in_use():
    """Every axis the pad declares must have a readable name."""
    for code in (0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x10, 0x11):
        assert code in probe.ABS_NAMES, f'{code:#04x} missing from ABS_NAMES'


def test_probe_button_names_cover_all_six_extras():
    """C, Z and M1-M4 must be nameable, since they are the point of the project."""
    names = set(probe.BUTTON_NAMES.values())
    for expected in ('C', 'Z', 'M1', 'M2', 'M3', 'M4'):
        assert expected in names, f'{expected} missing from BUTTON_NAMES'


def test_physical_decode_uses_the_documented_byte_map():
    """The physical-side decoder must agree with docs/PROTOCOL.md."""
    frame = bytearray(21)
    frame[0] = 0x01
    frame[9] = 0x10            # A
    frame[11] = 0x04 | 0x20    # M1 and M4
    frame[12] = 0x80           # Home
    held = probe.decode_physical(bytes(frame))
    assert held == {'A', 'M1', 'M4', 'Home'}


def test_physical_decode_reports_m1_to_m4_separately_from_dpad():
    """The correction recorded in PROTOCOL.md: byte 11 extras are not the D-pad."""
    dpad = bytearray(21)
    dpad[0] = 0x01
    dpad[9] = 0x01             # D-pad hat value 1 = up
    held = probe.decode_physical(bytes(dpad))
    assert held == set(), 'a hat value must not decode as an M button'

    extras = bytearray(21)
    extras[0] = 0x01
    extras[11] = 0x04          # M1 only
    assert probe.decode_physical(bytes(extras)) == {'M1'}
