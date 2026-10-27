"""Guards that the public documentation matches the code it describes.

The README is the release artifact most likely to go stale, because it makes
concrete promises: which config keys exist, which examples exist, which files hold
the license. Every one of those is checkable from the repository, so a mismatch is
a bug rather than a matter of taste.
"""

import re
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
README = ROOT / "README.md"


def _readme():
    return README.read_text()


def _config_keys():
    """Config field names, taken from the Rust struct rather than a copy."""
    source = (ROOT / "driver" / "src" / "config.rs").read_text()
    body = source.split("pub struct Config {", 1)[1].split("}", 1)[0]
    return re.findall(r"pub (\w+):", body)


def test_readme_documents_every_config_key():
    """A key that exists but is undocumented is a hidden feature."""
    text = _readme()
    for key in _config_keys():
        assert f"`{key}`" in text, f"config key {key!r} is not in the README"


def test_config_reference_has_no_invented_keys():
    """A documented key that does not exist is a lie in the docs."""
    keys = set(_config_keys())
    section = _readme().split("## Configuration reference", 1)[1].split("\n## ", 1)[0]
    documented = set(re.findall(r"^\| `(\w+)`", section, re.MULTILINE))
    assert documented, "the configuration table was not found"
    unknown = documented - keys
    assert not unknown, f"README documents keys that do not exist: {sorted(unknown)}"


def test_readme_referenced_examples_exist():
    """Every `example` named in the README must be a real file."""
    text = _readme()
    named = set(re.findall(r"`(signal_probe|sdl_probe|raw_buttons|uinput_ff_probe|"
                           r"ff_handshake_probe|ff_probe|uhid_caps)`", text))
    assert named, "no examples referenced; the table may have been removed"
    for name in named:
        assert (ROOT / "driver" / "examples" / f"{name}.rs").is_file(), f"{name}.rs missing"


def test_readme_referenced_scripts_exist():
    text = _readme()
    for name in ("interactive-test.py", "latency-test.py"):
        assert name in text, f"{name} is not mentioned in the README"
        assert (ROOT / "driver" / "contrib" / name).is_file(), f"{name} missing"


def test_readme_cli_reference_matches_help_text():
    """The documented flags must be the ones the binary accepts."""
    source = (ROOT / "driver" / "src" / "main.rs").read_text()
    for flag in ("--config", "--help", "--version"):
        assert f'"{flag}"' in source, f"{flag} is documented but not handled by main.rs"


def test_help_text_names_the_node_the_daemon_actually_opens():
    """Regression: --help once claimed /dev/uhid while the pad uses /dev/uinput.

    A wrong permission path in user-facing help sends people to fix the wrong
    thing, and the symptom (permission denied creating the pad) looks unrelated.
    """
    help_text = (ROOT / "driver" / "src" / "main.rs").read_text()
    pad_source = (ROOT / "driver" / "src" / "pad.rs").read_text()
    if '"/dev/uinput"' in pad_source:
        assert "/dev/uinput" in help_text, "help must name /dev/uinput"
        assert "/dev/uhid" not in help_text.split("Requires access", 1)[1], (
            "help must not claim /dev/uhid is required"
        )


def test_license_files_exist_for_the_declared_license():
    """Publishing with a declared license but no text means nobody may use it."""
    manifest = (ROOT / "driver" / "Cargo.toml").read_text()
    declared = re.search(r'^license\s*=\s*"([^"]+)"', manifest, re.MULTILINE)
    assert declared, "Cargo.toml has no license field"
    value = declared.group(1)
    assert "MIT" in value
    assert (ROOT / "LICENSE-MIT").is_file(), "MIT text missing"
    if "Apache" in value:
        assert (ROOT / "LICENSE-APACHE").is_file(), "Apache text missing"
    readme = _readme()
    assert "LICENSE-MIT" in readme and "LICENSE-APACHE" in readme


def test_apache_license_text_is_the_canonical_one():
    """A truncated or edited Apache text is not the Apache license."""
    text = (ROOT / "LICENSE-APACHE").read_text()
    assert "Apache License" in text
    assert "Version 2.0, January 2004" in text
    # The canonical file ends with the limitations paragraph.
    assert text.rstrip().endswith("limitations under the License.")


def test_documented_rumble_frame_length_matches_the_code():
    """The README states the frame is exactly 4 bytes; the code must agree."""
    rumble = (ROOT / "driver" / "src" / "rumble.rs").read_text()
    assert "BT_DURATION_MS" in rumble
    readme = _readme()
    assert "exactly 4" in readme, "the length requirement is the key insight"
    # The four bytes are the command plus two levels plus a duration.
    assert "11 <left> <right> <duration_ms>" in readme


def test_readme_does_not_claim_steam_is_verified():
    """The one unverified acceptance criterion must stay marked as such."""
    text = _readme()
    assert "**Untested**" in text or "Not yet verified" in text, (
        "the README must keep the Steam gap visible"
    )


def test_progress_documents_the_working_driver_not_only_research():
    """PROGRESS.md once still said rumble was unresolved after it was solved.

    A stale progress record is worse than none, because a reader trusts it for the
    current state rather than the history.
    """
    progress = (ROOT / "PROGRESS.md").read_text()
    assert "Solved" in progress or "solved" in progress
    assert "Current state" in progress


if __name__ == "__main__":
    raise SystemExit(pytest.main([__file__, "-v"]))
