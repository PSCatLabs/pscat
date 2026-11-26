#!/usr/bin/env python3
"""Build the ChatGPT plugin ZIP (issue #1089).

    python3 scripts/build_chatgpt_plugin.py --sums SHA256SUMS
    python3 scripts/build_chatgpt_plugin.py --sums SHA256SUMS --skills-only
    python3 scripts/build_chatgpt_plugin.py --sums SHA256SUMS --remote URL --remote-auth oauth
    python3 scripts/build_chatgpt_plugin.py --sums SHA256SUMS --out DIR --root ROOT
    python3 scripts/build_chatgpt_plugin.py --sums SHA256SUMS --remote URL --remote-auth oauth --production

URL is the hosted server's endpoint, https://<your-pscat-mcp-host>/mcp.

Writes DIR/pscat-chatgpt-plugin-<version>.zip (with --skills-only,
pscat-chatgpt-plugin-<version>-skills-only.zip; with --remote,
pscat-cloud-chatgpt-plugin-<version>.zip; DIR defaults to dist/).
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
  server in place of the local one: a `streamable-http` entry named
  `pscat-cloud` (#1240; notes/1240-chatgpt-remote-mcp.md). The plugin is
  renamed `pscat-cloud` too (plugin.json `name`, display name, short
  description and description), so it installs beside the local `pscat`
  plugin: ChatGPT refuses a second plugin of the same name (#1254). The
  cloud profile (#1256) also leaves out everything that installs or runs a
  local pscat (`scripts/`, `pscat-release`, the `pscat-chatgpt-setup` skill)
  and rewrites the shared skills' local-only wording from the overlay in
  `plugin-chatgpt/cloud/` (see "Cloud overlay" below). The URL is an argument, never
  committed, and `--remote-auth` is required, because what ChatGPT can send
  differs by choice: `oauth` declares OAuth, which is what the production
  server (https://mcp.pscat.dev/mcp) uses (#1431); `none` declares no
  authentication, which is right only for a server that has none, and ChatGPT
  cannot connect it to one that answers 401; `bearer` declares a fixed bearer
  token (`api_key`), which the manifest schema accepts but ChatGPT's
  connection form cannot send (the build says so). The build never puts a
  token, a client secret or a `headers` entry in the ZIP. `oauth` writes
  `{"type": "oauth", "client": {"mode": "cimd"}}`: OpenAI's "prefer CIMD"
  setting, with the endpoints and the registration left to discovery (the
  server's protected-resource metadata), and DCR still the fallback.

Cloud overlay (`--remote` only). `plugin/skills/` is also what Claude Code and
Codex install, so it cannot carry cloud-only markers, and the local and
`--skills-only` ZIPs must stay byte for byte what they were. Instead
`plugin-chatgpt/cloud/<path of a ZIP file>` holds edits that the remote build
applies to that file, in the order written; the directory itself never ships.
A line of the form `<!-- cloud: OP "ANCHOR" -->` starts an edit, and the lines
up to the next such line are its new text (blank lines at either end do not
count). OP is one of:

- `replace-file` (no anchor, the only edit in its file): the file becomes the
  new text;
- `sub`: the text ANCHOR, which must occur exactly once, becomes the new text;
- `replace` / `drop`: a section or a block. An ANCHOR starting with `#` is a
  heading line, and the section is that heading through the line before the next
  heading. Any other ANCHOR is text found in exactly one blank-line-separated
  block, and that block is the target. Neither may sit inside a code fence;
- `replace-item` / `drop-item`: the one line that starts with ANCHOR, with the
  indented lines that continue it (a wrapped list item).

The edits that do not need blocks (`sub`, `replace-item`, `drop-item`) apply
first, in the order written, each to the result of the one before; `replace` and
`drop` are then resolved together against that result. A line that starts with
`<!-- cloud:` but is not one of these forms fails the build. An anchor that
matches nothing, or twice, fails the build, so a change to the
shared skill's wording that the overlay depended on is caught by
`python3 scripts/test_build_chatgpt_plugin.py`.

`--production` (with `--remote`) is the release workflow's switch (#1253): it
refuses a URL that is empty, not https, or that names the staging service
(anything containing `run.app`) or a reserved or example host, and the message
names the repository variable `PSCAT_MCP_PROD_URL` that the release reads the
URL from. Nothing is built when it refuses.

The other files are copied unchanged. Only tracked files are used (`git
ls-files`), so caches and untracked files never ship. The ZIP is
reproducible: entries sorted, timestamps fixed at 1980-01-01, modes 0644
or 0755 from Git's executable bit. Exit 0 and print the path, or 1 with
the problems.
"""

