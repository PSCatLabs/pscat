#!/usr/bin/env python3
"""Build the ChatGPT plugin ZIP (issue #1089).

    python3 scripts/build_chatgpt_plugin.py --sums SHA256SUMS
    python3 scripts/build_chatgpt_plugin.py --sums SHA256SUMS --skills-only
    python3 scripts/build_chatgpt_plugin.py --sums SHA256SUMS --remote URL --remote-auth none
    python3 scripts/build_chatgpt_plugin.py --sums SHA256SUMS --out DIR --root ROOT

URL is the hosted server's endpoint, https://<your-pscat-mcp-host>/mcp.

Writes DIR/pscat-chatgpt-plugin-<version>.zip (with --skills-only,
pscat-chatgpt-plugin-<version>-skills-only.zip; with --remote,
pscat-chatgpt-plugin-<version>-remote.zip; DIR defaults to dist/).
`--sums` is the pinned release's SHA256SUMS: get it with
`gh release download "$(grep '^v' plugin/pscat-release)" -R PSCatLabs/pscat -p SHA256SUMS`.
The build itself makes no network request.

The ZIP is the Agent Plugins 1.0 layout with `plugin.json` at its root:

- every tracked file under `plugin/` except `hooks/` (hooks do not run in
  ChatGPT in the cloud, and a public-submission ZIP may not contain them),
  `README.md`, and Claude Code's `.claude-plugin/plugin.json` (the icon
  beside it stays);
- every tracked file under `plugin-chatgpt/`, the ChatGPT-only additions
  (the `pscat-chatgpt-setup` skill). Their `{{PSCAT_RELEASE}}`,
  `{{PSCAT_VERSION}}` and `{{SHA256:<target>}}` placeholders are filled in
  from `plugin/pscat-release` and `--sums`; any other `{{...}}` is an error;
- with `--skills-only`, no `mcp.json` (a plugin that declares a local MCP
  server is "Desktop only" in ChatGPT);
- with `--remote URL`, an `mcp.json` generated for the hosted pscat-mcp
  server in place of the local one: a `streamable-http` entry named `pscat`
  (#1240; notes/1240-chatgpt-remote-mcp.md). The URL is an argument, never
  committed, and `--remote-auth` is required, because what ChatGPT can send
  differs by choice: `none` declares no authentication; `bearer` declares a
  fixed bearer token (`api_key`), which the manifest schema accepts but
  ChatGPT's connection form cannot send (the build says so). The build never
  puts a token, or a `headers` entry, in the ZIP.

The other skills are copied unchanged. Only tracked files are used (`git
ls-files`), so caches and untracked files never ship. The ZIP is
reproducible: entries sorted, timestamps fixed at 1980-01-01, modes 0644
or 0755 from Git's executable bit. Exit 0 and print the path, or 1 with
the problems.
"""

import argparse
import json
import pathlib
import re
import subprocess
import sys
import urllib.parse
import zipfile

ZIP_TIME = (1980, 1, 1, 0, 0, 0)
PLACEHOLDER = re.compile(r"\{\{([^{}]*)\}\}")
TARGETS = ("x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl")


def tracked(root, top):
    """(path relative to `top`, executable) for each tracked file under `top`."""
    out = subprocess.run(
        ["git", "-C", str(root), "ls-files", "-s", "-z", "--", top],
        check=True, capture_output=True, text=True,
    ).stdout
    files = []
    for rec in filter(None, out.split("\0")):
        meta, path = rec.split("\t", 1)
        mode = meta.split()[0]
        if mode == "120000":
            raise SystemExit(f"{path}: symlinks cannot ship in the plugin ZIP")
        files.append((path[len(top) + 1:], mode == "100755"))
    return files


def excluded(rel, skills_only):
    return (rel.startswith("hooks/") or rel in ("README.md", ".claude-plugin/plugin.json")
            or (skills_only and rel == "mcp.json"))


REMOTE_AUTH = {
    "none": {"type": "none"},
    "bearer": {"type": "api_key", "headerScheme": "bearer"},
}


def check_remote_url(url):
    """The hosted server's MCP endpoint, or a SystemExit saying what is wrong.

    OpenAI wants a public HTTPS endpoint "without embedded credentials or
    fragments"; a query string is refused too, so a token cannot ride in it."""
    u = urllib.parse.urlsplit(url)
    if u.scheme != "https" or not u.hostname:
        raise SystemExit(f"--remote {url!r}: expected an https:// URL with a host")
    if u.username is not None or u.password is not None or "@" in u.netloc:
        raise SystemExit("--remote: the URL must not carry credentials (user:password@)")
    if u.query or u.fragment or "?" in url or "#" in url:
        raise SystemExit("--remote: the URL must not have a query string or fragment "
                         "(a token must never travel in the URL)")
    if re.search(r"[<>{}\s]", url):
        raise SystemExit(f"--remote {url!r}: replace the placeholder with the real host")
    if u.path in ("", "/"):
        raise SystemExit("--remote: include the MCP path, usually /mcp")
    return url


