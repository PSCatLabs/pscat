#!/usr/bin/env python3
"""Tests for the plugin's pscat-mcp launcher (plugin/scripts/pscat-mcp.sh, #853).

    python3 scripts/test_plugin_launcher.py

No network (offline is a refused connection to 127.0.0.1:9): each test builds a fake GitHub release in a temp dir (archives for
the four Unix targets, holding a fake pscat-mcp that prints one marker line,
and SHA256SUMS) and points the launcher at it with PSCAT_RELEASE_BASE_URL=file://.
HOME, PATH and the cache are temp dirs too, so a real pscat on this machine is
not seen. A real pscat in /opt/homebrew/bin or /usr/local/bin (which the
launcher always checks) is older than the fake 9.9.9 and so is passed over.
Each test runs under every POSIX shell found (sh, dash, bash --posix).
"""
import hashlib
import io
import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LAUNCHER = os.path.join(ROOT, "plugin", "scripts", "pscat-mcp.sh")
TRIPLES = ("aarch64-apple-darwin", "x86_64-apple-darwin",
           "aarch64-unknown-linux-musl", "x86_64-unknown-linux-musl")
TAG = "v9.9.9-rc.1"
VERSION = "9.9.9-rc.1"
MARKER = "FAKE-PSCAT-MCP"

SHELLS = [["sh"]]
for extra in (["dash"], ["bash", "--posix"]):
    if shutil.which(extra[0]):
        SHELLS.append(extra)


def fake_mcp(version):
    return (f"#!/bin/sh\ncase \"$1\" in --version|-V) echo 'pscat-mcp {version}'; exit 0 ;; esac\n"
            f"echo \"{MARKER} $*\"\n")


