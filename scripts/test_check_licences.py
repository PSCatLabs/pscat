#!/usr/bin/env python3
"""Unit tests for scripts/check_licences.py (issue #601). Run: python3 scripts/test_check_licences.py"""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import check_licences as c  # noqa: E402


def entries(*specs):
    return [(e, [c.glob_to_regex(p) for p in e["paths"]]) for e in specs]


CODE = {"id": "code", "paths": ["**"], "ships_in": ["crate"], "license_files": []}
FONT = {"id": "font-A", "paths": ["fonts/catalog/A/**"], "ships_in": ["font-pack"]}
LIB = {"id": "lib", "paths": ["lib/typewriter_outlines.ps"], "ships_in": ["crate"]}


class Glob(unittest.TestCase):
    def test_star_stays_in_one_directory(self):
        r = c.glob_to_regex("gallery/*.ps")
        self.assertTrue(r.match("gallery/a.ps"))
        self.assertFalse(r.match("gallery/sub/a.ps"))

    def test_double_star_crosses_directories(self):
        r = c.glob_to_regex("fonts/catalog/A/**")
        self.assertTrue(r.match("fonts/catalog/A/OFL.txt"))
        self.assertTrue(r.match("fonts/catalog/A/sub/x.ttf"))
        self.assertFalse(r.match("fonts/catalog/B/x.ttf"))
        self.assertTrue(c.glob_to_regex("**").match("anything/at/all"))

    def test_prefix_star(self):
        r = c.glob_to_regex("fonts/Liberation*.ttf")
        self.assertTrue(r.match("fonts/LiberationSans-Bold.ttf"))
        self.assertFalse(r.match("fonts/Other.ttf"))


class Assets(unittest.TestCase):
    def test_new_font_without_entry_fails(self):
        files = ["fonts/catalog/A/OFL.txt", "fonts/catalog/A/A.ttf", "fonts/catalog/B/B.ttf"]
        probs = c.check_assets(files, entries(FONT, CODE))
        self.assertEqual(len(probs), 1)
        self.assertIn("fonts/catalog/B/B.ttf", probs[0])

    def test_own_render_under_notes_is_fine(self):
        self.assertEqual(c.check_assets(["notes/1-x.png"], entries(CODE)), [])

    def test_refs_directory_needs_an_entry(self):
        probs = c.check_assets(["docs/dev/design/x/refs/a.jpg"], entries(CODE))
        self.assertEqual(len(probs), 1)

    def test_stale_entry_fails(self):
        probs = c.check_assets(["src/a.rs"], entries(FONT, CODE))
        self.assertTrue(any("font-A" in p and "no tracked file" in p for p in probs))

    def test_missing_licence_file_fails(self):
        e = dict(LIB, license_files=["LICENSE-OFL"])
        probs = c.check_assets(["lib/typewriter_outlines.ps"], entries(e, CODE))
        self.assertTrue(any("LICENSE-OFL" in p for p in probs))

    def test_missing_catch_all_fails(self):
        self.assertTrue(c.check_assets(["lib/typewriter_outlines.ps"], entries(LIB)))


class Fonts(unittest.TestCase):
    def test_licensed_dir_passes(self):
        files = ["fonts/catalog/A/A.ttf", "fonts/catalog/A/OFL.txt",
                 "fonts/catalog/B/B.otf", "fonts/catalog/B/GUST-FONT-LICENSE.txt",
                 "fonts/catalog/specimen-a.png"]
        self.assertEqual(c.check_fonts(files), [])

    def test_unlicensed_dir_fails(self):
        probs = c.check_fonts(["fonts/catalog/A/A.ttf", "fonts/catalog/A/README.md"])
        self.assertEqual(len(probs), 1)
        self.assertIn("fonts/catalog/A/", probs[0])

    def test_licence_must_sit_next_to_the_font(self):
        self.assertTrue(c.check_fonts(["fonts/catalog/A/A.ttf", "fonts/catalog/A/sub/OFL.txt"]))


class Package(unittest.TestCase):
    OK = list(c.REQUIRED_IN_CRATE) + ["Cargo.toml", "Cargo.lock", "Cargo.toml.orig",
                                      ".cargo_vcs_info.json", "src/main.rs", "lib/typewriter_outlines.ps"]
    ENT = entries(LIB, CODE)

    def test_clean_listing_passes(self):
        self.assertEqual(c.check_package(self.OK, self.ENT, []), [])

    def test_missing_licence_file_fails(self):
        probs = c.check_package([p for p in self.OK if p != "LICENSE-OFL"], self.ENT, [])
        self.assertTrue(any("LICENSE-OFL" in p for p in probs))

    def test_catalog_font_in_package_fails(self):
        probs = c.check_package(self.OK + ["fonts/catalog/A/A.ttf"], self.ENT, [])
        self.assertTrue(any("fonts/catalog/A/A.ttf" in p for p in probs))

    def test_asset_that_does_not_ship_in_crate_fails(self):
        ents = entries(dict(id="img", paths=["art/**"], ships_in=["repo-only"]), CODE)
        probs = c.check_package(self.OK + ["art/x.svg"], ents, [])
        self.assertTrue(any("art/x.svg" in p for p in probs))

    def test_windows_separators_are_normalised(self):
        listing = [p.replace("/", "\\") for p in self.OK]
        self.assertEqual(c.check_package(listing, self.ENT, []), [])


class RealRepo(unittest.TestCase):
    """The checked-in assets.json and the tracked tree pass their own checks."""

    def test_repo_is_clean(self):
        files = c.tracked_files()
        ents = c.load_entries()
        self.assertEqual(c.check_assets(files, ents), [])
        self.assertEqual(c.check_fonts(files), [])


if __name__ == "__main__":
    unittest.main()
