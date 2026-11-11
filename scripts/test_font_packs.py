#!/usr/bin/env python3
"""Tests for font_packs.py: python3 scripts/test_font_packs.py"""
import io
import json
import os
import shutil
import sys
import tarfile
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import font_packs as fp  # noqa: E402


class Globs(unittest.TestCase):
    def test_assets_json_glob_semantics(self):
        self.assertTrue(fp.glob_match("fonts/base35/URW-symbols/**", "fonts/base35/URW-symbols/a.otf"))
        self.assertTrue(fp.glob_match("fonts/Liberation*.ttf", "fonts/LiberationSans-Bold.ttf"))
        self.assertFalse(fp.glob_match("fonts/*.ttf", "fonts/catalog/x.ttf"))
        self.assertFalse(fp.glob_match("fonts/catalog/A/**", "fonts/catalog/AB/x.ttf"))


class Repo(unittest.TestCase):
    def test_the_checked_in_table_passes(self):
        self.assertEqual(fp.check(fp.load_json("fonts/packs.json"), fp.load_json("licenses/assets.json")), [])

    def test_packs_carry_the_audited_licence_files_only(self):
        files, errors = fp.pack_files(fp.load_json("fonts/packs.json"), fp.load_json("licenses/assets.json"))
        self.assertEqual(errors, [])
        # Compiled into pscat since issue #757: no pack carries them.
        every = [a for files_ in files.values() for a, _, _, _ in files_]
        self.assertFalse([a for a in every if a.startswith(("URW-symbols/", "TeXGyre-"))])
        self.assertNotIn("texgyre", files)
        elite = {a: e["license"] for a, _, _, e in files["extras"] if a.startswith("SpecialElite/")}
        self.assertEqual(set(elite.values()), {"Apache-2.0"})
        self.assertIn("SpecialElite/LICENSE.txt", elite)

    def test_a_family_missing_from_the_table_is_reported(self):
        packs = fp.load_json("fonts/packs.json")
        del packs["packs"][0]["families"]["Bangers"]
        errors = fp.check(packs, fp.load_json("licenses/assets.json"))
        self.assertTrue(any(e.startswith("Bangers:") for e in errors), errors)


class Tar(unittest.TestCase):
    def test_deterministic_and_plain(self):
        a = fp.deterministic_tar([("B/b.ttf", b"bb"), ("A/a.ttf", b"a")])
        b = fp.deterministic_tar([("A/a.ttf", b"a"), ("B/b.ttf", b"bb")])
        self.assertEqual(a, b)
        with tarfile.open(fileobj=io.BytesIO(a)) as t:
            members = t.getmembers()
        self.assertEqual([m.name for m in members], ["A/a.ttf", "B/b.ttf"])
        self.assertTrue(all(m.isreg() and m.mtime == 0 and m.uid == 0 for m in members))


class Pin(unittest.TestCase):
    def test_pin_rewrites_the_constant_and_is_repeatable(self):
        d = tempfile.mkdtemp()
        try:
            rs = os.path.join(d, "pinned.rs")
            shutil.copy(os.path.join(fp.ROOT, fp.PINNED_RS), rs)
            index = os.path.join(d, "font-index.json")
            with open(index, "w") as f:
                json.dump({"index_schema": 1, "tag": "fonts-2026.10", "packs": []}, f)
            fp.pin(index, rs)
            fp.pin(index, rs)
            src = open(rs).read()
            self.assertEqual(src.count("pub const PINNED_INDEX"), 1)
            self.assertIn('tag: "fonts-2026.10"', src)
            with open(index, "rb") as f:
                self.assertIn(fp.sha256(f.read()), src)
            with open(index, "w") as f:
                json.dump({"tag": "v0.6.0"}, f)
            with self.assertRaises(SystemExit):
                fp.pin(index, rs)
        finally:
            shutil.rmtree(d)

    def test_tags(self):
        for ok in ["fonts-2026.10", "fonts-2026.10.3"]:
            self.assertTrue(fp.TAG_RE.match(ok))
        for bad in ["v0.6.0", "fonts-2026.1", "fonts-2026.10.", "fonts-latest"]:
            self.assertFalse(fp.TAG_RE.match(bad))


if __name__ == "__main__":
    unittest.main()
