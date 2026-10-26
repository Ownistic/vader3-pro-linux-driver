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
