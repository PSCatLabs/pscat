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
    """A minimal good tree: Cargo.toml, both marketplaces, plugin/ with real skills."""

    def setUp(self):
        self.root = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.root)
        self.write("Cargo.toml", '[package]\nname = "x"\nversion = "1.2.3"\n')
        self.write("plugin/skills/demo/SKILL.md", "---\nname: demo\ndescription: d\n---\nbody\n")
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
        self.codex_market = {
            "name": "m",
            "plugins": [{
                "name": "p",
                "source": {"source": "local", "path": "./plugin"},
                "policy": {"installation": "AVAILABLE", "authentication": "ON_INSTALL"},
                "category": "Design",
            }],
        }
        self.codex_plugin = {"name": "p", "version": "1.2.3"}
        self.mcp = {"mcpServers": {"s": {"type": "stdio", "command": "pscat-mcp"}}}
        self.save()

    def write(self, rel, text):
        path = os.path.join(self.root, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write(text)

    def save(self):
        self.write(".claude-plugin/marketplace.json", json.dumps(self.market))
        self.write("plugin/.claude-plugin/plugin.json", json.dumps(self.plugin))
        self.write(".agents/plugins/marketplace.json", json.dumps(self.codex_market))
        self.write("plugin/plugin.json", json.dumps(self.codex_plugin))
        self.write("plugin/mcp.json", json.dumps(self.mcp))

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

    def test_symlink_in_plugin(self):
        os.rename(os.path.join(self.root, "plugin", "skills"), os.path.join(self.root, "skills"))
        os.symlink("../skills", os.path.join(self.root, "plugin", "skills"))
        self.assertIn("symlink in a plugin directory", " ".join(self.errors()))

    def test_skill_name_must_match_directory(self):
        self.write("plugin/skills/demo/SKILL.md", "---\nname: other\ndescription: d\n---\n")
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

    def test_hooks_as_path(self):
        self.plugin["hooks"] = "./hooks.json"
        self.assertIn("hooks path", " ".join(self.errors()))
        self.write("plugin/hooks.json", json.dumps({"hooks": {"SessionStart": [
            {"hooks": [{"type": "command", "command": "/usr/local/bin/x"}]}]}}))
        self.assertIn("hooks.SessionStart", " ".join(self.errors()))

    def test_hook_command_paths(self):
        self.plugin["hooks"] = {"SessionStart": [
            {"hooks": [{"type": "command", "command": "sh ../scripts/x.sh"}]}]}
        self.assertIn("hooks.SessionStart", " ".join(self.errors()))


    def test_default_hooks_file(self):
        self.write("plugin/hooks/hooks.json", json.dumps({"hooks": {"SessionStart": [
            {"hooks": [{"type": "command", "command": "sh ../x.sh"}]}]}}))
        self.assertIn("hooks.SessionStart", " ".join(self.errors()))
        self.write("plugin/hooks/hooks.json", json.dumps({"hooks": {"SessionStart": [
            {"hooks": [{"type": "command", "command": "sh ${PLUGIN_ROOT}/x.sh"}]}]}}))
        self.assertIn("'x.sh' is not a file", " ".join(self.errors()))
        self.write("plugin/x.sh", "#!/bin/sh\n")
        self.assertEqual(self.errors(), [])
        self.write("plugin/hooks/hooks.json", json.dumps({"hooks": {"SessionStart": [
            {"hooks": [{"type": "command", "command": 'sh "${CLAUDE_PLUGIN_ROOT}/x.sh"'}]}]}}))
        self.assertEqual(self.errors(), [])
        self.plugin["hooks"] = {"SessionStart": []}
        self.assertIn("loads both", " ".join(self.errors()))

    def asset_errors(self):
        return [e for e in self.errors() if "backticks or a code block" in e]

    def test_asset_paths_in_backticks(self):
        self.write("plugin/skills/demo/logo.png", "x")
        self.write("plugin/README.md", "Run `pscat --png out.png f.ps`, "
                   "see ![logo](skills/demo/logo.png) and https://x.org/a.png.\n")
        self.write("plugin/skills/demo/ref.md", "Bare `.png`, `out-001.png`, "
                   "`https://x.org/a.png` and an indented block:\n\n    docs/a.png\n")
        self.assertEqual(self.asset_errors(), [])
        # A bundled file named by basename, by path, with double backticks.
        for body in ("See `logo.png`.", "See ``skills/demo/logo.png``."):
            self.write("plugin/README.md", body + "\n")
            errs = self.asset_errors()
            self.assertEqual(len(errs), 1, errs)
            self.assertIn("plugin/README.md:1", errs[0])
            self.assertIn("names a bundled file", errs[0])

    def test_asset_paths_outside_bundle_and_fences(self):
        self.write("plugin/skills/demo/ref.md", "x\n\n```sh\nopen ../x.png\n```\n"
                   "~~~\ncp /tmp/f.ttf .\n~~~\nafter `out.png`\n")
        errs = self.asset_errors()
        self.assertEqual(len(errs), 2, errs)
        self.assertIn("ref.md:4", errs[0])
        self.assertIn("path outside the bundle", errs[0])
        self.assertIn("ref.md:7", errs[1])
        self.write("plugin/skills/demo/ref.md", "[massing](https://github.com/o/r/blob/main/a.png)\n")
        self.assertEqual(self.asset_errors(), [])

    def test_codex_version_must_match_cargo(self):
        self.codex_plugin["version"] = "1.2.4"
        self.assertIn("plugin/plugin.json: version", " ".join(self.errors()))

    def test_codex_name_must_match_entry(self):
        self.codex_plugin["name"] = "q"
        self.assertIn("Codex marketplace entry", " ".join(self.errors()))

    def test_codex_policy_and_category(self):
        del self.codex_market["plugins"][0]["category"]
        self.codex_market["plugins"][0]["policy"]["installation"] = "MAYBE"
        text = " ".join(self.errors())
        self.assertIn("no category", text)
        self.assertIn("policy.installation", text)

    def test_codex_source_must_stay_in_repo(self):
        self.codex_market["plugins"][0]["source"]["path"] = "../elsewhere"
        self.assertIn("inside the repo", " ".join(self.errors()))
        self.codex_market["plugins"][0]["source"] = "./plugin"
        self.assertEqual(self.errors(), [])

    def test_codex_source_needs_plugin_json(self):
        os.makedirs(os.path.join(self.root, "other"))
        self.codex_market["plugins"][0]["source"]["path"] = "./other"
        self.assertIn("has no plugin.json", " ".join(self.errors()))

    def test_codex_mcp_needs_type_and_bare_command(self):
        self.mcp["mcpServers"]["s"] = {"command": "pscat-mcp"}
        self.assertIn("type None", " ".join(self.errors()))
        self.mcp["mcpServers"]["s"] = {"type": "stdio", "command": "../bin/pscat-mcp"}
        self.assertIn("bare executable", " ".join(self.errors()))
        self.mcp["mcpServers"]["s"] = {"type": "stdio", "command": "${PLUGIN_ROOT}/bin/x"}
        self.assertEqual(self.errors(), [])

    def test_codex_extension_paths(self):
        self.codex_plugin["extensions"] = {"com.openai": {"onboardingSkill": "./skills/none/SKILL.md"}}
        self.assertIn("onboardingSkill", " ".join(self.errors()))

    def test_bad_json(self):
        self.save()
        self.write("plugin/mcp.json", "{nope")
        self.assertIn("mcp.json", " ".join(pc.check(self.root)))


class RealRepo(unittest.TestCase):
    def test_repo_manifests_are_clean(self):
        self.assertEqual(pc.check(ROOT), [])


if __name__ == "__main__":
    unittest.main()
