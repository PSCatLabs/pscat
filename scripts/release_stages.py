#!/usr/bin/env python3
"""The publishing stages of a pscat interpreter release (issue #602).

    python3 scripts/release_stages.py check-pin
    python3 scripts/release_stages.py bump-manifests 0.6.0
    python3 scripts/release_stages.py version-info 0.7.0-rc.1
    python3 scripts/release_stages.py manifest --tag v0.6.0 --commit SHA \\
        --dist dist --crate target/package/pscat-0.6.0.crate
    python3 scripts/release_stages.py run --dist dist --crate FILE \\
        --stage draft --stage crates --stage npm --stage publish --stage plugin \\
        --stage registry [--plan] [--fake DIR] [--report-dir DIR]
    python3 scripts/release_stages.py rehearse --dist dist --crate FILE

`.github/workflows/release.yml` drives these; docs/dev/RELEASING.md is the
procedure. Each stage checks what is already in its registry before it
changes anything:

- **absent** (a definite "not found" from the registry, never a failed
  call) -> publish it;
- **present and identical** (same digest, or same contents and the same
  source commit) -> skip it;
- **present and different, or present but unverifiable** -> halt (exit 3)
  for a human to reconcile. Nothing here ever overwrites, deletes or
  re-uploads a published version.

So a release that failed half way is resumed by running it again: stages
that already finished skip, and the rest run.

Stages, in release order:

- `draft`: a draft GitHub Release for the tag holding the archives,
  SHA256SUMS and release-manifest.json (the manifest goes last, so a draft
  without one is known to be incomplete).
- `crates`: `cargo publish` to crates.io.
- `npm`: `npm publish` of the tarball scripts/build_npm.sh packed beside the
  .crate (release-inputs/<name>-<version>.tgz); enabled only when
  npm/package.json exists.
- `publish`: makes the draft GitHub Release public, after re-checking it.
- `plugin`: checks every plugin manifest (Claude Code and Codex) carries
  the release version; enabled only when a plugin manifest exists.
- `registry`: `mcp-publisher publish` of server.json to the official MCP
  Registry (#569), once crates.io serves this version's README with its
  `mcp-name:` line; enabled only when server.json exists. Last, so a
  registry outage leaves everything else released and only this stage to
  re-run.

`--plan` runs the same checks without changing anything and reports what
each stage would do. `--fake DIR` swaps every registry for a directory
(tests, and the release workflow's dry run). `rehearse` runs the
end-to-end, partial-failure and mismatch scenarios against fresh fake
registries with real artifacts.

Release candidates (#854): a version `X.Y.Z-rc.N` (tag `vX.Y.Z-rc.N`) is
a GitHub **pre-release** only. `draft` creates it with prerelease=true and
make_latest=false, `publish` makes it public without ever making it the
latest release, and `crates`, `npm` and `registry` report "disabled" for it
whatever stages are asked for, so a release candidate can never reach
crates.io, npm or the MCP Registry.

Exit status: 0 done, 1 error (network, auth, a tool failed: safe to
retry), 3 halted (a mismatch: do not retry until a human looks).
"""

import argparse
import base64
import glob
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CRATE = "pscat"
MANIFEST_SCHEMA = 1
MANIFEST_NAME = "release-manifest.json"
SUMS_NAME = "SHA256SUMS"
ARCHIVE_EXTS = (".tar.gz", ".zip")
SEMVER_RE = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")
# The one pre-release form a release may carry: a release candidate (#854).
VERSION_RE = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-rc\.([1-9]\d*))?$")
SHA1_RE = re.compile(r"^[0-9a-f]{40}$")
USER_AGENT = "pscat-release (https://github.com/PSCatLabs/pscat)"
STAGES = ("draft", "crates", "npm", "publish", "plugin", "registry")
# What a release candidate runs: the GitHub pre-release, nothing else public.
PRERELEASE_STAGES = ("draft", "publish", "plugin")
REGISTRY_API = "https://registry.modelcontextprotocol.io/v0.1/servers"

EXIT_OK = 0
EXIT_ERROR = 1
EXIT_HALT = 3


class Halt(Exception):
    """A registry holds something that is not this release, or that cannot
    be shown to be. A human has to reconcile it; retrying will not help."""


class StageError(Exception):
    """Something failed that may work on a retry (network, auth, a tool)."""


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def sha256_bytes(data):
    return hashlib.sha256(data).hexdigest()


def run(cmd, cwd=None, env=None, check=True):
    """Run a command, returning its stdout. A non-zero exit is a StageError
    carrying the command's stderr."""
    try:
        p = subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True)
    except OSError as e:
        raise StageError(f"cannot run {cmd[0]}: {e}") from e
    if check and p.returncode != 0:
        raise StageError(f"{' '.join(cmd)} failed ({p.returncode}): {p.stderr.strip() or p.stdout.strip()}")
    return p


# --------------------------------------------------------------------------
# Repository facts: version, pin, manifests
# --------------------------------------------------------------------------


def cargo_version(root=ROOT):
    """The [package] version in Cargo.toml."""
    in_package = False
    with open(os.path.join(root, "Cargo.toml"), encoding="utf-8") as f:
        for line in f:
            s = line.strip()
            if s.startswith("["):
                in_package = s == "[package]"
            elif in_package:
                m = re.match(r'^version\s*=\s*"([^"]+)"', s)
                if m:
                    return m.group(1)
    raise SystemExit("no [package] version in Cargo.toml")


def parse_version(version):
    """A sortable key for X.Y.Z or X.Y.Z-rc.N (semver precedence: an rc
    comes before its release), or None for anything else."""
    m = VERSION_RE.match(version or "")
    if not m:
        return None
    major, minor, patch, rc = m.groups()
    return (int(major), int(minor), int(patch), 0 if rc else 1, int(rc or 0))


def is_prerelease(version):
    """True for X.Y.Z-rc.N, False for X.Y.Z; exits on anything else."""
    if parse_version(version) is None:
        raise SystemExit(f"{version} is not X.Y.Z or X.Y.Z-rc.N")
    return "-" in version


def release_stages_for(version):
    """The stages release.yml runs for this version."""
    return list(PRERELEASE_STAGES if is_prerelease(version) else STAGES)


# The plugin launcher's release pin, relative to the repo root (#853).
PLUGIN_PIN = os.path.join("plugin", "pscat-release")


def pinned_index(root=ROOT):
    """(tag, sha256) from src/fontpacks/pinned.rs, or None while unpinned."""
    with open(os.path.join(root, "src/fontpacks/pinned.rs"), encoding="utf-8") as f:
        text = f.read()
    if re.search(r"PINNED_INDEX:\s*Option<Pinned>\s*=\s*None\s*;", text):
        return None
    tag = re.search(r'tag:\s*"([^"]+)"', text)
    sha = re.search(r'sha256:\s*"([0-9a-f]{64})"', text)
    if not (tag and sha):
        raise SystemExit("src/fontpacks/pinned.rs: cannot read PINNED_INDEX")
    return tag.group(1), sha.group(1)


def plugin_manifests(root=ROOT):
    """Every plugin manifest in the repo (#569): Claude Code's
    `.claude-plugin/{plugin,marketplace}.json` anywhere, and Codex's
    `.agents/plugins/marketplace.json` plus the `plugin.json` (portable, at
    the plugin root) or `.codex-plugin/plugin.json` of each local plugin it
    lists."""
    out = []
    for pattern in (".claude-plugin/*.json", "**/.claude-plugin/*.json"):
        for p in glob.glob(os.path.join(root, pattern), recursive=True):
            rel = os.path.relpath(p, root)
            if rel.split(os.sep)[0] in ("target", "node_modules", ".git"):
                continue
            if os.path.basename(p) in ("plugin.json", "marketplace.json") and p not in out:
                out.append(p)
    codex_market = os.path.join(root, ".agents", "plugins", "marketplace.json")
    if os.path.exists(codex_market):
        out.append(codex_market)
        with open(codex_market, encoding="utf-8") as f:
            entries = json.load(f).get("plugins") or []
        for entry in entries:
            src = entry.get("source") if isinstance(entry, dict) else None
            if isinstance(src, dict) and src.get("source") == "local":
                src = src.get("path")
            if not isinstance(src, str):
                continue  # a git, url or npm source is not in this repo
            for name in ("plugin.json", os.path.join(".codex-plugin", "plugin.json")):
                p = os.path.normpath(os.path.join(root, src, name))
                if os.path.exists(p) and p not in out:
                    out.append(p)
    return sorted(out)


def server_manifest(root=ROOT):
    """The MCP Registry entry, server.json at the repo root (#569)."""
    p = os.path.join(root, "server.json")
    return p if os.path.exists(p) else None