class Launcher(unittest.TestCase):
    shell = ["sh"]

    def setUp(self):
        self.tmp = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.plugin = self.path("plugin")
        os.makedirs(self.path("plugin/scripts"))
        shutil.copy(LAUNCHER, self.path("plugin/scripts/pscat-mcp.sh"))
        self.pin(TAG)
        self.release = self.path("release")
        self.cache = self.path("cache")
        os.makedirs(self.path("home"))
        os.makedirs(self.path("empty"))
        self.make_release(TAG, VERSION)
        self.base_url = "file://" + self.release

    def path(self, rel):
        return os.path.join(self.tmp, rel)

    def pin(self, tag):
        with open(self.path("plugin/pscat-release"), "w") as f:
            f.write("# pinned release\n" + tag + "\n")

    def make_release(self, tag, version, corrupt=False):
        d = os.path.join(self.release, tag)
        os.makedirs(d, exist_ok=True)
        sums = []
        for triple in TRIPLES:
            name = f"pscat-{version}-{triple}"
            buf = io.BytesIO()
            with tarfile.open(fileobj=buf, mode="w:gz") as tar:
                for exe, body in (("pscat-mcp", fake_mcp(version.split("-")[0])),
                                  ("pscat", "#!/bin/sh\necho fake-pscat\n")):
                    data = body.encode()
                    info = tarfile.TarInfo(f"{name}/{exe}")
                    info.size, info.mode = len(data), 0o755
                    tar.addfile(info, io.BytesIO(data))
                info = tarfile.TarInfo(f"{name}/lib")
                info.type, info.mode = tarfile.DIRTYPE, 0o755
                tar.addfile(info)
            data = buf.getvalue()
            with open(os.path.join(d, name + ".tar.gz"), "wb") as f:
                f.write(data)
            digest = hashlib.sha256(data + (b"x" if corrupt else b"")).hexdigest()
            sums.append(f"{digest}  {name}.tar.gz\n")
        sums.append(f"{'0' * 64}  pscat-{version}-x86_64-pc-windows-msvc.zip\n")
        with open(os.path.join(d, "SHA256SUMS"), "w") as f:
            f.writelines(sums)

    def env(self, path_dirs=(), **extra):
        env = {
            "HOME": self.path("home"),
            "PATH": ":".join([*path_dirs, "/usr/bin", "/bin", "/usr/sbin", "/sbin"]),
            "PSCAT_CACHE_DIR": self.cache,
            "PSCAT_RELEASE_BASE_URL": self.base_url,
            "LANG": "C",
        }
        env.update(extra)
        return env

    def run_launcher(self, *args, env=None, timeout=60):
        return subprocess.run([*self.shell, self.path("plugin/scripts/pscat-mcp.sh"), *args],
                              env=env or self.env(), capture_output=True, text=True,
                              timeout=timeout, stdin=subprocess.DEVNULL, cwd=self.path("empty"))

    def installed_dirs(self):
        d = os.path.join(self.cache, TAG)
        return sorted(os.listdir(d)) if os.path.isdir(d) else []

    def fake_install(self, version, name="inst"):
        d = self.path(name)
        os.makedirs(d)
        for exe, body in (("pscat-mcp", fake_mcp(version)), ("pscat", "#!/bin/sh\n")):
            p = os.path.join(d, exe)
            with open(p, "w") as f:
                f.write(body.replace(MARKER, "INSTALLED-" + MARKER))
            os.chmod(p, 0o755)
        return d

    # --- the tests -------------------------------------------------------------

    def test_cold_then_warm_start_keeps_stdout_clean(self):
        r = self.run_launcher("--some-flag")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(r.stdout, f"{MARKER} --some-flag\n")  # nothing but the server's own output
        self.assertIn("downloading pscat-9.9.9-rc.1-", r.stderr)
        self.assertTrue(os.path.islink(os.path.join(self.cache, "bin", "pscat")))
        # Warm: the release is gone, so any fetch would fail.
        shutil.rmtree(self.release)
        r = self.run_launcher("x")
        self.assertEqual((r.returncode, r.stdout, r.stderr), (0, f"{MARKER} x\n", ""))

    def test_locate_prints_the_stable_bin_dir(self):
        r = self.run_launcher("--locate")
        self.assertEqual(r.returncode, 0, r.stderr)
        d = r.stdout.strip()
        self.assertEqual(r.stdout.count("\n"), 1)
        # One stable path, not the versioned directory (#920).
        self.assertEqual(d, os.path.join(self.cache, "bin"))
        self.assertTrue(os.access(os.path.join(d, "pscat"), os.X_OK), d)
        self.assertTrue(os.path.isdir(os.path.join(os.path.dirname(os.path.realpath(os.path.join(d, "pscat"))), "lib")))
        # A second run, and a run for another pin, still print it.
        self.assertEqual(self.run_launcher("--locate").stdout.strip(), d)

    def test_parallel_cold_starts_install_once(self):
        cmd = [*self.shell, self.path("plugin/scripts/pscat-mcp.sh"), "--locate"]
        procs = [subprocess.Popen(cmd, env=self.env(), stdout=subprocess.PIPE,
                                  stderr=subprocess.PIPE, text=True, cwd=self.path("empty"))
                 for _ in range(8)]
        outs = [p.communicate(timeout=120) for p in procs]
        self.assertEqual([p.returncode for p in procs], [0] * 8, [e for _, e in outs])
        self.assertEqual(len({o for o, _ in outs}), 1, outs)
        entries = self.installed_dirs()
        self.assertEqual([e for e in entries if not e.endswith(".d")], sorted(
            t for t in TRIPLES if any(e == t for e in entries)))
        self.assertEqual(len([e for e in entries if e.endswith(".d")]), 1, entries)
        self.assertFalse([e for e in os.listdir(self.cache) if e.startswith(".lock")])

    def test_checksum_mismatch_installs_nothing(self):
        self.make_release(TAG, VERSION, corrupt=True)
        r = self.run_launcher()
        self.assertEqual(r.returncode, 1)
        self.assertEqual(r.stdout, "")
        self.assertIn("checksum mismatch", r.stderr)
        self.assertEqual(self.installed_dirs(), [])

    def test_offline_says_so(self):
        self.base_url = "http://127.0.0.1:9"  # connection refused: offline
        r = self.run_launcher()
        self.assertEqual(r.returncode, 1)
        self.assertEqual(r.stdout, "")
        self.assertIn("offline", r.stderr)
        self.assertIn("cargo install pscat", r.stderr)

    def test_missing_release_is_not_called_offline(self):
        self.pin("v9.9.12")  # the base URL works, the tag is not there
        r = self.run_launcher()
        self.assertEqual((r.returncode, r.stdout), (1, ""))
        self.assertIn("is the release published", r.stderr)
        self.assertNotIn("offline", r.stderr)

    def test_offline_falls_back_to_an_earlier_download(self):
        self.assertEqual(self.run_launcher("--locate").returncode, 0)
        self.pin("v9.9.10")
        self.base_url = "http://127.0.0.1:9"  # connection refused: offline
        r = self.run_launcher("y")
        self.assertEqual((r.returncode, r.stdout), (0, f"{MARKER} y\n"), r.stderr)
        self.assertIn("previously downloaded", r.stderr)

    def test_installed_pscat_mcp_new_enough_is_used(self):
        inst = self.fake_install("9.9.9")
        r = self.run_launcher("z", env=self.env([inst]))
        self.assertEqual((r.returncode, r.stdout), (0, f"INSTALLED-{MARKER} z\n"), r.stderr)
        self.assertFalse(os.path.exists(os.path.join(self.cache, TAG)))
        # Also from ~/.cargo/bin, which a GUI app's PATH often lacks.
        os.makedirs(self.path("home/.cargo"))
        shutil.move(inst, self.path("home/.cargo/bin"))
        r = self.run_launcher("z")
        self.assertEqual(r.stdout, f"INSTALLED-{MARKER} z\n", r.stderr)
        self.assertIn(".cargo/bin", r.stderr)
        # And PSCAT_MCP_BIN.
        other = self.fake_install("10.0.0", "other")
        r = self.run_launcher("z", env=self.env(PSCAT_MCP_BIN=os.path.join(other, "pscat-mcp")))
        self.assertEqual(r.stdout, f"INSTALLED-{MARKER} z\n", r.stderr)
        self.assertIn("other/pscat-mcp", r.stderr)
        # PSCAT_MCP_BIN naming a file not called pscat-mcp is run as named.
        os.rename(os.path.join(other, "pscat-mcp"), os.path.join(other, "pscat-mcp-10"))
        r = self.run_launcher("z", env=self.env(PSCAT_MCP_BIN=os.path.join(other, "pscat-mcp-10")))
        self.assertEqual((r.returncode, r.stdout), (0, f"INSTALLED-{MARKER} z\n"), r.stderr)

    def test_installed_pscat_mcp_too_old_is_passed_over(self):
        inst = self.fake_install("0.6.3")
        r = self.run_launcher("w", env=self.env([inst]))
        self.assertEqual((r.returncode, r.stdout), (0, f"{MARKER} w\n"), r.stderr)
        self.assertIn("needs pscat 9.9.9 or newer", r.stderr)

    def test_stale_lock_is_cleared(self):
        p = subprocess.Popen(["true"])
        p.wait()
        lock = os.path.join(self.cache, f".lock-{TAG}-")
        for t in TRIPLES:
            os.makedirs(lock + t)
            with open(os.path.join(lock + t, "pid"), "w") as f:
                f.write(f"{p.pid}\n")
        r = self.run_launcher("--locate", timeout=30)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("stale download lock", r.stderr)

    def test_hook(self):
        def hook(env=None):
            r = self.run_launcher("--hook", env=env)
            self.assertEqual(r.returncode, 0, r.stderr)
            return json.loads(r.stdout) if r.stdout.strip() else None

        out = hook()
        self.assertIn("downloads it", out["systemMessage"])
        self.assertIn("--locate", out["hookSpecificOutput"]["additionalContext"])
        self.assertFalse(os.path.exists(os.path.join(self.cache, TAG)), "the hook must not download")
        self.assertIn(os.path.join(self.cache, "bin", "pscat"), out["hookSpecificOutput"]["additionalContext"])
        self.assertIn("about 14 MB", out["systemMessage"])
        d = self.run_launcher("--locate").stdout.strip()
        out = hook()
        self.assertNotIn("systemMessage", out)
        self.assertIn(d + "/pscat", out["hookSpecificOutput"]["additionalContext"])
        self.assertIsNone(hook(self.env([self.fake_install("9.9.9")])))

    # --- pscat on PATH (#919) -----------------------------------------------------

    def local_bin(self):
        d = self.path("home/.local/bin")
        os.makedirs(d, exist_ok=True)
        return d

    def bin_in(self, d, name):
        return os.path.join(d, name)

    def test_locate_links_pscat_into_local_bin_on_path(self):
        d = self.local_bin()
        r = self.run_launcher("--locate", env=self.env([d]))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(r.stdout.strip(), os.path.join(self.cache, "bin"))
        for n in ("pscat", "pscat-mcp"):
            self.assertEqual(os.readlink(self.bin_in(d, n)), os.path.join(self.cache, "bin", n))
        self.assertIn("on PATH", r.stderr)
        # The link works, and a second run changes nothing.
        out = subprocess.run([self.bin_in(d, "pscat-mcp"), "a"], capture_output=True, text=True)
        self.assertEqual(out.stdout, f"{MARKER} a\n")
        before = os.lstat(self.bin_in(d, "pscat")).st_ino
        self.assertEqual(self.run_launcher("--locate", env=self.env([d])).returncode, 0)
        self.assertEqual(os.lstat(self.bin_in(d, "pscat")).st_ino, before)

    def test_server_start_links_too_and_stdout_stays_clean(self):
        d = self.local_bin()
        r = self.run_launcher("q", env=self.env([d]))
        self.assertEqual((r.returncode, r.stdout), (0, f"{MARKER} q\n"), r.stderr)
        self.assertTrue(os.path.islink(self.bin_in(d, "pscat")))

    def test_home_bin_is_used_when_local_bin_is_not_on_path(self):
        d = self.path("home/bin")
        os.makedirs(d)
        self.local_bin()  # exists but is not on PATH
        self.assertEqual(self.run_launcher("--locate", env=self.env([d])).returncode, 0)
        self.assertTrue(os.path.islink(self.bin_in(d, "pscat")))
        self.assertFalse(os.path.lexists(self.bin_in(self.path("home/.local/bin"), "pscat")))

    def test_no_link_without_a_home_dir_on_path_and_system_dirs_untouched(self):
        sysdir = self.path("sysbin")
        os.makedirs(sysdir)
        self.local_bin()  # exists, writable, but not on PATH
        r = self.run_launcher("--locate", env=self.env([sysdir]))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(r.stdout.strip(), os.path.join(self.cache, "bin"))
        self.assertEqual(os.listdir(sysdir), [])
        self.assertEqual(os.listdir(self.path("home/.local/bin")), [])
        self.assertIn("run it as " + os.path.join(self.cache, "bin", "pscat"), r.stderr)

    def test_unwritable_dir_falls_back_to_the_path(self):
        if os.geteuid() == 0:
            self.skipTest("root can write anywhere")
        d = self.local_bin()
        os.chmod(d, 0o555)
        self.addCleanup(os.chmod, d, 0o755)
        r = self.run_launcher("--locate", env=self.env([d]))
        self.assertEqual((r.returncode, r.stdout.strip()), (0, os.path.join(self.cache, "bin")), r.stderr)
        self.assertEqual(os.listdir(d), [])

    def test_a_foreign_pscat_is_never_overwritten(self):
        d = self.local_bin()
        for kind in ("file", "symlink", "dangling", "dir"):
            mine = self.bin_in(d, "pscat")
            if os.path.lexists(mine):
                shutil.rmtree(mine) if os.path.isdir(mine) and not os.path.islink(mine) else os.remove(mine)
            other = self.path("theirs")
            if kind == "file":
                with open(mine, "w") as f:
                    f.write("#!/bin/sh\necho theirs\n")
                os.chmod(mine, 0o755)
            elif kind == "symlink":
                with open(other, "w") as f:
                    f.write("x")
                os.symlink(other, mine)
            elif kind == "dangling":
                os.symlink(self.path("nowhere"), mine)
            else:
                os.mkdir(mine)
            snap = os.lstat(mine).st_ino
            r = self.run_launcher("--locate", env=self.env([d]))
            self.assertEqual(r.returncode, 0, (kind, r.stderr))
            self.assertEqual(os.lstat(mine).st_ino, snap, kind)
            if kind == "file":
                with open(mine) as f:
                    self.assertEqual(f.read(), "#!/bin/sh\necho theirs\n")
            self.assertNotIn("is on PATH (linked", r.stderr, kind)

    def test_a_pscat_elsewhere_on_path_is_left_alone(self):
        other = self.path("other")
        os.makedirs(other)
        p = os.path.join(other, "pscat")
        with open(p, "w") as f:
            f.write("#!/bin/sh\necho theirs\n")
        os.chmod(p, 0o755)
        d = self.local_bin()
        r = self.run_launcher("--locate", env=self.env([d, other]))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertFalse(os.path.lexists(self.bin_in(d, "pscat")))
        self.assertIn("left alone", r.stderr)

    def test_our_old_link_is_refreshed_and_opt_out_works(self):
        d = self.local_bin()
        env = self.env([d], PSCAT_NO_PATH_LINK="1")
        self.assertEqual(self.run_launcher("--locate", env=env).returncode, 0)
        self.assertEqual(os.listdir(d), [])
        # A link of ours that dangles (the cache was wiped) is made good again.
        mine = self.bin_in(d, "pscat")
        os.symlink(os.path.join(self.cache, "bin", "pscat"), mine)
        shutil.rmtree(self.cache)
        r = self.run_launcher("--locate", env=self.env([d]))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue(os.access(mine, os.X_OK))

    def test_bin_names_the_release_in_use(self):
        self.assertEqual(self.run_launcher("--locate").returncode, 0)
        self.pin("v9.9.11")
        self.make_release("v9.9.11", "9.9.11")
        self.assertEqual(self.run_launcher("--locate").returncode, 0)
        self.assertIn("v9.9.11", os.readlink(os.path.join(self.cache, "bin", "pscat")))
        self.pin(TAG)  # back to the first pin: bin/ follows, no download
        shutil.rmtree(self.release)
        self.assertEqual(self.run_launcher("--locate").returncode, 0)
        self.assertIn(TAG, os.readlink(os.path.join(self.cache, "bin", "pscat")))

    def test_shim_downloads_on_first_use_then_becomes_a_link(self):
        d = self.local_bin()
        env = self.env([d])
        r = self.run_launcher("--hook", env=env)
        self.assertEqual(r.returncode, 0, r.stderr)
        out = json.loads(r.stdout)
        ctx = out["hookSpecificOutput"]["additionalContext"]
        self.assertIn(d, ctx)
        self.assertIn(os.path.join(self.cache, "bin", "pscat"), ctx)
        self.assertIn("downloads it", out["systemMessage"])
        shim = self.bin_in(d, "pscat")
        self.assertFalse(os.path.islink(shim))
        self.assertTrue(os.access(shim, os.X_OK))
        self.assertFalse(os.path.lexists(self.bin_in(d, "pscat-mcp")))
        self.assertFalse(os.path.exists(os.path.join(self.cache, TAG)), "the hook must not download")
        # Rewriting it on the next session is a no-op.
        ino = os.lstat(shim).st_ino
        self.assertEqual(self.run_launcher("--hook", env=env).returncode, 0)
        self.assertEqual(os.lstat(shim).st_ino, ino)
        # First use: downloads, runs the real pscat, and the shim is now a link.
        run = subprocess.run([shim], env=env, capture_output=True, text=True, timeout=60)
        self.assertEqual((run.returncode, run.stdout), (0, "fake-pscat\n"), run.stderr)
        self.assertIn("downloading pscat-9.9.9-rc.1-", run.stderr)
        self.assertEqual(os.readlink(shim), os.path.join(self.cache, "bin", "pscat"))
        self.assertEqual(os.readlink(self.bin_in(d, "pscat-mcp")), os.path.join(self.cache, "bin", "pscat-mcp"))
        # And from here the hook has nothing to say: pscat-mcp is on PATH.
        self.assertEqual(self.run_launcher("--hook", env=env).stdout, "")

    def test_shim_offline_fails_clearly_and_foreign_pscat_blocks_the_shim(self):
        d = self.local_bin()
        env = self.env([d])
        self.assertEqual(self.run_launcher("--hook", env=env).returncode, 0)
        shim = self.bin_in(d, "pscat")
        self.base_url = "http://127.0.0.1:9"
        run = subprocess.run([shim], env=self.env([d]), capture_output=True, text=True, timeout=60)
        self.assertEqual(run.returncode, 127)
        self.assertIn("could not download it", run.stderr)
        self.assertEqual(run.stdout, "")
        # A real pscat found on PATH means no shim at all.
        os.remove(shim)
        other = self.path("other")
        os.makedirs(other)
        with open(os.path.join(other, "pscat"), "w") as f:
            f.write("#!/bin/sh\n")
        os.chmod(os.path.join(other, "pscat"), 0o755)
        self.assertEqual(self.run_launcher("--hook", env=self.env([d, other])).returncode, 0)
        self.assertFalse(os.path.lexists(shim))

    def test_shim_survives_a_quote_in_the_plugin_path(self):
        q = self.path("it's here")
        shutil.copytree(self.path("plugin"), q)
        d = self.local_bin()
        r = subprocess.run([*self.shell, os.path.join(q, "scripts", "pscat-mcp.sh"), "--hook"], env=self.env([d]),
                           capture_output=True, text=True, stdin=subprocess.DEVNULL, cwd=self.path("empty"))
        self.assertEqual(r.returncode, 0, r.stderr)
        run = subprocess.run([self.bin_in(d, "pscat")], env=self.env([d]), capture_output=True, text=True, timeout=60)
        self.assertEqual((run.returncode, run.stdout), (0, "fake-pscat\n"), run.stderr)

    def test_pin_with_release_version_matches_plain_asset_names(self):
        self.pin("v9.9.11")
        self.make_release("v9.9.11", "9.9.11")
        r = self.run_launcher("v")
        self.assertEqual((r.returncode, r.stdout), (0, f"{MARKER} v\n"), r.stderr)


def load_tests(loader, tests, pattern):
    suite = unittest.TestSuite()
    for shell in SHELLS:
        cls = type(f"Launcher_{shell[0]}", (Launcher,), {"shell": shell})
        suite.addTests(loader.loadTestsFromTestCase(cls))
    return suite


if __name__ == "__main__":
    if not (sys.platform.startswith("linux") or sys.platform == "darwin"):
        print("skipped: the launcher is POSIX sh for macOS and Linux")
        sys.exit(0)
    unittest.main(verbosity=1)
