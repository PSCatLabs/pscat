#!/usr/bin/env python3
"""Install the plugin with the real Claude Code and Codex CLIs on Windows (#866).

    python scripts/plugin_install_check_windows.py BIN_DIR

The Windows half of scripts/plugin_install_check.sh and
codex_plugin_install_check.sh. Needs `claude` and `codex` on PATH and BIN_DIR
holding a built pscat.exe and pscat-mcp.exe; no login and no network (the
release is a fake one made from BIN_DIR, scripts/fake_plugin_release.py,
served from file://). Never touches your real config: CLAUDE_CONFIG_DIR,
CODEX_HOME, USERPROFILE and the launcher's cache are temp dirs.

- Claude Code: `plugin install` from this checkout, then `claude mcp list`
  starts the plugin's server through the manifest's extensionless
  scripts/pscat-mcp, which Claude Code resolves to pscat-mcp.cmd. Three ways:
  pscat-mcp.exe on PATH (connects, downloads nothing); not on PATH (the
  launcher downloads the fake release and connects); not on PATH and offline
  (fails).
- Codex: `plugin add`, then `codex mcp get pscat --json` must give the
  installed scripts/pscat-mcp; Codex resolves that through PATHEXT to
  pscat-mcp.cmd (codex-rs/rmcp-client/src/program_resolver.rs), so this runs
  the .cmd beside it and lists the server's tools over MCP.
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OFFLINE = "http://127.0.0.1:9"  # connection refused, like being offline
FAIL = []


def ok(msg):
    print("ok  ", msg, flush=True)


def bad(msg):
    print("FAIL", msg, flush=True)
    FAIL.append(msg)


def run(argv, env, cwd, stdin=None, timeout=300):
    exe = shutil.which(argv[0], path=env["PATH"]) or argv[0]
    return subprocess.run([exe, *argv[1:]], env=env, cwd=cwd, input=stdin, capture_output=True,
                          text=True, timeout=timeout, encoding="utf-8", errors="replace")


def main(bin_dir):
    bin_dir = os.path.abspath(bin_dir)
    work = tempfile.mkdtemp()
    try:
        check(bin_dir, work)
    finally:
        shutil.rmtree(work, ignore_errors=True)
    return 1 if FAIL else 0


def check(bin_dir, work):
    proj, home = os.path.join(work, "project"), os.path.join(work, "home")
    os.makedirs(proj)
    os.makedirs(home)
    sysroot = os.environ.get("SystemRoot", r"C:\Windows")
    tools_path = [os.path.dirname(shutil.which(t)) for t in ("claude", "codex", "node") if shutil.which(t)]
    clean_path = os.pathsep.join([*dict.fromkeys(tools_path), os.path.join(sysroot, "System32"), sysroot,
                                  os.path.join(sysroot, "System32", "WindowsPowerShell", "v1.0")])
    base = {k: v for k, v in os.environ.items()
            if not k.upper().startswith("PSCAT_") and k.upper() != "CARGO_HOME"}
    base.update(USERPROFILE=home, HOME=home, LOCALAPPDATA=os.path.join(work, "localappdata"),
                CLAUDE_CONFIG_DIR=os.path.join(work, "claude"), CODEX_HOME=os.path.join(work, "codex"))
    os.makedirs(base["CLAUDE_CONFIG_DIR"])
    os.makedirs(base["CODEX_HOME"])

    def env(path, cache, url):
        return {**base, "PATH": path, "PSCAT_CACHE_DIR": cache, "PSCAT_RELEASE_BASE_URL": url}

    e = env(clean_path, os.path.join(work, "unused"), OFFLINE)
    if shutil.which("pscat-mcp", path=clean_path):
        bad("pscat-mcp is on the stripped PATH; cannot run the negative cases")
        return

    # --- Claude Code -----------------------------------------------------------
    for argv in (["claude", "plugin", "marketplace", "add", ROOT], ["claude", "plugin", "install", "pscat@pscat"]):
        r = run(argv, e, proj)
        (ok if r.returncode == 0 else bad)(" ".join(argv[:3]) + ("" if r.returncode == 0 else f": {r.stdout}{r.stderr}"))
    with open(os.path.join(ROOT, "plugin", "pscat-release")) as f:
        tag = [line.strip() for line in f if line.strip() and not line.startswith("#")][0]
    subprocess.run([sys.executable, os.path.join(ROOT, "scripts", "fake_plugin_release.py"),
                    bin_dir, os.path.join(work, "release"), tag], check=True)
    release_url = "file:///" + os.path.join(work, "release").replace("\\", "/")

    def mcp_list(path, cache, url):
        r = run(["claude", "mcp", "list"], env(path, cache, url), proj)
        return r.stdout + r.stderr

    # Positive case first: Claude Code caches a failed connection for a while.
    out = mcp_list(bin_dir + os.pathsep + clean_path, os.path.join(work, "cache-unused"), OFFLINE)
    (ok if "plugin:pscat:pscat" in out and "Connected" in out else bad)(
        f"with pscat-mcp.exe on PATH the server connects: {out.strip()}")
    (bad if os.path.exists(os.path.join(work, "cache-unused")) else ok)(
        "with pscat-mcp.exe on PATH nothing is downloaded")
    cache = os.path.join(work, "cache")
    out = mcp_list(clean_path, cache, release_url)
    (ok if "Connected" in out else bad)(f"without pscat-mcp the launcher downloads {tag} (local) and connects: {out.strip()}")
    installed = os.path.join(cache, tag, "x86_64-pc-windows-msvc", "pscat-mcp.exe")
    (ok if os.path.isfile(installed) else bad)(f"the download is cached at {installed}")
    out = mcp_list(clean_path, os.path.join(work, "cache-offline"), OFFLINE)
    (ok if "Failed" in out else bad)(f"without pscat-mcp and offline the server fails to start: {out.strip()}")

    # --- Codex -----------------------------------------------------------------
    for argv in (["codex", "plugin", "marketplace", "add", ROOT], ["codex", "plugin", "add", "pscat@pscat"]):
        r = run(argv, e, proj)
        (ok if r.returncode == 0 else bad)(" ".join(argv[:3]) + ("" if r.returncode == 0 else f": {r.stdout}{r.stderr}"))
    r = run(["codex", "mcp", "get", "pscat", "--json"], e, proj)
    try:
        t = json.loads(r.stdout)["transport"]
    except (ValueError, KeyError):
        bad(f"codex mcp get pscat: {r.stdout}{r.stderr}")
        return
    cmd = t["command"]
    (ok if os.path.basename(cmd) == "pscat-mcp" and not t.get("args") else bad)(
        f"Codex resolves the server to the installed entry point: {cmd} {t.get('args')}")
    twin = cmd + ".cmd"
    (ok if os.path.isfile(twin) else bad)(f"{twin} sits beside it for Codex's PATHEXT lookup")
    msgs = "".join(json.dumps(m) + "\n" for m in (
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": {"name": "codex-plugin-check", "version": "0"}}},
        {"jsonrpc": "2.0", "method": "notifications/initialized"},
        {"jsonrpc": "2.0", "id": 2, "method": "tools/list"}))
    senv = {**env(clean_path, os.path.join(work, "codex-cache"), release_url), **(t.get("env") or {})}
    p = subprocess.run([twin], input=msgs, capture_output=True, text=True, timeout=300,
                       env=senv, cwd=t.get("cwd"))
    replies = {}
    for line in p.stdout.splitlines():
        try:
            m = json.loads(line)
            replies[m.get("id")] = m
        except ValueError:
            bad(f"non-JSON on the MCP channel: {line!r}")
    names = [x["name"] for x in replies.get(2, {}).get("result", {}).get("tools", [])]
    (ok if "render_postscript" in names else bad)(
        f"the Codex-resolved server downloads and answers tools/list ({len(names)} tools; exit {p.returncode}; {p.stderr.strip()[-300:]})")


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")  # claude prints check marks
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    sys.exit(main(sys.argv[1]))