def npm_manifest(root=ROOT):
    p = os.path.join(root, "npm", "package.json")
    return p if os.path.exists(p) else None


def json_versions(path):
    """The version fields of a JSON manifest, as (key path, value) pairs:
    the top-level `version`, marketplace `metadata.version` and
    `plugins[].version`, and MCP Registry `packages[].version`."""
    with open(path, encoding="utf-8") as f:
        data = json.load(f)
    found = []
    if isinstance(data.get("version"), str):
        found.append(("version", data["version"]))
    meta = data.get("metadata")
    if isinstance(meta, dict) and isinstance(meta.get("version"), str):
        found.append(("metadata.version", meta["version"]))
    for key in ("plugins", "packages"):
        for i, item in enumerate(data.get(key) or []):
            if isinstance(item, dict) and isinstance(item.get("version"), str):
                found.append((f"{key}[{i}].version", item["version"]))
    return data, found


def set_json_versions(path, version):
    data, found = json_versions(path)
    if not found:
        return False
    if "version" in data and isinstance(data["version"], str):
        data["version"] = version
    meta = data.get("metadata")
    if isinstance(meta, dict) and isinstance(meta.get("version"), str):
        meta["version"] = version
    for key in ("plugins", "packages"):
        for item in data.get(key) or []:
            if isinstance(item, dict) and isinstance(item.get("version"), str):
                item["version"] = version
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)
        f.write("\n")
    return True


def set_cargo_version(root, version):
    path = os.path.join(root, "Cargo.toml")
    with open(path, encoding="utf-8") as f:
        lines = f.readlines()
    in_package = False
    for i, line in enumerate(lines):
        s = line.strip()
        if s.startswith("["):
            in_package = s == "[package]"
        elif in_package and re.match(r'^version\s*=\s*"', s):
            lines[i] = re.sub(r'"[^"]+"', f'"{version}"', line, count=1)
            break
    else:
        raise SystemExit("no [package] version in Cargo.toml")
    with open(path, "w", encoding="utf-8") as f:
        f.writelines(lines)


def bump_manifests(root, version):
    """Set every manifest's version; returns the files changed (relative)."""
    if not VERSION_RE.match(version):
        raise SystemExit(f"{version} is not X.Y.Z or X.Y.Z-rc.N")
    set_cargo_version(root, version)
    changed = ["Cargo.toml"]
    if set_lock_version(root, version):
        changed.append("Cargo.lock")
    if set_release_pin(root, version):
        changed.append(PLUGIN_PIN)
    return changed + bump_manifests_json_only(root, version)


def set_release_pin(root, version):
    """The release tag the plugin launcher downloads (#853, #867):
    `plugin/pscat-release` is set to `v<version>`, rc included, so a release
    PR needs no hand edit. Comments and the trailing newline are kept. A
    tree without the file (none before #853) is left alone; returns whether
    the file changed."""
    path = os.path.join(root, PLUGIN_PIN)
    if not os.path.exists(path):
        return False
    with open(path, encoding="utf-8") as f:
        lines = f.readlines()
    tags = [i for i, ln in enumerate(lines) if ln.strip() and not ln.lstrip().startswith("#")]
    if len(tags) != 1:
        raise SystemExit(f"{PLUGIN_PIN}: expected exactly one release tag line, found {len(tags)}")
    new = f"v{version}\n"
    if lines[tags[0]] == new:
        return False
    lines[tags[0]] = new
    with open(path, "w", encoding="utf-8") as f:
        f.writelines(lines)
    return True


def set_lock_version(root, version):
    """pscat's own entry in Cargo.lock, edited in place: no network, no
    re-resolution, so nothing else in the lockfile moves (CI builds with
    --locked and would catch a stale entry)."""
    path = os.path.join(root, "Cargo.lock")
    if not os.path.exists(path):
        return False
    with open(path, encoding="utf-8") as f:
        text = f.read()
    new, n = re.subn(r'(\[\[package\]\]\nname = "pscat"\nversion = )"[^"]+"', rf'\1"{version}"', text)
    if n != 1:
        raise SystemExit(f"Cargo.lock: expected one pscat package entry, found {n}")
    with open(path, "w", encoding="utf-8") as f:
        f.write(new)
    return True


def bump_manifests_json_only(root, version):
    """The plugin, npm and MCP Registry manifests that exist (none before
    #566/#569 add them)."""
    changed = []
    others = plugin_manifests(root)
    if npm_manifest(root):
        others.append(npm_manifest(root))
    if server_manifest(root):
        others.append(server_manifest(root))
    for p in others:
        if set_json_versions(p, version):
            changed.append(os.path.relpath(p, root))
    return changed


# --------------------------------------------------------------------------
# Artifacts: the release manifest and crate identity
# --------------------------------------------------------------------------


def archives_in(dist):
    names = sorted(n for n in os.listdir(dist) if n.endswith(ARCHIVE_EXTS))
    if not names:
        raise SystemExit(f"no release archives (*.tar.gz, *.zip) in {dist}")
    return names


# Members of a .crate that cargo writes at packaging time rather than copying
# from the source tree. Their bytes depend on the cargo version (the
# normalized Cargo.toml gains fields between releases), so the content digest
# leaves them out: Cargo.toml.orig is the source manifest, verbatim, and the
# VCS info's sha1 is compared on its own.
GENERATED_CRATE_FILES = ("Cargo.toml", ".cargo_vcs_info.json")


def crate_identity(path):
    """(content digest, vcs sha1 or None) of a .crate. The digest covers each
    source file's path and SHA-256, not tar metadata (mtimes) and not the
    files cargo generates, so two packagings of the same commit agree even
    when their bytes do not, or when different cargo versions made them."""
    lines = []
    sha1 = None
    has_orig = False
    try:
        with tarfile.open(path, "r:gz") as tar:
            for m in tar.getmembers():
                if not m.isfile():
                    continue
                f = tar.extractfile(m)
                data = f.read() if f else b""
                top, _, rel = m.name.partition("/")
                if rel == ".cargo_vcs_info.json":
                    try:
                        sha1 = json.loads(data).get("git", {}).get("sha1")
                    except (ValueError, AttributeError):
                        sha1 = None
                if rel == "Cargo.toml.orig":
                    has_orig = True
                if rel in GENERATED_CRATE_FILES:
                    continue
                lines.append(f"{m.name}\0{sha256_bytes(data)}\n")
    except (tarfile.TarError, OSError, EOFError) as e:
        raise Halt(f"{os.path.basename(path)} is not a readable .crate: {e}") from e
    if not has_orig:
        raise Halt(f"{os.path.basename(path)} has no Cargo.toml.orig; its source manifest cannot be compared")
    lines.sort()
    return sha256_bytes("".join(lines).encode()), sha1


def crate_readme(path):
    """README.md from the top of a .crate, as text, or None."""
    try:
        with tarfile.open(path, "r:gz") as tar:
            for m in tar.getmembers():
                if m.isfile() and m.name.partition("/")[2] == "README.md":
                    f = tar.extractfile(m)
                    return f.read().decode("utf-8", "replace") if f else None
    except (tarfile.TarError, OSError, EOFError):
        return None
    return None


def has_mcp_name(text, name):
    """Whether `text` carries the registry's ownership token `mcp-name: NAME`
    as a whole token (not the prefix of a longer name), as the registry's
    validator matches it."""
    for m in re.finditer(re.escape(f"mcp-name: {name}"), text):
        if not re.match(r"[A-Za-z0-9._/-]", text[m.end():m.end() + 1]):
            return True
    return False


def registry_identity(server):
    """What must agree between server.json and the registry's copy of a
    version: the name, the version and each package's coordinates and
    arguments (the registry adds its own metadata around them)."""
    pkgs = []
    for p in server.get("packages") or []:
        args = [(a.get("type"), a.get("name"), a.get("value")) for a in p.get("packageArguments") or []]
        pkgs.append((p.get("registryType"), p.get("identifier"), p.get("version"),
                     (p.get("transport") or {}).get("type"), args))
    return server.get("name"), server.get("version"), pkgs


def build_manifest(tag, commit, dist, crate_path):
    version = tag[1:] if tag.startswith("v") else tag
    if not VERSION_RE.match(version) or tag != f"v{version}":
        raise SystemExit(f"{tag} is not vX.Y.Z or vX.Y.Z-rc.N")
    if not SHA1_RE.match(commit):
        raise SystemExit(f"{commit} is not a full commit SHA")
    assets = []
    for name in archives_in(dist):
        p = os.path.join(dist, name)
        assets.append({"name": name, "sha256": sha256_file(p), "size": os.path.getsize(p)})
    content, sha1 = crate_identity(crate_path)
    if sha1 != commit:
        raise SystemExit(f"{os.path.basename(crate_path)} was packaged from {sha1}, not {commit}")
    manifest = {
        "manifest_schema": MANIFEST_SCHEMA,
        "name": CRATE,
        "version": version,
        "tag": tag,
        "commit": commit,
        "assets": assets,
        "crate": {
            "name": os.path.basename(crate_path),
            "sha256": sha256_file(crate_path),
            "content_sha256": content,
        },
    }
    return manifest


