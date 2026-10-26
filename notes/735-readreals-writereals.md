## `writereals` / `readreals` (issue #735, 2026-10-25)

Fast, exact save and load of a numeric array, built on #740's write roots. The
fractal_clouds report had a text cache (`lib/fc_cache.ps`) that read a million reals in
0.17 s; the issue asked for binary, which also writes and round-trips exactly.

**Interface.**

    array (path) writereals            -
    array (path) /f32 writereals       -
    (path) n readreals          array
    (path) n /f32 readreals     array

`/f64` is the default and can be named. Any other format name is `rangecheck`. Always
little-endian. Implementation: `src/ops/realsio.rs`.

**Choices, and why.**
- **Optional format as a trailing name.** An optional operand is told apart by type (a name on
  top versus a string or integer). PLRM operators with options (`setpagedevice`, `filter`) use
  names or dicts; a name is the lightest. A file-object variant (`array file writereals`) was
  left out: `(path)` is what the issue asked for.
- **Write path.** `WriteRoots::open(name, Truncate)`, the same call `(path) (w) file` makes, so
  the roots, `..`, symlink and hard-link checks are shared. A name starting `%` is
  `undefinedfilename`, as for `file`. An existing file is truncated.
- **Every element is checked before the file is opened.** A bad array (a string, a name, a
  nested array) is `typecheck` and never truncates the target.
- **Integers are written as reals** (`n as f64`: above 2^53 they round as `cvr` does).
- **Non-finite values are written and read as they are.** pscat does make infinities
  (`1e308 10 mul`), so `readreals` must give them back. The text workaround cannot: `inf` is not
  a literal. `-0.0`, subnormals and NaN payloads round-trip bit for bit in `/f64`.
- **`/f32` rounds to nearest binary32.** Beyond about 3.4e38 a value becomes an infinity, below
  about 1e-45 it becomes zero. Reading `/f32` is exact (every binary32 is a binary64).
- **A short file is `rangecheck`.** The PLRM defines `ioerror` as an I/O error that is not an
  end-of-file; a file that ends before `n` values is end-of-file. Asking for more values than
  the file holds is a range error, like `getinterval` past the end. `ioerror` is left for the OS
  refusing a write. `readreals` returns an array or raises; it has no `false` result like
  `readstring`, because a truncated cache is a bug to surface.
- **Extra bytes are ignored**, including a trailing partial value, so `n` can read a prefix.
- **Reads follow the read policy** (`file::read_resource`: the same search order and the same
  limit-aware read as `(path) (r) file`). It reads the whole file and keeps the first `n`
  values, so a huge file costs the same as `(r) file` on it.

**Limits (the #740 Codex review's lesson).**
- `readreals` calls `check_alloc` for `n * (width + size_of::<Object>())` before touching the
  file, and again for the array before building it, so `10000000 readreals` on a 1-element file
  is `limitexceeded` (memory), not `rangecheck`. `n` over the 16M `array` cap is `limitcheck`.
- `writereals` never copies the array. It reads elements with `PsArray::peek` (not `snapshot`,
  which clones every `Object`, an allocation the limit would not see) and encodes 8192 elements
  at a time into one fixed buffer.
- Each 64 KiB chunk is one `poll_read`: a step per KiB, plus the time and memory checks. The
  validation pass polls too. Without limits there is no cost. On wasm there is no poll and no
  disk: every write path is `invalidfileaccess`.

- A limit that stops a long write leaves the (already truncated) file short, so a later
  `readreals` of the full count is `rangecheck`, not a silently wrong array.

**Evidence** (release build, CPU time user+sys, best of 7, 1M reals `sqrt(i+1)`, Apple
silicon). The build of the array is about 148 ms and is in every row.

| step | text workaround | binary |
|---|---|---|
| write | `{ = } forall`, stdout redirected: 1227 ms | `writereals`: 156 ms |
| write | `cvs` into a 256 KiB buffer, `writestring`: 757 ms | |
| read | `(cache.txt) run`: 138 ms | `readreals`: 12 ms |

Net of the 148 ms array build, `writereals` is about 8 ms against 600-1100 ms for text, and
`readreals` is the process start-up plus about 10 ms against 138 ms for `run` (which is also
110 MB of resident memory against 55 MB). The files are 8 MB against 18 MB. Both round-trips are
exact for these values (0 mismatches in a million); text fails on `inf`.

**Not done.** `fileposition`-style partial reads (a count with an offset), a `/all` count, and
file-object forms. `SECURITY.md` still says `file` accepts only `r` (stale since #740).
