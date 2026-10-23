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
  (`pscat-mcp`), and its args and hook commands use no `./`, `../` or
  absolute paths except under `${CLAUDE_PLUGIN_ROOT}` / `${PLUGIN_ROOT}`;
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


def _check_servers(servers, rel, errors, need_type=False):
    for name, cfg in servers.items() if isinstance(servers, dict) else []:
        where = f"{rel}: mcpServers.{name}"
        kind = cfg.get("type")
        if need_type and kind not in ("stdio", "streamable-http"):
            errors.append(f"{where}: type {kind!r} must be \"stdio\" or \"streamable-http\"")
        cmd = cfg.get("command", "")
        if need_type and kind == "stdio" and not cmd:
            errors.append(f"{where}: a stdio server needs a command")
        if cmd and ("/" in cmd or "\\" in cmd) and not re.match(r"\$\{(CLAUDE_)?PLUGIN_ROOT\}", cmd):
            errors.append(f"{where}: command {cmd!r} must be a bare executable name")
        for a in cfg.get("args", []):
            _check_paths_in(a, f"{where}.args", errors)


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
                    for m in re.finditer(r"\$\{(?:CLAUDE_)?PLUGIN_ROOT\}/(\S+)", cmd):
                        if not os.path.isfile(os.path.join(plugin_dir, m.group(1))):
                            errors.append(f"{rel}: hooks.{event}: {m.group(1)!r} is not a file "
                                          "in the plugin directory")


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

    servers = manifest.get("mcpServers") or {}
    if os.path.exists(os.path.join(plugin_dir, ".mcp.json")):
        servers = {**(_load(os.path.join(plugin_dir, ".mcp.json"), errors) or {})
                   .get("mcpServers", {}), **servers}
    _check_servers(servers, rel, errors)

    hooks = manifest.get("hooks")
    if hooks and _default_hooks(plugin_dir):
        errors.append(f"{rel}: plugin.json declares hooks and hooks/hooks.json exists; "
                      "Claude Code loads both (keep one)")
    _check_hooks(plugin_dir, rel, hooks, errors)


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
        _check_servers(servers or {}, f"{rel}/mcp.json", errors, need_type=True)
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