def sums_text(assets):
    return "".join(f"{a['sha256']}  {a['name']}\n" for a in assets)


def write_manifest(manifest, dist):
    with open(os.path.join(dist, MANIFEST_NAME), "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)
        f.write("\n")
    with open(os.path.join(dist, SUMS_NAME), "w", encoding="utf-8") as f:
        f.write(sums_text(manifest["assets"]))


def load_manifest(dist):
    with open(os.path.join(dist, MANIFEST_NAME), encoding="utf-8") as f:
        m = json.load(f)
    if m.get("manifest_schema") != MANIFEST_SCHEMA:
        raise SystemExit(f"{MANIFEST_NAME}: unknown manifest_schema {m.get('manifest_schema')}")
    return m


def release_files(manifest):
    """Asset upload order: archives, SHA256SUMS, then the manifest last."""
    return [a["name"] for a in manifest["assets"]] + [SUMS_NAME, MANIFEST_NAME]


def changelog_notes(version, root=ROOT):
    path = os.path.join(root, "CHANGELOG.md")
    body = ""
    if is_prerelease(version):
        final = version.split("-")[0]
        body = (f"**Release candidate for {final}, for testing.** It is a GitHub pre-release only: "
                f"it is not on crates.io, npm or the MCP Registry, and `cargo install pscat` still "
                f"installs the latest stable version. The changes are listed in CHANGELOG.md when "
                f"{final} is released.\n\n")
    if os.path.exists(path):
        with open(path, encoding="utf-8") as f:
            text = f.read()
        m = re.search(rf"^## \[{re.escape(version)}\][^\n]*\n(.*?)(?=^## \[|\Z)", text, re.S | re.M)
        if m:
            body = m.group(1).strip() + "\n\n"
    return (
        f"{body}Each archive carries the licences (LICENSE-MIT, LICENSE-APACHE, LICENSE-OFL), "
        "THIRD_PARTY_NOTICES.md and the dependency notices (THIRD_PARTY_LICENSES.html). "
        f"{SUMS_NAME} has the SHA-256 of every archive; {MANIFEST_NAME} records the commit "
        "they were built from. Install guide: "
        "https://github.com/PSCatLabs/pscat/blob/main/docs/guides/getting-started.md"
    )


# --------------------------------------------------------------------------
# Backends: the real registries and a fake one
# --------------------------------------------------------------------------


class RealGitHub:
    """GitHub Releases through the gh CLI. Drafts never come back from a
    get-by-tag lookup, and a tag can have several, so releases are listed
    and matched on tag_name."""

    def __init__(self, repo):
        if not repo:
            raise SystemExit("set GITHUB_REPOSITORY or pass --repo")
        self.repo = repo

    def _api(self, *args):
        return run(["gh", "api", *args]).stdout

    def tag_commit(self, tag):
        p = run(["gh", "api", f"repos/{self.repo}/commits/{tag}", "--jq", ".sha"], check=False)
        if p.returncode != 0:
            if "No commit found" in p.stderr or "HTTP 404" in p.stderr or "HTTP 422" in p.stderr:
                return None
            raise StageError(f"cannot read tag {tag}: {p.stderr.strip()}")
        return p.stdout.strip()

    def releases(self, tag):
        out = self._api(
            "--paginate",
            f"repos/{self.repo}/releases?per_page=100",
            "--jq",
            ".[] | {id, tag_name, draft, prerelease, assets: [.assets[] | {id, name, digest}]} | tojson",
        )
        found = []
        for line in out.splitlines():
            if not line.strip():
                continue
            r = json.loads(line)
            if r["tag_name"] == tag:
                found.append(r)
        return found

    def create_draft(self, tag, title, notes, prerelease=False):
        extra = ["-F", "prerelease=true", "-f", "make_latest=false"] if prerelease else []
        out = self._api(
            "-X", "POST", f"repos/{self.repo}/releases",
            "-f", f"tag_name={tag}", "-f", f"name={title}", "-f", f"body={notes}", "-F", "draft=true",
            *extra,
        )
        return json.loads(out)["id"]

    def upload(self, release_id, path, name):
        self._api(
            "-X", "POST", "-H", "Content-Type: application/octet-stream",
            f"https://uploads.github.com/repos/{self.repo}/releases/{release_id}/assets?name={name}",
            "--input", path,
        )

    def asset_bytes(self, asset):
        p = subprocess.run(
            ["gh", "api", "-H", "Accept: application/octet-stream", f"repos/{self.repo}/releases/assets/{asset['id']}"],
            capture_output=True,
        )
        if p.returncode != 0:
            raise StageError(f"cannot download {asset['name']}: {p.stderr.decode(errors='replace').strip()}")
        return p.stdout

    def asset_sha256(self, asset):
        digest = asset.get("digest") or ""
        if digest.startswith("sha256:"):
            return digest[len("sha256:"):]
        return sha256_bytes(self.asset_bytes(asset))

    def publish(self, release_id, prerelease=False):
        # A pre-release can never be "latest" on GitHub; make_latest=false
        # says so again rather than rely on that rule.
        extra = ["-F", "prerelease=true", "-f", "make_latest=false"] if prerelease else []
        self._api("-X", "PATCH", f"repos/{self.repo}/releases/{release_id}", "-F", "draft=false", *extra)


class RealCrates:
    """crates.io: the HTTP API for lookups (not the sparse index, which lags
    after a publish), `cargo publish` to publish."""

    API = "https://crates.io/api/v1/crates"
    STATIC = "https://static.crates.io/crates"

    def _get(self, url, accept=None):
        headers = {"User-Agent": USER_AGENT}
        if accept:
            headers["Accept"] = accept
        req = urllib.request.Request(url, headers=headers)
        try:
            with urllib.request.urlopen(req, timeout=60) as r:
                return r.status, r.read()
        except urllib.error.HTTPError as e:
            return e.code, e.read()
        except (urllib.error.URLError, OSError) as e:
            raise StageError(f"GET {url}: {e}") from e

    def version(self, name, version):
        """{'checksum': ...} or None only on a definite 404."""
        status, body = self._get(f"{self.API}/{name}/{version}")
        if status == 404:
            return None
        if status != 200:
            raise StageError(f"crates.io answered {status} for {name} {version}")
        try:
            v = json.loads(body)["version"]
            return {"checksum": v["checksum"], "yanked": v.get("yanked", False)}
        except (ValueError, KeyError, TypeError) as e:
            raise StageError(f"crates.io: unreadable answer for {name} {version}: {e}") from e

    def download(self, name, version, dest):
        url = f"{self.STATIC}/{name}/{name}-{version}.crate"
        for attempt in range(6):  # the CDN can trail the API for a moment after a publish
            status, body = self._get(url)
            if status == 200:
                with open(dest, "wb") as f:
                    f.write(body)
                return
            time.sleep(5 * (attempt + 1))
        raise StageError(f"cannot download {url} ({status})")

    def readme(self, name, version):
        """The README crates.io rendered for a version (HTML), which is what
        the MCP Registry reads its `mcp-name:` token from; None if it is not
        there yet. crates.io renders it in a background job after a publish,
        so give it a few minutes."""
        api = f"{self.API}/{name}/{version}/readme"
        for attempt in range(8):
            status, body = self._get(api, accept="application/json")
            if status == 200:
                try:
                    url = json.loads(body)["url"]
                except (ValueError, KeyError, TypeError) as e:
                    raise StageError(f"crates.io: unreadable README pointer for {name} {version}: {e}") from e
                status, body = self._get(url)
                if status == 200:
                    return body.decode("utf-8", "replace")
            if status not in (403, 404):  # the CDN answers 403 for a missing object
                raise StageError(f"crates.io answered {status} for the README of {name} {version}")
            time.sleep(15 * (attempt + 1))
        return None

    def publish(self, repo_root, commit, crate_path):
        head = run(["git", "rev-parse", "HEAD"], cwd=repo_root).stdout.strip()
        if head != commit:
            raise Halt(f"the checkout is at {head}, not the release commit {commit}")
        run(["cargo", "publish", "--locked"], cwd=repo_root)
        return uploaded_crate_path(repo_root, os.path.basename(crate_path))


def uploaded_crate_path(repo_root, name):
    """Where `cargo publish` left the .crate it uploaded: target/package/tmp-crate/
    on current cargo (1.96 at least), target/package/ on older ones. `cargo
    package` still writes target/package/."""
    for sub in (("tmp-crate",), ()):
        path = os.path.join(repo_root, "target", "package", *sub, name)
        if os.path.exists(path):
            return path
    raise StageError(f"cargo publish succeeded, but left no {name} under target/package; "
                     "re-run to verify against crates.io")


def npm_tarball_name(name, version):
    """`npm pack`'s file name: a scope's @ dropped and its / made a -."""
    return f"{name.lstrip('@').replace('/', '-')}-{version}.tgz"


def npm_integrity(path):
    """npm's dist.integrity of a tarball: SHA-512 of its bytes, base64."""
    h = hashlib.sha512()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return "sha512-" + base64.b64encode(h.digest()).decode()


def npm_tarball_manifest(path):
    """package/package.json from inside an npm tarball."""
    try:
        with tarfile.open(path, "r:gz") as tar:
            f = tar.extractfile("package/package.json")
            return json.loads(f.read()) if f else None
    except (tarfile.TarError, KeyError, OSError, EOFError, ValueError):
        return None


class RealNpm:
    def view(self, name, version):
        # --prefer-online: npm caches packuments, and a cached miss from
        # before the publish would hide the new version.
        p = run(["npm", "view", f"{name}@{version}", "dist.integrity", "--json", "--prefer-online"],
                check=False)
        if p.returncode == 0:
            out = p.stdout.strip()
            if not out:  # npm prints nothing for a version that does not exist
                return None
            return {"integrity": json.loads(out)}
        text = p.stdout + p.stderr
        if "E404" in text:
            return None
        raise StageError(f"npm view {name}@{version} failed: {text.strip()}")

    def publish(self, tarball):
        # The exact tarball `verify` packed, so what is published is what
        # was checked. Trusted publishing (OIDC) supplies the credential.
        run(["npm", "publish", tarball, "--provenance", "--access", "public"])
        # The registry can take minutes to list a new version (0.6.3 took
        # more than one); back off for about five before the stage's own
        # check calls it missing.
        pkg = npm_tarball_manifest(tarball) or {}
        for attempt in range(6):
            if self.view(pkg.get("name"), pkg.get("version")) is not None:
                return
            time.sleep(15 * (attempt + 1))


class RealRegistry:
    """The official MCP Registry: its public API for lookups, the pinned
    mcp-publisher that release.yml's publish job installs to publish."""

    def version(self, name, version):
        """The registry's server.json for that version, or None only on a
        definite 404."""
        url = f"{REGISTRY_API}/{urllib.parse.quote(name, safe='')}/versions/{urllib.parse.quote(version, safe='')}"
        req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT, "Accept": "application/json"})
        try:
            with urllib.request.urlopen(req, timeout=60) as r:
                body = r.read()
        except urllib.error.HTTPError as e:
            if e.code == 404:
                return None
            raise StageError(f"the MCP Registry answered {e.code} for {name} {version}") from e
        except (urllib.error.URLError, OSError) as e:
            raise StageError(f"GET {url}: {e}") from e
        try:
            return json.loads(body)["server"]
        except (ValueError, KeyError, TypeError) as e:
            raise StageError(f"the MCP Registry: unreadable answer for {name} {version}: {e}") from e

    def publish(self, server_json):
        # Only the pinned, checksum-verified copy release.yml installs, never
        # whatever mcp-publisher happens to be on PATH.
        exe = os.environ.get("MCP_PUBLISHER")
        if not exe:
            raise StageError("MCP_PUBLISHER is not set: release.yml's publish job installs and verifies "
                             "the pinned mcp-publisher (see that step's log)")
        # Logged in here, right before use, not in a workflow step of its
        # own: a failed OIDC exchange then fails only this stage, after
        # crates.io and npm are done, and a re-run retries just this.
        run([exe, "login", "github-oidc"])
        run([exe, "publish", server_json])


