# Security policy

pscat runs PostScript, which is a programming language. A `.ps` file is a
program, and running one is running code, even though pscat is built so that
the code cannot do much outside its own page. This file says what that
program can and cannot do **today**, where pscat has no limit yet, and how to
report a problem. Every claim was checked against the code, and the "Checked
with" notes name how. The undefined and refused operators, the file reads and
writes, the read and write roots, the resource-limit figures, the limit flags
and the MCP behaviours below were also reproduced by running a build (a `ci`-
or `release`-profile build, October 2026); they are measurements, not
guarantees.

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
- A program that reads a file the pscat process can read, on the command line
  and in the library, where reads are open unless you confine them (see
  below). The MCP server does confine them.
- A program that writes a file inside a directory you named with
  `--allow-write`. That is what the flag is for.

These are:

- A program that **writes, deletes or renames** a file outside the directories
  `--allow-write` named, from inside the interpreter. With no `--allow-write`
  (the default, and always under the MCP server) that is any file on disk.
- A program that starts a process, opens a network connection or reads the
  environment, from inside the interpreter. None of these operators exist
  today, so any way to get one is a bug.
- A way for a program that runs under **read roots** (`--read-root`,
  `--confine-reads`, and every MCP call) to read, or list with
  `filenameforall`, a file outside them.
- A way for a program to get **past a limit that is set**: catching a limit
  error with `stopped`, or growing memory, steps or time without bound
  between the checks described in "Limits that exist". The MCP server sets
  them by default, so there this is a vulnerability.
- A crash, hang or memory-safety fault in the parser, the font loaders (Type 1,
  TrueType/OpenType), the image decoders (JPEG, Flate, LZW and the rest) or the
  PDF/SVG writers from a *crafted file* that should have been an ordinary
  PostScript error.
- A way for the MCP server to write outside what its documentation promises
  (the `out_path` files and a private scratch directory per call, described
  below), or to run a command the caller did not ask for.
- A way for a page on the website's playground or `web/` demo to run script in
  the visitor's page.

## The threat model

**Treat a `.ps` file from a stranger like a shell script from a stranger:
only run it where you would be happy to run that script.** pscat is not a
sandbox. It has no process isolation and no seccomp or similar. It has resource limits
you can turn on (the CLI and library: off by default; the MCP server: on by
default), file access you can narrow (reads: open by default on the command
line and in the library, confined under the MCP server; writes: off unless you
name a directory), and a small operator set with no process, network or
environment access. The
sections below separate what the operators allow from what the process around
them allows.

### What PostScript can and cannot do today

Checked with: `src/ops/file.rs`, `src/ops/realsio.rs`, `src/file.rs`,
`src/write_roots.rs`, `src/read_roots.rs`, `src/filenames.rs`, the operator
tables in `src/ops/`, and each line below run through `pscat --headless -` (a
`release` profile build of this commit).

**Reads files, by default anywhere the pscat process can read.**

- `(path) (r) file` opens any readable file. `(path) run` executes one as
  PostScript, and `(path) n readreals` reads `n` numbers from one
  ("Writes files", below). By default absolute paths and `..` are not
  restricted. A relative path is tried against the current directory first,
  then against pscat's own install directory (`lib/`, fonts).
- The bytes come back to the program, which can print them: `(/etc/hosts) (r)
  file 100 string readstring pop print` prints the first lines of
  `/etc/hosts`. So **a PostScript program can disclose the contents of any file
  you can read**, to wherever the output goes. That is your terminal for the
  CLI, and **the tool result sent back to the model** for the MCP
  `eval_postscript` tool. Under the MCP server that is narrowed to the read
  roots below, but not to nothing.
- `(template) proc scratch filenameforall` lists file names, which is
  disclosure too. It lists regular files only (directories are not yielded),
  in every place a relative name would be looked up, and an absolute template
  such as `(/etc/*)` lists that directory. `*` and `?` match within the last
  path component only, and a wildcard in a directory part matches nothing.
  `(path) status` still answers `false` for any name and so does not say
  whether a file exists.
- `run` on a file that is not PostScript fails with an error that names the
  first token it could not execute. That also discloses a little of the file.
