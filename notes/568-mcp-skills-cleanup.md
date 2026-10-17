## pscat-mcp protocol contract and bounds; skills published from skills/ (issue #568, 2026-10-16)

Part of release epic #562, a 0.6.0 blocker for the plugin (#569). Scope is
the issue's "Resolved spec": protocol fix, MCP-side safety bounds, skills
layout for an installed pscat, a use-case table. New task-level tools and
the eval set were deferred (#623).

**Protocol.** `initialize` used to echo whatever `protocolVersion` the
client sent. Now `SUPPORTED_VERSIONS = ["2025-11-25", "2025-06-18"]`, the two
revisions real clients were seen sending (probed with a tee wrapper on the
server's stdin and stdout):

- Claude Code 2.1.301 first sends 2026-07-28's `server/discover`, with the
  version in `_meta`. It takes -32601 as meaning "legacy server" and then
  sends `initialize` with 2025-11-25.
- Codex 0.163.0 sends `initialize` 2025-06-18.
- MCP Inspector CLI 2.9.0 and 2.11.0 send 2025-11-25.

For tools-only use the two revisions behave the same.

2025-03-26 is left out on purpose. A 2025-03-26 server must accept JSON-RPC
batches, and this one refuses them (-32600). A well-formed revision we don't
support gets our newest version back, as the legacy spec says a server
must. A missing or malformed version is refused with -32602 and
`data: {supported, requested}`, the error shape the spec shows. 2026-07-28
is not implemented (#621). Answering its probe with -32022 would tell a
dual-era client the server is modern, so the probe gets -32601.

**No SDK.** The official Rust SDK (rmcp) was considered and not adopted:

- It brings in tokio and a large dependency tree. That is supply-chain
  surface for #601's cargo-deny, and it would make this an owner-merge
  Cargo change.
- It would not provide the timeouts, output caps or `out_path`
  confinement. Those are this server's own policy.
- The protocol surface is four methods.

Compliance is checked against real clients instead. The official
conformance suite (`@modelcontextprotocol/conformance` 0.1.16) can't run
against this server: its `server` mode only takes `--url` (HTTP), and this
server is stdio. That is why CI uses the Inspector CLI.

**Bounds** (all in `src/bin/pscat_mcp.rs`, tested in `tests/mcp.rs`):
- `run_child`: wall-clock `--timeout` (default 60 s; the child is killed),
  4 MiB kept per stream with overflow killing the child, stdin fed on a
  thread. Renders go to a private scratch dir that is deleted afterwards (they
  used to pile up in the temp dir).
- `--max-input-bytes` (1 MiB) on `source`/`text`, a tool error on the
  request's own id. The transport line cap is 6x that plus 64 KiB, and a
  longer line is drained without buffering. Bad UTF-8 used to end the read
  loop and exit the server; now it is a -32700.
- `--max-image-bytes` (3,750,000 per call, about 5 MB base64, the Claude API's
  per-image limit) and 20 pages inline. Over that the call is a tool error,
  but an `out_path` file is still written and named.
- `out_path`: rooted at `--out-root` or the working directory (off when that
  is `/`); no `..`; the parent must exist and canonicalize inside the root; a
  symlink or directory target is refused; written as a `create_new` temp file
  plus `rename`, so a symlink raced in is replaced, not followed. Regular files
  inside the root are overwritten (documented). Multi-page renders number
  their files under the same rules.
- JSON-RPC hygiene: -32700 for parse errors, -32600 for batches, non-objects,
  bad ids and missing `jsonrpc`, -32602 for bad params and unknown tools
  (the spec's code; it used to be an `isError` result). Messages without a
  method (responses) get no reply. Arguments are type- and range-checked
  (`invalid_argument`; a negative width used to default silently to 612).
- Error results carry `structuredContent.error` (`kind`, `message`, plus
  `ps_error`/`offending_command`/`line` parsed from pscat's `%%[ Error: ...
  ]%%` line) and the same text in `content`. Claude Code's model saw the
  text in the smoke test below, so `structuredContent` does not hide it.
  Success results carry no `structuredContent`, so it never stands in for an
  image.
- `initialize` returns short `instructions` (the limits and the output root),
  and tool descriptions state the limits.

**Tested with real clients.** Both runs used a `cargo install --locked
--path .` build in a scratch prefix, run from an empty directory outside the
checkout, with a tee wrapper logging both directions:
- **Claude Code 2.1.301**, scripted with `claude -p --mcp-config ...
  --strict-mcp-config --allowedTools mcp__pscat__...`. The interactive
  `claude mcp add` was not used, to keep the user's config untouched.
  Handshake: `server/discover` got -32601, then `initialize` 2025-11-25.
  - An eval of `1 2 bogus`: the model quoted `undefined` / `bogus`.
  - A render with `out_path square.png`: written in the project, and the
    model described the blue square.
  - `out_path ../escape.png`: refused, nothing written.
  - Every stdout line was JSON.
- **Codex 0.163.0**, with `codex exec -c mcp_servers.pscat.command=...`:
  `initialize` 2025-06-18 and the same three calls with the same results.
- **MCP Inspector CLI 2.9.0**, run by `scripts/mcp_inspector_check.sh`,
  locally and in the new `mcp` workflow. It covers:
  - tools/list, plus the `--strict` schema portability check;
  - `--protocol-era auto`, Claude Code's discover-then-fallback;
  - a render, an eval and a structured error;
  - an out_path refusal, and handwrite.
- Claude Desktop was not tested. It may start servers in `/`, in which
  case `out_path` is off until `--out-root` is given; inline images still
  work.

**Skills.**
- `pscat`, `psart` and `psviz` moved to `skills/<name>/SKILL.md`, the single
  source for #569's plugin layouts. `.claude/skills/{pscat,psart,psviz}` are
  symlinks to them; `claude -p` in the worktree still listed all three.
  A Windows checkout without symlink support gets text files there, which
  only affects contributors' local Claude Code sessions.
- The skills now assume an installed `pscat`:
  - no `cargo build`, `target/` or `scripts/*.sh`;
  - the `pscat handwrite/typewrite/photo-etch` subcommands are used instead;
  - `(lib/...) run` works anywhere, because lib/ is embedded (#598);
  - `examples/`, `gallery/` and `docs/` are linked on GitHub;
  - the Ghostscript cross-checks are marked as needing a checkout;
  - a bare install may lack the font catalog, and unknown fonts silently
    become Helvetica.
- psart went from 635 lines to a 144-line SKILL.md plus `references/` (paint,
  dry media, board math, styles and type), moved over verbatim except for
  the paths. The use-case table is in `docs/guides/agents.md`.

**Not done / follow-ups.** 2026-07-28 (#621). Cancellation and concurrent
calls: a long call still delays `ping` until its timeout (#622). Task-level
tools and the eval set (#623). Interpreter memory and step limits: the
child's memory is bounded only by the timeout (#604). The `mcp` workflow
is not a required check.
