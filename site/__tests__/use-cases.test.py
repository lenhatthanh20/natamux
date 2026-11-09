"""Check recipe syntax and the entry links that readers follow to use cases."""

import json
from pathlib import Path
import re
import unittest


SITE = Path(__file__).resolve().parents[1]
GUIDES = SITE / "src/content/docs/docs/guides"


class UseCaseTests(unittest.TestCase):
    def case_anchors(self, page):
        cases = page.split("## Simple projects\n", 1)[1].split("## Complete settings recipes", 1)[0]
        return {
            re.sub(r"[^\w -]", "", heading.lower()).replace(" ", "-")
            for heading in re.findall(r"^### (.+)$", cases, re.M)
        }

    def test_chooser_covers_every_case_without_promising_context_approval(self):
        page = (GUIDES / "use-cases.mdx").read_text()
        chooser = page.split("## Choose your starting point\n", 1)[1].split("## Use a recipe", 1)[0]
        links = set(re.findall(r"\]\(#([\w-]+)\)", chooser))
        self.assertEqual(links, self.case_anchors(page))
        for row in chooser.splitlines():
            if row.startswith("|") and "](#" in row:
                self.assertNotIn("context", row.split("|")[3].lower())
        first_case = page.split("### First small change\n", 1)[1].split("### Documentation change", 1)[0]
        self.assertNotRegex(first_case.lower(), r"approve (?:the )?context")

    def test_homepage_previews_link_to_cases(self):
        page = (GUIDES / "use-cases.mdx").read_text()
        homepage = (SITE / "src/pages/index.astro").read_text()
        previews = homepage.split("const useCases = [", 1)[1].split("\n]", 1)[0]
        targets = re.findall(r"anchor: '([\w-]+)'", previews)
        self.assertEqual(len(set(targets)), 4)
        self.assertTrue(set(targets).issubset(self.case_anchors(page)))

    def test_complete_recipes_are_json_objects_with_stages(self):
        page = (GUIDES / "use-cases.mdx").read_text()
        recipes = re.findall(r"```json(?: [^\n]*)?\n(.*?)\n```", page, re.S)
        self.assertGreaterEqual(len(recipes), 4)
        for recipe in recipes:
            with self.subTest(recipe=recipe[:70]):
                settings = json.loads(recipe)
                self.assertIsInstance(settings, dict)
                self.assertTrue(settings["stages"])

    def test_homepage_scenarios_and_chooser_have_valid_targets(self):
        page = (GUIDES / "use-cases.mdx").read_text()
        headings = re.findall(r"^#{2,6} (.+)$", page, re.M)
        anchors = {
            re.sub(r"[^\w -]", "", heading.lower()).replace(" ", "-")
            for heading in headings
        }
        anchors.update(re.findall(r'\bid="([^"]+)"', page))
        homepage = (SITE / "src/pages/index.astro").read_text()
        targets = re.findall(r"guides/use-cases/#([\w-]+)", homepage)
        if "guides/use-cases/#${useCase.anchor}" in homepage:
            targets += re.findall(r"anchor: '([\w-]+)'", homepage)
        self.assertGreaterEqual(len(set(targets)), 4)
        targets += re.findall(r"\]\(#([\w-]+)\)", page)
        for target in targets:
            with self.subTest(target=target):
                self.assertIn(target, anchors)

    def test_catalog_links_resolve_to_documentation_files(self):
        page = (GUIDES / "use-cases.mdx").read_text()
        targets = re.findall(r"\]\((/docs/[^)#]+)(?:#[^)]*)?\)", page)
        self.assertTrue(targets)
        for target in targets:
            path = SITE / "src/content/docs" / target.strip("/")
            with self.subTest(target=target):
                self.assertTrue(any(candidate.is_file() for candidate in (
                    path.with_suffix(".md"), path.with_suffix(".mdx"),
                    path / "index.md", path / "index.mdx",
                )))


if __name__ == "__main__":
    unittest.main()
