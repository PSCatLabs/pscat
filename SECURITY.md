# Security policy

pscat runs PostScript, which is a programming language. A `.ps` file is a
program, and running one is running code, even though pscat is built so that
the code cannot do much outside its own page. This file says what that
program can and cannot do **today**, where pscat has no limit yet, and how to
report a problem. Every claim was checked against the code, and the "Checked
with" notes name how. The undefined and refused operators, the file reads, the
resource-limit figures, the limit flags and the MCP behaviours below were also
reproduced by running a build (a `ci`- or `release`-profile build, October
2026); they are measurements, not guarantees.

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
  output, where no limit was asked for. The command line and the library
  apply none unless you pass one (`--max-steps`, `--max-time`,
  `--max-memory`), and "Limits that cannot be enforced" lists what a limit
  cannot stop even when it is on. Reports of a *new* way to exhaust resources
  there are welcome as bugs, not as security reports.
- A program that reads a file the pscat process can read (see below).

These are:

- A program that **writes, deletes or renames** a file, starts a process, opens
  a network connection or reads the environment, from inside the interpreter.
  None of these operators exist today, so any way to get one is a bug.
- A way for a program to get **past a limit that is set**: catching a limit
  error with `stopped`, or growing memory, steps or time without bound
  between the checks described in "Limits that exist". The MCP server sets
  them by default, so there this is a vulnerability.
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
sandbox. It has no process isolation and no seccomp or similar. It has resource limits
you can turn on (the CLI and library: off by default; the MCP server: on by
default), and a small, mostly read-only operator set. The
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
  `winit`, as any GUI does.) Only the MCP server (`pscat-mcp`, or
  `pscat mcp`; `src/mcp_server.rs`) starts other programs. The other `Command::new` calls
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
| Interpreter steps per program (opt in: `--max-steps N`, `Limits::max_steps`) | none by default; MCP 25,000,000,000 | `src/interp.rs` |
| Wall-clock time per program (opt in: `--max-time SECS`, `Limits::max_time`) | none by default; MCP `--timeout` less a quarter, at most 5 s (55 s) | `src/interp.rs` |
| Heap growth per run (opt in: `--max-memory MIB`, `Limits::max_memory`) | none by default; MCP 1024 MiB | `src/interp.rs`, `src/memtrack.rs` |

A tail call such as `/f { f } def f` does not grow the execution stack, so the
stack limit does not stop it.

**The execution limits (issue #604).** Three caps, off by default on the
command line and in the library, and on by default in the MCP server:

- **Steps.** `--max-steps N`, or `Limits::max_steps` with
  `Interp::set_limits`. A step is one object executed, and each loop
  iteration also counts as one. The heaviest gallery piece (`alpine_lake.ps`
  at 144 dpi) takes 2.3 billion; the MCP default is 25 billion, about ten
  times that. At the interpreter's speed (about 60 million steps a second on
  the machine it was measured on) that is minutes, so under the MCP
  defaults the clock stops a runaway first; the step cap is the bound that
  does not depend on machine speed.
- **Time.** `--max-time SECS`, or `Limits::max_time`. Measured from the start
  of the program on the real clock. The MCP server passes its `--timeout` less a
  quarter of it, at most 5 s (55 s of the default 60 s), so the interpreter
  stops first and the render keeps its partial page; the server kills the
  child at the full `--timeout`.
- **Memory.** `--max-memory MIB`, or `Limits::max_memory`. Counts **heap
  bytes the process has gained** since the limit was set, from a counting
  global allocator (`pscat::TrackingAllocator`, installed by the `pscat`
  binary; a library user installs it too, and `set_limits` refuses a memory
  limit without it rather than ignoring it). It covers everything that
  reaches the heap: arrays, strings, dictionaries, the operand, dictionary
  and graphics-state stacks, paths, recorded SVG and PDF output. It is the
  process's heap, not the resident set and not one interpreter's share, so
  another thread's allocations count too. The MCP default is 1024 MiB; the
  heaviest gallery piece peaked at 0.4 GiB resident, canvas included.