- Reads are whole-file, before the program sees a byte. With no limit set, a
  huge file, a FIFO or a device node such as `/dev/zero` stalls the run and
  grows memory without end (`(/dev/zero) (r) file` reached about 1 GB in 3
  seconds). With a limit set, the read looks at the limits as the bytes arrive
  and stops with `limitexceeded` (see "Whole-file reads and the limits").

**Confining reads: read roots.** `--read-root DIR` (repeatable) on the command
line, `--confine-reads` (the same with no directory: only the copy of `lib/`
compiled into pscat), and `Interp::set_read_roots` in the library. They are
**off by default**, so the CLI and the library read anywhere, as above. Under
them (`src/read_roots.rs`, used by `file`, `run`, `readreals` and
`filenameforall`; `pscat handwrite` takes both flags too):

- A `..` component anywhere in a name or a `filenameforall` template is
  `invalidfileaccess`, before any lookup.
- Every file the search order tries is resolved first (a symlink is followed to
  where it really points) and is read only if it lies inside a root; the
  resolved path is what is read. So a symlink inside a root that points out
  is refused too.
- An absolute name outside the roots is `invalidfileaccess` whether or not the
  file exists, so the error does not say what exists there. A place in the
  search order that is outside the roots (pscat's install directory, say) is
  skipped as if it had no file, so `(lib/artkit.ps) run` still finds the copy
  compiled into pscat.
- `filenameforall` lists only directories inside the roots and yields only
  names that would open, so outside them it yields nothing and raises no error.
- The roots decide *where*, not *what*. Inside a root a FIFO or a hard link
  somebody else made is read as it would be without roots (under the
  execution limits, "Whole-file reads and the limits"). `/dev/zero` is outside
  every root unless one names `/dev`, and a symlink to it inside a root
  resolves outside and is refused.
- Fonts are not read by path: `findfont` matches a name against the file stems
  in the font directories and never joins a program's string into a path, so
  roots do not gate it.
- The check and the read are not atomic. A *different* local process that can
  already write inside a root could swap a directory for a symlink in between.

**Writes files, only inside write roots, which are empty by default.**
`--allow-write DIR` (repeatable) on the command line and `Interp::set_write_roots`
in the library name the directories a program may write. With none, every disk
path is `invalidfileaccess`: `(path) (w) file`, `(a) file`, `renamefile`,
`deletefile` and `writereals` all fail the same way. The flag works for a run,
`-i`, the REPL and the live window, and is refused with `--spool` (spooled jobs
never write). `pscat-mcp` never sets a write root ("The MCP server").

- `file` takes `r`, `w` (truncate or create) and `a` (append or create). `r+`,
  `w+`, `a+` and anything else are `invalidfileaccess`. `(%stdout)` and
  `(%stderr)` are the only names that need no root: they are the run's own
  standard output (the stream `print` writes to) and standard error. Any other `%device` name (`%pipe%`, `%stdin`, `%lineedit`) is
  `undefinedfilename`.
- The operators are `write`, `writestring`, `writehexstring`, `flush`,
  `flushfile`, `closefile`, `renamefile`, `deletefile` and, for numbers,
  `array (path) [/f32] writereals` and `(path) n [/f32] readreals` (packed
  little-endian binary, `/f64` by default). `writereals` opens the file as
  `(path) (w) file` does, so every check below applies, and it checks every
  element before it opens the file, so a bad array never truncates the target.
  `fileposition`, `setfileposition`, `getenv` and `putenv` are **not defined**.
- The path check (`src/write_roots.rs`; the one place that decides) works on
  the name as the program gave it: a relative name is relative to the current
  directory only, never the search order for reads, so it cannot reach pscat's
  install directory. It refuses:
  - any `..` component, even one that would stay inside;
  - a path that, resolved in full (an existing file through its symlinks, a
    new file through its parent directory), is not strictly inside a root. A
    root itself is never a file;
  - a name ending in `/` or `/.`, an existing target that is not a regular file
    (a directory, FIFO or device), a symlink that points nowhere (opening it
    would create its target, wherever that is), and on Unix an existing file
    with more than one hard link (another of its names may be outside the
    roots);
  - a name that is not valid UTF-8, and a NUL.
  A new file is created with `O_EXCL`, which never follows a final-component
  symlink, and an existing one is checked again just before it is opened.
- `renamefile` and `deletefile` act on the directory entry: only the parent
  directory is resolved, so deleting a symlink that sits inside a root removes
  the link and never what it points to. Both names must be inside a root, and
  directories are refused at either end, so only files are renamed or removed.
- No operator makes a symlink or a hard link, so any link a program could abuse
  was already inside a root. As with the read roots, a *different* local
  process that can already write inside a root could swap a directory for a
  symlink between the check and the open. `std` has no `openat`, so this is
  accepted rather than closed.
- Disk writes are not buffered, so a run that ends without `closefile` loses
  nothing. **A limit that stops a long write leaves the file as short as it got**
  (it was truncated when it was opened). A program given a write root can fill
  that disk ("Limits that cannot be enforced").
- `(path) n readreals` follows the *read* policy (read roots, whole-file read,
  the limits), with two limits of its own: it refuses before it reads when the
  array would pass the memory limit, and `n` over the 16,777,216-element array
  cap is `limitcheck`. A file shorter than `n` values is `rangecheck`.

**Cannot do the rest of what a shell does.**

- `%pipe%`, `%stdin`, `setpagedevice`, `getenv`, `putenv`, and any operator that
  starts a process or opens a socket are **not defined** (or, for the device
  names, `undefinedfilename`).
- The interpreter has no network code of its own, and no PostScript operator
  opens a connection. (The live window talks to the windowing system through
  `winit`, as any GUI does.) Two things in the pscat program start other
  programs, neither reachable from PostScript: the MCP server (`pscat-mcp`, or
  `pscat mcp`; `src/mcp_server.rs`) runs `pscat` children, and **`pscat fonts
  install`**, a command you run yourself, runs `curl` (HTTPS only, redirects
  included) to download font packs, and checks every byte against a SHA-256
  chain before it installs anything (`src/fontpacks/install.rs`). The other
  `Command::new` calls in the repository are in benchmarks and tests.
- Font names are matched against the font catalog's file *stems*
  (`src/font.rs`, `catalog_fid`); a name from a program is never joined into a
  path. The standard 35 faces are compiled into the binary and are found the
  same way (#757), so a bare install needs no font files, and no font is read
  from a path a program names.

**Output files are chosen by the person running pscat, not by the program.**
`--png`, `--svg`, `--pdf` and `--contact-sheet` write where the command line
says. PostScript cannot change that. (`quit` ends the run; it does not write.)
A program that is given `--allow-write` can write other files, but still not
the rendered output.

**`--spool DIR` runs every `.ps` or `.eps` file that lands in `DIR`.** Only
point it at a directory that people you trust can write to. A spooled program
reads anywhere the process can (the read-root flags are refused with `--spool`)
and writes no file (`--allow-write` is refused too).

**`pscat-mcp` is different. See the MCP section.**

**In the browser** (`web/pscat.js`, the website playground), pscat is a
WebAssembly module with no filesystem: `file` and `run` read no disk file,
because `std::fs` is unavailable on `wasm32-unknown-unknown`, and the search
order's disk places and the font catalog are compiled out. The one thing they
find is the copy of `lib/` compiled into the module (`(lib/artkit.ps) run`,
and `filenameforall` lists those names), which is public library code, and
the standard 35 faces. Every disk write is `invalidfileaccess` (there are no
write roots to name), and `(%stdout)` writes land in the captured output. A
program can draw and print into the page's own output box; the playground sets text with `textContent` and stepped
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
| Canvas, sheet or board size under the MCP server | at most 40,000,000 device pixels by default (`--max-pixels`, at most 64,000,000) | `src/mcp_server.rs` |
| Captured output (browser, `capture_stdout`) | 64 KiB | `src/interp.rs` |
| Interpreter steps per program (opt in: `--max-steps N`, `Limits::max_steps`) | none by default; MCP 25,000,000,000 | `src/interp.rs` |
| Wall-clock time per program (opt in: `--max-time SECS`, `Limits::max_time`) | none by default; MCP `--timeout` less a quarter, at most 5 s (55 s); a render job `--job-timeout` less the same margin (3600 s default) | `src/interp.rs` |
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

**Whole-file reads and the limits (issue #702).** `file` and `run` read the
whole file before the program sees a byte. When any limit is set, that read is
done in 64 KiB chunks and the limits are looked at after each one:

- **Time and memory** are checked as for any step. A regular file bigger than
  the memory cap is refused from its length before a byte is read, and the
  buffer's growth is checked before it is made. `(/dev/zero) (r) file pop`
  stops with `limitexceeded (memory)` at the cap (about 100 MB resident under
  `--max-memory 100`), or `(time)` first, where only a time limit is set.
- **Steps.** A kilobyte read counts as one step, so `--max-steps N` also bounds
  a read, to about N KiB. Nothing else about the step count changes.
- **FIFOs, terminals, sockets and other devices** can block in the open or in
  the read, so they are read on a helper thread while the interpreter waits
  10 ms at a time and looks at the limits between waits. A FIFO that nobody
  writes to ends in `limitexceeded (time)` at the time limit. Each read the
  limit cut short leaves that one helper thread behind (it exits on its next
  chunk, or stays blocked in the kernel until the process ends). The CLI and
  the MCP child exit and reclaim it; a long-lived program that embeds the
  library and reads a stalled FIFO under a limit over and over leaks a thread
  per read.
- **With no limit set nothing changes**: the read is the plain `std::fs::read`.
  With only `--max-steps` set (no time or memory cap), a FIFO with no writer
  still blocks for good, because no bytes arrive to be charged; set a time limit
  too. With only a time limit, `/dev/zero` is read as fast as the machine allows
  (about 1 GB a second) until the clock stops it, so set a memory cap as well,
  as the MCP defaults do. (Under the MCP server the read roots refuse
  `/dev/zero` before any of this: it is outside every root.)
- **`readreals` and `filenameforall` follow the same rules.** `readreals` is a
  whole-file read, charged as above, and refuses before it reads when the array
  it would build passes the memory cap. `filenameforall` charges one step per
  directory entry it looks at and checks the bytes of each name it keeps, so
  listing a huge directory stops on a limit. `writereals` polls the limits once
  per 8,192 elements it writes (64 KiB as `/f64`, 32 KiB as `/f32`).

Checked with: `tests/limits.rs` (`file_reads`), `tests/limits_memory.rs`,
`tests/cli.rs` and `tests/mcp.rs`.

### Limits that cannot be enforced

These are the gaps. A hostile program can still use all of them, with or
without the limits above:

- **One long operator runs to completion.** The limits are read between
  interpreter steps (and every 1024 loop iterations inside one), never inside
  an operator. `array`, `string` and `aload`, the operators that can ask for
  hundreds of megabytes in a step, check a memory limit before allocating, and
  a whole-file read looks at the limits as it goes ("Whole-file reads and the
  limits", above). Everything else is looked at only afterwards. For a hosted
  service the answer to anything that gets through is the outer kill: the MCP
  server kills its child at `--timeout`, and anything else should run the
  process under `ulimit`, a container's limits or `timeout`.
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
- **Limited output volume from `==`, `pstack` and `stack` only.** Each of
  them is bounded (#745): containers nest to depth 32, an array shows 1024
  elements, a string or name 4 KiB, one operand 16 KiB, and a whole stack
  32 KiB, with `... truncated` marking the cut. **`=` and `print` stay
  unbounded**, as do writes to `%stdout` and `%stderr` (printing, and counted
  the same): on the command line a program can print as much as it likes
  with them, bounded only by the steps and time it is allowed. The MCP server
  caps each child stream at 4 MiB.
- **No limit on bytes written to a write root.** The memory limit counts the
  heap, not the disk. A program that has been given `--allow-write` can fill
  that disk, bounded only by the steps and time it is allowed; one `string`
  or `array` holds at most 16,777,216 elements, so one `writereals` writes at
  most 128 MiB (`/f64`) and one `writestring` at most 16 MiB. Name only a
  directory you would let a program fill, and put a quota or a separate
  volume under it if the program is a stranger's.
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
fence off. The harness names no write root and no read root, so a mutated
program's disk writes are all refused, and what the write-root and read-root
checks do on a hostile name is covered by their unit tests
(`src/write_roots.rs`, `src/read_roots.rs`) and `tests/`, not by fuzzing.

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

`pscat-mcp` serves ten tools over stdio to whatever agent or client launched
it: `render_postscript`, `handwrite`, `chalkboard_math`, `review_sheet`,
`describe_art_capabilities` and `eval_postscript`, and the four job tools
`start_render`, `render_status`, `render_result` and `cancel_render`. It runs each call as a
child process (`pscat`, and `pscat handwrite` for `handwrite`; for
`chalkboard_math` the server writes the PostScript itself and runs it in a
`pscat` child; for `review_sheet` one `pscat` child per cell) as the same user, with the same
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
- **`pscat-mcp` never writes from PostScript (#740).** It gives no child a
  write root: `--allow-write` is never passed, and no tool argument or server
  flag can add one. In a child, `(path) (w) file`, `renamefile`, `deletefile`
  and `writereals` on any disk path are `invalidfileaccess`. What the server
  itself writes is the `out_path` files above, and each call's private scratch
  directory (where a render without `out_path` goes), removed after the call;
  both are paths the server chose, never ones the PostScript program named. `(%stdout)` and
  `(%stderr)` work, since they are the child's own streams, which the server
  captures: stdout is the tool's text result (or `eval_postscript`'s output),
  capped at 4 MiB like any child stream, and stderr comes back as a bounded
  `Warnings:` item after a successful render (at most 8 distinct lines of 300
  characters, #752) or whole in the error after a failure; `eval_postscript`
  does not return it on success. A program can therefore put any text
  in front of the model, as it always could by printing. It can also write a
  line `pscat: progress pages=N` to `%stderr` to set its own job's
  `pages_done` while it runs: that counter is only a status display, nothing
  is decided by it, and the line is removed from the stderr returned.
- **Programs read files only inside the read roots (#763).**
  `eval_postscript`, `render_postscript`, `review_sheet`, `start_render` and
  `handwrite` run arbitrary PostScript (or, for `handwrite`, trusted library
  code), and every child that runs a program gets `--confine-reads` and one
  `--read-root` per root
  ("Confining reads: read roots", above, has the rules). The roots are:
  - the server's **working directory** (Claude Code and Codex start it in the
    project directory), unless it is the filesystem root;
  - `$PSCAT_HOME` and the per-user pscat data directory, when they exist (and
    are not the filesystem root);
  - each `--allow-read DIR` the person who registered the server named
    (repeatable; a bad one stops the server at startup). `--allow-read /`
    is taken as asked.
  No tool argument can add a root, and an empty list still confines: every
  child gets `--confine-reads` first, so it can read nothing on disk but the
  copy of `lib/` compiled into pscat. `/dev/zero`, `/etc/hosts` and `../` names
  are `invalidfileaccess`, `filenameforall` outside the roots yields nothing,
  and a symlink inside a root that points out is refused. The model can still
  read **every file inside the roots**, and the working directory is often a
  whole project, so a `.env` or a key file there is readable, and a client
  that starts servers in your home directory makes all of it readable
  (`~/.ssh` included; only `/` is excluded); start the server in a directory
  that holds nothing you would mind it reading, and add other
  directories deliberately with `--allow-read DIR`.
  `PSCAT_LIB` is honoured only when it lies inside a root, and a `lib/` in the
  working directory or under `PSCAT_HOME` replaces the compiled-in copy.
  The interpreter's step, time and memory limits (below) bound what a
  program does with its reads, not what it may read; the roots do that.
- **Every call is bounded in time and memory held by the server.** The
  interpreter's own limits are on in the child (#604): a step limit
  (`--max-steps`, default 25 billion), a heap-growth limit (`--max-memory`
  in MiB, default 1024) and a time limit of `--timeout` less a margin (a quarter, at most 5 s);
  past one the call fails with `limit_exceeded` and a `limit` field
  (`steps`, `time` or `memory`), and `render_postscript` still returns the
  partial page. `0` turns the step or memory limit off. A wall-clock limit per
  call (`--timeout`, 60 s by default and up to 86400; the MCP client's own tool-call
  timeout may also need raising, see the agents guide) kills the child and returns a `timeout`
  tool error, the backstop for a program the limits cannot reach ("Limits
  that cannot be enforced"). Each child stream (stdout, stderr) is kept
  up to 4 MiB; past that the child is killed and the call fails with
  `output_too_large`. `source` and `text` are capped (`--max-input-bytes`,
  default 1 MiB, `input_too_large`), and a request line far over that cap is
  discarded without being buffered. Inline images are capped
  (`--max-image-bytes`, default 3,750,000 bytes per call, and 20 pages); a
  bigger render is refused inline, though a file asked for with `out_path` is
  still written. The child's heap growth is limited by `--max-memory`
  (default 1024 MiB); a whole-file read of a FIFO inside a root is stopped by
  those limits too (see "Whole-file reads and the limits"; `/dev/zero` is
  outside every root).
- **`chalkboard_math` takes LaTeX, never PostScript, and has two caps of its
  own (#688), both server flags (#711).** The server turns the LaTeX into a program with the
  `pscat::chalk` generator (#555), in-process and without file or
  environment access: the TeX goes in as an escaped string literal and every
  other value is checked and printed by Rust, so the input cannot add
  PostScript. The program runs in a `pscat` child under the limits above,
  like `render_postscript`. Checked before anything runs:
  - `latex` is at most **4096 bytes** by default, `--max-latex-bytes`
    (`input_too_large`, with `limit` and `size`), on top of `--max-input-bytes`. A ten-row derivation is about
    1 KB; 4 KB (about 40 rows) rendered in 25 s on the machine it was
    measured on with the default board, inside the default time limit. A
    long formula on a large board can take longer; the time limit then stops
    it with `limit_exceeded`.
  - The board is at most **40,000,000 device pixels** by default,
    `--max-pixels` (width and height in points times dpi/72), and 8000 a side
    (`output_too_large`, with
    `limit_pixels`, `pixels`, `width_px`, `height_px`). The interpreter
    already refuses more than 8000 pixels a side (64 million), so this cap
    bounds memory and render time with headroom rather than preventing an
    out-of-memory: the canvas (4 bytes a pixel, 256 MB at 64 million) is
    allocated before `--max-memory` starts counting; the rest of a board's
    memory is counted. 40 million covers an 8K screen (7680 x
    4320) or an A2 poster at 300 dpi; a 1920 x 1200 pt board at 300 dpi (40
    million) peaked at 0.63 GB resident under the default limits, and a
    36-million-pixel board took 8 s. A board that big is a PNG of several
    MB, over the inline image cap: ask for it with `out_path`.
  - SVG and PDF boards are written only to `out_path`: a board is tens of
    thousands of grain marks, 9 to 20 MB of SVG, too big to return inline.
  A TeX error comes back as `latex_error` with a stable `code`
  (`unsupported_command`, `unsupported_character`, `missing_argument`, ...)
  and the `token` it named, in one line, never a PostScript stack dump.
- **`review_sheet` runs every cell in its own child, under the same limits,
  and reads no file by name (#708).** `pscat sheet` itself runs its programs
  in-process with no step, time or memory limit, so the server does not call
  it: each program goes in inline (`source`, never a path, so the tool opens
  no file the caller named) and each cell is rendered by a `pscat` child with
  `--max-steps`, `--max-time` and `--max-memory`, as `render_postscript` does;
  the server only composes the finished pictures, in-process, with the
  `pscat::contact_sheet` layout and label code (pure drawing, no files). All
  the cells share the one `--timeout`: each child gets what is left of it,
  and the call fails with `timeout` when under a tenth of a second remains. A cell that
  stops at a limit is drawn as far as it got and named in the text beside the
  sheet. Caps, checked before any program runs, all `output_too_large`:
  - at most **32 cells** (`limit_cells`), each a child process;
  - the cells at most **`--max-pixels`** (default 40,000,000) in all
    (`limit_pixels`, `pixels`), before any enlargement, and a cell at most
    8000 pixels on a side even enlarged;
  - the sheet at most **`--max-pixels`** (`limit_pixels`, `width_px`,
    `height_px`) and 8000 a side (`limit_side_px`): the sheet is 4 bytes a
    pixel in the server, allocated before `--max-memory` counts, as for
    `chalkboard_math`;
  - the sources together at most `--max-input-bytes`, and a label at most 200
    characters (the label text is drawn in the server process, so its length
    is bounded). The inline image is capped by `--max-image-bytes`; ask for a
    big sheet with `out_path`, which follows the rules above (`.png` only).
- **Calls run side by side, and can be cancelled** (#622). Each tool call
  but the job tools runs on its own thread, at most 4 at once (more are
  refused with `busy`), so a runaway program no longer delays `ping` or other
  calls, and `notifications/cancelled` kills that call's child.
- **Render jobs are bounded too** (#741). `start_render` runs
  `render_postscript` in the background under `--job-timeout` (default 3600
  s, up to 86400) instead of `--timeout`, with the same step, memory, pixel,
  output and `out_path` rules; at most `--max-jobs` (default 2) run at once,
  at most 16 are kept, and a finished one's result (in the server's memory, up
  to the inline caps) is dropped after `--job-ttl` (default 3600 s). No job
  outlives a clean shutdown: when stdin closes, every job is cancelled and its
  child killed and reaped, and the calls already asked for finish (each within
  `--timeout`) and answer, before the server exits. A server killed outright
  cannot do that; on Unix each child (of a call or a job) is started with
  `--exit-with-parent PID` and stops itself, from a thread of its own that
  looks twice a second, within about half a second of the server's death, even
  when the program is stuck inside one long operator. On Windows the flag is
  accepted and does nothing: an orphaned child runs on until its own time
  limit (`--timeout`, or `--job-timeout` for a job, less the margin), which
  for a job, by default, nearly an hour of CPU. A job's child is started with
  `--progress`, which prints `pscat: progress pages=N` to its stderr after each
  slice in which a page was finished; the server reads those lines for
  `render_status` and removes them from the stderr it returns. Job ids are
  sequential, which is fine for one local client and not for a shared server:
  a hosted one (#689) would need unguessable ids.
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
  device pixels. Under that ceiling (64 million pixels, a canvas of 256 MB
  allocated before `--max-memory` counts) every tool that allocates a canvas
  shares one cap, `--max-pixels` (#711, default 40,000,000, at most
  64,000,000), and answers `output_too_large` with `limit_pixels`, `pixels`,
  `width_px` and `height_px`: `render_postscript` before the child starts (the
  page size is fixed by its arguments; a program cannot resize it), `handwrite`
  after the note's height is sized from its text and before it is drawn (the
  server passes `pscat handwrite --max-pixels`, and refuses a width over 8000
  pixels itself), `chalkboard_math` and `review_sheet` as above. An operator
  who raises `--max-pixels` should raise `--max-memory` with it.
- **`handwrite`'s options are validated before they reach PostScript.**
  `pscat handwrite` accepts only plain decimal numbers for the numeric
  options (and exactly three for `ink`), `plain` or `ruled` for `paper`, and
  escapes `text` as a PostScript string; the server checks types and ranges
  first and passes `text` after `--`, so it is always text.
- `handwrite` runs `pscat handwrite`, found next to the server like the other
  tools' `pscat` (the same file, so it cannot run a different binary), and needs
  no checkout, no `scripts/` directory, no shell, and no `PSCAT_ROOT`. It
  never starts a build. It does read the PostScript library through the
  resource resolver, so a `lib/` directory under `PSCAT_HOME` or in the
  server's working directory (both read roots), or named by `PSCAT_LIB` when
  that lies inside a root, is used in place of the copy compiled into `pscat`;
  those variables and that directory are trusted the same way they are for
  `render_postscript`.
- The server speaks only over stdin and stdout. It opens no port.

If you register `pscat-mcp` with an agent, assume the agent can read any file
inside its read roots (the working directory, which is often a whole project,
the pscat data directories, and each `--allow-read` directory) and receive the
contents, can write rendered images inside the output root, and can write
nothing else. Run it somewhere that is acceptable.

### Other tools

`scripts/*.sh` and `scripts/*.py` are developer tools. They run `pscat` and
other programs on your files and are not a security boundary. The website is
static pages plus the WebAssembly demo above, and has no server-side code.

## Reporting a limit that is missing

A missing limit from the lists above is a known gap, not a secret. If you want
one prioritised, say so on the issues linked here.
