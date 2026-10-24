#!/bin/sh
# SessionStart hook: tell the user when pscat / pscat-mcp is not installed
# (the plugin does not bundle them). Shared by Claude Code and Codex; both
# set CLAUDE_PLUGIN_ROOT, so hooks.json names this file from it.
# Plain POSIX sh with no variables, substitutions or other files, so the
# directory portal's validator can follow it (#684).
command -v pscat >/dev/null 2>&1 && command -v pscat-mcp >/dev/null 2>&1 || printf '%s\n' '{"systemMessage": "pscat plugin: pscat / pscat-mcp not found on PATH, so the pscat MCP tools will not start. Install them with: cargo install pscat (or a release binary from https://github.com/PSCatLabs/pscat/releases), then restart your agent."}'
