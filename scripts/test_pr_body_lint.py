#!/usr/bin/env python3
"""Tests for pr_body_lint.py: python3 scripts/test_pr_body_lint.py"""
import os
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import pr_body_lint as lint  # noqa: E402

SCRIPT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "pr_body_lint.py")


def flagged(text):
    return [f[2] for f in lint.findings(text)]


class Flags(unittest.TestCase):
    def test_negated_closers(self):
        for s in [
            "This does not fix #12.",
            "Doesn't fully resolve #12",
            "It won't close #12",
            "This never closed #12",
            "Stale catalog counts that this does not fix: #437.",  # PR #438, verbatim
        ]:
            self.assertEqual(flagged(s), [s], s)

    def test_partial_closers(self):
        for s in [
            "Partially fixes #12",
            "This partly resolves #12",
            "Helps fix #12",
            "Fixes #12 partially",
            "Fixes #12 for now",
            "Closes #12, the first half",
        ]:
            self.assertTrue(flagged(s), s)

    def test_every_reference_form(self):
        for ref in ["#12", "PSCatLabs/pscat#12", "https://github.com/PSCatLabs/pscat/issues/12"]:
            self.assertTrue(flagged(f"does not fix {ref}"), ref)
            self.assertTrue(flagged(f"does not fix: {ref}"), ref)

    def test_every_keyword(self):
        for kw in ["close", "closes", "closed", "fix", "fixes", "fixed", "resolve", "resolves", "resolved"]:
            self.assertTrue(flagged(f"This does not {kw} #12"), kw)
            self.assertTrue(flagged(f"This does not {kw.upper()} #12"), kw)


class Wrapped(unittest.TestCase):
    def test_negation_wrapped_onto_the_previous_line(self):
        self.assertTrue(flagged("This change does not\nfix #12 yet."))
        self.assertTrue(flagged("Body text that only\npartially fixes #12"))

    def test_a_heading_does_not_carry(self):
        self.assertEqual(flagged("## Not in scope\nCloses #12"), [])

    def test_negation_in_another_clause(self):
        self.assertEqual(flagged("Does not change the API, and fixes #12"), [])
        self.assertEqual(flagged("No API change \u2014 fixes #12"), [])

    def test_partial_wording_after_the_reference(self):
        self.assertTrue(flagged("Fixes #12 (not fully)"))
        self.assertTrue(flagged("Fix #12, part two"))

    def test_negation_later_in_the_clause(self):
        self.assertTrue(flagged("Fix #12 isn't complete"))
        self.assertTrue(flagged("Fixed: #12 is not done"))
        self.assertTrue(flagged("Fixes #12\u2019s first half"))

    def test_list_items(self):
        self.assertEqual(flagged("- Not in scope\nCloses #12"), [])
        self.assertTrue(flagged("- This change does not\n  fix #12"))

    def test_curly_apostrophe(self):
        self.assertTrue(flagged("This doesn\u2019t fix #12"))

    def test_a_finished_line_or_a_new_block_does_not_carry(self):
        self.assertEqual(flagged("Not in scope: the parser.\nCloses #12"), [])
        self.assertEqual(flagged("Not in scope: X\nCloses #12"), [])
        self.assertEqual(flagged("- does not touch the renderer\n- Closes #12"), [])


class Passes(unittest.TestCase):
    def test_plain_closers(self):
        for s in ["Closes #12", "Fixes: #12", "Resolves PSCatLabs/pscat#12", "Note: fixes #12"]:
            self.assertEqual(flagged(s), [], s)

    def test_negation_in_another_sentence(self):
        self.assertEqual(flagged("Does not change the API. Closes #12"), [])
        self.assertEqual(flagged("Not in scope: the parser; fixes #12"), [])
        self.assertEqual(flagged("Closes #12\nThis does not touch the renderer."), [])

    def test_help_far_from_the_keyword_is_not_partial(self):
        self.assertEqual(flagged("Adds help text to the CLI and fixes #12"), [])

    def test_refs_and_no_keyword(self):
        self.assertEqual(flagged("Refs #12 (does not fix it)"), [])
        self.assertEqual(flagged("Does not fix part of #12"), [])  # no keyword+reference pair

    def test_html_comments_are_ignored(self):
        self.assertEqual(flagged("<!-- does not fix #12 -->\nCloses #<!-- issue number -->"), [])

    def test_line_numbers_survive_comments(self):
        f = lint.findings("<!--\n\n-->\nThis does not fix #12")
        self.assertEqual(f[0][1], 4)


class Cli(unittest.TestCase):
    def run_cli(self, args, stdin=None, env=None):
        return subprocess.run(
            [sys.executable, SCRIPT, *args],
            input=stdin,
            capture_output=True,
            text=True,
            env={**os.environ, **(env or {})},
        )

    def test_exit_status(self):
        self.assertEqual(self.run_cli(["-"], stdin="Closes #12\n").returncode, 0)
        r = self.run_cli(["-"], stdin="Partially fixes #12\n")
        self.assertEqual(r.returncode, 1)
        self.assertIn("Refs #N", r.stdout)
        self.assertEqual(self.run_cli([]).returncode, 2)

    def test_env_and_file(self):
        self.assertEqual(self.run_cli(["--env", "B"], env={"B": "does not fix #3"}).returncode, 1)
        with tempfile.NamedTemporaryFile("w", suffix=".md", delete=False) as f:
            f.write("## Summary\n\nCloses #3\n")
        try:
            self.assertEqual(self.run_cli([f.name]).returncode, 0)
        finally:
            os.unlink(f.name)
        self.assertEqual(self.run_cli(["/no/such/body.md"]).returncode, 2)

    def test_git_range(self):
        with tempfile.TemporaryDirectory() as d:
            git = ["git", "-C", d, "-c", "user.name=t", "-c", "user.email=t@example.com"]
            subprocess.run([*git, "init", "-q"], check=True)
            subprocess.run([*git, "commit", "-q", "--allow-empty", "-m", "base"], check=True)
            subprocess.run([*git, "commit", "-q", "--allow-empty", "-m", "wip\n\nPartially fixes #9"], check=True)
            r = subprocess.run(
                [sys.executable, SCRIPT, "--git-range", "HEAD~1..HEAD"],
                cwd=d,
                capture_output=True,
                text=True,
            )
            self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
            self.assertIn("commit ", r.stdout)


if __name__ == "__main__":
    unittest.main()
