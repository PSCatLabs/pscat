# Security policy

pscat runs PostScript, which is a programming language. A `.ps` file is a
program, and running one is running code, even though pscat is built so that
the code cannot do much outside its own page. This file says what that
program can and cannot do **today**, where pscat has no limit yet, and how to
report a problem. Every claim was checked against the code, and the "Checked
with" notes name how. The undefined and refused operators, the file reads, the
resource-limit figures and the MCP behaviours below were also reproduced by
running a build (a `ci`-profile build, October 2026); they are measurements,
not guarantees.

## Reporting a vulnerability

Please do not open a public issue with the details.

1. On the repository's **Security** tab, use **Report a vulnerability**
   (GitHub private vulnerability reporting), if the button is there. The owner
   is enabling it; it was off when this file was written.
2. If it is not there, open a public issue titled `Security contact request`
   with **no details**: say only that you have a security report. The
   maintainer will reply with a private way to send it.

Include the pscat version (the release, or the commit hash if you built from source), your platform, a
minimal PostScript program or command that shows it, and what you expected.
This is a one-person project: expect an acknowledgement within about a week,
not an SLA. Fixes are made in the open once a report is understood, and the
reporter is credited unless they ask not to be.

### Supported versions

Security fixes go to `main` and to the latest release. Nothing older is
patched. Before 1.0, a fix may change behaviour in a minor release
([`docs/STABILITY.md`](docs/STABILITY.md)).

## What counts as a vulnerability

pscat's job is to run a PostScript program you chose. So these are **not**
vulnerabilities on their own:

- A program that runs forever, uses all your memory, or fills the disk with
  output. pscat has no limit on these yet (see "Limits that do not exist").
  Reports of a *new* way to do it are welcome as bugs, not as security
  reports.
- A program that reads a file the pscat process can read (see below).

These are:

- A program that **writes, deletes or renames** a file, starts a process, opens
  a network connection or reads the environment, from inside the interpreter.
  None of these operators exist today, so any way to get one is a bug.
- A crash, hang or memory-safety fault in the parser, the font loaders (Type 1,
  TrueType/OpenType), the image decoders (JPEG, Flate, LZW and the rest) or the
  PDF/SVG writers from a *crafted file* that should have been an ordinary
  PostScript error.
- A way for the MCP server to write outside what its documentation promises, or
  to run a command the caller did not ask for.
- A way for a page on the website's playground or `web/` demo to run script in
  the visitor's page.

## The threat model

**Treat a `.ps` file from a stranger like a shell script from a stranger:
only run it where you would be happy to run that script.** pscat is not a
sandbox. It has no process isolation, no seccomp or similar, and no resource
limits of its own. What it has is a small, mostly read-only operator set. The
sections below separate what the operators allow from what the process around
them allows.

### What PostScript can and cannot do today

Checked with: `src/ops/file.rs`, `src/file.rs`, the operator tables in
`src/ops/`, and each line below run through `pscat --headless -` (a `ci`
profile build of this commit).

**Reads files, anywhere the pscat process can read.**

- `(path) (r) file` opens any readable file. `(path) run` executes one as
  PostScript. Absolute paths and `..` are not restricted. A relative path is
  tried against the current directory first, then against pscat's own install
  directory (`lib/`, fonts).
- The bytes come back to the program, which can print them: `(/etc/hosts) (r)
  file 100 string readstring pop print` prints the first lines of
  `/etc/hosts`. So **a PostScript program can disclose the contents of any file
  you can read**, to wherever the output goes. That is your terminal for the
  CLI, and **the tool result sent back to the model** for the MCP
  `eval_postscript` tool.
- `run` on a file that is not PostScript fails with an error that names the
  first token it could not execute. That also discloses a little of the file.
- Reads are whole-file (`std::fs::read`) before the program sees a byte: a huge
  file, a FIFO or a device node such as `/dev/zero` stalls the run and grows
  memory without end (`(/dev/zero) (r) file` reached about 1 GB in 3 seconds).

**Cannot write, delete, rename, list or stat files.**

- `file` accepts only the access string `r`. `w`, `a`, `r+` and so on give
  `invalidfileaccess`, including for `%stdout`.
- `deletefile`, `renamefile`, `filenameforall`, `setfileposition` and
  `fileposition`, `writestring` and `writehexstring` are **not defined**. Neither
  are `%pipe%`, `%stdin`/`%stdout` device names, `getenv`, `putenv`,
  `setpagedevice`, or any operator that starts a process or opens a socket.
  `(path) status` always answers `false`, so it does not even say whether a file
  exists.
- The interpreter has no network code of its own, and no PostScript operator
  opens a connection. (The live window talks to the windowing system through
  `winit`, as any GUI does.) Of the shipped binaries, only `pscat-mcp`
  (`src/bin/pscat_mcp.rs`) starts other programs. The other `Command::new` calls
  in the repository are in benchmarks and tests.