import argparse
import ipaddress
import json
import pathlib
import re
import subprocess
import sys
import urllib.parse
import zipfile

ZIP_TIME = (1980, 1, 1, 0, 0, 0)
CLOUD_DIR = "cloud/"  # under plugin-chatgpt/: the --remote overlay, never shipped
# What the --remote build leaves out: it installs and runs no local pscat.
REMOTE_EXCLUDE = ("scripts/", "pscat-release", "skills/pscat-chatgpt-setup/")
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
    # OpenAI's plugin docs (build/plugins, "Configure MCP authentication"): `client`
    # is optional, `mode` cimd means "prefer a client ID metadata document" and
    # does not disable the DCR fallback. No client ID, secret or endpoint override:
    # discovery supplies them, so nothing here is specific to one host (#1431).
    "oauth": {"type": "oauth", "client": {"mode": "cimd"}},
}


def check_remote_url(url):
    """The hosted server's MCP endpoint, or a SystemExit saying what is wrong.

    OpenAI wants a public HTTPS endpoint "without embedded credentials or
    fragments"; a query string is refused too, so a token cannot ride in it.
    A token in the path is not detectable here: pass the endpoint only."""
    if not url.isascii() or any(ord(c) <= 32 or ord(c) == 127 for c in url):
        raise SystemExit("--remote: the URL must be plain ASCII with no whitespace or "
                         "control characters (use the xn-- form for an international host)")
    if re.search(r"[<>{}]", url):
        raise SystemExit(f"--remote {url!r}: replace the placeholder with the real host")
    try:
        u = urllib.parse.urlsplit(url)
        u.port  # raises ValueError for a bad port
    except ValueError as e:
        raise SystemExit(f"--remote {url!r}: not a valid URL ({e})")
    if u.scheme != "https" or not u.hostname:
        raise SystemExit(f"--remote {url!r}: expected an https:// URL with a host")
    if u.username is not None or u.password is not None or "@" in u.netloc:
        raise SystemExit("--remote: the URL must not carry credentials (user:password@)")
    if u.query or u.fragment or "?" in url or "#" in url:
        raise SystemExit("--remote: the URL must not have a query string or fragment "
                         "(a token must never travel in the URL)")
    if not u.path.strip("/"):
        raise SystemExit("--remote: include the MCP path, usually /mcp")
    # OpenAI needs a public endpoint; a loopback or private address cannot be one.
    try:
        addr = ipaddress.ip_address(u.hostname)
    except ValueError:
        addr = None
    if (addr is not None and not addr.is_global) or u.hostname == "localhost" \
            or u.hostname.endswith(".localhost"):
        raise SystemExit(f"--remote {url!r}: ChatGPT needs a public host, not a local or "
                         "private address")
    return u._replace(scheme="https", netloc=u.netloc.lower()).geturl()


# Hosts that can never be the production server (the --production check).
NOT_PRODUCTION_SUFFIXES = (".invalid", ".example", ".test", ".localhost", ".local")
NOT_PRODUCTION_HOSTS = ("example.com", "example.org", "example.net")


def check_production_url(url):
    """The production pscat-mcp endpoint the release ships in the plugin ZIP, or
    a SystemExit. Stricter than check_remote_url: never the staging Cloud Run
    service (a `*.run.app` address), never a placeholder host (#1253). `url` is
    the value of the repository variable PSCAT_MCP_PROD_URL."""
    var = "the repository variable PSCAT_MCP_PROD_URL"
    if url is None or not url.strip():
        raise SystemExit(f"--production: {var} is unset or empty; set it to the production "
                         "pscat-mcp endpoint (https://<host>/mcp), see docs/dev/RELEASING.md")
    if "run.app" in url.lower():
        raise SystemExit(f"--production: {var} ({url}) is a run.app address, the staging "
                         "service; the release ships the production URL only")
    if not url.lower().startswith("https://"):
        raise SystemExit(f"--production: {var} ({url}) must start with https://")
    checked = check_remote_url(url)
    host = urllib.parse.urlsplit(checked).hostname
    if host.endswith(NOT_PRODUCTION_SUFFIXES) or host in NOT_PRODUCTION_HOSTS \
            or host.endswith(tuple("." + h for h in NOT_PRODUCTION_HOSTS)):
        raise SystemExit(f"--production: {var} ({url}) names a placeholder host, not a production server")
    return checked


def remote_mcp_json(url, auth):
    """The ZIP's mcp.json for the hosted server (Agent Plugins 1.0)."""
    if auth not in REMOTE_AUTH:
        raise SystemExit(f"--remote-auth: expected one of {', '.join(REMOTE_AUTH)}")
    entry = {
        "type": "streamable-http",
        "url": check_remote_url(url),
        "extensions": {"com.openai": {"auth": json.loads(json.dumps(REMOTE_AUTH[auth]))}},
    }
    doc = {"$schema": "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
           "mcpServers": {REMOTE_NAME: entry}}
    return (json.dumps(doc, indent=2) + "\n").encode()


