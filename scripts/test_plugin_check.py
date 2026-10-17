#!/usr/bin/env python3
"""Tests for plugin_check.py: python3 scripts/test_plugin_check.py"""
import json
import os
import shutil
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import plugin_check as pc  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


class Fixture(unittest.TestCase):
    """A minimal good tree: Cargo.toml, a marketplace, plugin/ with a skills symlink."""

    def setUp(self):
        self.root = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.root)
        self.write("Cargo.toml", '[package]\nname = "x"\nversion = "1.2.3"\n')
        self.write("skills/demo/SKILL.md", "---\nname: demo\ndescription: d\n---\nbody\n")
        self.market = {
            "name": "m",
            "owner": {"name": "o"},
            "plugins": [{"name": "p", "source": "./plugin"}],
        }
        self.plugin = {
            "name": "p",
            "version": "1.2.3",
            "mcpServers": {"s": {"command": "pscat-mcp"}},
        }
        os.makedirs(os.path.join(self.root, "plugin"))
        os.symlink("../skills", os.path.join(self.root, "plugin", "skills"))
        self.save()

    def write(self, rel, text):
        path = os.path.join(self.root, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write(text)

    def save(self):
        self.write(".claude-plugin/marketplace.json", json.dumps(self.market))
        self.write("plugin/.claude-plugin/plugin.json", json.dumps(self.plugin))

    def errors(self):
        self.save()
        return pc.check(self.root)

    def test_good_tree_is_clean(self):
        self.assertEqual(self.errors(), [])

    def test_version_must_match_cargo(self):
        self.plugin["version"] = "1.2.4"
        self.assertIn("version", " ".join(self.errors()))

    def test_marketplace_entry_version_must_not_disagree(self):
        self.market["plugins"][0]["version"] = "0.0.1"
        self.assertIn("leave it out", " ".join(self.errors()))

    def test_source_must_stay_in_repo(self):
        self.market["plugins"][0]["source"] = "../elsewhere"
        self.assertIn("inside the repo", " ".join(self.errors()))

    def test_dangling_symlink(self):
        os.remove(os.path.join(self.root, "plugin", "skills"))
        os.symlink("../nope", os.path.join(self.root, "plugin", "skills"))
        self.assertIn("symlink", " ".join(self.errors()))

    def test_skill_name_must_match_directory(self):
        self.write("skills/demo/SKILL.md", "---\nname: other\ndescription: d\n---\n")
        self.assertIn("name 'other'", " ".join(self.errors()))

    def test_mcp_command_must_be_bare(self):
        self.plugin["mcpServers"]["s"]["command"] = "./target/release/pscat-mcp"
        self.assertIn("bare executable", " ".join(self.errors()))

    def test_mcp_args_must_not_name_repo_paths(self):
        self.plugin["mcpServers"]["s"]["args"] = ["--out-root", "./out"]
        self.assertIn("path", " ".join(self.errors()))
        self.plugin["mcpServers"]["s"]["args"] = ["${CLAUDE_PLUGIN_ROOT}/x"]
        self.assertEqual(self.errors(), [])

    def test_component_path_must_exist(self):
        self.plugin["commands"] = "./commands"
        self.assertIn("missing", " ".join(self.errors()))


class RealRepo(unittest.TestCase):
    def test_repo_manifests_are_clean(self):
        self.assertEqual(pc.check(ROOT), [])


if __name__ == "__main__":
    unittest.main()
