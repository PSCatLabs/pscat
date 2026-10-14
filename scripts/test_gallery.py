#!/usr/bin/env python3
"""Self-test for scripts/gallery.py (#251).

    python3 scripts/test_gallery.py

Runs against the real manifest and checkout: the clean manifest must pass, and
then each rule is broken in memory, one at a time, and must be reported.
"""
import copy
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS))
import check_site_links  # noqa: E402
import gallery  # noqa: E402
import site_pages  # noqa: E402


def piece(m, pid):
    return next(p for p in m["pieces"] if p["id"] == pid)


class GalleryManifest(unittest.TestCase):
    def setUp(self):
        self.m = gallery.load()

    def errs(self, m=None):
        return gallery.check(m or self.m)

    def has(self, text, m=None):
        errs = self.errs(m)
        self.assertTrue(any(text in e for e in errs), "%r not in %r" % (text, errs))

    def test_real_manifest_is_clean(self):
        self.assertEqual(self.errs(), [])

    def test_tracked_piece_without_entry(self):
        self.m["pieces"] = [p for p in self.m["pieces"] if p["id"] != "fern"]
        self.has("gallery/fern.ps has no entry")
        self.has("gallery/renders/fern.png is a render that no manifest entry uses")

    def test_missing_render(self):
        piece(self.m, "fern")["render"] = "gallery/renders/nope.png"
        self.has("render gallery/renders/nope.png does not exist")

    def test_missing_reference(self):
        piece(self.m, "mochi_denim_blue")["reference"]["file"] = "gallery/references/gone.jpg"
        self.has("reference gallery/references/gone.jpg does not exist")

    def test_unknown_category_and_library(self):
        piece(self.m, "fern")["category"] = "nope"
        piece(self.m, "alpine_lake")["libraries"].append("nolib")
        self.has("unknown category 'nope'")
        self.has("library 'nolib' is not in the manifest")

    def test_libraries_must_match_the_source(self):
        piece(self.m, "alpine_lake")["libraries"] = ["artkit"]
        self.has("libraries should be ['artkit', 'paintkit']")

    def test_page_must_match_bounding_box(self):
        piece(self.m, "fern")["page"] = "100x100"
        self.has("disagrees with the source's %%BoundingBox 620x800")

    def test_playground_needs_a_self_contained_source(self):
        piece(self.m, "alpine_lake")["playground"] = True
        self.has("marked playground but the source reads a file")

    def test_playground_names_must_not_collide(self):
        s = copy.deepcopy(next(s for s in self.m["specimens"] if s["id"] == "sierpinski"))
        s["id"] = "sierpinski_again"
        self.m["specimens"].append(s)
        self.has("collide in _site/examples/")

    def test_duplicate_id(self):
        self.m["pieces"].append(copy.deepcopy(self.m["pieces"][0]))
        self.has("is used twice")

    def test_stage_era_wording_is_rejected(self):
        piece(self.m, "fern")["blurb"] = "Drawn in Stage 3 with no arrays."
        piece(self.m, "lodestone")["technique"] = "Built for issue #19."
        self.has("stage or issue wording")

    def test_specimen_needs_a_render_or_a_source_and_size(self):
        self.m["specimens"][0].pop("render")
        self.m["specimens"][0].pop("source")
        self.has("needs a committed render")

    def test_readme_regions_must_be_in_sync(self):
        piece(self.m, "fern")["featured"] = True
        self.has("README.md is out of date with the manifest")

    def test_untracked_files_are_ignored(self):
        # the maintainer's work in progress must not fail the check
        self.assertNotIn("impasto_autumn", " ".join(self.errs()))

    def test_generated_pages_get_the_shell_and_pass_the_link_check(self):
        with tempfile.TemporaryDirectory() as tmp:
            site = Path(tmp) / "_site"
            site.mkdir()
            gallery.build(self.m, site)
            gallery.stage(self.m, site)
            for s in self.m["specimens"]:  # build_site.sh renders these with pscat
                if not s.get("render"):
                    (site / "assets/renders" / (s["id"] + ".png")).write_bytes(b"")
            pages = sorted(f.name for f in site.glob("*.html"))
            self.assertEqual(len(pages), 1 + len(self.m["pieces"]))
            partials = site_pages.load_partials(SCRIPTS.parent / "site" / "_partials")
            for f in site.glob("*.html"):
                out = site_pages.apply_shell(f.read_text(encoding="utf-8"), partials, f.name)
                self.assertIn('aria-current="page"', out)
                f.write_text(out, encoding="utf-8")
            # the rest of the site, minimal: enough for the gallery's own links to resolve
            for name, key in (("index.html", "home"), ("learn.html", "learn"), ("fonts.html", "fonts"),
                              ("playground.html", "playground"), ("guides.html", "guides"),
                              ("agents.html", "agents"), ("contribute.html", "contribute")):
                body = '<!DOCTYPE html><html><head><!--@head--></head><body><!--@header %s--><main>' % key
                if name == "index.html":
                    body += '<a href="gallery.html">g</a>' + "".join(
                        '<a href="%s">%s</a>' % (n, n) for n in
                        ("learn.html", "fonts.html", "playground.html", "guides.html", "agents.html",
                         "contribute.html"))
                body += "</main></body></html>"
                (site / name).write_text(site_pages.apply_shell(body, partials, name), encoding="utf-8")
            for f in ("style.css", "pscat.js", "pscat.wasm"):
                (site / f).write_text("")
            problems, _ = check_site_links.check(site, SCRIPTS.parent)
            self.assertEqual(problems, [])


if __name__ == "__main__":
    unittest.main()
