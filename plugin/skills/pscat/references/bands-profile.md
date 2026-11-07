# Slow renders: `--bands` and `--profile`

Read this when a render is slow, or when the user asks where the time goes.
Back to [SKILL.md](../SKILL.md).

## `--bands`: render on several threads

`pscat --bands N --png out.png f.ps` (N is 1..64) renders on N threads, which
take horizontal bands (2 per thread by default; `--band-rows R` for R-row
bands) from a queue, and gives the same PNG bytes as a plain render. It is only
faster for a program that reads `/BandBBox [llx lly urx ury]` (also `/BandIndex`,
0 = top, and `/BandCount`) and skips work outside it, e.g. a costly `colorimage`
procedure that computes only those rows plus a margin: the `psart` skill has a
runnable one ("Heavy noise renders"). PNG only; it refuses `--svg`, `--pdf`,
sweeps and `--profile`. `--max-steps` and `--max-time`
apply to each band. `--allow-write DIR` works, with one shared
`--max-write-bytes` budget; every band runs the whole program, so its file
writes happen once per band, unordered: guard them with `BandIndex 0 eq`. An unaware program pays for the whole run once per band,
but only band 0 writes stdout and stderr; an uncaught error is reported once.

`--lint` works with it: one report after the run and a `pscat: lint: note:` line
saying that band 0 (the top band) was checked for page count, gsave and stack
leaks, and the stitched page for blankness. A leak that only another band
makes (a branch on `/BandIndex`) is not seen.

The band names are in `userdict` and differ per band: a program that hashes,
serialises or `forall`s `userdict` (e.g. to build a cache key) must leave
`/BandIndex`, `/BandCount` and `/BandBBox` out, or every band computes a
different key (#868 is the related `forall`-order issue).

Over MCP, `render_postscript` and `start_render` take `bands` (see
[mcp.md](mcp.md)).

## `--profile`: where did the time go

`pscat --profile --headless f.ps` (or with `--png`) prints, on stderr, each
named procedure's call count and exclusive and inclusive wall time, slowest
exclusive first (`--profile-top N`, `--profile-json`). Use it to find where a
slow render went. It lists only named procedures: work in an anonymous data
procedure shows up as "0 of 0", so add `--profile-ops` for time per built-in
operator (`fbm2` against `mul`; about 3x slower) or move the work into a named
procedure. `--profile-graph` adds caller -> callee edges. Not with `--bands`.