class Fake:
    """A directory standing in for GitHub, crates.io, npm and the MCP Registry, with the
    semantics the stages rely on: drafts are found only by listing, a
    published release is immutable, a published crate or npm version is
    immutable, and each publish is logged. `fail-once` holds an operation
    name ("github.publish", "crates.publish", ...) that fails one time."""

    def __init__(self, root):
        self.root = root
        for d in ("github/assets", "crates", "npm", "mcp-registry"):
            os.makedirs(os.path.join(root, d), exist_ok=True)

    # -- plumbing
    def _path(self, *p):
        return os.path.join(self.root, *p)

    def _load(self, rel, default):
        p = self._path(rel)
        if not os.path.exists(p):
            return default
        with open(p, encoding="utf-8") as f:
            return json.load(f)

    def _save(self, rel, data):
        with open(self._path(rel), "w", encoding="utf-8") as f:
            json.dump(data, f, indent=2)

    def _maybe_fail(self, op):
        p = self._path("fail-once")
        if os.path.exists(p):
            with open(p, encoding="utf-8") as f:
                want = f.read().strip()
            if want == op:
                os.remove(p)
                raise StageError(f"injected failure: {op}")

    def log(self, event):
        with open(self._path("publish.log"), "a", encoding="utf-8") as f:
            f.write(event + "\n")

    def events(self):
        p = self._path("publish.log")
        if not os.path.exists(p):
            return []
        with open(p, encoding="utf-8") as f:
            return [line.strip() for line in f if line.strip()]

    # -- GitHub
    def set_tag(self, tag, commit):
        tags = self._load("github/tags.json", {})
        tags[tag] = commit
        self._save("github/tags.json", tags)

    def tag_commit(self, tag):
        return self._load("github/tags.json", {}).get(tag)

    def releases(self, tag):
        out = []
        for r in self._load("github/releases.json", []):
            if r["tag_name"] == tag:
                out.append({"id": r["id"], "tag_name": tag, "draft": r["draft"],
                            "prerelease": r.get("prerelease", False),
                            "assets": [{"id": a["id"], "name": a["name"], "digest": None} for a in r["assets"]]})
        return out

    def create_draft(self, tag, title, notes, prerelease=False):
        self._maybe_fail("github.create")
        rels = self._load("github/releases.json", [])
        rid = 1 + max([r["id"] for r in rels] or [0])
        rels.append({"id": rid, "tag_name": tag, "title": title, "draft": True, "prerelease": prerelease,
                     "latest": False, "assets": []})
        self._save("github/releases.json", rels)
        self.log(f"github.create {tag}" + (" prerelease" if prerelease else ""))
        return rid

    def upload(self, release_id, path, name):
        self._maybe_fail("github.upload")
        rels = self._load("github/releases.json", [])
        r = next(r for r in rels if r["id"] == release_id)
        if not r["draft"]:
            raise StageError(f"fake: release {release_id} is published and immutable")
        if any(a["name"] == name for a in r["assets"]):
            raise StageError(f"fake: asset {name} already exists")
        aid = 1 + max([a["id"] for x in rels for a in x["assets"]] or [0])
        shutil.copyfile(path, self._path("github/assets", str(aid)))
        r["assets"].append({"id": aid, "name": name})
        self._save("github/releases.json", rels)
        self.log(f"github.upload {name}")

    def asset_bytes(self, asset):
        with open(self._path("github/assets", str(asset["id"])), "rb") as f:
            return f.read()

    def asset_sha256(self, asset):
        return sha256_bytes(self.asset_bytes(asset))

    def publish(self, release_id, prerelease=False):
        self._maybe_fail("github.publish")
        rels = self._load("github/releases.json", [])
        r = next(r for r in rels if r["id"] == release_id)
        if not r["draft"]:
            raise StageError("fake: already published")
        r["draft"] = False
        r["prerelease"] = prerelease
        # GitHub's rule: a newly published release becomes latest unless it
        # is a pre-release.
        if not prerelease:
            for x in rels:
                x["latest"] = False
            r["latest"] = True
        self._save("github/releases.json", rels)
        self.log(f"github.publish {r['tag_name']}" + (" prerelease" if prerelease else ""))

    def latest(self):
        return next((r["tag_name"] for r in self._load("github/releases.json", []) if r.get("latest")), None)

    # -- crates.io
    def version(self, name, version):
        p = self._path("crates", f"{name}-{version}.crate")
        return {"checksum": sha256_file(p), "yanked": False} if os.path.exists(p) else None

    def download(self, name, version, dest):
        shutil.copyfile(self._path("crates", f"{name}-{version}.crate"), dest)

    def publish_crate(self, crate_path):
        self._maybe_fail("crates.publish")
        dest = self._path("crates", os.path.basename(crate_path))
        if os.path.exists(dest):
            raise StageError("fake: crate version already exists")
        shutil.copyfile(crate_path, dest)
        self.log(f"crates.publish {os.path.basename(crate_path)}")
        return dest

    # -- npm
    def npm_view(self, name, version):
        return self._load(f"npm/{name}-{version}.json", None)

    def npm_publish(self, name, version, integrity):
        self._maybe_fail("npm.publish")
        self._save(f"npm/{name}-{version}.json", {"integrity": integrity})
        self.log(f"npm.publish {name}@{version}")

    # -- MCP Registry (a published version is immutable there too)
    def _registry_rel(self, name, version):
        return f"mcp-registry/{name.replace('/', '__')}-{version}.json"

    def registry_view(self, name, version):
        self._maybe_fail("registry.view")
        return self._load(self._registry_rel(name, version), None)

    def registry_publish(self, server):
        self._maybe_fail("registry.publish")
        rel = self._registry_rel(server["name"], server["version"])
        if os.path.exists(self._path(rel)):
            raise StageError("fake: server version already exists")
        self._save(rel, server)
        self.log(f"registry.publish {server['name']}@{server['version']}")