- Font names are matched against the font catalog's file *stems*
  (`src/font.rs`, `catalog_fid`); a name from a program is never joined into a
  path.

**Output files are chosen by the person running pscat, not by the program.**
`--png`, `--svg`, `--pdf` and `--contact-sheet` write where the command line
says. PostScript cannot change that. (`quit` ends the run; it does not write.)

**`--spool DIR` runs every `.ps` or `.eps` file that lands in `DIR`.** Only
point it at a directory that people you trust can write to.

**`pscat-mcp` is different. See the MCP section.**

**In the browser** (`web/pscat.js`, the website playground), pscat is a
WebAssembly module with no filesystem: `file` and `run` find nothing, because
`std::fs` is unavailable on `wasm32-unknown-unknown`, and `src/paths.rs` and the
font catalog are compiled out. A program can draw and print into the page's
own output box; the playground sets text with `textContent` and stepped
execution means a runaway loop does not freeze the tab. The `run()` helper in
`web/pscat.js` runs to completion in one call and *will* block the tab on an
infinite loop, so a page that accepts untrusted programs should use `begin()`
and `step()` (as `web/index.html` and `site/playground.html` do). Captured
output is cut off at 64 KiB.

### Limits that exist

| Limit | Value | Where |
|---|---|---|
| Elements in one `string` or `array` | 16,777,216 (above that: `limitcheck`) | `src/ops/data.rs` |
| Execution stack depth | 10,000 (`execstackoverflow`) | `src/interp.rs` |
| Procedure `{ }` nesting while scanning | 500 (`limitcheck`) | `src/interp.rs` |
| Name-resolution cycles | detected, `limitcheck` | `src/interp.rs` |
| Canvas size | at most 8000 x 8000 device pixels (about 256 MB), `--page` 1..8000 points, `--dpi` 9..1200 | `src/main.rs`, `src/interp.rs` |
| Captured output (browser, `capture_stdout`) | 64 KiB | `src/interp.rs` |

A tail call such as `/f { f } def f` does not grow the execution stack, so the
stack limit does not stop it.

### Limits that do not exist yet

These are the gaps. A hostile program can use all of them today. Checked by
running each for a few seconds and watching the process (resident memory in
brackets):

- **No step, instruction or wall-clock limit.** `{ } loop` never finishes.
- **No total memory limit.** The 16 M cap is per object, not per program: a
  loop that keeps allocating arrays reached about 3 GB in 3 seconds
  (`/d 1 dict def { 16000000 array /k 1 index def } loop`).
- **No operand-stack depth limit.** `{ 1 } loop` grew to about 800 MB in 4
  seconds.
- **No graphics-state stack limit.** `{ gsave } loop` reached about 770 MB in 3
  seconds.
- **No path-length limit.** `0 0 moveto { 1 1 rlineto } loop` reached about
  1.8 GB in 4 seconds.
- **No cap on total decoded bytes from filters** (`FlateDecode`, `LZWDecode`,
  `RunLengthDecode`, ...). They decode as the program reads, so a small
  stream costs only what the program chooses to pull from it, but nothing stops
  a program from pulling without end.
- **No limit on output volume** on the command line: a program can print as
  much as it likes.
- **No fuzzing.** The parser, font loaders and image decoders have unit and
  golden tests, but have not been fuzzed. The interpreter's own code contains
  no `unsafe` apart from the WebAssembly export functions in `src/wasm.rs`;
  the dependencies (`tiny-skia`, `ttf-parser`, `zune-jpeg`, `flate2`, `winit`,
  `softbuffer`) have their own, and are not audited here.