def remote_mcp_json(url, auth):
    """The ZIP's mcp.json for the hosted server (Agent Plugins 1.0)."""
    if auth not in REMOTE_AUTH:
        raise SystemExit(f"--remote-auth: expected one of {', '.join(REMOTE_AUTH)}")
    entry = {
        "type": "streamable-http",
        "url": check_remote_url(url),
        "extensions": {"com.openai": {"auth": dict(REMOTE_AUTH[auth])}},
    }
    doc = {"$schema": "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
           "mcpServers": {"pscat": entry}}
    return (json.dumps(doc, indent=2) + "\n").encode()


def release_tag(root):
    lines = [l.strip() for l in (root / "plugin/pscat-release").read_text().splitlines()]
    tags = [l for l in lines if l and not l.startswith("#")]
    if len(tags) != 1 or not re.fullmatch(r"v\d+\.\d+\.\d+(-rc\.\d+)?", tags[0]):
        raise SystemExit("plugin/pscat-release: expected one tag vX.Y.Z or vX.Y.Z-rc.N")
    return tags[0]


def read_sums(path, version):
    sums = {}
    for line in pathlib.Path(path).read_text().splitlines():
        parts = line.split()
        if len(parts) == 2 and re.fullmatch(r"[0-9a-f]{64}", parts[0]):
            sums[parts[1].lstrip("*")] = parts[0]
    found = {}
    for t in TARGETS:
        name = f"pscat-{version}-{t}.tar.gz"
        if name not in sums:
            raise SystemExit(f"{path}: no SHA-256 for {name} (is it the SHA256SUMS of v{version}?)")
        found[t] = sums[name]
    return found


def fill(text, where, values):
    def sub(m):
        key = m.group(1)
        if key not in values:
            raise SystemExit(f"{where}: unknown placeholder {{{{{key}}}}}")
        return values[key]
    return PLACEHOLDER.sub(sub, text)


def build(root, sums_path, out_dir, skills_only=False, remote=None, remote_auth=None):
    if remote is not None and skills_only:
        raise SystemExit("--remote and --skills-only cannot be combined "
                         "(one drops mcp.json, the other replaces it)")
    if remote is None and remote_auth is not None:
        raise SystemExit("--remote-auth needs --remote")
    if remote is not None:
        if remote_auth is None:
            raise SystemExit("--remote needs --remote-auth none|bearer: say how the "
                             "server is protected, there is no default")
        remote_json = remote_mcp_json(remote, remote_auth)
    root = pathlib.Path(root).resolve()
    manifest = json.loads((root / "plugin/plugin.json").read_text())
    version = manifest["version"]
    tag = release_tag(root)
    sums = read_sums(sums_path, tag[1:])
    values = {"PSCAT_RELEASE": tag, "PSCAT_VERSION": tag[1:]}
    values.update({f"SHA256:{t}": h for t, h in sums.items()})

    entries = {}
    for top, templated in (("plugin", False), ("plugin-chatgpt", True)):
        for rel, exe in tracked(root, top):
            if top == "plugin" and excluded(rel, skills_only):
                continue
            if rel in entries:
                raise SystemExit(f"{top}/{rel}: also in another source directory")
            data = (root / top / rel).read_bytes()
            if templated:
                data = fill(data.decode(), f"{top}/{rel}", values).encode()
            entries[rel] = (data, exe)
    if "plugin.json" not in entries:
        raise SystemExit("plugin/plugin.json is not tracked")
    if remote is not None:
        if "mcp.json" not in entries:
            raise SystemExit("plugin/mcp.json is not tracked")
        entries["mcp.json"] = (remote_json, False)

    suffix = "-skills-only" if skills_only else "-remote" if remote is not None else ""
    out = pathlib.Path(out_dir) / f"pscat-chatgpt-plugin-{version}{suffix}.zip"
    out.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(out, "w") as z:
        for rel in sorted(entries):
            data, exe = entries[rel]
            info = zipfile.ZipInfo(rel, ZIP_TIME)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = (0o100755 if exe else 0o100644) << 16
            info.create_system = 3  # Unix, so the modes above are honoured
            z.writestr(info, data)
    return out


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--sums", required=True, help="the pinned release's SHA256SUMS")
    ap.add_argument("--out", default="dist", help="output directory (default dist/)")
    ap.add_argument("--root", default=pathlib.Path(__file__).resolve().parent.parent)
    ap.add_argument("--skills-only", action="store_true", help="leave out mcp.json")
    ap.add_argument("--remote", metavar="URL",
                    help="replace the local MCP server with the hosted one at this https URL "
                         "(its /mcp endpoint, e.g. https://<your-pscat-mcp-host>/mcp)")
    ap.add_argument("--remote-auth", choices=sorted(REMOTE_AUTH),
                    help="how the hosted server is protected (required with --remote): none, "
                         "or bearer (a fixed token; ChatGPT cannot send one, see "
                         "docs/guides/chatgpt-hosted.md)")
    args = ap.parse_args(argv)
    try:
        out = build(args.root, args.sums, args.out, args.skills_only, args.remote,
                    args.remote_auth)
        if args.remote_auth == "bearer":
            print("warning: ChatGPT's connection form has no bearer-token input; this ZIP "
                  "declares one but ChatGPT cannot send it (docs/guides/chatgpt-hosted.md)",
                  file=sys.stderr)
        print(out)
    except SystemExit as e:
        if isinstance(e.code, str):
            print(e.code, file=sys.stderr)
            return 1
        raise
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
