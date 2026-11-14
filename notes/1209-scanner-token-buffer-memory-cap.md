## The scanner asks the memory cap before a token's buffer grows (issue #1209, 2026-11-14)

### The bug

#1208 made every decode filter look at the limits every 64 KiB it decodes,
and `PsFile::read_into` ask `check_alloc` before a caller's buffer grows. The
scanner's own buffers (`read_string`, `read_hex_string`, `read_regular_run`,
`read_ascii85_string`, `read_binary_sequence`) grew by `Vec::push`, which
doubles. A token read from a decompression bomb therefore grew between two
looks at the heap, and a buffer that doubles can take as much memory in one
step as it already held: from 32 MiB to 64 MiB under a 40 MiB cap, and a
doubling that copies has old and new live together (about three times the
cap at worst). A hex string was worse: it collected one byte per nibble, twice
the result, and then allocated the result on top.

### What changed (`src/lexer.rs`, `src/file.rs`)

- `Lexer::put(&mut out, b)` is `push` with one compare, `len == capacity`;
  only then does the `#[cold]` `check_growth` work out the growth `Vec` will
  make and ask `PsFile::check_alloc`. `room` does the same for
  `ascii85`'s four-byte extends. Every push in the five readers above goes
  through them. The error is `MemoryExceeded`, from the same `Result` as the
  decoder's, so it is blamed on the operator scanning (`exec`, `token`) and
  is uncatchable.
- `PsFile::check_alloc` asks a `Filter`'s `DecodeBudget`; any other source has
  no limits to ask. A byte source (the main program, an executable string, a
  file read whole) already holds everything a token in it can be, and the
  whole-file read is checked when it is read.
- `read_hex_string` packs each pair as it goes (a pending high nibble) instead
  of collecting nibbles. Same result, same odd-nibble padding, half the peak.
- Not changed: `read_number_array` (at most 4 + 65535 x 8 bytes by its header)
  and the 8-byte binary header reads.

The check asks for the growth only (as `read_into` does), so a token can be
as big as the cap allows and no bigger. The tracking allocator counts a
`realloc` by its size change, so the figures the cap sees never include a
transient copy.

### Test

`tests/limits_memory.rs`: the bomb now runs as a string, a hex string and a
name, through `exec` and through `token` (and `{ token } stopped`), under a
40 MiB cap. The cap is not a power of two on purpose: at 32 MiB the unchecked
buffer would stop at the cap anyway. A sampler thread (no sleeping) records the
peak heap growth, which must stay within the cap plus 2 MiB. With the lexer
checks reverted the peak is about 64 MiB and the test fails.

### Performance (release, no limits, user time, best of 5, main 38ee4ed -> head)

See the PR. Large tokens (100 MB string, hex, name; and the same through a
RunLengthDecode filter) are within noise.

### Not done (proposed follow-ups)

- A name token is copied again by `String::from_utf8_lossy(..).into_owned()`
  and a string token by the conversion to an object: each briefly doubles the
  finished token, which is already under the cap.
- A byte source's tokens are not checked (see above).
