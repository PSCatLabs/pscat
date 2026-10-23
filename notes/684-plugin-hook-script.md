# 684: the missing-pscat hook is a script, not an inline command

The plugin directory's pre-submission checklist blocks a hook command that
is not "each path written in full from `${CLAUDE_PLUGIN_ROOT}`, with no
other variable, command substitution, wildcard, or inline program" when the
plugin folder is a subfolder of the repository (ours is `plugin/`). It also
holds scripts that use shell variables, substitutions or call other plugin
files. The old `hooks.json` command was an inline `command -v ... || printf`.

## What changed

- `plugin/hooks/check-pscat.sh`: the same `command -v` test and the same
  message, as one plain POSIX `sh` line: no variables, no `$(...)`, no
  backticks (the message used to quote `cargo install pscat` in backticks;
  now it reads "Install them with: cargo install pscat"), no other files.
  Committed with LF endings and mode 100755. No `.gitattributes` rule: the
  checklist stops validation for attributes that rewrite file contents.
- `plugin/hooks/hooks.json`: `sh "${CLAUDE_PLUGIN_ROOT}/hooks/check-pscat.sh"`.
  The `sh` prefix means a copy that loses the exec bit still runs. The
  quotes are what `claude plugin validate` asks for (an unquoted
  placeholder splits on a space in the install path).
- One shared file still serves both agents. Codex documents that it sets
  `PLUGIN_ROOT` and, "for compatibility with existing plugin hooks",
  `CLAUDE_PLUGIN_ROOT` and `CLAUDE_PLUGIN_DATA`
  (<https://learn.chatgpt.com/docs/hooks>), so no per-host hook files.
  The page does not say Codex runs a command through a shell; the hook
  only needs `sh` to be on PATH, since the command starts with `sh`, and
  the old inline command needed a shell for `&&` and `||` anyway (never
  run end to end in Codex, #654).
- `scripts/plugin_check.py`: a `${...PLUGIN_ROOT}/<file>` named in a hook
  must exist in the plugin directory.
- `scripts/plugin_install_check.sh` and `scripts/codex_plugin_install_check.sh`
  now run the command string from the installed `hooks.json` with
  `CLAUDE_PLUGIN_ROOT` set to the installed copy: it prints the
  `systemMessage` on a PATH without pscat and nothing with it. A real Codex
  hook run still needs the `/hooks` trust review (#654), so this emulation
  is the coverage. The Codex script also prints the script's mode in the
  Codex cache.

## Not verified here

The portal's Validate (owner-run). The hook inside a live Claude Code or
Codex session.
