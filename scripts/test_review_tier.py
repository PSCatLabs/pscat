#!/usr/bin/env python3
"""Tests for scripts/review_tier.py."""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import review_tier  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
FRONT = """---
repo: x
independent_review:
  cross_family_paths: ["/src/", "/npm/*.js", "/Cargo.lock"]
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

    def test_ignores_the_key_outside_the_block_and_the_frontmatter(self):
        text = FRONT.replace("independent_review:", "other_block:")
        self.assertEqual(review_tier.patterns(text), [])

    def test_no_frontmatter_means_no_patterns(self):
        self.assertEqual(review_tier.patterns("# SDLC\n"), [])

    def test_a_non_list_is_an_error(self):
        with self.assertRaises(ValueError):
            review_tier.patterns(FRONT.replace('["/src/", "/npm/*.js", "/Cargo.lock"]', '"/src/"'))

    def test_the_repo_sdlc_has_a_list(self):
        with open(os.path.join(HERE, "..", "SDLC.md"), encoding="utf-8") as f:
            self.assertIn("/src/", review_tier.patterns(f.read()))


class Matches(unittest.TestCase):
    sets = [["/src/", "/npm/*.js", "/Cargo.lock"]]

    def test_engine_and_runtime_paths_need_cross_family(self):
        paths = ["src/interp.rs", "src/subcmd/handwrite.rs", "npm/index.js", "Cargo.lock"]
        self.assertEqual(review_tier.matches(self.sets, paths), paths)

    def test_docs_libraries_and_npm_tests_do_not(self):
        paths = ["docs/guides/library.md", "lib/functional.ps", "npm/test/api.test.mjs",
                 "npm/README.md", "tests/src/x.rs"]
        self.assertEqual(review_tier.matches(self.sets, paths), [])

    def test_either_copy_of_the_list_counts(self):
        sets = [["/src/"], ["/lib/"]]
        self.assertEqual(review_tier.matches(sets, ["lib/warp.ps", "src/a.rs", "docs/x.md"]),
                         ["lib/warp.ps", "src/a.rs"])


if __name__ == "__main__":
    unittest.main()
