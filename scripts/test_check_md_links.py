#!/usr/bin/env python3
"""Tests for check_md_links.py: python3 scripts/test_check_md_links.py"""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import check_md_links as c  # noqa: E402


def run(files, only=None):
    """Check an in-memory repo: {path: text}. Returns the error list."""
    errors, _ = c.check(only, tracked=list(files), read=lambda p: files[p])
    return errors


class Slugs(unittest.TestCase):
    def test_issue_250_anchors(self):
        self.assertEqual(c.slugify("Building & running"), "building--running")
        self.assertEqual(
            c.slugify("Handwrite: string in, handwritten PNG out"),
            "handwrite-string-in-handwritten-png-out",
        )

    def test_backticks_and_links(self):
        self.assertEqual(c.slugify("The `pkwet` [brush](x.md)"), "the-pkwet-brush")

    def test_duplicates_get_suffixes(self):
        self.assertEqual(c.anchors_of("# A\n## A\n## A\n"), {"a", "a-1", "a-2"})

    def test_inline_code_in_a_heading_keeps_its_text(self):
        self.assertEqual(c.anchors_of("# Using `pscat`\n## `pkwet` and /Touch\n"), {"using-pscat", "pkwet-and-touch"})
        files = {"a.md": "[x](#using-pscat) [y](b.md#the-pkwet-brush)\n# Using `pscat`\n", "b.md": "## The `pkwet` brush\n"}
        self.assertEqual(run(files), [])

    def test_headings_in_code_fences_do_not_count(self):
        self.assertEqual(c.anchors_of("```\n# not a heading\n```\n# Real\n"), {"real"})


class Links(unittest.TestCase):
    def test_good_links(self):
        files = {
            "README.md": "[g](docs/g.md#hi) ![i](docs/i.png) <img src=\"docs/i.png\"> [d](docs/)",
            "docs/g.md": "# Hi\n[up](../README.md)\n",
            "docs/i.png": "",
        }
        self.assertEqual(run(files), [])

    def test_broken_file_fragment_and_case(self):
        files = {
            "README.md": "[a](nope.md)\n[b](docs/g.md#missing)\n[c](docs/G.md)\n",
            "docs/g.md": "# Hi\n",
        }
        errs = run(files)
        self.assertEqual(len(errs), 3, errs)
        self.assertTrue(errs[0].startswith("README.md:1:"))

    def test_code_is_not_links(self):
        files = {"README.md": "`[a](nope.md)`\n```\n[b](nope.md)\n```\n"}
        self.assertEqual(run(files), [])

    def test_same_file_fragment(self):
        self.assertEqual(run({"a.md": "# T\n[x](#t) [y](#u)\n"}), ["a.md:2: no heading '#u' in a.md (link: #u)"])

    def test_github_urls_resolve_against_checkout(self):
        base = "https://github.com/jeromebanks/postscript_interpreter"
        files = {
            "a.md": f"[ok]({base}/blob/main/b.md#L10) [bad]({base}/blob/main/gone.md) [ext](https://example.com/x)",
            "b.md": "",
        }
        errs = run(files)
        self.assertEqual(len(errs), 1, errs)
        self.assertIn("gone.md", errs[0])

    def test_skip_list(self):
        files = {"docs/archive/old.md": "[x](gone.md)", "notes/1-a.md": "[x](gone.md)"}
        self.assertEqual(run(files), [])

    def test_escaping_the_repo(self):
        self.assertEqual(len(run({"a.md": "[x](../../etc/passwd)"})), 1)


class SharedRepoRule(unittest.TestCase):
    """The GitHub-URL-to-path rule (scripts/repo_url.py, #429), pinned per caller.

    Each row is what the checkers answered before the rule was shared. The md
    checker takes http and https with ref main; the site checker takes https
    with any ref. Keep these rows unchanged: a difference is a behaviour change.
    """

    R = "https://github.com/jeromebanks/postscript_interpreter"

    # url tail (after the repo URL), md resolve(), site repo_path_for()
    ROWS = [
        ("/blob/main/docs/a.md", ("docs/a.md", ""), "docs/a.md"),
        ("/blob/main/docs/a.md#L10-L20", ("docs/a.md", "L10-L20"), "docs/a.md"),
        ("/blob/main/docs/a.md?plain=1", ("docs/a.md?plain=1", ""), "docs/a.md"),
        ("/blob/main/docs/a.md?x#frag", ("docs/a.md?x", "frag"), "docs/a.md"),
        ("/blob/main/docs/a%20b.md#top", ("docs/a b.md", "top"), "docs/a b.md"),
        ("/blob/main/docs/%C3%84.md", ("docs/\u00c4.md", ""), "docs/\u00c4.md"),
        ("/blob/main/", ("", ""), ""),
        ("/blob/main", ("", ""), None),
        ("/tree/main", ("", ""), None),
        ("/tree/main/", ("", ""), ""),
        ("/tree/main/docs/", ("docs", ""), "docs"),
        ("/blob/main/../x.md", ("../x.md", ""), "../x.md"),
        ("/blob/mainline/x.md", ("line/x.md", ""), "x.md"),  # latent quirk, kept
        ("/blob/v0.5.1/docs/a.md", (None, ""), "docs/a.md"),
        ("/blob/", (None, ""), None),
        ("/blob//docs/a.md", (None, ""), None),
        ("/blobmain/x.md", (None, ""), None),
        ("/raw/main/docs/a.md", (None, ""), None),
        ("/issues/47#issuecomment-1", (None, ""), None),
        ("", (None, ""), None),
    ]

    def test_md_resolve_per_row(self):
        for tail, md_expected, _ in self.ROWS:
            with self.subTest(tail=tail):
                self.assertEqual(c.resolve("docs/x.md", self.R + tail), md_expected)

    def test_site_repo_path_per_row(self):
        import check_site_links
        for tail, _, site_expected in self.ROWS:
            with self.subTest(tail=tail):
                self.assertEqual(check_site_links.repo_path_for(self.R + tail), site_expected)

    def test_scheme_and_host(self):
        import check_site_links
        http = self.R.replace("https", "http") + "/blob/main/docs/a.md"
        self.assertEqual(c.resolve("x.md", http), ("docs/a.md", ""))  # md takes http
        self.assertIsNone(check_site_links.repo_path_for(http))  # site takes https only
        other = "https://github.com/jeromebanks/postscript_interpreter-issue-429/blob/main/x.md"
        self.assertEqual(c.resolve("x.md", other), (None, ""))  # a different repo: external

    def test_match_returns_scheme_ref_and_tail(self):
        from repo_url import match
        self.assertEqual(match(self.R + "/blob/v0.5.1/a.md#x"), ("https", "v0.5.1", "a.md#x"))
        self.assertEqual(match(self.R + "/tree/main/d/", ref="main"), ("https", "main", "d/"))
        self.assertIsNone(match(self.R + "/tree/v1/d/", ref="main"))
        self.assertIsNone(match("https://example.com/x"))


if __name__ == "__main__":
    unittest.main()
