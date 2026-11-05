"""Offline installer tests; no release, devices, or user services are touched."""

import hashlib
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "install.sh"
ASSET = "vader3p-steam-linux-x86_64"


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        root = Path(self.temp.name)
        self.home = root / "home"
        self.home.mkdir()
        self.fixture = root / "release"
        self.fixture.mkdir()
        self.bin = root / "bin"
        self.bin.mkdir()
        payload = b"test release binary\n"
        (self.fixture / ASSET).write_bytes(payload)
        (self.fixture / (ASSET + ".sha256")).write_text(
            f"{hashlib.sha256(payload).hexdigest()}  {ASSET}\n"
        )
        self.fake("id", '#!/bin/sh\n[ "$1" = -u ] && echo 1000\n')
        self.fake("uname", '#!/bin/sh\ncase "$1" in -s) echo Linux;; -m) echo x86_64;; esac\n')
        self.fake(
            "curl",
            '#!/bin/sh\n'
            'while [ "$#" -gt 0 ]; do\n'
            '  case "$1" in https://*) url=$1;; --output) shift; out=$1;; esac\n'
            '  shift\n'
            'done\n'
            'cp "$FIXTURE/${url##*/}" "$out"\n',
        )
        self.env = os.environ.copy()
        self.env.update(HOME=str(self.home), FIXTURE=str(self.fixture),
                        PATH=f'{self.bin}:{os.environ["PATH"]}', TMPDIR=str(root))
        self.destination = self.home / ".local/bin/vader3p-steam"

    def fake(self, name, content):
        path = self.bin / name
        path.write_text(content)
        path.chmod(0o755)

    def run_installer(self, *args):
        return subprocess.run(["sh", str(SCRIPT), *args], env=self.env,
                              text=True, capture_output=True, check=False)

    def test_install_preserves_config_and_does_not_create_service(self):
        config = self.home / ".config/vader3p-steam/config.toml"
        config.parent.mkdir(parents=True)
        config.write_text('rumble = false\n')
        result = self.run_installer()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.destination.read_bytes(), b"test release binary\n")
        self.assertEqual(self.destination.stat().st_mode & 0o777, 0o755)
        self.assertEqual(config.read_text(), 'rumble = false\n')
        self.assertFalse((self.home / ".config/systemd").exists())

    def test_existing_binary_requires_explicit_replace(self):
        self.destination.parent.mkdir(parents=True)
        self.destination.write_bytes(b"old")
        result = self.run_installer()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.destination.read_bytes(), b"old")
        result = self.run_installer("--replace")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.destination.read_bytes(), b"test release binary\n")

    def test_checksum_failure_leaves_existing_binary_untouched(self):
        self.destination.parent.mkdir(parents=True)
        self.destination.write_bytes(b"old")
        (self.fixture / (ASSET + ".sha256")).write_text("bad checksum\n")
        result = self.run_installer("--replace")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Checksum verification failed", result.stderr)
        self.assertEqual(self.destination.read_bytes(), b"old")

    def test_manifest_with_extra_entries_is_rejected(self):
        checksum = self.fixture / (ASSET + ".sha256")
        checksum.write_text(checksum.read_text() + "another entry\n")
        result = self.run_installer()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.destination.exists())

    def test_missing_checksum_does_not_replace_or_leave_staging_files(self):
        self.destination.parent.mkdir(parents=True)
        self.destination.write_bytes(b"old")
        (self.fixture / (ASSET + ".sha256")).unlink()
        result = self.run_installer("--replace")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.destination.read_bytes(), b"old")
        self.assertEqual(list(self.destination.parent.iterdir()), [self.destination])

    def test_missing_release_and_symlink_fail_closed(self):
        (self.fixture / ASSET).unlink()
        result = self.run_installer()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.destination.exists())
        self.destination.parent.mkdir(parents=True, exist_ok=True)
        self.destination.symlink_to(self.home / "missing")
        result = self.run_installer("--replace")
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(self.destination.is_symlink())

    def test_unsupported_system_and_root_are_rejected(self):
        self.fake("uname", '#!/bin/sh\ncase "$1" in -s) echo Linux;; -m) echo aarch64;; esac\n')
        self.assertNotEqual(self.run_installer().returncode, 0)
        self.fake("id", '#!/bin/sh\necho 0\n')
        self.assertIn("not root", self.run_installer().stderr)


if __name__ == "__main__":
    unittest.main()
