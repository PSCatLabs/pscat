#!/usr/bin/env python3
"""Tests for build_chatgpt_plugin.py and plugin_check.check_chatgpt (#1089).

    python3 scripts/test_build_chatgpt_plugin.py

Builds from small Git fixtures (the build reads `git ls-files`) and from this
repository with a fake SHA256SUMS. No network.
"""
import json
import os
import pathlib
import re
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
        self.write("plugin-chatgpt/skills/pscat-chatgpt-setup/SKILL.md",
                   "---\nname: pscat-chatgpt-setup\ndescription: d\n---\n{{PSCAT_RELEASE}} {{PSCAT_VERSION}} "
                   "{{SHA256:x86_64-unknown-linux-musl}} {{SHA256:aarch64-unknown-linux-musl}}\n")
        self.write("plugin/skills/demo/references/more.md", "# More\n\nlocal text\n")
        self.write("plugin-chatgpt/cloud/skills/demo/SKILL.md",
                   '<!-- cloud: sub "same" -->\ncloud\n')
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
            "scripts/run.sh", "skills/demo/SKILL.md", "skills/demo/references/more.md",
            "skills/pscat-chatgpt-setup/SKILL.md"])

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
            text = z.read("skills/pscat-chatgpt-setup/SKILL.md").decode()
        self.assertIn(f"v1.2.3 1.2.3 {HX} {HA}", text)
        self.assertNotIn("{{", text)

    def test_unknown_placeholder(self):
        self.write("plugin-chatgpt/skills/pscat-chatgpt-setup/x.md", "{{NOPE}}")
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
        self.assertEqual(path.name, "pscat-cloud-chatgpt-plugin-1.2.3.zip")
        self.assertEqual(self.mcp(path), {
            "$schema": "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
            "mcpServers": {"pscat-cloud": {
                "type": "streamable-http", "url": URL,
                "extensions": {"com.openai": {"auth": {"type": "none"}}}}}})

    def test_bearer_declares_api_key_without_a_token(self):
        auth = self.mcp(self.build(remote=URL, remote_auth="bearer"))[
            "mcpServers"]["pscat-cloud"]["extensions"]["com.openai"]["auth"]
        self.assertEqual(auth, {"type": "api_key", "headerScheme": "bearer"})

    def test_no_headers_entry(self):
        for auth in ("none", "bearer"):
            server = self.mcp(self.build(remote=URL, remote_auth=auth))["mcpServers"]["pscat-cloud"]
            self.assertNotIn("headers", server)

    def manifest(self, path):
        with zipfile.ZipFile(path) as z:
            return json.loads(z.read("plugin.json"))

    def test_renamed_so_it_installs_beside_the_local_plugin(self):
        """#1254: ChatGPT refuses a second plugin named pscat."""
        local = self.build()
        path = self.build(remote=URL, remote_auth="none")
        self.assertNotEqual(path.name, local.name)
        self.assertEqual(self.manifest(local)["name"], "p")  # the local build is untouched
        m = self.manifest(path)
        self.assertEqual(m["name"], "pscat-cloud")
        self.assertEqual(m["version"], "1.2.3")
        iface = m["extensions"]["com.openai"]["interface"]
        self.assertEqual(iface["displayName"], "pscat cloud")
        self.assertIn("Hosted", iface["shortDescription"])
        # the mcp.json server key matches the plugin name, so tool names do not collide
        self.assertEqual(list(self.mcp(path)["mcpServers"]), [m["name"]])

    def test_manifest_fits_openai_limits(self):
        iface = self.manifest(self.build(remote=URL, remote_auth="none"))[
            "extensions"]["com.openai"]["interface"]
        for key in ("displayName", "shortDescription"):
            self.assertLessEqual(len(iface[key]), pc.OPENAI_TEXT_LIMITS[key], key)

    def test_description_is_about_the_hosted_server(self):
        desc = self.manifest(self.build(remote=URL, remote_auth="none"))["description"]
        self.assertNotIn("installed pscat", desc)
        self.assertNotIn("downloads", desc)
        self.assertIn("hosted pscat-mcp server", desc)
        self.assertIn("nothing needs to be installed", desc)

    def test_only_the_manifest_fields_change(self):
        local = self.manifest(self.build())
        remote = self.manifest(self.build(remote=URL, remote_auth="none"))
        for m in (local, remote):
            for key in ("name", "description"):
                m.pop(key, None)
            for key in ("displayName", "shortDescription"):
                m["extensions"]["com.openai"]["interface"].pop(key)
        self.assertEqual(local, remote)

    def test_cloud_profile_leaves_out_the_local_install(self):
        """#1256: no launchers, no release pin, no setup skill; the overlay never ships."""
        local = self.names(self.build())
        path = self.build(remote=URL, remote_auth="none")
        self.assertEqual(self.names(path), [
            ".claude-plugin/icon.png", "mcp.json", "plugin.json",
            "skills/demo/SKILL.md", "skills/demo/references/more.md"])
        self.assertEqual(sorted(set(local) - set(self.names(path))), [
            "pscat-release", "scripts/run.sh", "skills/pscat-chatgpt-setup/SKILL.md"])
        with zipfile.ZipFile(path) as z:
            self.assertEqual(z.getinfo("mcp.json").external_attr >> 16, 0o100644)

    def test_the_overlay_applies_to_the_cloud_build_only(self):
        source = (self.root / "plugin/skills/demo/SKILL.md").read_bytes()
        with zipfile.ZipFile(self.build(remote=URL, remote_auth="none")) as z:
            self.assertEqual(z.read("skills/demo/SKILL.md"),
                             b"---\nname: demo\ndescription: d\n---\ncloud\n")
            self.assertEqual(z.read("skills/demo/references/more.md"),
                             (self.root / "plugin/skills/demo/references/more.md").read_bytes())
        for kw in ({}, {"skills_only": True}):
            path = self.build(**kw)
            self.assertFalse(any(n.startswith("cloud/") for n in self.names(path)))
            with zipfile.ZipFile(path) as z:
                self.assertEqual(z.read("skills/demo/SKILL.md"), source)

    def test_an_overlay_for_a_file_that_is_not_there_fails(self):
        self.write("plugin-chatgpt/cloud/skills/gone/SKILL.md", '<!-- cloud: sub "x" -->\ny\n')
        with self.assertRaisesRegex(SystemExit, "no file skills/gone/SKILL.md"):
            self.build(remote=URL, remote_auth="none")
        self.build()  # the local build does not read the overlay

    def test_an_overlay_for_an_excluded_file_fails(self):
        self.write("plugin-chatgpt/cloud/scripts/run.sh", '<!-- cloud: sub "x" -->\ny\n')
        with self.assertRaisesRegex(SystemExit, "no file scripts/run.sh"):
            self.build(remote=URL, remote_auth="none")

    def test_an_overlay_anchor_that_stopped_matching_fails(self):
        self.write("plugin/skills/demo/SKILL.md", "---\nname: demo\ndescription: d\n---\nnew\n")
        with self.assertRaisesRegex(SystemExit, "matches 0 times"):
            self.build(remote=URL, remote_auth="none")

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
        self.assertEqual(self.mcp(path)["mcpServers"]["pscat-cloud"]["url"], "https://h.example:8443/Mcp")

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
        self.assertTrue(ok.stdout.strip().endswith("/pscat-cloud-chatgpt-plugin-1.2.3.zip"))
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
        self.write("plugin-chatgpt/skills/pscat-chatgpt-setup/SKILL.md",
                   f"---\nname: pscat-chatgpt-setup\ndescription: d\n---\nv1.2.3 {HX}\n")
        self.assertTrue(any("by hand" in e for e in self.errors()))

    def test_unknown_placeholder(self):
        self.write("plugin-chatgpt/skills/pscat-chatgpt-setup/x.md", "{{NOPE}}")
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



