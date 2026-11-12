#!/usr/bin/env python3
"""Tests for scripts/review_tier.py."""
import contextlib
import io
import os
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import review_tier  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
LIST = '["/src/", "/npm/*.js", "/Cargo.lock"]'
FRONT = f"""---
repo: x
independent_review:
  cross_family_paths: {LIST}
  max_rounds: 2
light_gate:
  paths: ["docs/*"]
---
# body
cross_family_paths: ["/docs/"]
"""


class Patterns(unittest.TestCase):
    def test_reads_the_list_inside_the_block(self):
        self.assertEqual(review_tier.patterns(FRONT), ["/src/", "/npm/*.js", "/Cargo.lock"])

    def test_no_block_or_no_frontmatter_is_none(self):
        self.assertIsNone(review_tier.patterns(FRONT.replace("independent_review:", "other:")))
        self.assertIsNone(review_tier.patterns("# SDLC\n"))

    def test_blank_lines_comments_bom_and_trailing_comment_are_fine(self):
        text = "﻿" + FRONT.replace(
            "independent_review:\n",
            "independent_review:   # who reviews\n\n# a note\n",
        ).replace(LIST, LIST + "  # one line")
        self.assertEqual(review_tier.patterns(text), ["/src/", "/npm/*.js", "/Cargo.lock"])

    def test_a_deeper_key_does_not_count(self):
        text = FRONT.replace(f"  cross_family_paths: {LIST}", f"  other:\n    cross_family_paths: {LIST}")
        with self.assertRaises(ValueError):  # the block exists but has no list: fail closed
            review_tier.patterns(text)

    def test_bad_values_fail_closed(self):
        for bad in ['"/src/"', "[]", "[1]", '["/src/"] junk', "[unclosed"]:
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                review_tier.patterns(FRONT.replace(LIST, bad))

    def test_the_repo_list_has_no_typos(self):
        with open(os.path.join(HERE, "..", "SDLC.md"), encoding="utf-8") as f:
            listed = review_tier.patterns(f.read())
        self.assertIn("/src/", listed)
        tracked = subprocess.run(["git", "-C", HERE, "ls-files", "--full-name", ".."], capture_output=True,
                                 text=True, check=True).stdout.split()
        self.assertEqual(review_tier.unmatched(listed, tracked), [])

    def test_the_tier_scripts_are_listed_and_owner_merged(self):
        with open(os.path.join(HERE, "..", "SDLC.md"), encoding="utf-8") as f:
            listed = review_tier.patterns(f.read())
        with open(os.path.join(HERE, "..", ".github", "CODEOWNERS"), encoding="utf-8") as f:
            owners = f.read()
        for script in ("/scripts/review_tier.py", "/scripts/codeowners_check.py"):
            self.assertIn(script, listed)
            self.assertIn(script, owners)


class Matches(unittest.TestCase):
    sets = [["/src/", "/npm/*.js", "/Cargo.lock"]]

    def test_engine_and_runtime_paths_need_cross_family(self):
        paths = ["src/interp.rs", "src/subcmd/handwrite.rs", "npm/index.js", "Cargo.lock"]
        self.assertEqual(review_tier.matches(self.sets, paths), paths)

    def test_docs_libraries_and_npm_tests_do_not(self):
        paths = ["docs/guides/library.md", "lib/functional.ps", "npm/test/api.test.mjs",
                 "npm/README.md", "tests/src/x.rs", "fuzz/Cargo.lock"]
        self.assertEqual(review_tier.matches(self.sets, paths), [])

    def test_either_copy_of_the_list_counts(self):
        sets = [["/src/"], ["/lib/"]]
        self.assertEqual(review_tier.matches(sets, ["lib/warp.ps", "src/a.rs", "docs/x.md"]),
                         ["lib/warp.ps", "src/a.rs"])


def run_main(args, cwd):
    out, err = io.StringIO(), io.StringIO()
    old = os.getcwd()
    os.chdir(cwd)
    try:
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = review_tier.main(["review_tier.py", *args])
    finally:
        os.chdir(old)
    return code, out.getvalue()


class Main(unittest.TestCase):
    """main() in a throwaway repository."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.repo = self.tmp.name
        self.git("init", "-q", "-b", "main")
        self.git("config", "user.email", "t@example.com")
        self.git("config", "user.name", "t")
        self.write("SDLC.md", FRONT)
        self.write("src/a.rs", "a")
        self.write("docs/x.md", "x")
        self.git("add", "-A")
        self.git("commit", "-qm", "base")
        self.git("checkout", "-qb", "pr")

    def tearDown(self):
        self.tmp.cleanup()

    def git(self, *args):
        subprocess.run(["git", "-C", self.repo, *args], check=True, capture_output=True)

    def write(self, rel, text):
        path = os.path.join(self.repo, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as f:
            f.write(text)

    def commit(self, msg="change"):
        self.git("add", "-A")
        self.git("commit", "-qm", msg)

    def test_docs_only_pr_is_agent(self):
        self.write("docs/x.md", "y")
        self.commit()
        self.assertEqual(run_main(["--base", "main"], self.repo), (0, "agent\n"))

    def test_src_pr_is_cross_family_even_from_a_subdirectory(self):
        self.write("src/a.rs", "b")
        self.commit()
        self.assertEqual(run_main(["--base", "main"], os.path.join(self.repo, "docs")),
                         (0, "cross-family\n  src/a.rs\n"))
        self.assertEqual(run_main(["src/a.rs"], os.path.join(self.repo, "docs")),
                         (0, "cross-family\n  src/a.rs\n"))

    def test_emptying_the_list_in_the_pr_does_not_lower_the_tier(self):
        self.write("SDLC.md", FRONT.replace(LIST, '["/nothing/"]'))
        self.git("mv", "src/a.rs", "docs/a.rs")
        self.commit()
        code, out = run_main(["--base", "main"], self.repo)
        self.assertEqual(code, 0)
        self.assertTrue(out.startswith("cross-family\n"))
        self.assertIn("  src/a.rs\n", out)

    def test_no_list_anywhere_fails_closed(self):
        self.write("SDLC.md", "# no frontmatter\n")
        self.assertEqual(run_main(["src/a.rs"], self.repo), (2, ""))
        self.assertEqual(run_main(["docs/x.md"], self.repo), (2, ""))

    def test_usage_and_bad_base_are_errors(self):
        self.assertEqual(run_main([], self.repo)[0], 2)
        self.assertEqual(run_main(["--base", "no-such-ref"], self.repo)[0], 2)

    def test_check_paths(self):
        self.assertEqual(run_main(["--check-paths"], self.repo)[0], 1)  # /npm/*.js, /Cargo.lock
        self.write("SDLC.md", FRONT.replace(LIST, '["/src/"]'))
        self.assertEqual(run_main(["--check-paths"], self.repo), (0, ""))


if __name__ == "__main__":
    unittest.main()
