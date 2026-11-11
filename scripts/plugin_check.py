#!/usr/bin/env python3
"""Check the Claude Code and Codex plugin manifests (issue #569).

    python3 scripts/plugin_check.py            # check the repo
    python3 scripts/plugin_check.py ROOT       # check another tree (tests)

Claude Code's marketplace is `.claude-plugin/marketplace.json` at the repo
root; each plugin it lists sits in its own directory with
`.claude-plugin/plugin.json`. Codex's marketplace is
`.agents/plugins/marketplace.json`; each local plugin it lists has a portable
`plugin.json` at its root and its MCP servers in `mcp.json`. Today both list
one plugin, `plugin/`, which holds the skills as real files (`skills/` at the
repo root is a symlink to `plugin/skills`). `claude plugin validate` checks
Claude's schema; Codex has no validator. This checks what neither does:

- the plugin's `version` equals the crate's (`Cargo.toml` [package]), and no
  marketplace entry carries a different one (plugin.json wins at install
  time, so a stale entry version only misleads);
- every marketplace `source` is a `./` path inside the repo, and every
  component path in plugin.json starts with `./`, stays inside the plugin
  directory and exists;
- a plugin directory holds no symlinks at all: Claude Code's install
  follows them, but Codex's skips them, so a symlinked `skills` installs no
  skills there (and Git for Windows checks symlinks out as text files);
- each skill directory has a SKILL.md whose frontmatter `name` matches the
  directory and has a `description`;
- nothing the plugin installs names a repo-relative or absolute path: an MCP
  server's `command` is a bare executable name found on the user's PATH
  (`sh`), and its args and hook commands use no `./`, `../` or
  absolute paths except under `${CLAUDE_PLUGIN_ROOT}` / `${PLUGIN_ROOT}`;
  a `${...PLUGIN_ROOT}/<file>` that a server or hook names exists in the
  plugin directory;
- each host's placeholder in each host's file (#853): Claude Code expands
  `${CLAUDE_PLUGIN_ROOT}` in `.claude-plugin/plugin.json` and `.mcp.json`
  (https://code.claude.com/docs/en/plugins-reference#environment-variables);
  Codex expands only `${PLUGIN_ROOT}` / `${PLUGIN_DATA}` in `mcp.json` args,
  env and cwd (codex-rs/codex-mcp/src/agent_plugin_config.rs), not Claude
  Code's `${CLAUDE_PROJECT_DIR}` or `${user_config.KEY}`. A server both
  manifests declare runs the same command and args in both, apart from the
  launcher's `--plugin-NAME=VALUE` host bindings (#910);
- no top-level `bin/` in a plugin directory: claude.ai and Cowork refuse to
  install a plugin that has one
  (https://claude.com/docs/plugins/platform-support);
- `pscat-release`, the release the plugin's launcher downloads (#853), holds
  one tag `vX.Y.Z` equal to the crate version, or a pre-release
  `vX.Y.Z-rc.N` of a version not older than the crate's (a trial pin);
- no plugin Markdown file (README, SKILL.md, references) writes the path of
  a bundled image or font in backticks or a code block, or any
  slash-containing image or font path (which points outside the bundle).
  The directory's pre-submission checklist holds a version for review
  otherwise (#848). A bare output name such as `out.png` is allowed;
- the hooks both tools discover by default (`hooks/hooks.json`) follow the
  same path rule;
- Codex: the marketplace entry has `policy.installation`,
  `policy.authentication` and `category`, its source is a local `./` path
  inside the repo, and `mcp.json` servers declare a `type` (stdio servers a
  bare `command`).

Exit 0 when clean, 1 with one line per problem otherwise.
"""
import json
import os
import re
import sys
import tomllib