SEVEN_TOOLS = ("chalkboard_math", "describe_art_capabilities", "get_renderer_info", "handwrite",
               "render_postscript", "review_sheet", "validate_postscript")
CORE_SKILLS = ("pscat", "psart", "psviz", "functional-postscript")
# What the local plugin does that the hosted server cannot (#1256).
LOCAL_ONLY = ("scripts/pscat-mcp", "--locate", "cargo install", "pscat-release",
              "CLAUDE_PLUGIN_ROOT")
NO_SUCH_TOOL = ("eval_postscript", "start_render", "render_status", "render_result",
                "cancel_render", "out_path")


class CloudRepo(unittest.TestCase):
    """The pscat-cloud ZIP built from this repository (#1256), checked statically."""

    @classmethod
    def setUpClass(cls):
        cls.tmp = pathlib.Path(tempfile.mkdtemp())
        tag = b.release_tag(ROOT)
        (cls.tmp / "SHA256SUMS").write_text(sums_text(tag[1:]))
        cls.sums = cls.tmp / "SHA256SUMS"
        cls.path = b.build(ROOT, cls.sums, cls.tmp / "cloud", remote=URL, remote_auth="none")
        cls.local = b.build(ROOT, cls.sums, cls.tmp / "local")
        cls.zip = zipfile.ZipFile(cls.path)
        cls.names = cls.zip.namelist()
        cls.out = cls.tmp / "unzipped"
        cls.zip.extractall(cls.out)

    @classmethod
    def tearDownClass(cls):
        cls.zip.close()
        shutil.rmtree(cls.tmp)

    def md_texts(self):
        return {n: self.zip.read(n).decode() for n in self.names if n.endswith(".md")}

    def test_1_manifests_are_valid_json(self):
        for name in ("plugin.json", "mcp.json"):
            self.assertIsInstance(json.loads(self.zip.read(name)), dict, name)
        manifest = json.loads(self.zip.read("plugin.json"))
        self.assertEqual(manifest["name"], "pscat-cloud")
        self.assertEqual(manifest["version"], json.loads(
            (ROOT / "plugin/plugin.json").read_text())["version"])  # not bumped here

    def test_2_declares_streamable_http_with_the_url(self):
        servers = json.loads(self.zip.read("mcp.json"))["mcpServers"]
        self.assertEqual(list(servers), ["pscat-cloud"])
        self.assertEqual(servers["pscat-cloud"]["type"], "streamable-http")
        self.assertEqual(servers["pscat-cloud"]["url"], URL)
        self.assertTrue(servers["pscat-cloud"]["url"].startswith("https://"))

    def test_3_no_stdio_server_and_no_local_executor(self):
        text = self.zip.read("mcp.json").decode() + self.zip.read("plugin.json").decode()
        for word in ("command", "args", "stdio", "scripts/", "mcpServers\": {\"pscat\""):
            self.assertNotIn(word, text)
        self.assertNotIn("mcpServers", json.loads(self.zip.read("plugin.json")))
        for name, info in ((i.filename, i) for i in self.zip.infolist()):
            self.assertFalse(info.external_attr >> 16 & 0o111, f"{name} is executable")

    def test_4_has_the_core_skills_and_their_references(self):
        tracked = {rel for rel, _ in b.tracked(ROOT, "plugin") if rel.startswith("skills/")}
        self.assertEqual({n for n in self.names if n.startswith("skills/")}, tracked)
        for skill in CORE_SKILLS:
            self.assertIn(f"skills/{skill}/SKILL.md", self.names)
        refs = [n for n in self.names if "/references/" in n]
        self.assertGreaterEqual(len(refs), 10)

    def test_5_leaves_out_scripts_the_release_pin_and_the_setup_skill(self):
        for n in self.names:
            self.assertFalse(n.startswith("scripts/") or n == "pscat-release"
                             or "pscat-chatgpt-setup" in n or n.startswith(("hooks/", "cloud/")), n)
        self.assertNotIn("pscat-chatgpt-setup", " ".join(self.md_texts().values()))
        # the desktop ZIP still has them
        with zipfile.ZipFile(self.local) as z:
            self.assertIn("scripts/pscat-mcp", z.namelist())
            self.assertIn("skills/pscat-chatgpt-setup/SKILL.md", z.namelist())

    def test_6_no_broken_references_to_excluded_files(self):
        for name, text in self.md_texts().items():
            for bad in LOCAL_ONLY:
                self.assertNotIn(bad, text, f"{name} mentions {bad}")

    def test_6b_no_tool_the_hosted_server_lacks(self):
        for name, text in self.md_texts().items():
            for bad in NO_SUCH_TOOL:
                self.assertNotIn(bad, text, f"{name} mentions {bad}")

    def test_6c_the_core_skill_names_the_seven_tools_and_a_failed_connection(self):
        text = self.md_texts()["skills/pscat/SKILL.md"]
        for tool in SEVEN_TOOLS:
            self.assertIn(f"`{tool}`", text)
        self.assertIn("connection is not working", text)
        self.assertIn("Do not try to install, download or run pscat yourself", text)
        mcp = self.md_texts()["skills/pscat/references/mcp.md"]
        for tool in SEVEN_TOOLS:
            self.assertIn(f"`{tool}`", mcp)

    def test_6d_links_between_the_files_resolve(self):
        link = re.compile(r"\]\(([^)#\s]+)(?:#[^)]*)?\)")
        for name, text in self.md_texts().items():
            for target in link.findall(text):
                if "://" in target or target.startswith("mailto:"):
                    continue
                dest = (self.out / name).parent / target
                self.assertTrue(dest.resolve().exists(), f"{name}: {target} is not in the ZIP")

    def test_6g_the_core_skill_links_every_reference_file(self):
        text = self.md_texts()["skills/pscat/SKILL.md"]
        for n in self.names:
            if n.startswith("skills/pscat/references/"):
                self.assertIn(n[len("skills/pscat/"):], text, n)

    def test_6e_the_skills_still_have_valid_frontmatter(self):
        errors = []
        for skill in CORE_SKILLS:
            pc._check_skill(str(self.out / "skills" / skill), f"skills/{skill}", errors)
        self.assertEqual(errors, [])

    def test_6f_only_the_overlaid_files_differ_from_the_shared_sources(self):
        overlaid = {rel[len(b.CLOUD_DIR):] for rel, _ in b.tracked(ROOT, "plugin-chatgpt")
                    if rel.startswith(b.CLOUD_DIR)}
        self.assertTrue(overlaid)
        for n in self.names:
            if n in overlaid:
                self.assertNotEqual(self.zip.read(n), (ROOT / "plugin" / n).read_bytes(), n)
            elif n not in ("mcp.json", "plugin.json"):
                self.assertEqual(self.zip.read(n), (ROOT / "plugin" / n).read_bytes(), n)

    def test_7_testzip(self):
        self.assertIsNone(self.zip.testzip())

    def test_the_desktop_zips_are_the_shared_files_byte_for_byte(self):
        """The local and --skills-only ZIPs read nothing from the overlay (#1256)."""
        skills_only = b.build(ROOT, self.sums, self.tmp / "so", skills_only=True)
        for path in (self.local, skills_only):
            with zipfile.ZipFile(path) as z:
                for n in z.namelist():
                    if not n.startswith("skills/pscat-chatgpt-setup/"):
                        self.assertEqual(z.read(n), (ROOT / "plugin" / n).read_bytes(), n)
                self.assertFalse(any(n.startswith("cloud/") for n in z.namelist()))

    def test_plugin_check_covers_the_overlay(self):
        errors = []
        pc.check_chatgpt(str(ROOT), errors)
        self.assertEqual(errors, [])


