from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[1]


class DocumentationTests(unittest.TestCase):
    def test_required_project_documents_exist(self):
        for name in (
            "PRD.md", "PLAN.md", "PROGRESS.md", "README.md",
            "docs/RESEARCH.md", "docs/PROTOCOL.md", "docs/ARCHITECTURE.md",
            "docs/TESTING.md", "docs/phases/00-setup.md",
            "docs/phases/01-research.md", "docs/OPEN_QUESTIONS.md",
        ):
            with self.subTest(name=name):
                self.assertTrue((ROOT / name).is_file())

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
