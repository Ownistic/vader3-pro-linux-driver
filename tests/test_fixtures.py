import hashlib
import json
from pathlib import Path
import unittest


class DescriptorFixtureTests(unittest.TestCase):
    def test_recorded_descriptor_identity(self):
        root = Path(__file__).parent / "fixtures"
        for name in ("vader3pro-bluetooth-xbox-descriptor.json", "vader3pro-bluetooth-dinput-descriptor.json"):
            with self.subTest(name=name):
                fixture = json.loads((root / name).read_text())
                descriptor = bytes.fromhex(fixture["hex"])
                self.assertEqual(len(descriptor), fixture["bytes"])
                self.assertEqual(hashlib.sha256(descriptor).hexdigest(), fixture["sha256"])
                self.assertEqual(fixture["kind"], "recorded-hid-report-descriptor-not-input-reports")


class MKeyOrderFixtureTests(unittest.TestCase):
    """The M1-M4 bit order, owner-confirmed by ordered single-key presses.

    This replaces a table-derived assignment: a coordinated multi-key run could
    not confirm press order and once reported M2 where M4 was expected.
    """

    def setUp(self):
        path = Path(__file__).parent / "fixtures" / "vader3pro-bluetooth-dinput-mkeys.json"
        self.fixture = json.loads(path.read_text())
        self.packets = [bytes.fromhex(row["hex"]) for row in self.fixture["reports"]]

    def test_fixture_shape(self):
        self.assertEqual(self.fixture["kind"], "recorded-passive-hidraw-input")
        self.assertEqual(self.fixture["report_bytes"], 21)
        self.assertTrue(self.packets)
        for packet in self.packets:
            self.assertEqual(len(packet), 21)
            self.assertEqual(packet[0], 1)

    def test_declared_mapping_matches_the_specified_order(self):
        self.assertEqual(
            self.fixture["observed_byte11"],
            {"M1": "04", "M2": "08", "M3": "10", "M4": "20"},
        )

    def test_each_press_sets_exactly_one_m_bit(self):
        for packet in self.packets:
            mask = packet[11] & 0x3C
            if mask:
                self.assertEqual(mask & (mask - 1), 0, f"{mask:#04x} is not one bit")

    def test_key_order_follows_the_coordinated_sequence(self):
        order = []
        for packet in self.packets:
            mask = packet[11] & 0x3C
            if mask and (not order or order[-1] != mask):
                order.append(mask)
        distinct = []
        for mask in order:
            if mask not in distinct:
                distinct.append(mask)
        self.assertEqual(distinct, [0x04, 0x08, 0x10, 0x20])

    def test_m_presses_do_not_set_standard_buttons(self):
        for packet in self.packets:
            if packet[11] & 0x3C:
                self.assertEqual(packet[9] & 0xF0, 0, "M press set a face button")
                self.assertEqual(packet[10], 0, "M press set a shoulder/menu button")
                self.assertEqual(packet[9] & 0x0F, 0, "M press set the D-pad hat")


class InputFixtureTests(unittest.TestCase):
    def setUp(self):
        path = Path(__file__).parent / "fixtures" / "vader3pro-bluetooth-dinput-abcz.json"
        self.fixture = json.loads(path.read_text())
        self.packets = [bytes.fromhex(row["hex"]) for row in self.fixture["reports"]]

    def test_recorded_packet_shape_and_order(self):
        self.assertEqual(self.fixture["kind"], "recorded-passive-hidraw-input")
        self.assertEqual(len(self.packets), 14)
        self.assertEqual(self.fixture["report_bytes"], 21)
        self.assertEqual(self.fixture["declared_report_bytes_including_id"], 15)
        times = [row["elapsed_ns"] for row in self.fixture["reports"]]
        self.assertTrue(all(a < b for a, b in zip(times, times[1:])))
        for packet in self.packets:
            self.assertEqual(len(packet), 21)
            self.assertEqual(packet[0], 1)
            self.assertEqual(packet[15:], bytes(6))

    def test_observed_abcz_press_release_sequence(self):
        # These assertions preserve observed fixture facts, not a runtime parser
        # or proof of simultaneous presses, physical remapping, or Steam support.
        fields = (("A", 9, 0x10), ("B", 9, 0x20), ("C", 11, 0x01), ("Z", 11, 0x02))
        states = []
        for packet in self.packets:
            state = tuple(name for name, offset, mask in fields if packet[offset] & mask)
            if not states or state != states[-1]:
                states.append(state)
        self.assertEqual(states, [(), ("A",), (), ("B",), (), ("C",), (), ("Z",), ()])
        self.assertEqual(self.fixture["expected_press_order"], [state[0] for state in states if state])


if __name__ == "__main__":
    unittest.main()