class FakeCrates:
    def __init__(self, fake):
        self.fake = fake

    def version(self, name, version):
        return self.fake.version(name, version)

    def download(self, name, version, dest):
        self.fake.download(name, version, dest)

    def publish(self, repo_root, commit, crate_path):
        return self.fake.publish_crate(crate_path)

    def readme(self, name, version):
        """crates.io's rendering, faked as the README inside the stored .crate."""
        p = self.fake._path("crates", f"{name}-{version}.crate")
        return crate_readme(p) if os.path.exists(p) else None


class FakeNpm:
    """The registry keeps each version's integrity, as npm's does."""

    def __init__(self, fake):
        self.fake = fake

    def view(self, name, version):
        return self.fake.npm_view(name, version)

    def publish(self, tarball):
        pkg = npm_tarball_manifest(tarball)
        self.fake.npm_publish(pkg["name"], pkg["version"], npm_integrity(tarball))


class FakeRegistry:
    """Like the real registry, it refuses a cargo package that crates.io
    does not have at that version."""

    def __init__(self, fake):
        self.fake = fake

    def version(self, name, version):
        return self.fake.registry_view(name, version)

    def publish(self, server_json):
        with open(server_json, encoding="utf-8") as f:
            server = json.load(f)
        for p in server.get("packages") or []:
            if p.get("registryType") == "cargo" and self.fake.version(p["identifier"], p["version"]) is None:
                raise StageError(f"fake: {p['identifier']} {p['version']} is not on crates.io")
        self.fake.registry_publish(server)


# --------------------------------------------------------------------------
# Stages
# --------------------------------------------------------------------------


class Ctx:
    def __init__(self, manifest, dist, crate_path, repo_root, github, crates, npm, plan, registry=None):
        self.m = manifest
        self.dist = dist
        self.crate_path = crate_path
        self.root = repo_root
        self.github = github
        self.crates = crates
        self.npm = npm
        self.plan = plan
        self.registry = registry
        self.prerelease = is_prerelease(manifest["version"])
        self.local = {a["name"]: a["sha256"] for a in manifest["assets"]}
        self.local[SUMS_NAME] = sha256_file(os.path.join(dist, SUMS_NAME))
        self.local[MANIFEST_NAME] = sha256_file(os.path.join(dist, MANIFEST_NAME))


def check_provenance(remote, m, where):
    for key in ("name", "version", "tag", "commit"):
        if remote.get(key) != m[key]:
            raise Halt(f"{where}: {key} is {remote.get(key)!r}, this release is {m[key]!r}")


def the_release(ctx):
    """The one release (draft or not) for the tag, or None. Two -> halt."""
    rels = ctx.github.releases(ctx.m["tag"])
    if len(rels) > 1:
        raise Halt(f"{len(rels)} GitHub releases exist for {ctx.m['tag']} (ids "
                   f"{', '.join(str(r['id']) for r in rels)}); delete the extra drafts by hand")
    return rels[0] if rels else None


def check_tag(ctx):
    commit = ctx.github.tag_commit(ctx.m["tag"])
    if commit is None:
        raise Halt(f"tag {ctx.m['tag']} does not exist on GitHub; push it first")
    if commit != ctx.m["commit"]:
        raise Halt(f"tag {ctx.m['tag']} points at {commit}, the artifacts were built from {ctx.m['commit']}")


def reference_digests(ctx, rel):
    """What each asset of an existing release must hash to. A release that
    already carries a manifest is judged by that manifest (after its
    provenance is checked against this release): a re-run whose fresh
    builds differ bit-for-bit from the draft adopts the draft rather than
    halting on it. Otherwise, by this run's own artifacts."""
    by_name = {a["name"]: a for a in rel["assets"]}
    if MANIFEST_NAME in by_name:
        raw = ctx.github.asset_bytes(by_name[MANIFEST_NAME])
        try:
            remote = json.loads(raw)
        except ValueError as e:
            raise Halt(f"the release's {MANIFEST_NAME} is not JSON: {e}") from e
        check_provenance(remote, ctx.m, f"release {ctx.m['tag']} {MANIFEST_NAME}")
        ref = {a["name"]: a["sha256"] for a in remote.get("assets", [])}
        ref[SUMS_NAME] = sha256_bytes(sums_text(remote.get("assets", [])).encode())
        ref[MANIFEST_NAME] = sha256_bytes(raw)
        return ref, True
    return dict(ctx.local), False


def verify_release_assets(ctx, rel, allow_missing):
    """Check every asset of `rel`; returns the names still to upload.
    Halts on an unexpected, different or (unless allowed) missing asset."""
    ref, adopted = reference_digests(ctx, rel)
    by_name = {a["name"]: a for a in rel["assets"]}
    extra = sorted(set(by_name) - set(ref))
    if extra:
        raise Halt(f"release {ctx.m['tag']} has assets this release does not: {', '.join(extra)}")
    for name, asset in sorted(by_name.items()):
        got = ctx.github.asset_sha256(asset)
        if got != ref[name]:
            raise Halt(f"release {ctx.m['tag']}: {name} has SHA-256 {got}, expected {ref[name]}")
    missing = [n for n in release_files_from(ref) if n not in by_name]
    if missing and not allow_missing:
        raise Halt(f"release {ctx.m['tag']} is missing {', '.join(missing)}")
    for name in missing:
        if ref[name] != ctx.local.get(name):
            raise Halt(f"release {ctx.m['tag']} is missing {name}, and this run's copy differs "
                       "from the one its manifest records; reconcile the draft by hand")
    return missing, adopted


def release_files_from(ref):
    names = [n for n in ref if n not in (SUMS_NAME, MANIFEST_NAME)]
    return sorted(names) + [SUMS_NAME, MANIFEST_NAME]


def check_prerelease_flag(ctx, rel):
    """A release for an rc tag must be a pre-release, and one for X.Y.Z
    must not be: anything else was made or edited by hand."""
    got = bool(rel.get("prerelease"))
    if got != ctx.prerelease:
        want = "a pre-release" if ctx.prerelease else "not a pre-release"
        raise Halt(f"release {ctx.m['tag']} (id {rel['id']}) has prerelease={str(got).lower()}; "
                   f"this release must be {want}. Fix the flag or delete the draft by hand")


PRERELEASE_OFF = "a release candidate is a GitHub pre-release only (#854)"


def stage_draft(ctx):
    check_tag(ctx)
    rel = the_release(ctx)
    if rel is None:
        if ctx.plan:
            return "would create", f"a draft release {ctx.m['tag']} with {len(release_files(ctx.m))} assets"
        rid = ctx.github.create_draft(ctx.m["tag"], f"pscat {ctx.m['version']}",
                                      changelog_notes(ctx.m["version"], ctx.root), prerelease=ctx.prerelease)
        for name in release_files(ctx.m):
            ctx.github.upload(rid, os.path.join(ctx.dist, name), name)
        return "published", f"draft release {ctx.m['tag']} (id {rid})"
    check_prerelease_flag(ctx, rel)
    missing, adopted = verify_release_assets(ctx, rel, allow_missing=rel["draft"])
    state = "draft" if rel["draft"] else "published release"
    if not missing:
        how = "its manifest's provenance and digests" if adopted else "this run's digests"
        return "skipped", f"{state} {ctx.m['tag']} already complete; matches {how}"
    if ctx.plan:
        return "would upload", f"{', '.join(missing)} to the draft {ctx.m['tag']}"
    for name in missing:
        ctx.github.upload(rel["id"], os.path.join(ctx.dist, name), name)
    return "published", f"completed the draft {ctx.m['tag']}: uploaded {', '.join(missing)}"


