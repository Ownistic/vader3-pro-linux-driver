"""Check active documentation against the shipped UHID product surface."""

import re
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ACTIVE_DOCS = (
    ROOT / "README.md",
    ROOT / "ARCHITECTURE.md",
    ROOT / "VADER3P_PROTOCOL.md",
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
    def test_human_owner_narrative_uses_first_person(self):
        human_owner_patterns = (
            r"\b(?:the )?owner\s+(?:reported|confirmed|asked|requested|authorized|selected|"
            r"connected|reconnected|executed|observed|chose|clarified|said|felt)\b",
            r"\bowner's\s+(?:unit|controller|Vader|Legion Go|hardware)\b",
            r"\bask the owner\b|\bowner-reported\b|\bowner-confirmed\b|"
            r"\bowner-coordinated\b|\bowner-completed\b",
            r"\b(?:the|a|an|earlier)\s+my\b|\bmy\s+my\b",
        )
        docs = [path for path in ROOT.glob("*.md") if path.name != "AGENTS.md"]
        docs.extend(path for path in (ROOT / "docs").rglob("*.md"))
        docs.extend((ROOT / "tests/fixtures").rglob("*.md"))
        for path in docs:
            text = path.read_text()
            for pattern in human_owner_patterns:
                with self.subTest(path=path.relative_to(ROOT), pattern=pattern):
                    self.assertIsNone(re.search(pattern, text, re.IGNORECASE))

    def test_all_config_fields_are_documented(self):
        source = (ROOT / "driver/src/config.rs").read_text()
        docs = "\n".join(
            (ROOT / name).read_text()
            for name in ("README.md", "ARCHITECTURE.md")
        )
        for struct in ("Config", "PadConfig", "MotionConfig"):
            for field in rust_struct_fields(source, struct):
                with self.subTest(struct=struct, field=field):
                    self.assertRegex(docs, rf"\b{re.escape(field)}\b")

    def test_documented_toml_samples_use_config_struct_fields(self):
        source = (ROOT / "driver/src/config.rs").read_text()
        allowed = {
            "root": rust_struct_fields(source, "Config"),
            "pad": rust_struct_fields(source, "PadConfig"),
            "motion": rust_struct_fields(source, "MotionConfig"),
        }
        docs = "\n".join(
            (ROOT / name).read_text()
            for name in ("README.md", "ARCHITECTURE.md")
        )
        samples = re.findall(r"```toml\s*\n(.*?)\n```", docs, re.DOTALL)
        self.assertTrue(samples, "Public docs must contain a TOML configuration sample")

        for sample_index, text in enumerate(samples):
            parsed = tomllib.loads(text)
            with self.subTest(sample=sample_index):
                self.assertTrue(set(parsed).issubset(allowed["root"]))
                for section, values in parsed.items():
                    if isinstance(values, dict):
                        self.assertIn(section, ("pad", "motion"))
                        self.assertTrue(set(values).issubset(allowed[section]))

        architecture = (ROOT / "ARCHITECTURE.md").read_text()
        self.assertIn("[pad] backend = \"uhid\"", architecture)
        self.assertIn("explicit `uinput` is rejected", architecture)

    def test_unknown_configuration_keys_are_not_documented(self):
        source = (ROOT / "driver/src/config.rs").read_text()
        architecture = (ROOT / "ARCHITECTURE.md").read_text()
        allowed = rust_struct_fields(source, "Config") - {"pad", "motion"}
        allowed.update(f"pad.{field}" for field in rust_struct_fields(source, "PadConfig"))
        allowed.update(f"motion.{field}" for field in rust_struct_fields(source, "MotionConfig"))
        documented = set(re.findall(r"^\| `([^`]+)` \|", architecture, re.MULTILINE))
        self.assertTrue(documented)
        self.assertEqual(documented, allowed)

    def test_missing_config_behavior_distinguishes_default_and_explicit_paths(self):
        architecture = re.sub(r"\s+", " ", (ROOT / "ARCHITECTURE.md").read_text())
        readme = re.sub(r"\s+", " ", (ROOT / "README.md").read_text())
        migration = re.sub(r"\s+", " ", (ROOT / "docs/uhid-migration.md").read_text())

        for name, text in (("architecture", architecture), ("README", readme)):
            with self.subTest(doc=name):
                self.assertRegex(text, r"(?i)(?:omit|without) `--config`")
                self.assertIn("conventional file", text)
                self.assertIn("absent", text)
                self.assertIn("defaults", text)
                self.assertIn("`--config PATH`", text)
                self.assertIn("does not fall back to defaults", text)

        self.assertIn("broken symlink", architecture)
        self.assertIn("valid symlinks", architecture.lower())
        self.assertIn("service's `ExecStart`", migration)
        self.assertIn("explicit `--config PATH`", migration)
        self.assertIn("exact file exists", migration)
        self.assertIn("does not fall back", migration)

    def test_architecture_diagrams_use_mermaid(self):
        architecture = (ROOT / "ARCHITECTURE.md").read_text()
        diagrams = re.findall(r"```mermaid\s*\n(.*?)\n```", architecture, re.DOTALL)
        self.assertGreaterEqual(len(diagrams), 2)
        self.assertTrue(any(diagram.startswith("flowchart ") for diagram in diagrams))
        self.assertTrue(any(diagram.startswith("stateDiagram-v2") for diagram in diagrams))
        self.assertNotIn("```text", architecture)

    def test_help_and_readme_name_uhid_not_uinput_requirement(self):
        help_text = (ROOT / "driver/src/main.rs").read_text()
        readme = (ROOT / "README.md").read_text()
        self.assertIn("/dev/uhid", help_text)
        self.assertIn("/dev/uhid", readme)
        self.assertNotIn("/dev/uinput", help_text)
        self.assertNotIn("/dev/uinput", readme)
        self.assertIn("explicit `uinput` is rejected", (ROOT / "ARCHITECTURE.md").read_text())

    def test_license_files_and_release_status_match_manifest(self):
        readme = (ROOT / "README.md").read_text()
        manifest = (ROOT / "driver/Cargo.toml").read_text()
        declaration = re.search(r'^license\s*=\s*"([^"]+)"', manifest, re.MULTILINE)
        if declaration is None:
            self.fail("Cargo.toml must declare the package license")
        self.assertEqual(declaration.group(1), "MIT OR Apache-2.0")
        self.assertIn("pre-1.0", (ROOT / "driver/README.md").read_text())
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
        protocol = (ROOT / "VADER3P_PROTOCOL.md").read_text()
        self.assertIn("exactly 4 bytes", protocol)
        self.assertIn("11 <left> <right> <duration_ms>", protocol)
        rumble = (ROOT / "driver/src/rumble.rs").read_text()
        self.assertIn("BT_FRAME_LEN: usize = 4", rumble)

    def test_motion_limitations_and_steam_visibility_are_explicit(self):
        readme = re.sub(r"\s+", " ", (ROOT / "README.md").read_text())
        for term in (
            "experimental",
            "third gyro component",
            "have not hardware-retested the current pitch correction",
        ):
            self.assertIn(term, readme)
        architecture = (ROOT / "ARCHITECTURE.md").read_text()
        for term in ("fabricated", "third", "EVIOCGRAB", "prevent device enumeration"):
            self.assertIn(term, architecture)
        self.assertTrue((ROOT / "docs/steam-controller-visibility.md").is_file())

    def test_current_guides_document_automatic_controller_lifecycle(self):
        readme = (ROOT / "README.md").read_text()
        architecture = (ROOT / "ARCHITECTURE.md").read_text()
        testing = (ROOT / "docs/TESTING.md").read_text()
        migration = (ROOT / "docs/uhid-migration.md").read_text()
        driver_readme = (ROOT / "driver/README.md").read_text()

        for name, text in (
            ("README.md", readme),
            ("ARCHITECTURE.md", architecture),
            ("docs/TESTING.md", testing),
            ("docs/uhid-migration.md", migration),
        ):
            with self.subTest(doc=name):
                self.assertIn("eligible", text)
                self.assertIn("bluetooth_address", text)
        self.assertIn("waits when no eligible controller is connected", driver_readme)
        self.assertIn("same identity", driver_readme)
        self.assertIn("detects later connections", readme)
        self.assertIn("every second", architecture)
        self.assertIn("same derived", architecture)
        self.assertIn("one 1.2-second grace period", architecture)
        self.assertIn("valid private per-user runtime directory", readme)
        self.assertIn("cooperating processes running as the same user", architecture)
        self.assertIn("uncooperative daemon", migration)
        self.assertIn("Steam's device-list refresh is not guaranteed", testing)

        current_guides = "\n".join(
            (ROOT / name).read_text()
            for name in (
                "README.md",
                "ARCHITECTURE.md",
                "driver/README.md",
                "docs/TESTING.md",
                "docs/uhid-migration.md",
            )
        )
        for stale_claim in (
            "must be connected at startup",
            "connected when the daemon starts",
            "rumble or motion requires",
            "UHID device survives physical link loss",
            "one virtual DualSense Edge",
        ):
            with self.subTest(stale_claim=stale_claim):
                self.assertNotIn(stale_claim, current_guides)

    def test_virtual_bluetooth_report_contract_is_documented(self):
        protocol = (ROOT / "VADER3P_PROTOCOL.md").read_text()
        for term in (
            "78-byte input and output reports",
            "CRC-32/ISO-HDLC",
            "domain byte `A1`",
            "CRC domain `A2`",
            "CRC domain `A3`",
            "Feature `09` carries the virtual identity",
            "Linux",
            "SDL",
            "provisional",
            "tickle reports",
        ):
            with self.subTest(term=term):
                self.assertIn(term, protocol)

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
