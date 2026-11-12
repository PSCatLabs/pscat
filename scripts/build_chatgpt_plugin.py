#!/usr/bin/env python3
"""Build the ChatGPT plugin ZIP (issue #1089).

    python3 scripts/build_chatgpt_plugin.py --sums SHA256SUMS
    python3 scripts/build_chatgpt_plugin.py --sums SHA256SUMS --skills-only
    python3 scripts/build_chatgpt_plugin.py --sums SHA256SUMS --out DIR --root ROOT

Writes DIR/pscat-chatgpt-plugin-<version>.zip (with --skills-only,
pscat-chatgpt-plugin-<version>-skills-only.zip; DIR defaults to dist/).
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
  server is "Desktop only" in ChatGPT).

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


def build(root, sums_path, out_dir, skills_only=False):
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

    suffix = "-skills-only" if skills_only else ""
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
    args = ap.parse_args(argv)
    try:
        print(build(args.root, args.sums, args.out, args.skills_only))
    except SystemExit as e:
        if isinstance(e.code, str):
            print(e.code, file=sys.stderr)
            return 1
        raise
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
