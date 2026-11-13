# Resource resolver: FIFOs and huge files at a search-order place (#1177)

`paths::open` read each disk candidate with `std::fs::read`. A FIFO with no
writer at `$PSCAT_HOME/lib/handscript.ps` (or the same name in the current
directory) blocks in `open(2)` before any interpreter limit applies, and a
multi-GB file was read whole.

## What changed

- `Roots::open_with` screens each disk candidate of a relative name
  (`plausible_candidate`): `metadata` succeeds and is not a regular file, skip;
  a `lib/` name over `MAX_LIB_BYTES` (64 MiB), skip (silently: this runs before the read roots
  are consulted, and a note could name a path outside them). A
  candidate that does not exist is passed to the reader, as before. This covers
  `run`, `file` and `status`, limited and unlimited. An absolute name is read as
  written, and `read_path_limited` (#702) is untouched.
- `paths::open` reads through `read_regular`: open `O_NONBLOCK` (Unix), check
  the opened handle's `is_file()`, check its length against `size_cap(name)`,
  and read through the same handle with `take(cap + 1)`. So a swap between the
  screen and the read cannot slip a FIFO in, and a file that grows cannot pass
  the cap. `libc` moved from a dev-dependency to a Unix dependency for
  `O_NONBLOCK` (already in the tree through winit; no new crate).

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
