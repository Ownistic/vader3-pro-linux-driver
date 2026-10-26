import contextlib
import hashlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from tools import inventory


class InventoryTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def device(self, name="hidraw0", identity="0005:0000D7D7:00000041", descriptor=b"\x05\x01\x09\x05"):
        device = self.root / name / "device"
        device.mkdir(parents=True)
        (device / "uevent").write_text(
            f"HID_ID={identity}\nHID_NAME=Private controller\n"
            "HID_UNIQ=AA:BB:CC:DD:EE:FF\nHID_PHYS=/private/path\n"
        )
        (device / "report_descriptor").write_bytes(descriptor)
        return device

    def test_bluetooth_identity_and_descriptor_digest(self):
        self.device()
        result = inventory.inventory(self.root)
        self.assertTrue(result["complete"])
        device = result["devices"][0]
        self.assertEqual((device["bus"], device["vendor"], device["product"]), ("0005", "d7d7", "0041"))
        self.assertEqual(device["descriptor"]["sha256"], hashlib.sha256(b"\x05\x01\x09\x05").hexdigest())
        self.assertNotIn("hex", device["descriptor"])
        self.assertIn("unknown", device["bluetooth_transport"])
        self.assertFalse(result["hardware_support_verified"])

    def test_omits_names_addresses_physical_and_local_paths(self):
        self.device()
        text = json.dumps(inventory.inventory(self.root))
        for private in ("AA:BB", "Private controller", "/private/path", str(self.root)):
            self.assertNotIn(private, text)

    def test_unrelated_devices_are_not_published_or_read_for_descriptors(self):
        self.device(identity="0003:000017EF:00000001")
        with mock.patch.object(inventory, "read_bounded", wraps=inventory.read_bounded) as read:
            result = inventory.inventory(self.root)
        self.assertEqual(result["devices"], [])
        self.assertEqual(read.call_count, 1)
        self.assertTrue(result["complete"])

    def test_exact_product_filter_and_multiple_nodes(self):
        self.device()
        self.device("hidraw1", "0003:0000D7D7:00000002")
        self.assertEqual(len(inventory.inventory(self.root)["devices"]), 2)
        self.assertEqual(len(inventory.inventory(self.root, product=0x41)["devices"]), 1)

    def test_descriptor_bytes_are_opt_in(self):
        self.device()
        result = inventory.inventory(self.root, include_descriptor=True)
        self.assertEqual(result["devices"][0]["descriptor"]["hex"], "05010905")

    def test_missing_root_is_error_not_no_hardware(self):
        result = inventory.inventory(self.root / "missing")
        self.assertFalse(result["complete"])
        self.assertEqual(result["errors"][0]["error"], "FileNotFoundError")

    def test_missing_identity_and_descriptor(self):
        path = self.device()
        (path / "uevent").unlink()
        result = inventory.inventory(self.root)
        self.assertFalse(result["complete"])
        self.assertEqual(result["devices"], [])
        (path / "uevent").write_text("HID_ID=0005:0000D7D7:00000041")
        (path / "report_descriptor").unlink()
        result = inventory.inventory(self.root)
        self.assertIsNone(result["devices"][0]["descriptor"])
        self.assertFalse(result["complete"])

    def test_malformed_duplicate_and_out_of_range_identity(self):
        for data in (b"HID_ID=bad", b"HID_ID=0005:d7d7:0041\nHID_ID=0005:d7d7:0041", b"HID_ID=0005:FFFFD7D7:0041", b"HID_ID=\xff", b"HID_NAME=only"):
            with self.subTest(data=data), self.assertRaises(ValueError):
                inventory.parse_identity(data)

    def test_size_limits_and_empty_descriptor(self):
        device = self.device()
        for descriptor in (b"", b"x" * (inventory.MAX_DESCRIPTOR + 1)):
            (device / "report_descriptor").write_bytes(descriptor)
            self.assertFalse(inventory.inventory(self.root)["complete"])
        (device / "uevent").write_bytes(b"x" * (inventory.MAX_METADATA + 1))
        self.assertFalse(inventory.inventory(self.root)["complete"])

    def test_permissions_and_disappearance_report_only_error_class(self):
        self.device()
        for error in (PermissionError("private path"), FileNotFoundError("private path")):
            with self.subTest(error=type(error).__name__):
                with mock.patch.object(inventory, "read_bounded", side_effect=error):
                    result = inventory.inventory(self.root)
                self.assertFalse(result["complete"])
                self.assertNotIn("private path", json.dumps(result))

    def test_sysfs_device_symlink_is_supported_without_publishing_target(self):
        target = self.device()
        linked = self.root / "hidraw2"
        linked.mkdir()
        (linked / "device").symlink_to(target, target_is_directory=True)
        result = inventory.inventory(self.root)
        self.assertEqual(len(result["devices"]), 2)
        self.assertNotIn(str(target), json.dumps(result))

    def test_non_nodes_ignored_and_empty_inventory_is_not_support_proof(self):
        (self.root / "unrelated").mkdir()
        result = inventory.inventory(self.root)
        self.assertTrue(result["complete"])
        self.assertEqual(result["devices"], [])
        self.assertFalse(result["hardware_support_verified"])

    def test_cli_json_and_failure_exit_status(self):
        self.device()
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            code = inventory.main(["--sysfs-root", str(self.root), "--descriptor"])
        self.assertEqual(code, 0)
        self.assertEqual(len(json.loads(output.getvalue())["devices"]), 1)
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(inventory.main(["--sysfs-root", str(self.root / "missing")]), 2)

    def test_cli_invalid_identifiers_rejected(self):
        for value in ("not-hex", "-1", "10000"):
            with self.subTest(value=value), contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit) as raised:
                    inventory.main([f"--vid={value}"])
                self.assertEqual(raised.exception.code, 2)


if __name__ == "__main__":
    unittest.main()