COMPONENT_KEYS = ("skills", "commands", "agents", "outputStyles", "workflows")
ASSET_EXT = "png|jpe?g|gif|webp|svg|ttf|otf|ttc|woff2?"
ASSET_TOKEN = re.compile(r"[^\s`'\"()<>=,;|\[\]{}]+\.(?:%s)(?![\w.-])" % ASSET_EXT, re.I)
INLINE_CODE = re.compile(r"(`+)(.+?)(?<!`)\1(?!`)")
FENCE = re.compile(r"^\s*(`{3,}|~{3,})")
PATHY = re.compile(r"(^|[\s'\"=])(\.\.?/|/(?!dev/null)[A-Za-z])")


def _inside(path, root):
    path, root = os.path.realpath(path), os.path.realpath(root)
    return path == root or path.startswith(root + os.sep)


def _load(path, errors):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError) as e:
        errors.append(f"{path}: {e}")
        return None


def crate_version(root):
    with open(os.path.join(root, "Cargo.toml"), "rb") as f:
        return tomllib.load(f)["package"]["version"]


def _check_paths_in(text, where, errors):
    stripped = re.sub(r"\$\{(CLAUDE_)?PLUGIN_(ROOT|DATA)\}/?", "R/", text)
    if PATHY.search(stripped):
        errors.append(f"{where}: names a relative or absolute path ({text!r}); "
                      "use a PATH executable or ${CLAUDE_PLUGIN_ROOT}")


def _check_skill(skill_dir, rel, errors):
    md = os.path.join(skill_dir, "SKILL.md")
    if not os.path.isfile(md):
        errors.append(f"{rel}: no SKILL.md")
        return
    with open(md, encoding="utf-8") as f:
        text = f.read()
    m = re.match(r"---\n(.*?)\n---\n", text, re.S)
    if not m:
        errors.append(f"{rel}/SKILL.md: no frontmatter")
        return
    front = dict(
        line.split(":", 1) for line in m.group(1).splitlines() if ":" in line
    )
    name = front.get("name", "").strip()
    if name != os.path.basename(skill_dir):
        errors.append(f"{rel}/SKILL.md: name {name!r} != directory "
                      f"{os.path.basename(skill_dir)!r}")
    if not front.get("description", "").strip():
        errors.append(f"{rel}/SKILL.md: no description")


def _code_lines(text):
    """Yield (line number, code text) for each fenced-block line and each inline
    backtick span. A 4-space indented block is not treated as code: list
    continuation lines in the skills are indented that deep."""
    fence = None
    for n, line in enumerate(text.splitlines(), 1):
        m = FENCE.match(line)
        if fence:
            if m and m.group(1)[0] == fence[0] and len(m.group(1)) >= len(fence):
                fence = None
            else:
                yield n, line
        elif m:
            fence = m.group(1)
        else:
            for span in INLINE_CODE.finditer(line):
                yield n, span.group(2)


def _check_asset_paths(root, plugin_dir, errors):
    """The directory checklist (https://claude.com/docs/plugins/pre-submission-checklist,
    "Files in the plugin folder") says: "To show a bundled image in the README, use
    Markdown image syntax. Don't refer to bundled images or fonts from commands,
    hooks, or scripts, or write their paths in backticks or a code block." A bare
    output name (`out.png`) is not a bundled file and passes. A token that names a
    bundled file (by path or basename), or has a `/` (a repo or absolute path that
    is not in the bundle), is reported (#848)."""
    ext = re.compile(r"\.(?:%s)$" % ASSET_EXT, re.I)
    bundled = set()
    for dirpath, _, filenames in os.walk(plugin_dir):
        bundled.update(f.lower() for f in filenames if ext.search(f))
    for dirpath, _, filenames in sorted(os.walk(plugin_dir)):
        for name in sorted(filenames):
            if not name.endswith(".md"):
                continue
            path = os.path.join(dirpath, name)
            with open(path, encoding="utf-8") as f:
                text = f.read()
            for n, code in _code_lines(text):
                for tok in ASSET_TOKEN.findall(code):
                    if "://" in tok:
                        continue
                    base = os.path.basename(tok).lower()
                    if "/" in tok or base in bundled:
                        why = ("names a bundled file" if base in bundled
                               else "is a path outside the bundle")
                        errors.append(
                            f"{os.path.relpath(path, root)}:{n}: `{tok}` {why} in "
                            "backticks or a code block; the directory checklist holds the "
                            "version for review (show a bundled image with Markdown image "
                            "syntax, link a repo file by URL)")