def check_crate_matches(ctx, remote_checksum, local_path, what):
    """Same bytes, or same contents from the same commit; else halt."""
    if remote_checksum == sha256_file(local_path):
        return "same .crate bytes"
    with tempfile.TemporaryDirectory() as d:
        dl = os.path.join(d, "remote.crate")
        ctx.crates.download(CRATE, ctx.m["version"], dl)
        if sha256_file(dl) != remote_checksum:
            raise Halt(f"crates.io served a .crate whose SHA-256 is not its recorded checksum ({what})")
        r_content, r_sha1 = crate_identity(dl)
    l_content, _ = crate_identity(local_path)
    if r_sha1 is None:
        raise Halt(f"{CRATE} {ctx.m['version']} on crates.io has no .cargo_vcs_info.json; "
                   "its source commit cannot be verified")
    if r_sha1 != ctx.m["commit"]:
        raise Halt(f"{CRATE} {ctx.m['version']} on crates.io was published from {r_sha1}, "
                   f"not {ctx.m['commit']}")
    if r_content != l_content:
        raise Halt(f"{CRATE} {ctx.m['version']} on crates.io has different contents from this release's .crate")
    return f"same contents and source commit {r_sha1[:12]}"


def stage_crates(ctx):
    if ctx.prerelease:
        return "disabled", f"{CRATE} {ctx.m['version']} is not published to crates.io: {PRERELEASE_OFF}"
    v = ctx.crates.version(CRATE, ctx.m["version"])
    if v is not None:
        how = check_crate_matches(ctx, v["checksum"], ctx.crate_path, "existing version")
        return "skipped", f"{CRATE} {ctx.m['version']} already on crates.io ({how})"
    if ctx.plan:
        return "would publish", f"{CRATE} {ctx.m['version']} to crates.io"
    uploaded = ctx.crates.publish(ctx.root, ctx.m["commit"], ctx.crate_path)
    v = ctx.crates.version(CRATE, ctx.m["version"])
    if v is None:
        raise StageError(f"published {CRATE} {ctx.m['version']}, but crates.io does not list it yet; re-run to verify")
    if v["checksum"] != sha256_file(uploaded):
        raise Halt(f"crates.io's checksum for {CRATE} {ctx.m['version']} is not the .crate cargo uploaded")
    l_content, _ = crate_identity(ctx.crate_path)
    u_content, u_sha1 = crate_identity(uploaded)
    if (u_content, u_sha1) != (l_content, ctx.m["commit"]):
        raise Halt("the .crate cargo published differs from the one the release workflow verified")
    return "published", f"{CRATE} {ctx.m['version']} to crates.io (sha256 {v['checksum'][:12]})"


def stage_npm(ctx):
    if ctx.prerelease:
        return "disabled", f"{ctx.m['version']} is not published to npm: {PRERELEASE_OFF}"
    pkg = npm_manifest(ctx.root)
    if not pkg:
        return "disabled", "no npm/package.json (the npm channel is not enabled)"
    with open(pkg, encoding="utf-8") as f:
        meta = json.load(f)
    if meta.get("version") != ctx.m["version"]:
        raise Halt(f"npm/package.json is version {meta.get('version')}, this release is {ctx.m['version']}")
    # The tarball verify packed (scripts/build_npm.sh --pack release-inputs),
    # beside the .crate. Never packed here: a rebuilt wasm need not be
    # byte-identical, and what is published must be what verify checked.
    tarball = os.path.join(os.path.dirname(os.path.abspath(ctx.crate_path)),
                           npm_tarball_name(meta["name"], ctx.m["version"]))
    if not os.path.exists(tarball):
        raise Halt(f"no npm tarball {os.path.basename(tarball)} beside the .crate; "
                   "the verify job packs it (scripts/build_npm.sh --pack release-inputs)")
    inner = npm_tarball_manifest(tarball)
    if not inner or (inner.get("name"), inner.get("version")) != (meta["name"], ctx.m["version"]):
        got = f"{inner.get('name')}@{inner.get('version')}" if inner else "no readable package.json"
        raise Halt(f"{os.path.basename(tarball)} holds {got}, not {meta['name']}@{ctx.m['version']}")
    local = npm_integrity(tarball)
    remote = ctx.npm.view(meta["name"], ctx.m["version"])
    if remote is not None:
        if remote["integrity"] != local:
            raise Halt(f"{meta['name']}@{ctx.m['version']} on npm has integrity {remote['integrity']}, "
                       f"this release's tarball is {local}")
        return "skipped", f"{meta['name']}@{ctx.m['version']} already on npm (same integrity)"
    if ctx.plan:
        return "would publish", f"{meta['name']}@{ctx.m['version']} to npm"
    ctx.npm.publish(tarball)
    remote = ctx.npm.view(meta["name"], ctx.m["version"])
    if remote is None:
        raise StageError(f"published {meta['name']}@{ctx.m['version']}, but npm does not list it yet; "
                         "re-run to verify")
    if remote["integrity"] != local:
        raise Halt(f"npm's integrity for {meta['name']}@{ctx.m['version']} is not the tarball's")
    return "published", f"{meta['name']}@{ctx.m['version']} to npm ({local[:19]}...)"


def stage_publish(ctx):
    check_tag(ctx)
    rel = the_release(ctx)
    if rel is None:
        if ctx.plan:
            return "would publish", f"the draft release {ctx.m['tag']} (after the draft stage creates it)"
        raise Halt(f"no release exists for {ctx.m['tag']}; run the draft stage first")
    check_prerelease_flag(ctx, rel)
    missing, _ = verify_release_assets(ctx, rel, allow_missing=ctx.plan and rel["draft"])
    kind = "pre-release" if ctx.prerelease else "release"
    if not rel["draft"]:
        return "skipped", f"{kind} {ctx.m['tag']} is already published and matches"
    if ctx.plan:
        return "would publish", f"the draft {kind} {ctx.m['tag']}"
    ctx.github.publish(rel["id"], prerelease=ctx.prerelease)
    return "published", f"{kind} {ctx.m['tag']} is public" + (" (not latest)" if ctx.prerelease else "")


def stage_plugin(ctx):
    files = plugin_manifests(ctx.root)
    if not files:
        return "disabled", "no plugin manifest (the plugin channel is not enabled)"
    wrong = []
    for p in files:
        _, found = json_versions(p)
        for key, value in found:
            if value != ctx.m["version"]:
                wrong.append(f"{os.path.relpath(p, ctx.root)} {key}={value}")
    if wrong:
        raise Halt("plugin manifests do not carry the release version (run scripts/release.sh): " + "; ".join(wrong))
    return "skipped", f"{len(files)} plugin manifest(s) already at {ctx.m['version']}"


def stage_registry(ctx):
    if ctx.prerelease:
        return "disabled", f"{ctx.m['version']} is not published to the MCP Registry: {PRERELEASE_OFF}"
    path = server_manifest(ctx.root)
    if not path:
        return "disabled", "no server.json (the MCP Registry channel is not enabled)"
    data, found = json_versions(path)
    v = ctx.m["version"]
    wrong = [f"{key}={value}" for key, value in found if value != v]
    if wrong or not found:
        raise Halt("server.json does not carry the release version (run scripts/release.sh): "
                   + ("; ".join(wrong) or "no version"))
    pkgs = [(p.get("registryType"), p.get("identifier")) for p in data.get("packages") or []]
    if pkgs != [("cargo", CRATE)]:
        raise Halt(f"server.json must list one package, the cargo crate {CRATE}; it lists {pkgs}")
    name = data.get("name")
    # The registry checks ownership against the README crates.io serves for
    # this version, which can never change once published: catch a missing
    # token here, offline, before anything is published.
    if not has_mcp_name(crate_readme(ctx.crate_path) or "", name):
        raise Halt(f"the .crate's README.md has no visible `mcp-name: {name}` line")
    try:
        remote = ctx.registry.version(name, v)
    except StageError as e:
        if not ctx.plan:
            raise
        # A real release stops at any plan failure in `assemble`, before
        # crates.io and npm; a registry outage must not do that.
        return "would publish", f"{name} {v} to the MCP Registry (not reachable for the plan: {e})"
    if remote is not None:
        if registry_identity(remote) != registry_identity(data):
            raise Halt(f"{name} {v} in the MCP Registry differs from server.json: "
                       f"{registry_identity(remote)} vs {registry_identity(data)}")
        return "skipped", f"{name} {v} already in the MCP Registry (same packages)"
    if ctx.plan:
        return "would publish", f"{name} {v} to the MCP Registry"
    html = ctx.crates.readme(CRATE, v)
    if html is None:
        raise StageError(f"crates.io has no rendered README for {CRATE} {v} yet; re-run")
    if not has_mcp_name(html, name):
        raise Halt(f"the README crates.io serves for {CRATE} {v} has no `mcp-name: {name}`; "
                   "that version can never be registered as a cargo package")
    ctx.registry.publish(path)
    remote = ctx.registry.version(name, v)
    if remote is None:
        raise StageError(f"published {name} {v}, but the MCP Registry does not list it yet; re-run to verify")
    if registry_identity(remote) != registry_identity(data):
        raise Halt(f"the MCP Registry's {name} {v} is not this server.json")
    return "published", f"{name} {v} to the MCP Registry"


