# Resource resolver: FIFOs and huge files at a search-order place (#1177)

`paths::open` read each disk candidate with `std::fs::read`. A FIFO with no
writer at `$PSCAT_HOME/lib/handscript.ps` (or the same name in the current
directory) blocks in `open(2)` before any interpreter limit applies, and a
multi-GB file was read whole.

## What changed

- Readers now get a `Probe { name, search }` with each candidate
  (`Roots::open_probed`, `paths::Reader`). For a search-order candidate the
  reader calls `Probe::open`, which opens the path once (`O_NONBLOCK` on Unix),
  checks the opened handle is a regular file, and for a `lib/` name that it is
  within `MAX_LIB_BYTES`; anything else is "not here" (silently: a note could
  name a path outside the read roots). The reader then reads that same handle:
  `paths::open` and the unlimited `run`/`file` readers through
  `paths::read_handle` (a `take(cap + 1)` read), the limited ones through
  `file::read_file_limited` (the #702 reader, taking the handle). No path is
  opened twice, so a swap between check and read cannot slip a FIFO in.
- Read-root admission (`open_confined`) happens before `Probe::open`, so a
  link out of the roots is `invalidfileaccess` whatever its target's type or size.
- An absolute name is `Opened::AsWritten`: read by path exactly as before
  (`/dev/stdin`, `/dev/null`, FIFOs with a writer), including at
  `paths::open`.
- `libc` moved from a dev-dependency to a Unix dependency for `O_NONBLOCK`
  (already in the tree through winit; no new crate).

## The cap, from real data

Largest `lib/` file: `paintkit.ps`, 598 KB. Largest catalog font:
`NotoSansKR-Regular.ttf`, 10.4 MB. So `lib/` names get 64 MiB (about 100x) and
everything else, which includes any font file and data a program names, 512 MiB.
`font.rs` does not read font bytes through the resolver (it lists directories
through `paths::catalog_dirs` and reads files itself), so the cap does not touch
font loading; `tests/resource_candidates.rs` still checks that a 48 MiB `.ttf`
resolves.

## Not done

- `ops/file.rs` readers are unchanged: a size cap there would start refusing big
  regular data files that load today.
- `font.rs`'s own `std::fs::read` of catalog files is unchanged.
