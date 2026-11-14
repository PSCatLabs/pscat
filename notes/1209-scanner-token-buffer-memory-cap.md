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
  no limits of its own; a byte source gets the interpreter's through
  `Lexer::with_limits` (see Codex round 1).
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

### Codex round 1 (PR #1221)

Four P2s, all fixed, each with a test in `tests/limits_memory.rs`:

1. **ASCII85's last, partial group** extended the buffer without `room`. Guarded.
2. **A binary sequence's up-front reserve** (up to 1 MiB, from a header that
   can claim 4 GB) was taken before any check; it is put to the cap first
   (`room`), so a header alone under a tight cap is `limitexceeded`, not an
   allocation and then a catchable `syntaxerror`.
3. **Byte sources had no budget.** The interpreter now hands every scanner it
   makes the shared `DecodeBudget` (`Lexer::with_limits`: main program,
   executable string, `run`, `token`), and `ask` uses it before the file's own.
   `exec` of a string also asks for the copy the scanner reads
   (`check_alloc(s.len())`). With no limits the budget's probe is `NONE`: a
   check only happens at a doubling, never per byte, so the main program's
   scan is unchanged.
4. **A finished name was copied** (`from_utf8_lossy(..).into_owned()`, and
   `s.to_string()` in `classify_word`). `Lexer::name_text` turns the buffer
   into the `String` without a copy (`String::from_utf8`), falls back to a
   lossy copy, charged at three times its size, only for invalid UTF-8, and
   puts any name of 64 KiB or more to the cap for what the interpreter does
   next: it interns the name, which copies it. A string token is moved into
   its object, not copied, so it needed nothing.

Tests: the new cases are deterministic (the program must stop with
`limitexceeded` blamed on `token` or `exec`), except where the difference from
no check is only a transient peak: with `ask`'s pre-check the heap never holds
the grown buffer, and without it the operator's own end-of-operator check
still catches the result. That one case (the ASCII85 partial group) is in the
sampled loop, and read 67,219,714 bytes against a 41,943,040 cap with the
guard removed, on three runs.

### Performance (release, no limits, user time, best of 5, main 38ee4ed -> head)

See the PR. Large tokens (100 MB string, hex, name; and the same through a
RunLengthDecode filter) are within noise.

### Not done (proposed follow-ups)

- A name is still interned by copy (`name::intern(&str)`): `intern_rc` takes
  an `Rc<str>`, which a `String` cannot become without a copy either.