A program stopped by a limit gets the error `limitexceeded (steps)`,
`(time)` or `(memory)` (`PsError::StepsExceeded`, `TimeExceeded` or `MemoryExceeded` in the library; `%%[ Error:
limitexceeded (steps); ... ]%%` on stderr and a non-zero exit from the CLI;
`kind: limit_exceeded` with a `limit` field from the MCP server). **`stopped`
cannot catch it**, so a program cannot loop around the limit, and it is
never written to `$error`. The page drawn so far is kept and written, as
after any other error. The limits apply to one program: the step count and
the clock restart with each `begin_source`/`run_str`; the memory cap counts
growth from `set_limits`.

Checked with: `tests/limits.rs`, `tests/limits_memory.rs`, `tests/cli.rs` and
`tests/mcp.rs`, and each of these run against a `release` build with
`--max-memory 256`, which stopped every one of them before the process
passed about 250 MB resident: `{ 1 } loop`, `{ gsave } loop`,
`0 0 moveto { 1 1 rlineto } loop`, and (refused before allocating)
`/d 1 dict def { 16000000 array /k 1 index def } loop` and
`/a 16000000 array def { a aload pop } loop`.

### Limits that cannot be enforced

These are the gaps. A hostile program can still use all of them, with or
without the limits above:

- **One long operator runs to completion.** The limits are read between
  interpreter steps (and every 1024 loop iterations inside one), never inside
  an operator. `array`, `string` and `aload`, the operators that can ask for
  hundreds of megabytes in a step, check a memory limit before allocating.
  Everything else is looked at only afterwards. The clearest case is a
  whole-file read: `(/dev/zero) (r) file pop` reads before the program sees a
  byte, never finishes, and went to 1.07 GB resident in 6 seconds under
  `--max-memory 100`; a FIFO that nobody writes to blocks the same way
  forever. For a hosted service the answer is the outer kill: the MCP server
  kills its child at `--timeout` (tested with a FIFO), and anything else
  should run the process under `ulimit`, a container's limits or `timeout`.
- **Memory is looked at once per slice**, so a program can pass its cap by
  what 512 steps (when a time or memory limit is set) can allocate before the
  next look. With a 256 MiB cap, an operand-stack, graphics-state-stack and
  path-growth loop each stopped at about the cap (peak resident 217 to 243
  MB), so for those the overshoot is small; an operator that allocates a lot
  per step and is not one of the three that check first could overshoot more.
- **A spike that is freed again inside one slice is not seen.** Memory is
  read between slices, so only what is still allocated then counts; a program
  that allocates and drops it again within 512 steps passes. Measured: a
  100,000-element procedure literal that is scanned and then `pop`ped (4 MB)
  runs clean under `--max-memory 1`; kept with `def`, it fails.
- **A step limit is approximate by up to a slice.** Objects are held to the cap
  (the inner loop is given no more steps than the program has left). Loop
  iterations are charged in blocks of 1024, and a program that ends inside its
  last slice is not charged for that slice's objects, so a finishing program
  can pass the cap by under one slice (4096 steps, 512 with a time or memory
  limit) plus 1023 loop iterations: `9 {} repeat 1 2 3` runs clean under
  `--max-steps 10`. A program that does not finish is stopped within a slice,
  and within 1024 iterations inside a loop. The count is not kept at all when no
  limit is set. Counting every step exactly means returning the executed count
  from the interpreter's inner loop; a version that did cost 1.9% in
  instructions retired on `fillkit.ps`.
- **No limit on output volume** on the command line: a program can print as
  much as it likes, bounded only by the steps and time it is allowed. The MCP
  server caps each child stream at 4 MiB.
- **No cap on total decoded bytes from filters** (`FlateDecode`,
  `LZWDecode`, `RunLengthDecode`, ...) as such. They decode as the program
  reads, so what a program keeps counts against the memory limit and what it
  pulls and discards costs steps and time.
- **No time limit in the browser build**, which has no clock: the `wasm32`
  module reads time as 0. A step limit applies (`Interp::set_limits`), but
  the playground does not set one; it runs in slices (`begin` and `step` in
  `web/pscat.js`) so a runaway does not freeze the tab.
- **`--spool` takes no limits yet**: the flags are refused with it, and the
  spool runs each file without a cap.
- **`pscat handwrite` and the other subcommands take no limit flags.** The MCP
  `handwrite` tool runs trusted library code on escaped text, bounded by
  `--timeout`.
