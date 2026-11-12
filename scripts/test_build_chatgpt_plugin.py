#!/usr/bin/env python3
"""Tests for build_chatgpt_plugin.py and plugin_check.check_chatgpt (#1089).

    python3 scripts/test_build_chatgpt_plugin.py

Builds from small Git fixtures (the build reads `git ls-files`) and from this
repository with a fake SHA256SUMS. No network.
"""
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import unittest
import zipfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import build_chatgpt_plugin as b  # noqa: E402
import plugin_check as pc  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parent.parent
HX = "a" * 64
HA = "b" * 64


def sums_text(version):
    return (f"{HX}  pscat-{version}-x86_64-unknown-linux-musl.tar.gz\n"
            f"{HA}  pscat-{version}-aarch64-unknown-linux-musl.tar.gz\n"
            f"{'c' * 64}  pscat-{version}.tgz\n")


class Fixture(unittest.TestCase):
    """A Git repo with a tiny plugin/ and plugin-chatgpt/."""

    def setUp(self):
        self.root = pathlib.Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root)
        self.out = self.root / "out"
        manifest = {"name": "p", "version": "1.2.3", "extensions": {"com.openai": {
            "interface": {"displayName": "p", "shortDescription": "short"}}}}
        self.write("plugin/plugin.json", json.dumps(manifest))
        self.write("plugin/mcp.json", '{"mcpServers": {}}')
        self.write("plugin/pscat-release", "# pin\nv1.2.3\n")
        self.write("plugin/README.md", "readme")
        self.write("plugin/hooks/hooks.json", "{}")
        self.write("plugin/.claude-plugin/plugin.json", "{}")
        self.write("plugin/.claude-plugin/icon.png", "png")
        self.write("plugin/skills/demo/SKILL.md", "---\nname: demo\ndescription: d\n---\nsame\n")
        self.write("plugin/scripts/run.sh", "#!/bin/sh\n", mode=0o755)
        self.write("plugin-chatgpt/skills/setup/SKILL.md",
                   "---\nname: setup\ndescription: d\n---\n{{PSCAT_RELEASE}} {{PSCAT_VERSION}} "
                   "{{SHA256:x86_64-unknown-linux-musl}} {{SHA256:aarch64-unknown-linux-musl}}\n")
        self.write("plugin/untracked.txt", "never ships", add=False)
        self.sums = self.root / "SHA256SUMS"
        self.sums.write_text(sums_text("1.2.3"))

    def write(self, rel, text, mode=0o644, add=True):
        p = self.root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text)
        p.chmod(mode)
        if add:
            self.git("add", rel)

    def git(self, *args):
        if not (self.root / ".git").exists():
            subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        subprocess.run(["git", "-C", str(self.root), *args], check=True, capture_output=True)

    def build(self, **kw):
        return b.build(self.root, self.sums, self.out, **kw)

    def names(self, path):
        with zipfile.ZipFile(path) as z:
            return sorted(z.namelist())