REMOTE_NAME = "pscat-cloud"
REMOTE_DISPLAY_NAME = "pscat cloud"
REMOTE_SHORT_DESCRIPTION = "Hosted PostScript renderer"  # OpenAI's limit is 30 characters
REMOTE_DESCRIPTION = (
    "PostScript for precision and imagination: render documents, charts and generative "
    "art to PNG, SVG and PDF with the pscat interpreter. Agent skills plus a connection "
    "to the hosted pscat-mcp server, so nothing needs to be installed.")


def remote_manifest(manifest):
    """plugin.json for the --remote build: a different plugin from the local one.

    ChatGPT refuses to install a plugin whose name is already taken (#1254), so the
    hosted build is `pscat-cloud` and the local `pscat` plugin can sit beside it. The
    local description ("Uses your installed pscat, or downloads...") is wrong here."""
    doc = json.loads(json.dumps(manifest))  # deep copy
    doc["name"] = REMOTE_NAME
    doc["description"] = REMOTE_DESCRIPTION
    iface = doc.setdefault("extensions", {}).setdefault("com.openai", {}).setdefault(
        "interface", {})
    iface["displayName"] = REMOTE_DISPLAY_NAME
    iface["shortDescription"] = REMOTE_SHORT_DESCRIPTION
    return (json.dumps(doc, indent=2, ensure_ascii=False) + "\n").encode()


DIRECTIVE = re.compile(r'^<!-- cloud: (replace-file|replace|drop|sub|replace-item|drop-item)'
                       r'(?: "(.*?)")? -->[ \t]*$', re.M | re.S)
HEADING = re.compile(r"#{1,6} ")
FENCE_LINE = re.compile(r"\s*(?:`{3,}|~{3,})")


def parse_overlay(where, text):
    """[(op, anchor, new text)] from a cloud overlay file. An anchor may span lines."""
    found = list(DIRECTIVE.finditer(text))
    rest = DIRECTIVE.sub(lambda m: "\n" * m.group(0).count("\n"), text)
    stray = re.search(r"^[ \t]*<!-- cloud:.*$", rest, re.M)
    if stray:  # a typo'd op, a missing quote or ` -->`: never ship it as text
        raise SystemExit(f"{where}: not a valid cloud edit: {stray.group(0).strip()!r}")
    lead = text[:found[0].start()] if found else text
    if re.sub(r"<!--.*?-->", "", lead, flags=re.S).strip():
        raise SystemExit(f"{where}: text before the first <!-- cloud: ... --> line")
    out = []
    for m, nxt in zip(found, found[1:] + [None]):
        op, anchor = m.group(1), m.group(2)
        if (op == "replace-file") != (anchor is None) or anchor == "":
            raise SystemExit(f"{where}: {op} "
                             f"{'needs an anchor' if anchor is None else 'takes no anchor'}")
        new = text[m.end():nxt.start() if nxt else len(text)].strip("\n")
        if op in ("drop", "drop-item") and new:
            raise SystemExit(f"{where}: {op} {anchor!r} takes no new text")
        out.append((op, anchor, new))
    if any(op == "replace-file" for op, _, _ in out) and len(out) != 1:
        raise SystemExit(f"{where}: replace-file must be the only edit in its file")
    return out


def _once(where, op, anchor, hits):
    if len(hits) != 1:
        raise SystemExit(f"{where}: {op} {anchor!r} matches {len(hits)} times, expected "
                         "exactly once (the shared text changed? update the overlay)")
    return hits[0]


def _blocks(text):
    """(blocks, tail): blank-line separated blocks, and for each whether it starts in a code fence."""
    body = text.rstrip("\n")
    blocks = re.split(r"\n{2,}", body)
    infence, inside = [], False
    for b in blocks:
        infence.append(inside)
        if sum(1 for l in b.split("\n") if FENCE_LINE.match(l)) % 2:
            inside = not inside
    return blocks, infence, text[len(body):]


