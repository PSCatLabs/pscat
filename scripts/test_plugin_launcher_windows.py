#!/usr/bin/env python3
"""Tests for the plugin launcher's Windows twin (#866): plugin/scripts/pscat-mcp.cmd
and pscat-mcp.ps1, and the extensionless entry point under Git Bash.

    python BIN_DIR=target/ci scripts/test_plugin_launcher_windows.py
    python scripts/test_plugin_launcher_windows.py target\\ci

Windows only (elsewhere it says so and exits 0). BIN_DIR holds a built
pscat.exe and pscat-mcp.exe: each test packs them into a fake release (the
Windows .zip, laid out as scripts/package_release.sh does, and SHA256SUMS)
served from a local HTTP server, so a download, a 404 and a refused
connection (offline) are all real HTTP outcomes and no network is used. The
server is the real pscat-mcp.exe, so a test speaks MCP to it through the
launcher: that is the check that the .cmd keeps PowerShell out of the stdio
path. USERPROFILE, LOCALAPPDATA, PATH and the cache are temp dirs, so a real
pscat on this machine is not seen. The Unix tests are
test_plugin_launcher.py; these mirror them.
"""
import functools
import hashlib
import http.server
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import unittest
import zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCRIPTS = os.path.join(ROOT, "plugin", "scripts")
HOOKS = os.path.join(ROOT, "plugin", "hooks", "hooks.json")
TRIPLE = "x86_64-pc-windows-msvc"
BIN_DIR = os.environ.get("BIN_DIR", "")


def crate_version():
    with open(os.path.join(ROOT, "Cargo.toml"), encoding="utf-8") as f:
        return re.search(r'^version = "([^"]+)"', f.read(), re.M).group(1)