class Build(Fixture):
    def test_contents(self):
        self.assertEqual(self.names(self.build()), [
            ".claude-plugin/icon.png", "mcp.json", "plugin.json", "pscat-release",
            "scripts/run.sh", "skills/demo/SKILL.md", "skills/setup/SKILL.md"])

    def test_skills_only_drops_mcp_json(self):
        path = self.build(skills_only=True)
        self.assertTrue(path.name.endswith("-skills-only.zip"))
        self.assertNotIn("mcp.json", self.names(path))
        self.assertIn("scripts/run.sh", self.names(path))

    def test_reproducible(self):
        first = self.build().read_bytes()
        self.assertEqual(first, self.build().read_bytes())

    def test_modes_and_copies(self):
        with zipfile.ZipFile(self.build()) as z:
            self.assertEqual(z.getinfo("scripts/run.sh").external_attr >> 16, 0o100755)
            self.assertEqual(z.getinfo("plugin.json").external_attr >> 16, 0o100644)
            self.assertEqual(z.read("skills/demo/SKILL.md"),
                             (self.root / "plugin/skills/demo/SKILL.md").read_bytes())

    def test_placeholders_filled(self):
        with zipfile.ZipFile(self.build()) as z:
            text = z.read("skills/setup/SKILL.md").decode()
        self.assertIn(f"v1.2.3 1.2.3 {HX} {HA}", text)
        self.assertNotIn("{{", text)

    def test_unknown_placeholder(self):
        self.write("plugin-chatgpt/skills/setup/x.md", "{{NOPE}}")
        with self.assertRaisesRegex(SystemExit, "unknown placeholder"):
            self.build()

    def test_sums_must_match_the_pin(self):
        self.sums.write_text(sums_text("9.9.9"))
        with self.assertRaisesRegex(SystemExit, "no SHA-256"):
            self.build()

    def test_symlink_refused(self):
        os.symlink("SKILL.md", self.root / "plugin/skills/demo/link.md")
        self.git("add", "plugin/skills/demo/link.md")
        with self.assertRaisesRegex(SystemExit, "symlink"):
            self.build()

    def test_same_path_in_both_sources(self):
        self.write("plugin-chatgpt/skills/demo/SKILL.md", "x")
        with self.assertRaisesRegex(SystemExit, "also in another"):
            self.build()


class Check(Fixture):
    def errors(self):
        errors = []
        pc.check_chatgpt(str(self.root), errors)
        return errors

    def test_clean(self):
        self.assertEqual(self.errors(), [])

    def test_short_description_limit(self):
        data = json.loads((self.root / "plugin/plugin.json").read_text())
        data["extensions"]["com.openai"]["interface"]["shortDescription"] = "x" * 31
        self.write("plugin/plugin.json", json.dumps(data))
        self.assertTrue(any("shortDescription is 31" in e for e in self.errors()))

    def test_bad_icon(self):
        data = json.loads((self.root / "plugin/plugin.json").read_text())
        data["extensions"]["com.openai"]["interface"]["logo"] = "./.claude-plugin/icon.png"
        self.write("plugin/plugin.json", json.dumps(data))
        self.assertTrue(any("square PNG" in e for e in self.errors()))

    def test_hand_written_version_or_hash(self):
        self.write("plugin-chatgpt/skills/setup/SKILL.md",
                   f"---\nname: setup\ndescription: d\n---\nv1.2.3 {HX}\n")
        self.assertTrue(any("by hand" in e for e in self.errors()))

    def test_unknown_placeholder(self):
        self.write("plugin-chatgpt/skills/setup/x.md", "{{NOPE}}")
        self.assertTrue(any("unknown placeholder" in e for e in self.errors()))

    def test_name_clash_with_shared_skill(self):
        self.write("plugin-chatgpt/skills/demo/SKILL.md", "---\nname: demo\ndescription: d\n---\n")
        self.assertTrue(any("already has a skill" in e for e in self.errors()))


class Repo(unittest.TestCase):
    """This repository's own plugin, built with a fake SHA256SUMS."""

    def test_repo_builds_and_checks(self):
        tmp = pathlib.Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, tmp)
        tag = b.release_tag(ROOT)
        (tmp / "SHA256SUMS").write_text(sums_text(tag[1:]))
        path = b.build(ROOT, tmp / "SHA256SUMS", tmp)
        with zipfile.ZipFile(path) as z:
            names = z.namelist()
            self.assertIn("plugin.json", names)
            self.assertIn("skills/pscat-chatgpt-setup/SKILL.md", names)
            self.assertFalse(any(n.startswith("hooks/") for n in names))
            self.assertNotIn(".claude-plugin/plugin.json", names)
            setup = z.read("skills/pscat-chatgpt-setup/SKILL.md").decode()
            self.assertIn(tag, setup)
            self.assertNotIn("{{", setup)
            for n in names:  # every shared file is copied byte for byte
                if not n.startswith("skills/pscat-chatgpt-setup/"):
                    self.assertEqual(z.read(n), (ROOT / "plugin" / n).read_bytes(), n)
        errors = []
        pc.check_chatgpt(str(ROOT), errors)
        self.assertEqual(errors, [])


if __name__ == "__main__":
    unittest.main()