def _check_no_bin(root, plugin_dir, errors):
    if os.path.isdir(os.path.join(plugin_dir, "bin")):
        errors.append(f"{os.path.relpath(plugin_dir, root)}/bin: a top-level bin/ directory makes "
                      "claude.ai and Cowork refuse the whole plugin; keep executables in scripts/ "
                      "(https://claude.com/docs/plugins/platform-support)")


def _semver(text):
    return tuple(int(x) for x in re.split(r"[-+]", text)[0].split("."))


def _check_release_pin(root, plugin_dir, version, errors):
    path = os.path.join(plugin_dir, "pscat-release")
    if not os.path.exists(path):
        return
    where = os.path.relpath(path, root)
    with open(path, encoding="utf-8") as f:
        tags = [ln.strip() for ln in f if ln.strip() and not ln.lstrip().startswith("#")]
    m = re.fullmatch(r"v(\d+\.\d+\.\d+)(-[0-9A-Za-z.-]+)?", tags[0]) if len(tags) == 1 else None
    if not m:
        errors.append(f"{where}: must hold exactly one release tag such as v{version} (got {tags!r})")
        return
    base, pre = m.group(1), m.group(2)
    if tags[0] == f"v{version}":
        return
    if pre is None and base != version:
        errors.append(f"{where}: pins {tags[0]} but Cargo.toml is {version}; a release pin "
                      "equals the crate version")
    elif pre is not None and _semver(base) < _semver(version):
        errors.append(f"{where}: pins the pre-release {tags[0]}, older than Cargo.toml {version}")


def _check_no_symlinks(root, plugin_dir, errors):
    for dirpath, dirnames, filenames in os.walk(plugin_dir):
        for name in dirnames + filenames:
            p = os.path.join(dirpath, name)
            if os.path.islink(p):
                errors.append(f"{os.path.relpath(p, root)}: symlink in a plugin directory "
                              "(Codex skips symlinks when it installs a plugin; keep real files)")


def _check_skills(plugin_dir, rel, errors):
    skills_root = os.path.join(plugin_dir, "skills")
    if not os.path.isdir(skills_root):
        return
    skills = sorted(
        d for d in os.listdir(skills_root)
        if os.path.isdir(os.path.join(skills_root, d))
    )
    if not skills:
        errors.append(f"{rel}/skills: no skills")
    for d in skills:
        _check_skill(os.path.join(skills_root, d), f"{rel}/skills/{d}", errors)


def _plugin_file_refs(text, plugin_dir, where, errors):
    """A `${...PLUGIN_ROOT}/<file>` a server or hook names must ship in the plugin."""
    for m in re.finditer(r"\$\{(?:CLAUDE_)?PLUGIN_ROOT\}/([^\s\x22\x27]+)", text):
        if not os.path.isfile(os.path.join(plugin_dir, m.group(1))):
            errors.append(f"{where}: {m.group(1)!r} is not a file in the plugin directory")


def _check_placeholders(text, host, where, errors):
    """Claude Code expands ${CLAUDE_PLUGIN_ROOT}; Codex's mcp.json only ${PLUGIN_ROOT}."""
    wrong = (r"\$\{PLUGIN_(ROOT|DATA)\}" if host == "claude"
             else r"\$\{(CLAUDE_PLUGIN_(ROOT|DATA)|CLAUDE_PROJECT_DIR|user_config\.\w+)\}")
    m = re.search(wrong, text)
    if m:
        right = "${CLAUDE_PLUGIN_ROOT}" if host == "claude" else "${PLUGIN_ROOT}"
        errors.append(f"{where}: {m.group(0)} is not expanded by "
                      f"{'Claude Code' if host == 'claude' else 'Codex'} here; use {right}")


