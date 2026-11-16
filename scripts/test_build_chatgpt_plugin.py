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


URL = "https://pscat-mcp.example.run.app/mcp"


class Remote(Fixture):
    """--remote (#1240): the hosted server's mcp.json in place of the local one."""

    def mcp(self, path):
        with zipfile.ZipFile(path) as z:
            return json.loads(z.read("mcp.json"))

    def test_none(self):
        path = self.build(remote=URL, remote_auth="none")
        self.assertTrue(path.name.endswith("-remote.zip"))
        self.assertEqual(self.mcp(path), {
            "$schema": "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
            "mcpServers": {"pscat": {
                "type": "streamable-http", "url": URL,
                "extensions": {"com.openai": {"auth": {"type": "none"}}}}}})

    def test_bearer_declares_api_key_without_a_token(self):
        auth = self.mcp(self.build(remote=URL, remote_auth="bearer"))[
            "mcpServers"]["pscat"]["extensions"]["com.openai"]["auth"]
        self.assertEqual(auth, {"type": "api_key", "headerScheme": "bearer"})

    def test_no_headers_entry(self):
        for auth in ("none", "bearer"):
            server = self.mcp(self.build(remote=URL, remote_auth=auth))["mcpServers"]["pscat"]
            self.assertNotIn("headers", server)

    def test_everything_else_unchanged(self):
        local = self.names(self.build())
        path = self.build(remote=URL, remote_auth="none")
        self.assertEqual(self.names(path), local)
        with zipfile.ZipFile(path) as z:
            self.assertEqual(z.read("skills/demo/SKILL.md"),
                             (self.root / "plugin/skills/demo/SKILL.md").read_bytes())
            self.assertEqual(z.getinfo("mcp.json").external_attr >> 16, 0o100644)

    def test_reproducible(self):
        first = self.build(remote=URL, remote_auth="none").read_bytes()
        self.assertEqual(first, self.build(remote=URL, remote_auth="none").read_bytes())

    def test_bad_urls(self):
        for bad, why in [
            ("http://h.example/mcp", "https"),
            ("https:///mcp", "https"),
            ("https://user:pw@h.example/mcp", "credentials"),
            ("https://@h.example/mcp", "credentials"),
            ("https://h.example/mcp?token=abc", "query"),
            ("https://h.example/mcp?", "query"),
            ("https://h.example/mcp#frag", "query string or fragment"),
            ("https://<your-pscat-mcp-host>/mcp", "placeholder"),
            ("https://h.example", "MCP path"),
            ("https://h.example/", "MCP path"),
            ("https://h.example//", "MCP path"),
            ("https://[::1/mcp", "not a valid URL"),
            ("https://h.example:99999/mcp", "not a valid URL"),
            ("https://h.example:abc/mcp", "not a valid URL"),
            ("https://h.example/mcp\n", "whitespace"),
            ("https://h.example/mcp ", "whitespace"),
            ("https://ex\u00e4mple.com/mcp", "ASCII"),
            ("https://localhost/mcp", "public host"),
            ("https://127.0.0.1/mcp", "public host"),
            ("https://[::1]/mcp", "public host"),
            ("https://10.0.0.5:8443/mcp", "public host"),
        ]:
            with self.subTest(url=bad), self.assertRaisesRegex(SystemExit, why):
                self.build(remote=bad, remote_auth="none")

    def test_url_is_normalised(self):
        path = self.build(remote="HTTPS://H.Example:8443/Mcp", remote_auth="none")
        self.assertEqual(self.mcp(path)["mcpServers"]["pscat"]["url"], "https://h.example:8443/Mcp")

    def test_auth_is_required_and_checked(self):
        with self.assertRaisesRegex(SystemExit, "--remote-auth"):
            self.build(remote=URL)
        with self.assertRaisesRegex(SystemExit, "one of"):
            self.build(remote=URL, remote_auth="oauth")
        with self.assertRaisesRegex(SystemExit, "needs --remote"):
            self.build(remote_auth="none")

    def test_not_with_skills_only(self):
        with self.assertRaisesRegex(SystemExit, "cannot be combined"):
            self.build(remote=URL, remote_auth="none", skills_only=True)

    def test_cli(self):
        script = str(ROOT / "scripts/build_chatgpt_plugin.py")
        base = [sys.executable, script, "--root", str(self.root), "--sums", str(self.sums),
                "--out", str(self.out)]
        ok = subprocess.run(base + ["--remote", URL, "--remote-auth", "bearer"],
                            capture_output=True, text=True)
        self.assertEqual(ok.returncode, 0, ok.stderr)
        self.assertIn("cannot send it", ok.stderr)  # the bearer warning
        self.assertTrue(ok.stdout.strip().endswith("-remote.zip"))
        bad = subprocess.run(base + ["--remote", "http://h.example/mcp", "--remote-auth", "none"],
                             capture_output=True, text=True)
        self.assertEqual(bad.returncode, 1)
        self.assertIn("https", bad.stderr)
        for extra, why in [(["--remote", URL], "--remote-auth"),
                           (["--remote", URL, "--remote-auth", "none", "--skills-only"],
                            "cannot be combined"),
                           (["--remote", "https://[::1/mcp", "--remote-auth", "none"],
                            "not a valid URL")]:
            r = subprocess.run(base + extra, capture_output=True, text=True)
            self.assertEqual(r.returncode, 1, r.stderr)
            self.assertIn(why, r.stderr)
            self.assertNotIn("Traceback", r.stderr)


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

    def test_repo_remote_build_differs_only_in_mcp_json(self):
        tmp = pathlib.Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, tmp)
        tag = b.release_tag(ROOT)
        (tmp / "SHA256SUMS").write_text(sums_text(tag[1:]))
        local = b.build(ROOT, tmp / "SHA256SUMS", tmp / "a")
        remote = b.build(ROOT, tmp / "SHA256SUMS", tmp / "b", remote=URL, remote_auth="none")
        with zipfile.ZipFile(local) as zl, zipfile.ZipFile(remote) as zr:
            self.assertEqual(zl.namelist(), zr.namelist())
            differ = [n for n in zl.namelist() if zl.read(n) != zr.read(n)]
            self.assertEqual(differ, ["mcp.json"])
            server = json.loads(zr.read("mcp.json"))["mcpServers"]
            self.assertEqual(list(server), ["pscat"])
            self.assertNotIn("command", server["pscat"])  # no local stdio server left


if __name__ == "__main__":
    unittest.main()
