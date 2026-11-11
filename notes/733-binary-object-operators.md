## Binary object operators (issue #733, 2026-10-25)

`setobjectformat`, `currentobjectformat`, `printobject`, `writeobject`, and the scanner reading
binary object sequences (token types 128 to 131) back. Unblocked by #740's file objects.
Implementation: `src/binobj.rs` (encode and decode, pure), `src/ops/binobj.rs` (operators),
`Token::BinarySeq` in `src/lexer.rs`, `decode_sequence`/`Action::RunSequence` in `src/interp.rs`.

**Source of truth.** No PLRM text is in the repo. The layout was taken from the PLRM as remembered
and then checked against Ghostscript 10.07.1, both by probing it and by reading its own
implementation (`Resource/Init/gs_btokn.ps`, which is PostScript). `tests/binobj.rs` compares the
bytes gs writes for 19 objects in all four formats (local only; CI has no gs).

**Choices, and why.**
- **Default format 2** (IEEE, low byte first). gs computes its default from the host's real
  format and byte order, which is 2 on any little-endian host. pscat hard-codes 2, so output is
  the same on every platform, and a program that never calls `setobjectformat` works.
- **Format 0 is `undefined`** for both operators, as in gs.
- **Native formats 3 and 4 write as 1 and 2** under tokens 130 and 131. A native real on a
  little-endian IEEE host is the IEEE bytes in that order; gs writes the same bytes.
- **Long header.** gs's own long header (needed past 65535 bytes) starts with two wrong bytes
  (`01 32` where 0x80/0x81 belongs), so gs cannot read it back. pscat writes the PLRM form
  (`type 0 count(2) total(4)`), and gs reads pscat's. The gs comparison only covers short
  sequences.
- **Limits are `limitcheck`, not silent wrap-around.** gs wraps an integer past 32 bits and
  truncates a 70000-byte string's length to 16 bits, which writes bytes that do not read back as
  what was written. pscat raises `limitcheck` for an integer outside i32, a real that is not a
  finite binary32 value, a string, name or array over 65535 long, the empty name (its length 0 is
  the system name table's marker), nesting over 500, and a sequence over 64 MiB. The last one is
  what stops a cyclic array, and a DAG that expands to billions of objects, before they are
  allocated. gs caps the whole sequence at 65536 references; pscat allows more, since the header
  has 32 bits of length.
- **`typecheck` for anything with no binary form** (dictionary, file, operator, save, gstate).
  gs says `typecheck` for a dictionary and `undefined` for a file or an operator; `typecheck` is
  the one error that says "wrong kind of object".
- **Reals are binary32.** The writer rounds (`0.1` reads back as `0.10000000149011612`). Not
  finite after rounding is `limitcheck` (gs also says `limitcheck`); reading an infinity or NaN
  is `undefinedresult` (gs too).
- **Reading.** A scanned sequence runs its top-level objects as if each had been a token (gs
  does the same: `(<seq 7 8>) cvx exec` pushes 7 and 8), while `token` returns the executable
  array `{7 8}` and leaves the bytes after the sequence alone (no delimiter is eaten). Inside
  braces it is a nested procedure. Fixed-point reals (a length field of 1 to 32 fraction bits)
  and immediately evaluated names (type 6) are read. Names with length 0 or 0xFFFF (system and
  user name table indices) are a `syntaxerror`.
- **The decoder treats its input as hostile** (the scanner is a fuzz target). Every offset and
  length is bounds-checked. A sequence decodes to at most one object per 8 bytes of its body (one
  budget for the whole sequence), so a cycle or a layout that shares one array between many
  parents runs out of budget (`syntaxerror`) rather than looping or blowing up exponentially. The
  scanner also asks the memory limit about that much before it builds anything. Nesting past 500 is `limitcheck` (recursion is bounded).
  The lexer reads the claimed length byte by byte and never preallocates from it.
- **Names are stored as UTF-8 text** in pscat, so a name whose bytes are not UTF-8 reads back with
  U+FFFD in place of them and writes those bytes, which will not match gs's. That limit is pscat's
  existing name type, not this format.
- **A token that starts with 128 to 131 is now a sequence.** It used to start a name. A byte in
  that range inside a name is still an ordinary character (gs agrees), and the full test suite
  and selftest show no program in the repo relied on the old behaviour.

**Codex round 1 (PR #1103).**
- The writer checks the 64 MiB cap and charges the memory limit after every element, not every
  array (one array can name one 65535-byte string 65535 times). The reader allows at most as many
  copied string and name bytes as the body holds, since slots may share characters and each copy is
  new storage (`syntaxerror` past that).
- `0 setobjectformat` turns binary scanning off: a token starting with 128 to 131 is then an
  ordinary name, as in gs (`Lexer::binary_tokens`, set from the format before each token).
- A scanned executable string runs (an `exec` follows it in the frame the scanner builds); inside a
  procedure body it is still only pushed.
- The object format is saved by `save` and brought back by `restore`.

**Scope note: this cannot save a million-element array.** The slot's length field is 16 bits, so
an array holds at most 65535 elements. A long numeric array should go through `writereals`
(#735); a million reals could only be written here as nested arrays, which gs would not read
as one array either.

**Not done (follow-ups).**
- Binary tokens 132 to 159 (a lone 32-bit integer, 16-bit and 8-bit integers, a real, a boolean,
  a string, a system name index). They still scan as names. Rare in files; the object sequence
  is what these operators produce.
- The system name table (names with length 0) and the user name table (`defineusername`).
  Printers use them; nothing here writes them. gs's table is 481 entries (PLRM Appendix F).
- `printobject` writes raw bytes to stdout. A captured stdout (the wasm playground, the
  MCP server) holds them as bytes and is cut at 64 KiB like any other output.
