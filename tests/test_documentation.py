from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[1]


class DocumentationTests(unittest.TestCase):
    def test_public_project_documents_exist(self):
        for name in (
            "README.md", "ARCHITECTURE.md", "VADER3P_PROTOCOL.md",
            "docs/TESTING.md", "docs/steam-controller-visibility.md",
            "docs/uhid-migration.md", "driver/README.md",
        ):
            with self.subTest(name=name):
                self.assertTrue((ROOT / name).is_file())

    def test_internal_planning_and_evidence_are_not_public_documents(self):
        for name in (
            "PRD.md", "PLAN.md", "PROGRESS.md", "docs/RESEARCH.md",
            "docs/PROTOCOL.md", "docs/ARCHITECTURE.md", "docs/HANDOFF.md",
            "docs/OPEN_QUESTIONS.md", "docs/phases", "docs/evidence",
            "docs/prompts",
        ):
            with self.subTest(name=name):
                self.assertFalse((ROOT / name).exists())

    def test_public_docs_do_not_link_to_local_archive_or_internal_records(self):
        paths = list(ROOT.glob("*.md")) + list((ROOT / "docs").rglob("*.md"))
        paths.extend((ROOT / "tests/fixtures").rglob("*.md"))
        internal = re.compile(
            r"(?:local/research|(?:^|[/])(?:PRD|PLAN|PROGRESS)\.md|"
            r"docs/(?:evidence|phases|prompts|RESEARCH\.md|PROTOCOL\.md|"
            r"ARCHITECTURE\.md|HANDOFF\.md|OPEN_QUESTIONS\.md))"
        )
        for path in paths:
            with self.subTest(document=str(path.relative_to(ROOT))):
                self.assertIsNone(internal.search(path.read_text()))

    def test_local_markdown_links_resolve(self):
        paths = list(ROOT.glob("*.md")) + list((ROOT / "docs").rglob("*.md"))
        for path in paths:
            for target in re.findall(r"\[[^\]]*\]\(([^)]+)\)", path.read_text()):
                if "://" in target or target.startswith("#"):
                    continue
                target = target.split("#", 1)[0]
                with self.subTest(document=str(path.relative_to(ROOT)), target=target):
                    self.assertTrue((path.parent / target).exists())


if __name__ == "__main__":
    unittest.main()
