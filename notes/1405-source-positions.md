## Source positions for procedure bodies (#1405, 2026-11-25)

Built on `feature/0.8.0` (which carries the host API of #1410). A backtrace
frame (`host::TraceFrame`) now has `line: Option<usize>` and `file:
Option<String>`: where the running element of a procedure was written, so an
editor or an agent can jump to it. `ErrorDetails::line` is still where
*execution* was (the line that called the procedure). The CLI prints one line
per frame after the unchanged `%%[ Error ... ]%%` line. `ErrorDetails` itself is
unchanged.

### Design: a side table, runs of lines, keyed by the body's storage

`Object` has no room for a position and a position on every number and name
would cost every program, so `src/srcpos.rs` keeps a table with one entry per
procedure body scanned from the main program or a `run` file. This is the
table #1338 built for `--lint`, generalised (the lint reads the same table now,
with its extras, the byte span and how a body ends, kept only while it traces).

- **Key**: the address of the body's storage (`PsArray::storage_site`), with a
  `Weak` handle in the entry so the address cannot be reused while it exists.
  `bind` (in place), `cvx`, `cvlit` and `getinterval` (a view, offset into the
  same storage) keep the storage, so they keep the lines. `copy`, `astore`,
  `array`, `aload` and run-time procedures have new storage and no entry.
- **Value**: the first element's line, plus `(element, line)` for each element
  that starts a new line, in one arena `Vec` per table (no allocation of its
  own per body). Source is several elements to a line, so this is about a byte
  per element; a four-byte line per element, which the lint's table used, is
  four times that. A lookup is a binary search, only at error time (at most 16
  frames).
- **Not a span with a re-scan**: that needs every source text kept (a prelude
  is read by a `run_str` that is gone by the time of the error) and a second
  tokenizer that must agree with the first on line endings, `//name`, binary
  tokens and the object format. It would save roughly the 43 bytes an entry
  costs and take on all of that.
- **Not a position on every object**: `Object` is full, a field would be copied
  with every operand, and a copied element (`aload astore`) would carry a line
  that is no longer true.

### Which text a line belongs to

A line is meaningless without its text. A `run` file's name is the name `run`
was given. The main program is one text per `run_str`, whose line 1 is its own,
so its entries are named only when the host called `set_main_file_name` while it
was scanned; those outlive the program (psanim loads its prelude in one
`run_str` and the errors are in later ones: `set_main_file_name(Some("scene.ps"))`,
load, `set_main_file_name(None)`). An unnamed main program's entries are dropped
when the next program begins, so a frame never carries a line of another text:
`line: None` is the right answer there. `ErrorDetails::file` is unchanged
(`None` for the main program), so the frame text's own line 1 is not labelled
`scene.ps`. The lint keeps its old rule and forgets everything per program.

Executable strings, `token`, and `eexec` streams record nothing (the scanner's
`line()` is `None` for them, as in the error report).

### Bounds and charging

- Allocation goes through the tracking allocator, so `--max-memory` counts it;
  `register` also asks the cap before the hash map or the runs arena doubles
  (the one large allocation), and the scan fails with `MemoryExceeded`.
- Entries and runs are capped at 2^22 per table (past it a body gets no line),
  the distinct names at 256.
- Entries of freed bodies are swept when a table has quadrupled (floor 16384),
  so a file run again and again does not grow it. The sweep's visits are
  charged to the decode budget, a step per 16 entries (`register` returns the
  work, `scan_procedure` charges it through the lexer); dropping an unnamed
  program's table at `begin_source` is bounded by what that program charged
  when it built it.
- Error time: one pass over at most 16 frames, a binary search each, after the
  existing bounded name scan.

### Cost (release build, this machine; `/usr/bin/time -l` instructions retired,
`target` base = `feature/0.8.0` at `4c957b7b`)

| Program | instructions retired | wall, best of 9 | peak RSS |
|---|---|---|---|
| `lib/artkit.ps` + `lib/paintkit.ps` loaded | +1.4% (148.5M -> 150.6M) | 0.009 s, no change | +0.1 MB of 14 (+0.7%) |
| 200,000 five-line procedures (16 MB of source, nothing but definitions) | +5.4% (5.72G -> 6.03G) | 0.325 -> 0.311 s (noise) | +10% |
| 300,000 one-line `{ n pop }` definitions (8 MB) | +5.4% (2.84G -> 2.99G) | 0.159 s, no change | +21% |

Live heap per procedure (tracking allocator, 100,000 definitions): one line
+43 bytes (349 -> 392), five lines +74 bytes (432 -> 506). The last two rows are
worst cases: a file that is only procedures. The code between the tokens of a
real program dwarfs it. No operator, no per-step work and nothing in the step
loop changed (the table is behind one pointer in `Interp`).

### The default path is unchanged

`scripts/identity_check.sh` with the base and head binaries (`--loader
lib/artkit.ps`) rendered 127 of 127 pages of 96 files identically. The `%%[ Error ... ]%%` line is
byte-identical; the CLI adds `  in NAME, line N (file): excerpt` lines after it
for an error inside procedures (eight at most), which no tool in the repo
reads (`mcp_server.rs` and the npm `PostScriptError` read the bracketed line).

### Decisions worth knowing

- `put` into a scanned body keeps the slot's old line (the cheap, honest
  answer: the slot was written there); no lookup was added to `put`.
  `copy`, `astore` and `putinterval` into one drop its whole entry (a
  lookup in the cold bulk operators; the runs stay until the next sweep),
  since many slots changed at once (Codex round 1, PR #1429).
- Refusals count toward a sweep (Codex round 1): a table at its cap on
  records or names admits nothing, so the entry count never reaches the
  sweep's trigger. A refused registration counts, and a sweep runs once the
  refusals reach a sixteenth of the table (at least 64), so its charge is a
  bounded share of the scans refused. A sweep charges a bucket pass plus
  every run it copies, before it runs, and asks the memory cap for the
  compaction.
- The table is always on. An opt-out for a host that wants the memory back
  would be one `bool` checked where `scan_procedure` is entered; it was not
  asked for.
- Hosts that name sources must reset the name: it persists across runs.

### Proposed follow-ups

- `[stack-leak]` and the `gsave` imbalance lint findings still name the
  scanner's line, not the line of the procedure that left the item; the table
  now makes that possible.
- A `Lint` finding for a `def` inside a procedure at its own line already
  existed (#1338); nothing new for it here.
- Lines for procedures read by executable strings, relative to a name the host
  gives the string (`run_named`), if an embedding runs user code that way.