STAGE_FUNCS = {
    "draft": stage_draft,
    "crates": stage_crates,
    "npm": stage_npm,
    "publish": stage_publish,
    "plugin": stage_plugin,
    "registry": stage_registry,
}


def run_stages(ctx, stages):
    """Run stages in order, stopping at the first halt or error. Returns
    (exit status, results)."""
    results = []
    status = EXIT_OK
    for name in stages:
        try:
            outcome, detail = STAGE_FUNCS[name](ctx)
        except Halt as e:
            results.append({"stage": name, "result": "halted", "detail": str(e)})
            status = EXIT_HALT
            break
        except StageError as e:
            results.append({"stage": name, "result": "error", "detail": str(e)})
            status = EXIT_ERROR
            break
        results.append({"stage": name, "result": outcome, "detail": detail})
    for name in stages[len(results):]:
        results.append({"stage": name, "result": "not run", "detail": ""})
    return status, results


def report(manifest, results, status, plan, backend):
    return {
        "tag": manifest["tag"],
        "version": manifest["version"],
        "commit": manifest["commit"],
        "mode": "plan" if plan else "publish",
        "backend": backend,
        "status": {EXIT_OK: "ok", EXIT_ERROR: "error", EXIT_HALT: "halted"}[status],
        "stages": results,
        "assets": manifest["assets"],
        "crate": manifest["crate"],
    }


def report_markdown(rep):
    lines = [
        f"## Release {rep['tag']} ({rep['mode']}, {rep['backend']}): {rep['status']}",
        "",
        f"Commit `{rep['commit']}`",
        "",
        "| Stage | Result | Detail |",
        "|---|---|---|",
    ]
    for s in rep["stages"]:
        detail = s["detail"].replace("|", "\\|").replace("\n", " ")
        lines.append(f"| {s['stage']} | {s['result']} | {detail} |")
    lines += ["", "| Asset | SHA-256 |", "|---|---|"]
    for a in rep["assets"]:
        lines.append(f"| {a['name']} | `{a['sha256']}` |")
    lines.append(f"| {rep['crate']['name']} | `{rep['crate']['sha256']}` |")
    return "\n".join(lines) + "\n"


def make_ctx(args, fake_dir=None):
    manifest = load_manifest(args.dist)
    if fake_dir:
        fake = Fake(fake_dir)
        github, crates, npm, registry = fake, FakeCrates(fake), FakeNpm(fake), FakeRegistry(fake)
    else:
        github = RealGitHub(args.repo or os.environ.get("GITHUB_REPOSITORY"))
        crates, npm, registry = RealCrates(), RealNpm(), RealRegistry()
    return Ctx(manifest, args.dist, args.crate, args.repo_root, github, crates, npm, args.plan, registry)


# --------------------------------------------------------------------------
# Rehearsal: the acceptance scenarios against fake registries
# --------------------------------------------------------------------------


def altered_crate(src, dest):
    """A copy of a .crate with one file's contents changed: the same
    version, a different artifact."""
    with tarfile.open(src, "r:gz") as tin, tarfile.open(dest, "w:gz") as tout:
        changed = False
        for m in tin.getmembers():
            data = tin.extractfile(m).read() if m.isfile() else None
            if data is not None and not changed and m.name.endswith("/Cargo.toml.orig"):
                data += b"\n# not the released crate\n"
                m.size = len(data)
                changed = True
            tout.addfile(m, io.BytesIO(data) if data is not None else None)


def rehearse(dist, crate_path, repo_root, log=print):
    """Run every scenario; returns a list of (name, ok, detail)."""
    m = load_manifest(dist)
    if is_prerelease(m["version"]):
        return rehearse_prerelease(dist, crate_path, repo_root, log)
    outcomes = []
    all_stages = list(STAGES)

    def ctx_for(d, plan=False):
        fake = Fake(d)
        return fake, Ctx(m, dist, crate_path, repo_root, fake, FakeCrates(fake), FakeNpm(fake), plan,
                         FakeRegistry(fake))

    def results_of(results):
        return {r["stage"]: r["result"] for r in results}

    def expect(name, cond, detail):
        outcomes.append((name, bool(cond), detail))
        log(f"{'PASS' if cond else 'FAIL'}  {name}: {detail}")

    with tempfile.TemporaryDirectory() as tmp:
        # 1. End to end: plan, then publish, then a re-run that skips everything.
        d = os.path.join(tmp, "e2e")
        fake, ctx = ctx_for(d, plan=True)
        fake.set_tag(m["tag"], m["commit"])
        st, res = run_stages(ctx, all_stages)
        expect("plan changes nothing", st == EXIT_OK and fake.events() == [], f"exit {st}, {results_of(res)}")
        fake, ctx = ctx_for(d)
        st, res = run_stages(ctx, all_stages)
        r = results_of(res)
        expect("end-to-end publish", st == EXIT_OK and r["draft"] == r["crates"] == r["publish"] == "published",
               f"exit {st}, {r}")
        before = fake.events()
        st, res = run_stages(ctx, all_stages)
        r = results_of(res)
        expect("re-run of a finished release skips every stage",
               st == EXIT_OK and fake.events() == before and "published" not in r.values(), f"exit {st}, {r}")

        # 2. Partial failure: crates.io succeeds, publishing the release fails; a re-run finishes.
        d = os.path.join(tmp, "partial")
        fake, ctx = ctx_for(d)
        fake.set_tag(m["tag"], m["commit"])
        with open(os.path.join(d, "fail-once"), "w", encoding="utf-8") as f:
            f.write("github.publish")
        st, res = run_stages(ctx, all_stages)
        r = results_of(res)
        expect("partial failure stops after crates.io", st == EXIT_ERROR and r["crates"] == "published"
               and r["publish"] == "error", f"exit {st}, {r}")
        st, res = run_stages(ctx, all_stages)
        r = results_of(res)
        crate_pubs = [e for e in fake.events() if e.startswith("crates.publish")]
        expect("re-run completes without republishing", st == EXIT_OK and r["crates"] == "skipped"
               and r["publish"] == "published" and len(crate_pubs) == 1, f"exit {st}, {r}, crate publishes {len(crate_pubs)}")

        # 3. Mismatch: a different .crate already sits at this version.
        d = os.path.join(tmp, "mismatch")
        fake, ctx = ctx_for(d)
        fake.set_tag(m["tag"], m["commit"])
        altered_crate(crate_path, os.path.join(d, "crates", f"{CRATE}-{m['version']}.crate"))
        st, res = run_stages(ctx, all_stages)
        r = results_of(res)
        expect("different crate at the version halts", st == EXIT_HALT and r["crates"] == "halted"
               and r["publish"] == "not run", f"exit {st}, {r}")

        # 4. Mismatch: a published release for the tag with a different archive.
        d = os.path.join(tmp, "gh-mismatch")
        fake, ctx = ctx_for(d)
        fake.set_tag(m["tag"], m["commit"])
        rid = fake.create_draft(m["tag"], "x", "x")
        other = os.path.join(tmp, "other-archive")
        with open(other, "wb") as f:
            f.write(b"some other build")
        fake.upload(rid, other, m["assets"][0]["name"])
        fake.publish(rid)
        st, res = run_stages(ctx, all_stages)
        r = results_of(res)
        expect("different archive in a published release halts", st == EXIT_HALT and r["draft"] == "halted",
               f"exit {st}, {r}")

        # 5. Provenance: a draft whose manifest says another commit.
        d = os.path.join(tmp, "provenance")
        fake, ctx = ctx_for(d)
        fake.set_tag(m["tag"], m["commit"])
        rid = fake.create_draft(m["tag"], "x", "x")
        bad = dict(m, commit="0" * 40)
        badp = os.path.join(tmp, MANIFEST_NAME)
        with open(badp, "w", encoding="utf-8") as f:
            json.dump(bad, f)
        fake.upload(rid, badp, MANIFEST_NAME)
        st, res = run_stages(ctx, all_stages)
        r = results_of(res)
        expect("draft built from another commit halts", st == EXIT_HALT and r["draft"] == "halted", f"exit {st}, {r}")

        # 6. The tag moved: it no longer points at the commit the artifacts came from.
        d = os.path.join(tmp, "moved-tag")
        fake, ctx = ctx_for(d)
        fake.set_tag(m["tag"], "f" * 40)
        st, res = run_stages(ctx, all_stages)
        r = results_of(res)
        expect("tag pointing elsewhere halts before anything is created",
               st == EXIT_HALT and r["draft"] == "halted" and fake.events() == [], f"exit {st}, {r}")
    return outcomes