VERSION = crate_version().split("-")[0]
TAG = f"v{VERSION}-rc.1"  # an rc pin of the built version: the built exe is new enough
GIT_SH = shutil.which("sh") or r"C:\Program Files\Git\bin\sh.exe"
MSGS = "".join(json.dumps(m) + "\n" for m in (
    {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": {"name": "launcher-test", "version": "0"}}},
    {"jsonrpc": "2.0", "method": "notifications/initialized"},
    {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
))


class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a):
        pass


def fake_cmd(version, marker="INSTALLED-FAKE"):
    return ("@echo off\r\n"
            'if "%~1"=="--version" (\r\n'
            f"  echo pscat-mcp {version}\r\n"
            "  exit /b 0\r\n"
            ")\r\n"
            f"echo {marker} %*\r\n")


class WinLauncher(unittest.TestCase):
    def setUp(self):
        # A space, an \u00e9 (in the runner's console code page, 437) and an
        # \u0141 (not in it), as in a profile like C:\Users\\u0141ucja Zo\u00eb: every
        # quoting layer (cross-spawn, the .cmd's for /f and its --exe handoff,
        # cygpath) meets all three.
        self.tmp = tempfile.mkdtemp(prefix="pscat t\u00e9st \u0141 ")
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.plugin = self.path("plugin")
        os.makedirs(self.path("plugin/scripts"))
        for f in ("pscat-mcp", "pscat-mcp.cmd", "pscat-mcp.ps1", "pscat-mcp.sh"):
            shutil.copy(os.path.join(SCRIPTS, f), self.path("plugin/scripts/" + f))
        self.pin(TAG)
        self.release = self.path("release")
        self.cache = self.path("cache")
        for d in ("home", "localappdata", "empty"):
            os.makedirs(self.path(d))
        self.make_release(TAG, f"{VERSION}-rc.1")
        handler = functools.partial(Quiet, directory=self.release)
        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        self.addCleanup(self.stop_server)
        self.base_url = f"http://127.0.0.1:{self.server.server_address[1]}"

    def stop_server(self):
        if self.server:
            self.server.shutdown()
            self.server.server_close()
            self.server = None

    def path(self, rel):
        return os.path.join(self.tmp, *rel.split("/"))

    def pin(self, tag):
        with open(self.path("plugin/pscat-release"), "w") as f:
            f.write("# pinned release\n" + tag + "\n")

    def make_release(self, tag, version, corrupt=False):
        d = os.path.join(self.release, tag)
        os.makedirs(d, exist_ok=True)
        name = f"pscat-{version}-{TRIPLE}"
        buf = io.BytesIO()
        with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
            for exe in ("pscat.exe", "pscat-mcp.exe"):
                z.write(os.path.join(BIN_DIR, exe), f"{name}/{exe}")
            z.writestr(f"{name}/lib/README", "lib\n")
        data = buf.getvalue()
        with open(os.path.join(d, name + ".zip"), "wb") as f:
            f.write(data)
        digest = hashlib.sha256(data + (b"x" if corrupt else b"")).hexdigest()
        with open(os.path.join(d, "SHA256SUMS"), "w", newline="\n") as f:
            f.write(f"{'0' * 64}  pscat-{version}-x86_64-unknown-linux-musl.tar.gz\n")
            f.write(f"{digest}  {name}.zip\n")

    def env(self, path_dirs=(), **extra):
        env = {k: v for k, v in os.environ.items()
               if not k.upper().startswith("PSCAT_") and k.upper() not in ("CARGO_HOME", "PATH")}
        sysroot = os.environ.get("SystemRoot", r"C:\Windows")
        env.update({
            "USERPROFILE": self.path("home"),
            "HOME": self.path("home"),
            "LOCALAPPDATA": self.path("localappdata"),
            "PATH": os.pathsep.join([*path_dirs, os.path.join(sysroot, "System32"), sysroot,
                                     os.path.join(sysroot, "System32", "Wbem"),
                                     os.path.join(sysroot, "System32", "WindowsPowerShell", "v1.0")]),
            "PSCAT_CACHE_DIR": self.cache,
            "PSCAT_RELEASE_BASE_URL": self.base_url,
        })
        env.update(extra)
        return {k: v for k, v in env.items() if v is not None}

    def run_argv(self, argv, env=None, stdin="", timeout=180):
        # stdout (MCP, --hook, --locate) is UTF-8; stderr comes in the console
        # code page, so its non-ASCII letters may not survive (the asserts on
        # it are ASCII).
        return subprocess.run(argv, env=env or self.env(), input=stdin, capture_output=True,
                              encoding="utf-8", errors="replace", timeout=timeout,
                              cwd=self.path("empty"))

    def run_cmd(self, *args, env=None, stdin="", timeout=180):
        return self.run_argv([self.path("plugin/scripts/pscat-mcp.cmd"), *args], env, stdin, timeout)

    def tools(self, r):
        """The tool names a handshake's stdout lists; every stdout line must be JSON-RPC."""
        replies = [json.loads(line) for line in r.stdout.splitlines() if line.strip()]
        for msg in replies:
            self.assertEqual(msg.get("jsonrpc"), "2.0", r.stdout)
        by_id = {m.get("id"): m for m in replies}
        return sorted(t["name"] for t in by_id.get(2, {}).get("result", {}).get("tools", []))

    def assertSamePath(self, got, want, msg=None):
        # samefile: Windows may spell one directory long or 8.3 (RUNNER~1).
        self.assertTrue(got and os.path.exists(got) and os.path.samefile(got, want),
                        f"{got!r} is not {want!r}" + (f": {msg}" if msg else ""))

    def installed(self):
        return os.path.join(self.cache, TAG, TRIPLE)

    # --- the tests -------------------------------------------------------------

    def test_cold_then_warm_start_serves_mcp(self):
        r = self.run_cmd(stdin=MSGS)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("render_postscript", self.tools(r))
        self.assertIn(f"downloading pscat-{VERSION}-rc.1-{TRIPLE}.zip", r.stderr)
        self.assertTrue(os.path.isfile(os.path.join(self.installed(), "pscat-mcp.exe")))
        self.assertTrue(os.path.isdir(os.path.join(self.installed(), "lib")))
        self.stop_server()  # warm: any fetch would now fail
        r = self.run_cmd(stdin=MSGS)
        self.assertEqual((r.returncode, r.stderr), (0, ""))
        self.assertIn("render_postscript", self.tools(r))

    def test_locate_prints_the_bundle_dir(self):
        r = self.run_cmd("--locate")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(r.stdout.count("\n"), 1, r.stdout)
        d = r.stdout.strip()
        self.assertTrue(os.path.isfile(os.path.join(d, "pscat.exe")), d)
        self.assertSamePath(d, self.installed())

    def test_default_cache_is_localappdata(self):
        r = self.run_cmd("--locate", env=self.env(PSCAT_CACHE_DIR=None))
        self.assertEqual(r.returncode, 0, r.stderr)
        want = os.path.join(self.path("localappdata"), "pscat", "plugin", TAG, TRIPLE)
        self.assertSamePath(r.stdout.strip(), want)

    def test_parallel_cold_starts_install_once(self):
        procs = [subprocess.Popen([self.path("plugin/scripts/pscat-mcp.cmd"), "--locate"],
                                  env=self.env(), stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                  stdin=subprocess.DEVNULL, encoding="utf-8", errors="replace",
                                  cwd=self.path("empty"))
                 for _ in range(8)]
        outs = [p.communicate(timeout=300) for p in procs]
        self.assertEqual([p.returncode for p in procs], [0] * 8, [e for _, e in outs])
        self.assertEqual(len({o for o, _ in outs}), 1, outs)
        self.assertEqual(os.listdir(os.path.join(self.cache, TAG)), [TRIPLE])  # no work dirs left
        self.assertFalse([e for e in os.listdir(self.cache) if e.startswith(".lock")])

    def test_checksum_mismatch_installs_nothing(self):
        self.make_release(TAG, f"{VERSION}-rc.1", corrupt=True)
        r = self.run_cmd(stdin=MSGS)
        self.assertEqual((r.returncode, r.stdout), (1, ""))
        self.assertIn("checksum mismatch", r.stderr)
        self.assertFalse(os.path.exists(self.installed()))

    def test_offline_says_so(self):
        self.base_url = "http://127.0.0.1:9"  # connection refused: offline
        r = self.run_cmd(stdin=MSGS)
        self.assertEqual((r.returncode, r.stdout), (1, ""))
        self.assertIn("offline", r.stderr)
        self.assertIn("cargo install pscat", r.stderr)

    def test_missing_release_is_not_called_offline(self):
        self.pin("v9.9.12")  # the server answers 404: the tag is not there
        r = self.run_cmd("--locate")
        self.assertEqual((r.returncode, r.stdout), (1, ""))
        self.assertIn("is the release published", r.stderr)
        self.assertNotIn("offline", r.stderr)

    def test_offline_falls_back_to_an_earlier_download(self):
        self.assertEqual(self.run_cmd("--locate").returncode, 0)
        self.pin("v9.9.10")
        self.base_url = "http://127.0.0.1:9"
        r = self.run_cmd(stdin=MSGS)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("render_postscript", self.tools(r))
        self.assertIn("previously downloaded", r.stderr)

    def test_installed_pscat_mcp_new_enough_is_used(self):
        inst = self.path("inst")
        os.makedirs(inst)
        with open(os.path.join(inst, "pscat-mcp.cmd"), "w", newline="") as f:
            f.write(fake_cmd("99.0.0"))
        r = self.run_cmd("a", "b", env=self.env([inst]))
        self.assertEqual((r.returncode, r.stdout.strip()), (0, "INSTALLED-FAKE a b"), r.stderr)
        self.assertFalse(os.path.exists(os.path.join(self.cache, TAG)))
        # PSCAT_MCP_BIN, any name.
        other = os.path.join(inst, "pscat-mcp-99.cmd")
        os.rename(os.path.join(inst, "pscat-mcp.cmd"), other)
        r = self.run_cmd("z", env=self.env(PSCAT_MCP_BIN=other))
        self.assertEqual(r.stdout.strip(), "INSTALLED-FAKE z", r.stderr)
        self.assertIn("pscat-mcp-99.cmd", r.stderr)
        # The Cargo bin directory, which a GUI app's PATH often lacks.
        cargo_bin = self.path("home/.cargo/bin")
        os.makedirs(cargo_bin)
        shutil.copy(os.path.join(BIN_DIR, "pscat-mcp.exe"), cargo_bin)
        r = self.run_cmd("--locate")
        self.assertSamePath(r.stdout.strip(), cargo_bin, r.stderr)

    def test_installed_pscat_mcp_too_old_is_passed_over(self):
        inst = self.path("inst")
        os.makedirs(inst)
        with open(os.path.join(inst, "pscat-mcp.cmd"), "w", newline="") as f:
            f.write(fake_cmd("0.0.1"))
        r = self.run_cmd(stdin=MSGS, env=self.env([inst]))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("render_postscript", self.tools(r))
        self.assertIn(f"needs pscat {VERSION} or newer", r.stderr)

    def test_launcher_as_pscat_mcp_bin_does_not_recurse(self):
        r = self.run_cmd("--locate", timeout=120,
                         env=self.env(PSCAT_MCP_BIN=self.path("plugin/scripts/pscat-mcp.cmd")))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertSamePath(r.stdout.strip(), self.installed())

    def test_hook(self):
        def hook(env=None):
            r = self.run_cmd("--hook", env=env)
            self.assertEqual(r.returncode, 0, r.stderr)
            return json.loads(r.stdout) if r.stdout.strip() else None

        out = hook()
        self.assertIn("downloads it", out["systemMessage"])
        self.assertIn("--locate", out["hookSpecificOutput"]["additionalContext"])
        self.assertFalse(os.path.exists(os.path.join(self.cache, TAG)), "the hook must not download")
        d = self.run_cmd("--locate").stdout.strip()
        out = hook()
        self.assertNotIn("systemMessage", out)
        self.assertIn(os.path.join(d, "pscat.exe"), out["hookSpecificOutput"]["additionalContext"])
        inst = self.path("inst")
        os.makedirs(inst)
        with open(os.path.join(inst, "pscat-mcp.cmd"), "w", newline="") as f:
            f.write(fake_cmd("99.0.0"))
        with open(os.path.join(inst, "pscat.cmd"), "w", newline="") as f:
            f.write("@echo off\r\n")
        self.assertIsNone(hook(self.env([inst])))

    def hook_command(self, backslashes=False):
        with open(HOOKS) as f:
            cmd = json.load(f)["hooks"]["SessionStart"][0]["hooks"][0]["command"]
        # Claude Code substitutes Windows paths with forward slashes, except in
        # a hook that runs through PowerShell, which keeps backslashes.
        root = self.plugin if backslashes else self.plugin.replace("\\", "/")
        return cmd.replace("${CLAUDE_PLUGIN_ROOT}", root)

    def test_hook_command_runs_in_powershell(self):
        # Without Git Bash, Claude Code runs a shell-form hook in PowerShell.
        shells = [["powershell.exe", "-NoProfile", "-Command"]]
        if shutil.which("pwsh"):
            shells.append([shutil.which("pwsh"), "-NoProfile", "-Command"])
        for shell in shells:
            for backslashes in (False, True):
                r = self.run_argv([*shell, self.hook_command(backslashes)])
                self.assertEqual(r.returncode, 0, (shell, backslashes, r.stderr))
                self.assertIn("downloads it", json.loads(r.stdout)["systemMessage"], shell)

    def git_bash_env(self):
        git_usr = os.path.join(os.path.dirname(os.path.dirname(GIT_SH)), "usr", "bin")
        return self.env([git_usr])

    @unittest.skipUnless(os.path.isfile(GIT_SH), "Git Bash not installed")
    def test_hook_command_runs_in_git_bash(self):
        # With Git Bash, Claude Code runs it there: ./pscat-mcp is then the sh
        # entry point, which hands over to the PowerShell twin.
        r = self.run_argv([GIT_SH, "-c", self.hook_command()], env=self.git_bash_env())
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("downloads it", json.loads(r.stdout)["systemMessage"])

    @unittest.skipUnless(os.path.isfile(GIT_SH), "Git Bash not installed")
    def test_entry_point_under_git_bash_serves_mcp(self):
        entry = self.path("plugin/scripts/pscat-mcp").replace("\\", "/")
        r = self.run_argv([GIT_SH, entry], env=self.git_bash_env(), stdin=MSGS)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("render_postscript", self.tools(r))
        r = self.run_argv([GIT_SH, entry, "--locate"], env=self.git_bash_env())
        self.assertSamePath(r.stdout.strip(), self.installed(), r.stderr)

    def test_pin_with_release_version_matches_plain_asset_names(self):
        self.pin(f"v{VERSION}")
        self.make_release(f"v{VERSION}", VERSION)
        r = self.run_cmd("--locate")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn(f"downloading pscat-{VERSION}-{TRIPLE}.zip", r.stderr)


if __name__ == "__main__":
    if sys.platform != "win32":
        print("skipped: the Windows launcher tests run on Windows (test_plugin_launcher.py is the Unix half)")
        sys.exit(0)
    if len(sys.argv) > 1 and not sys.argv[1].startswith("-"):
        BIN_DIR = sys.argv.pop(1)
    BIN_DIR = os.path.abspath(BIN_DIR) if BIN_DIR else ""
    if not all(os.path.isfile(os.path.join(BIN_DIR, e)) for e in ("pscat.exe", "pscat-mcp.exe")):
        sys.exit(f"usage: {sys.argv[0]} BIN_DIR (a directory with pscat.exe and pscat-mcp.exe)")
    unittest.main(verbosity=2)
