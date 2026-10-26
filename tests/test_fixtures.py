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


if __name__ == "__main__":
    unittest.main()
