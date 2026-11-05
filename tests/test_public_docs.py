"""Check active documentation against the shipped UHID product surface."""

import re
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ACTIVE_DOCS = (
    ROOT / "README.md",
    ROOT / "driver/README.md",
    ROOT / "docs/TESTING.md",
    ROOT / "docs/uhid-migration.md",
)


def rust_struct_fields(source: str, name: str) -> set[str]:
    match = re.search(rf"pub struct {name}\s*\{{([^{{}}]*)\}}", source)
    if match is None:
        raise AssertionError(f"Rust struct {name} was not found")
    return set(re.findall(r"pub (\w+)\s*:", match.group(1)))


class PublicDocumentationTests(unittest.TestCase):
    def test_all_config_fields_are_documented(self):
        source = (ROOT / "driver/src/config.rs").read_text()
        readme = (ROOT / "README.md").read_text()
        for struct in ("Config", "PadConfig", "MotionConfig"):
            for field in rust_struct_fields(source, struct):
                with self.subTest(struct=struct, field=field):
                    self.assertRegex(readme, rf"\b{re.escape(field)}\b")

    def test_documented_toml_samples_use_config_struct_fields(self):
        source = (ROOT / "driver/src/config.rs").read_text()
        allowed = {
            "root": rust_struct_fields(source, "Config"),
            "pad": rust_struct_fields(source, "PadConfig"),
            "motion": rust_struct_fields(source, "MotionConfig"),
        }
        readme = (ROOT / "README.md").read_text()
        samples = re.findall(r"```toml\s*\n(.*?)\n```", readme, re.DOTALL)
        self.assertTrue(samples, "README must contain a TOML configuration sample")

        for sample_index, text in enumerate(samples):
            parsed = tomllib.loads(text)
            with self.subTest(sample=sample_index):
                self.assertTrue(set(parsed).issubset(allowed["root"]))
                for section, values in parsed.items():
                    if isinstance(values, dict):
                        self.assertIn(section, ("pad", "motion"))
                        self.assertTrue(set(values).issubset(allowed[section]))

        self.assertIn("[pad] backend = \"uhid\"", readme)
        self.assertIn("explicit `uinput` is rejected", readme)

    def test_help_and_readme_name_uhid_not_uinput_requirement(self):
        help_text = (ROOT / "driver/src/main.rs").read_text()
        readme = (ROOT / "README.md").read_text()
        self.assertIn("/dev/uhid", help_text)
        self.assertIn("/dev/uhid", readme)
        self.assertNotIn("/dev/uinput", help_text)
        self.assertNotIn("/dev/uinput", readme)
        self.assertIn("explicit `uinput` is rejected", readme)

    def test_license_files_and_release_status_match_manifest(self):
        readme = (ROOT / "README.md").read_text()
        manifest = (ROOT / "driver/Cargo.toml").read_text()
        declaration = re.search(r'^license\s*=\s*"([^"]+)"', manifest, re.MULTILINE)
        if declaration is None:
            self.fail("Cargo.toml must declare the package license")
        self.assertEqual(declaration.group(1), "MIT OR Apache-2.0")
        self.assertIn("pre-1.0", readme[:1500])
        for name in ("LICENSE-MIT", "LICENSE-APACHE"):
            self.assertIn(name, readme)
            self.assertTrue((ROOT / name).is_file())
        self.assertIn("Dual-licensed under [MIT]", readme)
        self.assertIn("or [Apache-2.0]", readme)
        apache = (ROOT / "LICENSE-APACHE").read_text()
        self.assertIn("Version 2.0, January 2004", apache)
        self.assertTrue(apache.rstrip().endswith("limitations under the License."))

    def test_cli_flags_and_rumble_frame_are_consistent(self):
        source = (ROOT / "driver/src/main.rs").read_text()
        for flag in ("--config", "--help", "--version"):
            self.assertIn(f'"{flag}"', source)
        readme = (ROOT / "README.md").read_text()
        self.assertIn("exactly 4 bytes", readme)
        self.assertIn("11 <left> <right> <duration_ms>", readme)
        rumble = (ROOT / "driver/src/rumble.rs").read_text()
        self.assertIn("BT_FRAME_LEN: usize = 4", rumble)

    def test_motion_limitations_and_steam_visibility_are_explicit(self):
        readme = (ROOT / "README.md").read_text()
        for term in (
            "fabricated",
            "unknown third value",
            "not validate this build",
            "EVIOCGRAB",
            "does not remove the device",
        ):
            self.assertIn(term, readme)
        self.assertTrue((ROOT / "docs/steam-controller-visibility.md").is_file())

    def test_migration_guide_documents_owned_legacy_cleanup_safely(self):
        readme = (ROOT / "README.md").read_text()
        guide_path = ROOT / "docs/uhid-migration.md"
        guide = guide_path.read_text()
        self.assertIn("[migration guide](docs/uhid-migration.md)", readme)
        for artifact in (
            "~/.config/systemd/user/vader3p-steam.service",
            "~/.config/environment.d/vader3p-steam.conf",
            "SDL_GAMECONTROLLERCONFIG",
            "systemctl --user status vader3p-steam.service",
            "systemctl --user cat vader3p-steam.service",
            "systemctl --user show vader3p-steam.service",
            "systemctl --user stop vader3p-steam.service",
            "systemctl --user disable vader3p-steam.service",
        ):
            self.assertIn(artifact, guide)
        self.assertIn("ExecStart=%h/.local/bin/vader3p-steam --config", guide)
        self.assertIn("RestartSec=3", guide)
        self.assertIn("does not grant device permissions", guide)
        self.assertIn("does not restore or start", guide)
        self.assertIn("04b4:2412", guide)
        self.assertNotIn("systemctl --user revert", guide.split("Optional user service", 1)[1])

    def test_documented_examples_and_local_markdown_links_exist(self):
        for doc in ACTIVE_DOCS:
            text = doc.read_text()
            for example in re.findall(r"cargo run[^\n]*--example\s+([\w-]+)", text):
                self.assertTrue((ROOT / f"driver/examples/{example}.rs").is_file(), example)
            for script in re.findall(r"python3\s+([\w./-]+\.py)", text):
                candidates = (doc.parent / script, ROOT / script)
                self.assertTrue(any(candidate.is_file() for candidate in candidates), script)

            for target in re.findall(r"\[[^\]]*\]\(([^)]+)\)", text):
                if target.startswith(("http://", "https://", "mailto:", "#")):
                    continue
                path = target.split("#", 1)[0]
                if path:
                    self.assertTrue((doc.parent / path).resolve().exists(), f"{doc}: {target}")


if __name__ == "__main__":
    unittest.main()
