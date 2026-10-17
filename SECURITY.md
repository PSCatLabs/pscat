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
`src/bin/pscat_mcp.rs`, `src/subcmd/handwrite.rs`, and calls to a built server.

- **`out_path` is an unconfined file write.** `render_postscript` and
  `handwrite` write the artifact to whatever path the caller gives, create or
  **overwrite** it, and accept absolute paths and `..` (a call with a `..` path
  wrote outside the scratch directory). The bytes are a PNG, SVG or PDF, but
  an SVG or PDF contains text that the PostScript program chose, so the file
  is not content the caller cannot influence. Any client that can call these
  tools can therefore overwrite any file the server's user can write. There is
  no allow-list, no confinement to a directory, and no refusal to overwrite.
  The `out_path` of a multi-page render also writes `out-001.ext`, `out-002.ext`
  and so on beside it.
- **`eval_postscript` and `render_postscript` run arbitrary PostScript** with
  the file-read behaviour above. A model that calls them can read files and
  receive the contents.
- **A runaway program blocks the server.** The server handles one request at a
  time, waits for the child with no timeout, and buffers all of the child's
  output in memory. Measured: after an `eval_postscript` of `{ } loop`, a
  following `ping` got no reply until the child process was killed by hand. A
  program that prints in a loop (`{ (x...) print } loop`) writes about 45 MB a
  second, all of it held by the server, whose address space grew by about 700 MB
  in 6 seconds (its resident size under-reports on macOS, which compresses
  repetitive text). The server has no cancellation either.
- **Page size is bounded** because the server passes `width` and `height` to the
  CLI, which refuses anything over 8000 (it exits with its usage text, and the
  server reports "no output produced").
- **`handwrite`'s options are validated before they reach PostScript.** Until
  #600 `scripts/handwrite.sh` spliced `ink`, `size` and the other options into
  PostScript source unchecked, and read a `text` that began with `-` as an
  option. `pscat handwrite` now accepts only plain decimal numbers for the
  numeric options (and exactly three for `ink`), `plain` or `ruled` for
  `paper`, and escapes `text` as a PostScript string; the server passes `text`
  after `--`, so it is always text. A value that is not a number fails with a
  usage error and renders nothing. This gave a caller nothing it did not already
  have through `eval_postscript`; it closes an injection point for a future
  version that narrows what the tools may do.
- `handwrite` runs `pscat handwrite`, found next to the server like the other
  tools' `pscat` (the same file, so it cannot run a different binary), and needs
  no checkout, no `scripts/` directory, no shell, and no `PSCAT_ROOT` (that
  variable is no longer read by the server). It never starts a build. It does
  read the PostScript library through the resource resolver, so a `lib/`
  directory named by `PSCAT_HOME` or `PSCAT_LIB`, or in the server's working
  directory, is used in place of the copy compiled into `pscat`; those variables
  and that directory are trusted the same way they are for `render_postscript`.
- The server speaks only over stdin and stdout. It opens no port.

If you register `pscat-mcp` with an agent, assume the agent can write any file
you can and read any file you can, and run it somewhere that is acceptable. The
bounds this section lists as missing (confinement of `out_path`, timeouts,
output caps) are planned work in
[#568](https://github.com/jeromebanks/postscript_interpreter/issues/568); this
file will be updated when they ship, and not before.

### Other tools

`scripts/*.sh` and `scripts/*.py` are developer tools. They run `pscat` and
other programs on your files and are not a security boundary. The website is
static pages plus the WebAssembly demo above, and has no server-side code.

## Reporting a limit that is missing

A missing limit from the lists above is a known gap, not a secret. If you want
one prioritised, say so on the issues linked here.
