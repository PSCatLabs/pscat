#!/usr/bin/env python3
"""Tests for codeowners_check.py: python3 scripts/test_codeowners_check.py"""
import os
import subprocess
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import codeowners_check as c  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

RULES = c.parse(
    """
# a comment
/.github/workflows/   @owner
/Cargo.toml           @owner
/LICENSE*             @owner   # forward: #563
*.lock                @owner
docs/secret/          @owner
/docs/secret/public.md
"""
)


class Matching(unittest.TestCase):
    def test_directory_rule_owns_everything_under_it(self):
        self.assertTrue(c.owned(RULES, ".github/workflows/ci.yml"))
        self.assertTrue(c.owned(RULES, ".github/workflows/sub/x.yml"))
        self.assertFalse(c.owned(RULES, ".github/ISSUE_TEMPLATE/bug.yml"))

    def test_root_anchor(self):
        self.assertTrue(c.owned(RULES, "Cargo.toml"))
        self.assertFalse(c.owned(RULES, "crates/x/Cargo.toml"))

    def test_glob_stops_at_slash(self):
        self.assertTrue(c.owned(RULES, "LICENSE-MIT"))
        self.assertTrue(c.owned(RULES, "LICENSE"))
        self.assertFalse(c.owned(RULES, "docs/LICENSE-notes"))

    def test_unanchored_name_matches_at_any_depth(self):
        self.assertTrue(c.owned(RULES, "Cargo.lock"))
        self.assertTrue(c.owned(RULES, "deep/dir/Cargo.lock"))

    def test_slash_in_the_middle_anchors(self):
        self.assertTrue(c.owned(RULES, "docs/secret/a.md"))
        self.assertFalse(c.owned(RULES, "x/docs/secret/a.md"))

    def test_last_match_wins_and_ownerless_rule_unowns(self):
        self.assertFalse(c.owned(RULES, "docs/secret/public.md"))

    def test_unanchored_directory_matches_at_any_depth(self):
        r = c.parse(".claude-plugin/ @o")
        self.assertTrue(c.owned(r, ".claude-plugin/plugin.json"))
        self.assertTrue(c.owned(r, "plugins/pscat/.claude-plugin/plugin.json"))
        self.assertFalse(c.owned(r, "plugins/pscat/claude-plugin/plugin.json"))

    def test_double_star(self):
        r = c.parse("/a/**/z.txt @o")
        self.assertTrue(c.owned(r, "a/z.txt"))
        self.assertTrue(c.owned(r, "a/b/c/z.txt"))
        self.assertFalse(c.owned(r, "b/a/z.txt"))

    def test_comment_only_and_blank_lines_ignored(self):
        self.assertEqual(c.parse("# x\n\n   \n"), [])

    def test_forward_flag(self):
        flags = {p: f for p, _, f in RULES}
        self.assertTrue(flags["/LICENSE*"])
        self.assertFalse(flags["/Cargo.toml"])

    def test_check_paths_reports_dead_non_forward_rules(self):
        rules = c.parse("/Cargo.toml @o\n/gone.toml @o\n/later.toml @o # forward: #1")
        self.assertEqual(c.check_paths(rules, ["Cargo.toml"]), ["/gone.toml"])


class BaseAndPr(unittest.TestCase):
    """--base judges the base's rules and the PR's rules separately."""

    def test_a_pr_cannot_unown_a_path_with_an_ownerless_rule(self):
        base = c.parse("/Cargo.toml @o\n/.github/CODEOWNERS @o")
        pr = c.parse("/Cargo.toml\n/.github/CODEOWNERS")  # ownerless: un-owns both
        self.assertFalse(c.owned(pr, "Cargo.toml"))
        self.assertTrue(c.owned_by_any([base, pr], "Cargo.toml"))
        self.assertTrue(c.owned_by_any([base, pr], ".github/CODEOWNERS"))

    def test_a_pr_cannot_unown_a_path_by_deleting_the_line(self):
        base = c.parse("/Cargo.toml @o")
        self.assertTrue(c.owned_by_any([base, []], "Cargo.toml"))

    def test_a_path_neither_owns_is_free(self):
        self.assertFalse(c.owned_by_any([c.parse("/a @o"), c.parse("/b @o")], "src/x.rs"))

    def test_new_pr_rule_adds_protection(self):
        self.assertTrue(c.owned_by_any([[], c.parse("/new.toml @o")], "new.toml"))


class Repo(unittest.TestCase):
    """The checked-in CODEOWNERS file itself."""

    def test_every_entry_exists_or_is_marked_forward(self):
        r = subprocess.run(
            [sys.executable, os.path.join(ROOT, "scripts", "codeowners_check.py"), "--check-paths"],
            cwd=ROOT, capture_output=True, text=True,
        )
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)

    def test_codeowners_protects_itself_and_the_workflows(self):
        with open(os.path.join(ROOT, c.CODEOWNERS), encoding="utf-8") as f:
            rules = c.parse(f.read())
        self.assertTrue(c.owned(rules, ".github/CODEOWNERS"))
        self.assertTrue(c.owned(rules, ".github/workflows/ci.yml"))
        self.assertTrue(c.owned(rules, "Cargo.toml"))
        self.assertTrue(c.owned(rules, "Cargo.lock"))
        self.assertFalse(c.owned(rules, "src/interp.rs"))


if __name__ == "__main__":
    unittest.main()