def _check_servers(servers, rel, errors, need_type=False, plugin_dir=None, host="claude"):
    for name, cfg in servers.items() if isinstance(servers, dict) else []:
        where = f"{rel}: mcpServers.{name}"
        kind = cfg.get("type")
        if need_type and kind not in ("stdio", "streamable-http"):
            errors.append(f"{where}: type {kind!r} must be \"stdio\" or \"streamable-http\"")
        cmd = cfg.get("command", "")
        if need_type and kind == "stdio" and not cmd:
            errors.append(f"{where}: a stdio server needs a command")
        if host == "codex" and "${" in cmd:
            errors.append(f"{where}: command {cmd!r}: Codex does not expand placeholders in "
                          "command; use a bare command and put ${PLUGIN_ROOT} in args")
        elif cmd and ("/" in cmd or "\\" in cmd) and not re.match(r"\$\{(CLAUDE_)?PLUGIN_ROOT\}", cmd):
            errors.append(f"{where}: command {cmd!r} must be a bare executable name")
        _check_placeholders(cmd, host, f"{where}.command", errors)
        for a in cfg.get("args", []):
            _check_paths_in(a, f"{where}.args", errors)
            _check_placeholders(a, host, f"{where}.args", errors)
            if plugin_dir:
                _plugin_file_refs(a, plugin_dir, f"{where}.args", errors)
        for v in (cfg.get("env") or {}).values():
            _check_placeholders(str(v), host, f"{where}.env", errors)


def _default_hooks(plugin_dir):
    p = os.path.join(plugin_dir, "hooks", "hooks.json")
    return p if os.path.isfile(p) else None


def _check_hooks(plugin_dir, rel, hooks, errors):
    """`hooks` from a manifest (inline, a ./path, or None for the default
    hooks/hooks.json); checks the command paths."""
    if hooks is None:
        hooks = "./hooks/hooks.json" if _default_hooks(plugin_dir) else {}
    if isinstance(hooks, str):  # a path to a hooks file
        path = os.path.join(plugin_dir, hooks)
        if not hooks.startswith("./") or not os.path.isfile(path):
            errors.append(f"{rel}: hooks path {hooks!r} is missing or not ./")
            return
        hooks = (_load(path, errors) or {}).get("hooks", {})
    for event, groups in hooks.items() if isinstance(hooks, dict) else []:
        for g in groups:
            for h in g.get("hooks", []):
                if h.get("type") == "command":
                    cmd = h.get("command", "")
                    _check_paths_in(cmd, f"{rel}: hooks.{event}", errors)
                    # A script named from the plugin root must ship in the plugin.
                    _plugin_file_refs(cmd, plugin_dir, f"{rel}: hooks.{event}", errors)


