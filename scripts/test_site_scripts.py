#!/usr/bin/env python3
"""Self-test for scripts/site_pages.py and scripts/check_site_links.py.

    python3 scripts/test_site_scripts.py

Builds a tiny site in a temp dir, checks it passes, then breaks it one way at
a time and checks the right complaint appears.
"""
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent
PARTIALS = SCRIPTS.parent / "site" / "_partials"
sys.path.insert(0, str(SCRIPTS))
import check_site_links  # noqa: E402
import site_examples  # noqa: E402
import site_pages  # noqa: E402

PAGE = """<!DOCTYPE html><html><head><!--@head--><title>t</title></head><body>
<!--@header %s-->%s<!--@footer--></body></html>"""


def build(tmp, pages, extra_files=("style.css", "pscat.js", "pscat.wasm")):
    """Write pages ({name: (nav_key, body)}), apply the shell, return the site dir."""
    site = Path(tmp) / "_site"
    site.mkdir()
    for f in extra_files:
        (site / f).write_text("")
    for name, (key, body) in pages.items():
        (site / name).write_text(PAGE % (key, body))
    partials = site_pages.load_partials(PARTIALS)
    for f in sorted(site.glob("*.html")):
        f.write_text(site_pages.apply_shell(f.read_text(), partials, f.name))
    return site


# every nav destination must exist, so a fixture has all nine slots' pages
NAV = {"index.html": "home", "learn.html": "learn", "gallery.html": "gallery",
       "fonts.html": "fonts", "playground.html": "playground", "guides.html": "guides",
       "agents.html": "agents", "contribute.html": "contribute"}


def full(**over):
    pages = {n: (k, "<main><h1>%s</h1></main>" % n) for n, k in NAV.items()}
    pages.update(over)
    return pages


def run_check(site, repo):
    return check_site_links.check(site, repo)[0]


class SiteScripts(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.repo = Path(self.tmp.name) / "repo"
        (self.repo / "docs").mkdir(parents=True)
        (self.repo / "docs" / "a.md").write_text("x")
        self.addCleanup(self.tmp.cleanup)

    def test_clean_site_passes(self):
        site = build(self.tmp.name, full())
        self.assertEqual(run_check(site, self.repo), [])

    def test_broken_relative_link(self):
        site = build(self.tmp.name, full(**{"guides.html": ("guides", '<a href="nope.html">x</a>')}))
        self.assertTrue(any("broken a link: nope.html" in p for p in run_check(site, self.repo)))

    def test_missing_fragment(self):
        site = build(self.tmp.name, full(**{"guides.html": ("guides", '<a href="learn.html#gone">x</a>')}))
        self.assertTrue(any("no #gone" in p for p in run_check(site, self.repo)))

    def test_github_link_checked_against_checkout(self):
        ok = '<a href="%s/blob/main/docs/a.md#top">ok</a>' % check_site_links.REPO_URL
        bad = '<a href="%s/blob/main/docs/moved.md">bad</a>' % check_site_links.REPO_URL
        site = build(self.tmp.name, full(**{"guides.html": ("guides", ok + bad)}))
        probs = run_check(site, self.repo)
        self.assertEqual(len(probs), 1)
        self.assertIn("moved.md", probs[0])

    def test_link_case_must_match_exactly(self):
        # passes on a case-insensitive file system unless the check is exact
        bad = '<a href="Learn.html">x</a><a href="%s/blob/main/DOCS/a.md">y</a>' % check_site_links.REPO_URL
        site = build(self.tmp.name, full(**{"guides.html": ("guides", bad)}))
        probs = run_check(site, self.repo)
        self.assertTrue(any("Learn.html" in p for p in probs), probs)
        self.assertTrue(any("DOCS/a.md" in p for p in probs), probs)

    def test_orphan_page(self):
        pages = full(**{"lost.html": ("guides", "<main>lost</main>")})
        site = build(self.tmp.name, pages)
        self.assertTrue(any("lost.html: orphan" in p for p in run_check(site, self.repo)))

    def test_playground_option_without_file(self):
        body = '<select><option value="gone.ps">g</option></select>'
        site = build(self.tmp.name, full(**{"playground.html": ("playground", body)}))
        self.assertTrue(any("gone.ps" in p for p in run_check(site, self.repo)))

    def test_try_it_link_without_example(self):
        body = ('<a href="playground.html?example=learn/a.ps">ok</a>'
                '<a href="playground.html?example=learn/gone.ps">bad</a>')
        site = build(self.tmp.name, full(**{"learn.html": ("learn", body)}))
        (site / "examples" / "learn").mkdir(parents=True)
        (site / "examples" / "learn" / "a.ps").write_text("1 ==")
        probs = run_check(site, self.repo)
        self.assertEqual(len(probs), 1, probs)
        self.assertIn("learn/gone.ps", probs[0])

    def test_example_markers_expand(self):
        ex = Path(self.tmp.name) / "ex"
        ex.mkdir()
        (ex / "a.ps").write_text("%!PS\n1 2 add ==\n")
        (ex / "a.out").write_text("3\n")
        (ex / "b.ps").write_text("%!PS\n%%BoundingBox: 0 0 30 20\n0 0 5 5 rectfill showpage\n")
        site = Path(self.tmp.name) / "s"
        (site / "assets" / "learn").mkdir(parents=True)
        (site / "assets" / "learn" / "b.png").write_bytes(b"")
        used = set()
        out = site_examples.expand('<!--@example a--><!--@example b "a square"-->', ex, site, used)
        self.assertEqual(used, {"a", "b"})
        self.assertIn('href="playground.html?example=learn/a.ps"', out)
        self.assertIn("1 2 add ==", out)
        self.assertIn('<pre class="output"><code>3</code></pre>', out)
        self.assertIn('<img src="assets/learn/b.png" alt="a square" width="30" height="20"', out)
        # a drawing needs alt text; an unknown example fails
        with self.assertRaises(ValueError):
            site_examples.expand("<!--@example b-->", ex, site, set())
        with self.assertRaises(ValueError):
            site_examples.expand("<!--@example zz-->", ex, site, set())

    def test_missing_nav_marker_fails_the_build(self):
        with self.assertRaises(ValueError):
            site_pages.apply_shell("<html><head><!--@head--></head><body></body></html>",
                                   site_pages.load_partials(PARTIALS), "x.html")

    def test_unknown_nav_key_fails_the_build(self):
        with self.assertRaises(ValueError):
            site_pages.apply_shell(PAGE % ("nowhere", ""), site_pages.load_partials(PARTIALS), "x.html")

    def test_cli_exit_codes(self):
        site = build(self.tmp.name, full())
        cmd = [sys.executable, str(SCRIPTS / "check_site_links.py"), str(site), "--repo", str(self.repo)]
        self.assertEqual(subprocess.run(cmd, capture_output=True).returncode, 0)
        (site / "guides.html").write_text((site / "guides.html").read_text().replace(
            "</main>", '<a href="nope.html">x</a></main>'))
        self.assertEqual(subprocess.run(cmd, capture_output=True).returncode, 1)


if __name__ == "__main__":
    unittest.main()