- The memory limit needs the counting allocator, so a library user who has
  not installed it gets only steps and time (and an error if they ask for
  memory).

### Fuzzing

The scanner and the whole interpreter are fuzzed with
[cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) (`fuzz/`, targets
`lexer` and `interp`; the interpreter target runs under tight limits). A
seed corpus is committed (`fuzz/seeds/`) and replayed by `cargo test`
(`tests/fuzz_corpus.rs`), as is every crash once fixed
(`fuzz/regressions/`). The `fuzz` workflow
(`.github/workflows/fuzz.yml`) runs both targets for ten minutes each every
week, and on request; a crash opens an issue and uploads the input. Writing the
harness turned up one panic, now fixed and kept as a regression input:
`9223372036854775807 packedarray` aborted with `capacity overflow`. A
first local run of each target (one and a half minutes for `interp`,
a minute for `lexer`, 600,000 inputs for the lexer) found nothing more; that
is a smoke test, not a clean bill. Two things the fuzzing does not report on
purpose: leaks (a program that puts a dictionary into itself builds a
reference cycle that is never freed; LeakSanitizer is switched off, and that
is the program's own doing, not memory unsafety) and the unbounded whole-file
read above, which a mutated `file` path could reach and the harness does not
fence off.

What is **not** fuzzed yet: the Type 1 and TrueType font loaders, the image
decoders (JPEG, Flate, LZW and the rest) other than as the interpreter
reaches them through a program, and the PDF and SVG writers. A reader for PDF
files does not exist yet, so there is no target for it. The dependencies
(`tiny-skia`, `ttf-parser`, `zune-jpeg`, `flate2`, `winit`, `softbuffer`) have
their own fuzzing and are not audited here. The interpreter's own code contains
no `unsafe` apart from the WebAssembly export functions in `src/wasm.rs` and
the counting allocator in `src/memtrack.rs`, which forwards every call to the
system allocator and adds two atomic counters.

Stopping a runaway run that a limit cannot reach is still the caller's job:
use your shell's `ulimit`, a container's memory and CPU limits, or `timeout`.

### The MCP server (`pscat-mcp`)

`pscat-mcp` serves four tools over stdio to whatever agent or client launched
it: `render_postscript`, `handwrite`, `describe_art_capabilities` and
`eval_postscript`. It runs each call as a child process (`pscat`, and
`pscat handwrite` for `handwrite`) as the same user, with the same
environment and working directory as the server. Checked with:
`src/mcp_server.rs`, `tests/mcp.rs` (which tests every bound below),
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
  it is the interpreter's file access; the interpreter's step, time and memory
  limits (below) do not confine what a program can read.
- **Every call is bounded in time and memory held by the server.** The
  interpreter's own limits are on in the child (#604): a step limit
  (`--max-steps`, default 25 billion), a heap-growth limit (`--max-memory`
  in MiB, default 1024) and a time limit of `--timeout` less a margin (a quarter, at most 5 s);
  past one the call fails with `limit_exceeded` and a `limit` field
  (`steps`, `time` or `memory`), and `render_postscript` still returns the
  partial page. `0` turns the step or memory limit off. A wall-clock limit per
  call (`--timeout`, default 60 s) kills the child and returns a `timeout`
  tool error, the backstop for a program the limits cannot reach ("Limits
  that cannot be enforced"). Each child stream (stdout, stderr) is kept
  up to 4 MiB; past that the child is killed and the call fails with
  `output_too_large`. `source` and `text` are capped (`--max-input-bytes`,
  default 1 MiB, `input_too_large`), and a request line far over that cap is
  discarded without being buffered. Inline images are capped
  (`--max-image-bytes`, default 3,750,000 bytes per call, and 20 pages); a
  bigger render is refused inline, though a file asked for with `out_path` is
  still written. The child's heap growth is limited by `--max-memory`
  (default 1024 MiB); what that cannot see, such as a read of `/dev/zero`,
  grows until the timeout stops it (see "Limits that cannot be enforced").
- **One call at a time.** A runaway program delays later requests, `ping`
  included, until its limit or timeout stops it; there is no cancellation yet
  (#622).
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
