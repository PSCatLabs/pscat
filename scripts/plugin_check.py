#!/usr/bin/env python3
"""Check the Claude Code plugin manifests (issue #569).

    python3 scripts/plugin_check.py            # check the repo
    python3 scripts/plugin_check.py ROOT       # check another tree (tests)

The marketplace is `.claude-plugin/marketplace.json` at the repo root; each
plugin it lists sits in its own directory with `.claude-plugin/plugin.json`
(today one plugin, `plugin/`, whose `skills` is a symlink to the top-level
`skills/`). `claude plugin validate` checks the schema; this checks what it
does not:

- the plugin's `version` equals the crate's (`Cargo.toml` [package]), and no
  marketplace entry carries a different one (plugin.json wins at install
  time, so a stale entry version only misleads);
- every marketplace `source` is a `./` path inside the repo, and every
  component path in plugin.json starts with `./`, stays inside the plugin
  directory and exists;
- every symlink in a plugin resolves to something inside the repo (install
  copies the plugin directory and follows symlinks; one that dangles or
  points outside would ship nothing or ship the wrong thing);
- each skill directory has a SKILL.md whose frontmatter `name` matches the
  directory and has a `description`;
- nothing the plugin installs names a repo-relative or absolute path: an MCP
  server's `command` is a bare executable name found on the user's PATH
  (`pscat-mcp`), and its args and hook commands use no `./`, `../` or
  absolute paths except under `${CLAUDE_PLUGIN_ROOT}`.

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
    stripped = re.sub(r"\$\{CLAUDE_PLUGIN_(ROOT|DATA)\}/?", "R/", text)
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

    # Every symlink anywhere in the plugin must resolve inside the repo.
    for dirpath, dirnames, filenames in os.walk(plugin_dir, followlinks=True):
        for name in dirnames + filenames:
            p = os.path.join(dirpath, name)
            if os.path.islink(p) and (not os.path.exists(p) or not _inside(p, root)):
                errors.append(f"{os.path.relpath(p, root)}: symlink dangles or "
                              "points outside the repo")

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

    skills_root = os.path.join(plugin_dir, "skills")
    if os.path.isdir(skills_root):
        skills = sorted(
            d for d in os.listdir(skills_root)
            if os.path.isdir(os.path.join(skills_root, d))
        )
        if not skills:
            errors.append(f"{rel}/skills: no skills")
        for d in skills:
            _check_skill(os.path.join(skills_root, d), f"{rel}/skills/{d}", errors)

    servers = manifest.get("mcpServers") or {}
    if os.path.exists(os.path.join(plugin_dir, ".mcp.json")):
        servers = {**(_load(os.path.join(plugin_dir, ".mcp.json"), errors) or {})
                   .get("mcpServers", {}), **servers}
    for name, cfg in servers.items() if isinstance(servers, dict) else []:
        where = f"{rel}: mcpServers.{name}"
        cmd = cfg.get("command", "")
        if cmd and ("/" in cmd or "\\" in cmd) and not cmd.startswith("${CLAUDE_PLUGIN_ROOT}"):
            errors.append(f"{where}: command {cmd!r} must be a bare executable name")
        for a in cfg.get("args", []):
            _check_paths_in(a, f"{where}.args", errors)

    hooks = manifest.get("hooks") or {}
    if isinstance(hooks, str):  # a path to a hooks file
        if not hooks.startswith("./") or not os.path.isfile(os.path.join(plugin_dir, hooks)):
            errors.append(f"{rel}: hooks path {hooks!r} is missing or not ./")
        hooks = (_load(os.path.join(plugin_dir, hooks), errors) or {}).get("hooks", {}) \
            if os.path.isfile(os.path.join(plugin_dir, hooks)) else {}
    for event, groups in hooks.items() if isinstance(hooks, dict) else []:
        for g in groups:
            for h in g.get("hooks", []):
                if h.get("type") == "command":
                    _check_paths_in(h.get("command", ""), f"{rel}: hooks.{event}", errors)


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