def check_plugin(root, plugin_dir, entry, version, errors):
    rel = os.path.relpath(plugin_dir, root)
    manifest = _load(os.path.join(plugin_dir, ".claude-plugin", "plugin.json"), errors)
    if manifest is None:
        return
    if manifest.get("name") != entry.get("name"):
        errors.append(f"{rel}: plugin.json name {manifest.get('name')!r} != "
                      f"marketplace entry {entry.get('name')!r}")
    if manifest.get("version") != version:
        errors.append(f"{rel}/.claude-plugin/plugin.json: version "
                      f"{manifest.get('version')!r} != Cargo.toml {version!r}")

    _check_no_symlinks(root, plugin_dir, errors)

    for key in COMPONENT_KEYS:
        value = manifest.get(key)
        paths = [value] if isinstance(value, str) else list(value or [])
        for p in paths:
            if not isinstance(p, str):
                continue
            target = os.path.join(plugin_dir, p)
            if not p.startswith("./"):
                errors.append(f"{rel}: {key} path {p!r} must start with ./")
            elif ".." in p.split("/") or not os.path.exists(target):
                errors.append(f"{rel}: {key} path {p!r} is missing or escapes")

    _check_skills(plugin_dir, rel, errors)
    _check_asset_paths(root, plugin_dir, errors)

    servers = manifest.get("mcpServers") or {}
    if os.path.exists(os.path.join(plugin_dir, ".mcp.json")):
        servers = {**(_load(os.path.join(plugin_dir, ".mcp.json"), errors) or {})
                   .get("mcpServers", {}), **servers}
    _check_servers(servers, rel, errors, plugin_dir=plugin_dir, host="claude")
    _check_no_bin(root, plugin_dir, errors)
    _check_release_pin(root, plugin_dir, version, errors)

    hooks = manifest.get("hooks")
    if hooks and _default_hooks(plugin_dir):
        errors.append(f"{rel}: plugin.json declares hooks and hooks/hooks.json exists; "
                      "Claude Code loads both (keep one)")
    _check_hooks(plugin_dir, rel, hooks, errors)


def _check_same_servers(plugin_dir, rel, codex_servers, errors):
    """A server in both Codex's mcp.json and Claude Code's plugin.json must run
    the same program: same command, same args once each host's root
    placeholder is normalized and the launcher's `--plugin-NAME=VALUE` host
    bindings are set aside."""
    path = os.path.join(plugin_dir, ".claude-plugin", "plugin.json")
    if not os.path.isfile(path):
        return
    try:
        with open(path, encoding="utf-8") as f:
            claude = (json.load(f).get("mcpServers") or {})
    except (OSError, ValueError):
        return  # reported by check_plugin
    if not isinstance(claude, dict) or not isinstance(codex_servers, dict):
        return

    def norm(cfg):
        # `--plugin-NAME=VALUE` arguments are the launcher's host bindings
        # (#910: Claude Code's userConfig and project directory), which only
        # one host can expand; the launcher drops them before pscat-mcp.
        args = [re.sub(r"\$\{(CLAUDE_)?PLUGIN_ROOT\}", "${ROOT}", a) for a in cfg.get("args", [])
                if not re.match(r"--plugin-[a-z-]+=", a)]
        return cfg.get("command"), args

    for name in sorted(set(claude) & set(codex_servers)):
        if norm(claude[name]) != norm(codex_servers[name]):
            errors.append(f"{rel}: mcpServers.{name} runs {norm(claude[name])} for Claude Code "
                          f"but {norm(codex_servers[name])} for Codex (mcp.json); keep them the same")


POLICY_INSTALL = ("AVAILABLE", "INSTALLED_BY_DEFAULT", "NOT_AVAILABLE")


def check_codex_plugin(root, plugin_dir, entry, version, errors):
    rel = os.path.relpath(plugin_dir, root)
    manifest = _load(os.path.join(plugin_dir, "plugin.json"), errors)
    if manifest is None:
        return
    if manifest.get("name") != entry.get("name"):
        errors.append(f"{rel}/plugin.json: name {manifest.get('name')!r} != "
                      f"Codex marketplace entry {entry.get('name')!r}")
    if manifest.get("version") != version:
        errors.append(f"{rel}/plugin.json: version {manifest.get('version')!r} "
                      f"!= Cargo.toml {version!r}")
    _check_no_symlinks(root, plugin_dir, errors)
    _check_skills(plugin_dir, rel, errors)
    mcp = os.path.join(plugin_dir, "mcp.json")
    if os.path.exists(mcp):
        servers = (_load(mcp, errors) or {}).get("mcpServers")
        if not servers:
            errors.append(f"{rel}/mcp.json: no mcpServers")
        _check_servers(servers or {}, f"{rel}/mcp.json", errors, need_type=True,
                       plugin_dir=plugin_dir, host="codex")
        _check_same_servers(plugin_dir, rel, servers or {}, errors)
    openai = (manifest.get("extensions") or {}).get("com.openai") or {}
    _check_hooks(plugin_dir, rel, openai.get("hooks"), errors)
    for key in ("apps", "onboardingSkill"):
        p = openai.get(key)
        if isinstance(p, str) and (not p.startswith("./")
                                   or not os.path.exists(os.path.join(plugin_dir, p))):
            errors.append(f"{rel}/plugin.json: extensions.com.openai.{key} {p!r} "
                          "is missing or not ./")