def rehearse_prerelease(dist, crate_path, repo_root, log=print):
    """The scenarios for a release candidate (#854): it becomes a GitHub
    pre-release that is never latest, and nothing reaches crates.io, npm or
    the MCP Registry, even when every stage is asked for."""
    m = load_manifest(dist)
    outcomes = []
    rc_stages = release_stages_for(m["version"])

    def ctx_for(d, plan=False):
        fake = Fake(d)
        return fake, Ctx(m, dist, crate_path, repo_root, fake, FakeCrates(fake), FakeNpm(fake), plan,
                         FakeRegistry(fake))

    def results_of(results):
        return {r["stage"]: r["result"] for r in results}

    def expect(name, cond, detail):
        outcomes.append((name, bool(cond), detail))
        log(f"{'PASS' if cond else 'FAIL'}  {name}: {detail}")

    def off_github(fake):
        return [e for e in fake.events() if not e.startswith("github.")]

    with tempfile.TemporaryDirectory() as tmp:
        # 1. Plan, publish, re-run.
        d = os.path.join(tmp, "e2e")
        fake, ctx = ctx_for(d, plan=True)
        fake.set_tag(m["tag"], m["commit"])
        st, res = run_stages(ctx, rc_stages)
        expect("rc: plan changes nothing", st == EXIT_OK and fake.events() == [], f"exit {st}, {results_of(res)}")
        fake, ctx = ctx_for(d)
        fake.set_tag("v0.0.1", m["commit"])
        rid = fake.create_draft("v0.0.1", "stable", "x")
        fake.publish(rid)
        st, res = run_stages(ctx, rc_stages)
        r = results_of(res)
        rel = fake.releases(m["tag"])
        expect("rc: published as a GitHub pre-release",
               st == EXIT_OK and r["draft"] == r["publish"] == "published"
               and len(rel) == 1 and rel[0]["prerelease"] and not rel[0]["draft"], f"exit {st}, {r}")
        expect("rc: never the latest release", fake.latest() == "v0.0.1", f"latest is {fake.latest()}")
        before = fake.events()
        st, res = run_stages(ctx, rc_stages)
        r = results_of(res)
        expect("rc: re-run skips every stage",
               st == EXIT_OK and fake.events() == before and "published" not in r.values(), f"exit {st}, {r}")

        # 2. Every stage asked for, a different crate already at the
        # version: still nothing but GitHub, and no halt on the crate.
        d = os.path.join(tmp, "all-stages")
        fake, ctx = ctx_for(d)
        fake.set_tag(m["tag"], m["commit"])
        altered_crate(crate_path, os.path.join(d, "crates", f"{CRATE}-{m['version']}.crate"))
        st, res = run_stages(ctx, list(STAGES))
        r = results_of(res)
        expect("rc: crates, npm and registry stay disabled when asked for",
               st == EXIT_OK and r["crates"] == r["npm"] == r["registry"] == "disabled"
               and off_github(fake) == [], f"exit {st}, {r}, other events {off_github(fake)}")

        # 3. A failure publishing the pre-release, then a resume.
        d = os.path.join(tmp, "partial")
        fake, ctx = ctx_for(d)
        fake.set_tag(m["tag"], m["commit"])
        with open(os.path.join(d, "fail-once"), "w", encoding="utf-8") as f:
            f.write("github.publish")
        st, res = run_stages(ctx, rc_stages)
        r1 = results_of(res)
        st2, res = run_stages(ctx, rc_stages)
        r2 = results_of(res)
        expect("rc: failed publish resumes", st == EXIT_ERROR and r1["publish"] == "error"
               and st2 == EXIT_OK and r2["draft"] == "skipped" and r2["publish"] == "published",
               f"exit {st} {r1}, then exit {st2} {r2}")

        # 4. A draft for the rc tag that is not marked pre-release halts.
        d = os.path.join(tmp, "not-prerelease")
        fake, ctx = ctx_for(d)
        fake.set_tag(m["tag"], m["commit"])
        fake.create_draft(m["tag"], "x", "x", prerelease=False)
        st, res = run_stages(ctx, rc_stages)
        r = results_of(res)
        expect("rc: a draft not marked pre-release halts", st == EXIT_HALT and r["draft"] == "halted",
               f"exit {st}, {r}")
    return outcomes


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)

    sub.add_parser("check-pin", help="fail while src/fontpacks/pinned.rs has PINNED_INDEX = None")

    b = sub.add_parser("bump-manifests", help="set every manifest's version")
    b.add_argument("version")

    v = sub.add_parser("version-info", help="print prerelease=true|false and the stages for VERSION "
                       "(GITHUB_OUTPUT lines; release.yml reads them)")
    v.add_argument("version")

    mf = sub.add_parser("manifest", help="write release-manifest.json and SHA256SUMS into --dist")
    mf.add_argument("--tag", required=True)
    mf.add_argument("--commit", required=True)
    mf.add_argument("--dist", required=True)
    mf.add_argument("--crate", required=True)

    for name in ("run", "rehearse"):
        p = sub.add_parser(name)
        p.add_argument("--dist", required=True, help="directory with the archives and the manifest")
        p.add_argument("--crate", required=True, help="the .crate built from the release commit")
        p.add_argument("--repo-root", default=ROOT)
        if name == "run":
            p.add_argument("--stage", action="append", choices=STAGES, required=True)
            p.add_argument("--plan", action="store_true", help="check only; change nothing")
            p.add_argument("--fake", metavar="DIR", help="use a fake registry in DIR")
            p.add_argument("--repo", help="owner/name (default: $GITHUB_REPOSITORY)")
            p.add_argument("--report-dir", help="write release-report.json/.md here")

    args = ap.parse_args(argv)

    if args.cmd == "check-pin":
        pin = pinned_index()
        if pin is None:
            print("src/fontpacks/pinned.rs has PINNED_INDEX = None: publish and pin the first fonts-* "
                  "release before an interpreter release (docs/dev/RELEASING-FONTS.md).", file=sys.stderr)
            return EXIT_ERROR
        print(f"pinned font index: {pin[0]} sha256 {pin[1]}")
        return EXIT_OK

    if args.cmd == "bump-manifests":
        for f in bump_manifests(ROOT, args.version):
            print(f)
        return EXIT_OK

    if args.cmd == "version-info":
        pre = is_prerelease(args.version)
        print(f"prerelease={str(pre).lower()}")
        print("stages=" + " ".join(f"--stage {s}" for s in release_stages_for(args.version)))
        return EXIT_OK

    if args.cmd == "manifest":
        m = build_manifest(args.tag, args.commit, args.dist, args.crate)
        write_manifest(m, args.dist)
        print(json.dumps(m, indent=2))
        return EXIT_OK

    if args.cmd == "rehearse":
        outcomes = rehearse(args.dist, args.crate, args.repo_root)
        failed = [n for n, ok, _ in outcomes if not ok]
        print(f"{len(outcomes) - len(failed)}/{len(outcomes)} scenarios behaved")
        return EXIT_ERROR if failed else EXIT_OK

    ctx = make_ctx(args, args.fake)
    if cargo_version(args.repo_root) != ctx.m["version"] and not args.fake:
        print(f"Cargo.toml is {cargo_version(args.repo_root)}, the manifest is {ctx.m['version']}", file=sys.stderr)
        return EXIT_HALT
    status, results = run_stages(ctx, args.stage)
    rep = report(ctx.m, results, status, args.plan, "fake" if args.fake else "real")
    md = report_markdown(rep)
    print(md)
    if args.report_dir:
        os.makedirs(args.report_dir, exist_ok=True)
        with open(os.path.join(args.report_dir, "release-report.json"), "w", encoding="utf-8") as f:
            json.dump(rep, f, indent=2)
        with open(os.path.join(args.report_dir, "release-report.md"), "w", encoding="utf-8") as f:
            f.write(md)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as f:
            f.write(md + "\n")
    return status


if __name__ == "__main__":
    sys.exit(main())
