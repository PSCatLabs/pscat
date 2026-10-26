## `readreals` / `writereals`: `/all`, offsets, file objects (issue #773, 2026-10-25)

Follow-up to `notes/735-readreals-writereals.md`, which left these three out. Code:
`src/ops/realsio.rs`, with one new method, `PsFile::read_into`, in `src/file.rs`.

**Interface.**

    (path) /all readreals                  array    % every value in the file
    (path) offset n readreals              array    % n values from value `offset`
    (path) offset /all readreals           array    % from value `offset` to the end
    file n readreals                       array    % from the file's position
    file /all readreals                    array
    array file writereals                  -        % at the file's position

Each takes `/f32` after the count, as before (`(path) /all /f32 readreals`).

**Choices, and why.**
- **`/all`, not `-1`.** The PLRM's optional operands are names (`/f32` here, the `filter`
  names, `setpagedevice` keys); `-1` is its sentinel only where a count is *returned*
  (`bytesavailable`). A count in a program is often computed, and an `n 1 sub` that went
  negative must not turn into a silent read of the whole file, so `-1` stays `rangecheck`.
  Any other name in the count position is `rangecheck`; no count at all (`(p) /f32
  readreals`) is `typecheck`.
- **Offset first, in values.** `index count` is `getinterval`'s order. The unit is a value of
  the chosen format (so it is the same number for the array that was written), not a byte.
  `(path) n` and `(path) offset n` are told apart by the type under the count: a string, or an
  integer with the string below it. `5 5 readreals` with nothing below is `stackunderflow`
  (it was `typecheck` while the second integer was taken for the path).
- **Offset edge cases.** An offset past the last value is `rangecheck`, as is a negative one or
  one too big for `usize` (never a panic). `offset == values` with `/all` is an empty array.
  With a count, `offset + n` past the end is `rangecheck`. A trailing partial value is
  ignored (not an error), as it always was.
- **No offset on a file object.** A file has a position and `setfileposition` (bytes) moves
  it; a second offset in values next to it would be two ways to say one thing.
  `file 1 2 readreals` is `typecheck`.
- **The file-object write is `writestring`'s path.** It goes through `ops::file::write_bytes`,
  so a `%stdout` file writes through `Interp::write_stdout` in order with `print` (and a
  captured stdout sees the bytes), a disk file is buffered by `DiskSink` and flushed as #762
  says, an `(a)` file appends, and after `setfileposition` the write lands there. A disk file's
  whole write is checked against `--max-write-bytes` first (`limitcheck`, nothing written),
  then each chunk is charged as it goes; `%stdout` is not counted, as for `writestring`. Every
  element is still checked before the first byte, so `typecheck` writes nothing. A closed or
  input file is `ioerror`, even for an empty array. The file is not closed.
- **The file-object read takes the file's own bytes.** `PsFile::read_into` appends up to `max`
  bytes and moves the position, byte count and line count exactly as that many `read_byte`
  calls (CR, LF and CRLF each end one line), so `currentfile 2 readreals` followed by an error
  reports the right `Line:` (tested with values whose bytes are CR and LF). A file backed by
  bytes knows what is left: a short read is `rangecheck` and moves nothing, and `/all` takes
  only whole values, leaving a partial one unread. A decode filter cannot say how much is
  left; a short read from one is `rangecheck` with what it read already consumed, and `/all`
  drains it in chunks, with the 16M cap and the memory check applied as the array grows.
  A closed or output file is `ioerror`.

**Limits.**
- A count, and an `/all` result, over the 16,777,216-element array cap is `limitcheck` (never
  a cut-short array). `/all` on a path counts the file's values after the read and refuses
  before building the array; the array's `Object`s are `check_alloc`ed before they are made,
  so a file that does not fit is `limitexceeded (memory)`, not an out-of-memory abort.
- A path read still takes the whole file into memory first (the same limit-aware read as
  `(r) file`, so read roots apply). Reading a slice of a huge file therefore costs the file;
  open it with `(r) file` and `setfileposition` to skip bytes already in memory, or keep the
  file small. A ranged disk read would need its own copy of the limit-aware reader (FIFOs,
  the read thread, the embedded `lib/` lookup), which is more than this issue's size.
- Each 64 KiB chunk is one `poll_read`, in every new form too.

**Tests.** `tests/realsio_files.rs` (all forms, edge cases, `%stdout`, append, buffering,
`setfileposition`, the write cap, `currentfile` line numbers, filters, step limits, the CLI
flush at exit), `tests/realsio_memory.rs` (memory limit, array cap). `tests/realsio.rs` is
unchanged apart from the `5 5 readreals` case.

**Not done.** A ranged disk read for path offsets (above); `file offset n readreals`; writing
to `%stderr` is allowed (as `writestring` allows it) but untested.
