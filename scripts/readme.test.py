"""Acceptance checks for the root README; run with python3 scripts/readme.test.py."""

from pathlib import Path
import re
import unittest
from urllib.parse import unquote, urlsplit


ROOT = Path(__file__).resolve().parents[1]
README = ROOT / "README.md"
INSTALL = ROOT / "site/src/content/docs/docs/getting-started/install.mdx"
DOCKER = ROOT / "docs/docker.md"


def links(markdown):
    return [
        (match.group(0).startswith("!"), match.group(1), match.group(2))
        for match in re.finditer(r"!?\[([^\]]+)\]\(([^)\s]+)\)", markdown)
    ]


def exact_case_path(path):
    current = ROOT
    for part in path.parts:
        if part not in {child.name for child in current.iterdir()}:
            return None
        current /= part
    return current


def heading_slugs(markdown):
    slugs = set()
    for heading in re.findall(r"^#{1,6} +(.+)$", markdown, re.MULTILINE):
        heading = re.sub(r"\[[^\]]+\]\([^)]+\)", "", heading)
        heading = re.sub(r"[^\w\s-]", "", heading.lower()).strip()
        slugs.add(re.sub(r"\s+", "-", heading))
    return slugs


def local_link_resolves(destination):
    path_text, _, fragment = unquote(destination).partition("#")
    if not path_text or path_text.startswith("/"):
        return False
    path = Path(path_text)
    if ".." in path.parts:
        return False
    target = exact_case_path(path)
    if target is None or not target.exists():
        return False
    if fragment:
        return target.is_file() and fragment in heading_slugs(target.read_text())
    return True


class ReadmeAcceptance(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.readme = README.read_text()
        cls.install = INSTALL.read_text()
        cls.docker = DOCKER.read_text()
        cls.links = links(cls.readme)

    def test_case_1_first_impression_and_entry_points(self):
        opening = self.readme.split("\n## ", 1)[0]
        self.assertTrue(opening.startswith("# Natamux\n"))
        self.assertEqual(len(re.findall(r"^# ", self.readme, re.MULTILINE)), 1)
        for word in ("coding agents", "worktree", "terminal", "dashboard"):
            self.assertIn(word, opening.lower())
        self.assertLess(len(opening.split()), 150, "entry points should appear near the top")

        images = [(alt, target) for is_image, alt, target in links(opening) if is_image]
        self.assertEqual(len(images), 1)
        alt, target = images[0]
        self.assertIn("dashboard", alt.lower())
        self.assertGreaterEqual(len(alt.split()), 7)
        self.assertNotIn(".png", alt.lower())
        self.assertTrue(target.startswith("site/public/screens/"))
        image = ROOT / target
        self.assertTrue(image.is_file())
        png = image.read_bytes()[:24]
        self.assertEqual(png[:8], b"\x89PNG\r\n\x1a\n")
        self.assertGreaterEqual(int.from_bytes(png[16:20], "big"), 1000)

        destinations = [target for is_image, _, target in links(opening) if not is_image]
        self.assertIn("https://github.com/lenhatthanh20/natamux/releases/latest", destinations)
        self.assertIn("https://natamux.github.io/docs/getting-started/install/", destinations)
        self.assertIn("https://codespaces.new/lenhatthanh20/natamux", destinations)

    def test_case_2_both_getting_started_paths(self):
        self.assertIn("Natamux-mac-arm64.dmg", self.install)
        self.assertIn("Natamux-mac-arm64.dmg", self.readme)
        self.assertIn("macOS on Apple silicon", self.readme)
        self.assertIn("scripts/dev.sh demo", self.readme)
        self.assertIn("scripts/dev.sh seed", self.readme)
        for value in ("5173", "devtoken"):
            self.assertIn(value, self.readme)
            self.assertIn(value, self.docker)
        self.assertRegex(self.readme, r"(?s)scripts/dev\.sh seed.{0,80}fake agents")
        self.assertRegex(self.readme, r"(?s)development Docker images.{0,80}do not include a real coding agent")
        for name in ("Natamux-mac-x64.dmg", "Natamux-win-x64.exe", "Natamux-linux-x86_64.AppImage", "Natamux-linux-amd64.deb"):
            self.assertIn(name, self.readme)
            self.assertIn(name, self.install)

    def test_case_3_scannable_workflow_and_repository_map(self):
        headings = re.findall(r"^## (.+)$", self.readme, re.MULTILINE)
        self.assertEqual(headings, ["Get started", "What you can do", "Repository guide", "Security and privacy", "License"])
        feature_section = self.readme.split("## What you can do\n", 1)[1].split("\n## ", 1)[0]
        self.assertGreaterEqual(len(re.findall(r"^- \*\*", feature_section, re.MULTILINE)), 5)
        for concept in ("task", "git worktree", "terminal", "agent", "stages", "Needs you", "review"):
            self.assertIn(concept.lower(), self.readme.lower())
        for package in ("core", "daemon", "web", "desktop", "cli"):
            self.assertIn(f"packages/{package}/", self.readme)
        self.assertIn("plugins/natamux/", self.readme)
        for guide in ("getting-started/install/", "getting-started/quick-start/", "docs/dev-setup.md"):
            self.assertIn(guide, self.readme)
        paragraphs = [
            block for block in re.split(r"\n\s*\n", self.readme)
            if not block.startswith(("|", "-", "1.", "2.", "3.", "```"))
        ]
        self.assertLess(max(len(paragraph.split()) for paragraph in paragraphs), 100)

    def test_case_4_requirements_limits_and_trust(self):
        self.assertIn("installed, logged-in coding agent", self.readme)
        for text in (self.readme, self.install):
            self.assertNotIn("paused", text)
            for step in ("Open Anyway", "More info", "Run anyway", "chmod +x"):
                self.assertIn(step, text)
        self.assertIn("installed and logged in", self.install)
        self.assertIn("need git", self.readme)
        self.assertIn("no account or telemetry", self.readme)
        self.assertIn("no account", (ROOT / "PRIVACY.md").read_text())
        self.assertIn("no telemetry", (ROOT / "PRIVACY.md").read_text())
        self.assertIn("private vulnerability reporting", self.readme)
        self.assertTrue((ROOT / "LICENSE").read_text().startswith("MIT License"))
        for target in ("PRIVACY.md", "SECURITY.md", "LICENSE"):
            self.assertIn(target, [destination for _, _, destination in self.links])
        self.assertNotRegex(self.readme.lower(), r"(nothing|no data) leaves your machine|agents are sandboxed")

    def test_case_5_markdown_links_and_change_scope(self):
        self.assertEqual(self.readme.count("```"), 2)
        self.assertTrue(local_link_resolves("docs/docker.md#production"))
        self.assertFalse(local_link_resolves("docs/Docker.md#production"))
        local = [target for _, _, target in self.links if not urlsplit(target).scheme]
        self.assertTrue(local)
        for target in local:
            with self.subTest(target=target):
                self.assertTrue(local_link_resolves(target))
        for _, _, target in self.links:
            if urlsplit(target).scheme:
                with self.subTest(target=target):
                    self.assertEqual(urlsplit(target).scheme, "https")
                    self.assertIn(urlsplit(target).hostname, {"github.com", "natamux.github.io", "codespaces.new"})
                    if urlsplit(target).hostname == "natamux.github.io":
                        route = urlsplit(target).path.removeprefix("/docs/").strip("/")
                        page = ROOT / "site/src/content/docs/docs" / (route or "index")
                        self.assertTrue(page.with_suffix(".md").is_file() or page.with_suffix(".mdx").is_file())


if __name__ == "__main__":
    unittest.main()