def check_codex(root, version, errors):
    path = os.path.join(root, ".agents", "plugins", "marketplace.json")
    if not os.path.exists(path):
        return
    market = _load(path, errors)
    if market is None:
        return
    where = ".agents/plugins/marketplace.json"
    if not market.get("name"):
        errors.append(f"{where}: no name")
    plugins = market.get("plugins") or []
    if not plugins:
        errors.append(f"{where}: no plugins")
    for entry in plugins:
        name = entry.get("name", "?")
        policy = entry.get("policy") or {}
        if policy.get("installation") not in POLICY_INSTALL:
            errors.append(f"{where}: {name}: policy.installation "
                          f"{policy.get('installation')!r} not one of {POLICY_INSTALL}")
        if not policy.get("authentication"):
            errors.append(f"{where}: {name}: no policy.authentication")
        if not entry.get("category"):
            errors.append(f"{where}: {name}: no category")
        src = entry.get("source")
        if isinstance(src, dict):
            if src.get("source") != "local":
                continue  # git, url or npm: not ours to resolve
            src = src.get("path")
        if not isinstance(src, str):
            errors.append(f"{where}: {name}: source has no local path")
            continue
        plugin_dir = os.path.join(root, src)
        if not src.startswith("./") or ".." in src.split("/") or not _inside(plugin_dir, root):
            errors.append(f"{where}: {name}: source {src!r} must be ./ inside the repo")
        elif not os.path.isfile(os.path.join(plugin_dir, "plugin.json")):
            errors.append(f"{where}: {name}: {src} has no plugin.json")
        else:
            check_codex_plugin(root, plugin_dir, entry, version, errors)


def check(root):
    errors = []
    version = crate_version(root)
    market = _load(os.path.join(root, ".claude-plugin", "marketplace.json"), errors)
    if market is None:
        return errors
    plugins = market.get("plugins") or []
    if not plugins:
        errors.append("marketplace.json: no plugins")
    for entry in plugins:
        name = entry.get("name", "?")
        src = entry.get("source")
        if "version" in entry and entry["version"] != version:
            errors.append(f"marketplace.json: {name}: version {entry['version']!r} "
                          f"!= Cargo.toml {version!r} (leave it out: plugin.json has it)")
        if not isinstance(src, str):
            continue  # a remote source (github, url, ...) is not ours to resolve
        plugin_dir = os.path.join(root, src)
        if not src.startswith("./") or ".." in src.split("/") or not _inside(plugin_dir, root):
            errors.append(f"marketplace.json: {name}: source {src!r} must be ./ inside the repo")
        elif not os.path.isfile(os.path.join(plugin_dir, ".claude-plugin", "plugin.json")):
            errors.append(f"marketplace.json: {name}: {src} has no .claude-plugin/plugin.json")
        else:
            check_plugin(root, plugin_dir, entry, version, errors)
    check_codex(root, version, errors)
    return errors


def main(argv):
    root = argv[1] if len(argv) > 1 else os.path.dirname(
        os.path.dirname(os.path.abspath(__file__)))
    errors = check(root)
    for e in errors:
        print(e)
    if not errors:
        print(f"plugin manifests OK (version {crate_version(root)})")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