def apply_overlay(where, text, overlay):
    """The shared file `text` with the overlay's edits applied (see the module docstring)."""
    ops = parse_overlay(where, overlay)
    if not ops:
        raise SystemExit(f"{where}: the overlay file has no edits")
    if ops[0][0] == "replace-file":
        return ops[0][2] + "\n"
    blocks_ops = []
    for op, anchor, new in ops:
        if op == "sub":
            _once(where, op, anchor, [0] * text.count(anchor))
            text = text.replace(anchor, new)
        elif op in ("replace-item", "drop-item"):
            lines = text.split("\n")
            i = _once(where, op, anchor, [i for i, l in enumerate(lines) if l.startswith(anchor)])
            j = i + 1
            while j < len(lines) and lines[j][:1] in (" ", "\t") and lines[j].strip():
                j += 1
            lines[i:j] = [new] if op == "replace-item" else []
            text = "\n".join(lines)
        else:
            blocks_ops.append((op, anchor, new))
    if not blocks_ops:
        return text
    blocks, infence, tail = _blocks(text)
    heads = [i for i, b in enumerate(blocks)
             if not infence[i] and HEADING.match(b.split("\n")[0])]
    ranges = []
    for op, anchor, new in blocks_ops:
        if anchor.startswith("#"):
            i = _once(where, op, anchor, [i for i in heads if blocks[i].split("\n")[0] == anchor])
            j = next((h for h in heads if h > i), len(blocks))
        else:
            i = _once(where, op, anchor, [i for i, b in enumerate(blocks) if anchor in b])
            j = i + 1
            if infence[i] or sum(1 for l in blocks[i].split("\n") if FENCE_LINE.match(l)) % 2:
                raise SystemExit(f"{where}: {op} {anchor!r} falls inside a code fence")
        ranges.append((i, j, new))
    ranges.sort()
    for (_, j, _), (i2, _, _) in zip(ranges, ranges[1:]):
        if i2 < j:
            raise SystemExit(f"{where}: two edits overlap")
    out, k = [], 0
    for i, j, new in ranges:
        out += blocks[k:i] + ([new] if new else [])
        k = j
    out += blocks[k:]
    return "\n\n".join(out) + tail


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


def build(root, sums_path, out_dir, skills_only=False, remote=None, remote_auth=None, production=False):
    if production:
        if remote is None:
            raise SystemExit("--production needs --remote")
        remote = check_production_url(remote)
    if remote is not None and skills_only:
        raise SystemExit("--remote and --skills-only cannot be combined "
                         "(one drops mcp.json, the other replaces it)")
    if remote is None and remote_auth is not None:
        raise SystemExit("--remote-auth needs --remote")
    if remote is not None:
        if remote_auth is None:
            raise SystemExit("--remote needs --remote-auth oauth|none|bearer: say how the "
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
    overlays = {}
    for top, templated in (("plugin", False), ("plugin-chatgpt", True)):
        for rel, exe in tracked(root, top):
            if top == "plugin-chatgpt" and rel.startswith(CLOUD_DIR):
                overlays[rel[len(CLOUD_DIR):]] = (root / top / rel).read_text()
                continue
            if top == "plugin" and excluded(rel, skills_only):
                continue
            if remote is not None and rel.startswith(REMOTE_EXCLUDE):
                continue
            if rel in entries:
                raise SystemExit(f"{top}/{rel}: also in another source directory")
            data = (root / top / rel).read_bytes()
            if templated:
                data = fill(data.decode(), f"{top}/{rel}", values).encode()
            entries[rel] = (data, exe)
    if remote is not None:
        for rel, overlay in sorted(overlays.items()):
            where = f"plugin-chatgpt/{CLOUD_DIR}{rel}"
            if rel not in entries:
                raise SystemExit(f"{where}: no file {rel} in the ZIP to apply it to")
            data, exe = entries[rel]
            entries[rel] = (apply_overlay(where, data.decode(), overlay).encode(), exe)
    if "plugin.json" not in entries:
        raise SystemExit("plugin/plugin.json is not tracked")
    if remote is not None:
        if "mcp.json" not in entries:
            raise SystemExit("plugin/mcp.json is not tracked")
        entries["mcp.json"] = (remote_json, False)
        entries["plugin.json"] = (remote_manifest(manifest), entries["plugin.json"][1])

    if remote is not None:
        name = f"{REMOTE_NAME}-chatgpt-plugin-{version}.zip"
    else:
        name = f"pscat-chatgpt-plugin-{version}{'-skills-only' if skills_only else ''}.zip"
    out = pathlib.Path(out_dir) / name
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
                    help="how the hosted server is protected (required with --remote): oauth "
                         "(the production server), none (a server with no sign-in), or "
                         "bearer (a fixed token; ChatGPT cannot send one, see "
                         "docs/guides/chatgpt-hosted.md)")
    ap.add_argument("--production", action="store_true",
                    help="release builds: refuse an empty, non-https, run.app (staging) or "
                         "placeholder URL (#1253)")
    args = ap.parse_args(argv)
    try:
        out = build(args.root, args.sums, args.out, args.skills_only, args.remote,
                    args.remote_auth, args.production)
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