Stopping a runaway run is the caller's job: use your shell's `ulimit`, a
container's memory and CPU limits, or `timeout`. Resource limits for the
interpreter and fuzzing are planned work
([#604](https://github.com/jeromebanks/postscript_interpreter/issues/604)),
not shipped.

### The MCP server (`pscat-mcp`)

`pscat-mcp` serves four tools over stdio to whatever agent or client launched
it: `render_postscript`, `handwrite`, `describe_art_capabilities` and
`eval_postscript`. It runs each call as a child process (`pscat`, and
`pscat handwrite` for `handwrite`) as the same user, with the same
environment and working directory as the server. Checked with:
`src/bin/pscat_mcp.rs`, `tests/mcp.rs` (which tests every bound below),
`scripts/mcp_inspector_check.sh`, and Claude Code and Codex sessions against
an installed build (October 2026, #568).

- **`out_path` writes are confined to one output root.** The root is the
  directory given with `--out-root DIR` when the server is registered, or
  else the server's working directory (Claude Code and Codex start it in the
  project directory). If that working directory is the filesystem root,
  `out_path` is switched off. A relative `out_path` starts at the root; an
  absolute one must land inside it. Refused: any `..` component, a directory
  that does not exist (the server creates none), a directory that resolves
  outside the root through a symlink, and an existing target that is not a
  regular file (a symlink, a directory, a device). Least privilege on top of
  that (#627): the file name must end in the extension of the output format
  (`.png`, `.svg` or `.pdf`, compared without regard to case; `handwrite` makes `.png`), so a render cannot
  be written as `Makefile`, `.envrc` or a git hook, and no directory below
  the root may start with a dot (`.git/`, `.claude/`, `.github/`), judged on
  the resolved path so a symlink to one is refused too. Both refusals are
  `out_path_refused` and name the `required_extension`. The bytes are written to a new temp file in the target's
  directory (`create_new`, which never opens an existing file or link) and
  renamed over the target, so an existing **regular file inside the root is
  replaced, but only one that already has the render's extension** (the name
  must end in it, so nothing else can be targeted), and a symlink swapped in after the check is replaced rather than
  followed. The directory is checked again just before the temp file is
  created and before the rename; a process that can already write inside the
  root and swaps a directory for a symlink at exactly that moment can still
  redirect the write (std has no `openat`, and the race needs local write
  access the tools themselves never give). A multi-page render writes `name-001.ext`, `name-002.ext`, ...
  beside it under the same rules. The bytes are a PNG, SVG or PDF, but an SVG
  or PDF contains text the PostScript program chose. Renders without
  `out_path` go to a private temp directory that is deleted after the call.
- **`eval_postscript` and `render_postscript` run arbitrary PostScript** with
  the file-read behaviour above. A model that calls them can read any file
  the server's user can read and receive the contents. This is not confined:
  it is the interpreter's file access, and the interpreter limits are #604.
- **Every call is bounded in time and memory held by the server.** A
  wall-clock limit per call (`--timeout`, default 60 s) kills the child and
  returns a `timeout` tool error. Each child stream (stdout, stderr) is kept
  up to 4 MiB; past that the child is killed and the call fails with
  `output_too_large`. `source` and `text` are capped (`--max-input-bytes`,
  default 1 MiB, `input_too_large`), and a request line far over that cap is
  discarded without being buffered. Inline images are capped
  (`--max-image-bytes`, default 3,750,000 bytes per call, and 20 pages); a
  bigger render is refused inline, though a file asked for with `out_path` is
  still written. The child's own memory is not limited: a program that
  allocates without end grows until the timeout stops it (see "Limits that
  do not exist yet").
- **One call at a time.** A runaway program delays later requests, `ping`
  included, until its timeout; there is no cancellation yet (#622).
- **Bad input gets a bounded, structured answer, and stdout stays clean.**
  Malformed JSON, invalid UTF-8, batches and non-object messages get JSON-RPC
  errors (-32700, -32600) and the session continues; an unknown tool is
  -32602; a bad argument (wrong type, out of range) is a tool error with
  `structuredContent.error.kind` `invalid_argument`. Only JSON-RPC is written
  to stdout: child output is captured through pipes and diagnostics go to
  stderr.
- **Protocol.** Only the revisions in the server's supported list (2025-11-25
  and 2025-06-18) are ever answered; see
  [the agents guide](docs/guides/agents.md#protocol).
- **Page size is bounded** because the server checks `width` and `height`
  (1..8000) and `dpi` (9..1200), and the CLI refuses a canvas over 8000 x 8000
  device pixels.
- **`handwrite`'s options are validated before they reach PostScript.**
  `pscat handwrite` accepts only plain decimal numbers for the numeric
  options (and exactly three for `ink`), `plain` or `ruled` for `paper`, and
  escapes `text` as a PostScript string; the server checks types and ranges
  first and passes `text` after `--`, so it is always text.
- `handwrite` runs `pscat handwrite`, found next to the server like the other
  tools' `pscat` (the same file, so it cannot run a different binary), and needs
  no checkout, no `scripts/` directory, no shell, and no `PSCAT_ROOT`. It
  never starts a build. It does read the PostScript library through the
  resource resolver, so a `lib/` directory named by `PSCAT_HOME` or
  `PSCAT_LIB`, or in the server's working directory, is used in place of the
  copy compiled into `pscat`; those variables and that directory are trusted
  the same way they are for `render_postscript`.
- The server speaks only over stdin and stdout. It opens no port.

If you register `pscat-mcp` with an agent, assume the agent can read any file
you can read and write files inside the output root, and run it somewhere that
is acceptable.

### Other tools

`scripts/*.sh` and `scripts/*.py` are developer tools. They run `pscat` and
other programs on your files and are not a security boundary. The website is
static pages plus the WebAssembly demo above, and has no server-side code.

## Reporting a limit that is missing

A missing limit from the lists above is a known gap, not a secret. If you want
one prioritised, say so on the issues linked here.