class Overlay(unittest.TestCase):
    """build_chatgpt_plugin.apply_overlay on small texts."""

    DOC = ("# T\n\nintro\n\n## One\n\none body\n\n```sh\n# not a heading\n\ncode\n```\n\n"
           "## Two\n\ntwo body\n- item a\n  more a\n- item b\n\n## Three\n\nlast\n")

    def apply(self, overlay, doc=None):
        return b.apply_overlay("t", self.DOC if doc is None else doc, overlay)

    def test_sub(self):
        out = self.apply('<!-- cloud: sub "two body" -->\n2 body\n')
        self.assertIn("\n2 body\n- item a", out)
        self.assertEqual(out.replace("2 body", "two body"), self.DOC)

    def test_sub_may_span_lines_and_delete(self):
        out = self.apply('<!-- cloud: sub "- item a\n  more a\n" -->\n')
        self.assertEqual(out, self.DOC.replace("- item a\n  more a\n", ""))

    def test_replace_and_drop_a_section(self):
        out = self.apply('<!-- cloud: replace "## One" -->\n## One\n\nnew\n')
        self.assertEqual(out, self.DOC.replace(
            "one body\n\n```sh\n# not a heading\n\ncode\n```", "new"))
        out = self.apply('<!-- cloud: drop "## One" -->\n')
        self.assertEqual(out, "# T\n\nintro\n\n## Two\n\ntwo body\n- item a\n  more a\n"
                              "- item b\n\n## Three\n\nlast\n")
        self.assertTrue(self.apply('<!-- cloud: drop "## Three" -->\n').endswith("item b\n"))

    def test_replace_and_drop_a_block(self):
        out = self.apply('<!-- cloud: replace "intro" -->\nINTRO\n')
        self.assertTrue(out.startswith("# T\n\nINTRO\n\n## One"))
        self.assertNotIn("intro", self.apply('<!-- cloud: drop "intro" -->\n'))

    def test_items(self):
        out = self.apply('<!-- cloud: drop-item "- item a" -->\n')
        self.assertEqual(out, self.DOC.replace("- item a\n  more a\n", ""))
        out = self.apply('<!-- cloud: replace-item "- item a" -->\n- A\n')
        self.assertEqual(out, self.DOC.replace("- item a\n  more a", "- A"))

    def test_replace_file(self):
        self.assertEqual(self.apply("<!-- cloud: replace-file -->\nall new\n"), "all new\n")
        with self.assertRaisesRegex(SystemExit, "only edit"):
            self.apply('<!-- cloud: replace-file -->\nx\n<!-- cloud: sub "intro" -->\ny\n')

    def test_an_anchor_must_match_exactly_once(self):
        for overlay in ('<!-- cloud: sub "nowhere" -->\nx\n', '<!-- cloud: sub "body" -->\nx\n',
                        '<!-- cloud: replace "## Nowhere" -->\nx\n',
                        '<!-- cloud: replace "body" -->\nx\n',
                        '<!-- cloud: drop-item "- item" -->\n',
                        '<!-- cloud: drop-item "- nothing" -->\n'):
            with self.subTest(overlay=overlay), self.assertRaisesRegex(SystemExit, "matches"):
                self.apply(overlay)

    def test_a_heading_inside_a_code_fence_is_not_a_heading(self):
        with self.assertRaisesRegex(SystemExit, "matches 0 times"):
            self.apply('<!-- cloud: drop "# not a heading" -->\n')
        with self.assertRaisesRegex(SystemExit, "inside a code fence"):
            self.apply('<!-- cloud: replace "code" -->\nx\n')

    def test_overlapping_edits_and_bad_overlays(self):
        with self.assertRaisesRegex(SystemExit, "overlap"):
            self.apply('<!-- cloud: drop "## One" -->\n<!-- cloud: drop "one body" -->\n')
        with self.assertRaisesRegex(SystemExit, "text before"):
            self.apply("stray\n<!-- cloud: drop \"intro\" -->\n")
        with self.assertRaisesRegex(SystemExit, "takes no new text"):
            self.apply('<!-- cloud: drop "intro" -->\nnew\n')
        with self.assertRaisesRegex(SystemExit, "no edits"):
            self.apply("<!-- only a comment -->\n")
        with self.assertRaisesRegex(SystemExit, "needs an anchor"):
            self.apply("<!-- cloud: sub -->\nx\n")

    def test_a_leading_comment_is_allowed(self):
        out = self.apply('<!-- Contributors: why -->\n<!-- cloud: drop "intro" -->\n')
        self.assertNotIn("intro", out)


if __name__ == "__main__":
    unittest.main()
