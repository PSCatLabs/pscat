# The pscat MCP server in full

Read this when the pscat MCP tools are connected and you need a job, a limit,
`bands`, or the registration command. Back to [SKILL.md](../SKILL.md).

`pscat-mcp` (installed beside `pscat`) serves ten tools over stdio:
`render_postscript` (source → PNG images inline, or SVG text, or a PDF
written to `out_path`; adds a `Lint:` block when the page looks wrong),
`eval_postscript` (what the program printed, or the error with the
offending command and line), `handwrite` (see
[handwriting-chalk.md](handwriting-chalk.md)), `chalkboard_math` (LaTeX → a
chalkboard PNG; same file), `review_sheet` (the review sheet in
[sheets.md](sheets.md), from inline programs: byte for byte what
`pscat sheet` writes) and `describe_art_capabilities`, plus render jobs for
anything slow.

## Time limits and render jobs

Each call has a time limit (60 s by default; whoever registers the server raises it with
`pscat-mcp --timeout SECS`, and the client's own tool timeout must allow it too). A render
that may take longer goes through **`start_render`** (same arguments as `render_postscript`,
returns a `job_id` at once), then **`render_status`** (`queued`, `running`, `done`, `failed`,
`cancelled`, with `pages_done`) until it is finished, then **`render_result`** (exactly what
`render_postscript` returns); **`cancel_render`** stops one. A job may run an hour by default
(`--job-timeout`), two at once (`--max-jobs`). Raise `--max-steps` (or `0`) too, or the step
limit stops a long render first.

## Bands

Both render tools take **`bands`** (1 to the server's `--max-bands`, default 4):
`pscat --bands N`, worth it only for a program that reads `/BandBBox` (see
[bands-profile.md](bands-profile.md)). PNG only; the `Lint:` block covers band 0 and the
stitched page, and a job's `pages_done` stays 0 until it ends. The step and time limits
apply to each band; the call's or job's wall-clock limit to the whole render.

## Showing the render to the user

The PNG a render tool returns is for you, the model; the user usually cannot see
it (the `pscat` skill's "Show the result to the user" has the per-host details).
A render without `out_path` leaves no file anywhere, so the tool reply has no path
to hand over. Pass `out_path` on the render you mean to show: the reply then ends
with `wrote <absolute path>`, which is the PNG to send. `handwrite`,
`chalkboard_math`, `review_sheet`, and `render_result` for a job started with an
`out_path` do the same. The server's instructions and these tools' descriptions say so too.

## Reading your own files and libraries

Programs read files only in the server's working directory, the pscat data
directories (`$PSCAT_HOME` and the per-user one, whose `lib/` is on the `lib/`
search order) and each `--allow-read DIR`; the built-in `lib/` is always
available. `--allow-read` is a flag of the server's command line with no
environment variable, so another plugin's directory is not readable unless
whoever registered the server adds it; copy the files into the project instead.
`PSCAT_LIB` is read like any other path: outside these roots it is not found.

## Limits, `out_path` and registering

The interpreter stops a runaway program itself, with a `limit_exceeded` error whose
`limit` field says `steps`, `time` or `memory` (a render still returns the partial page);
`stopped` cannot catch it.

`out_path` must be inside the server's output root (the server's instructions name it;
`--out-root DIR`, which the plugin takes from `out-root = DIR` in
`~/.config/pscat/plugin.conf`), end in the output format's extension (`.png`, `.svg`,
`.pdf`) and not sit in a dot-directory such as `.git/`, and large inline images are
refused: lower `dpi` or pass `out_path`. A workspace outside the root: save there, then
`cp` the file; never decode an inline image.

Register it with `claude mcp add pscat -- pscat-mcp` (Claude Code) or
`codex mcp add pscat -- pscat-mcp` (Codex).
