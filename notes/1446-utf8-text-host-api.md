## host API: the utf8-in-byte-font check for an embedding application (issue #1446, 2026-11-26)

psanim's `check` (psanim#38) builds its own `Interp` and wants the
`[utf8-in-byte-font]` finding of #1412. It could only get it by calling the
doc(hidden) `trace_utf8_text()` and could not read the result (the findings
were formatted into lint messages inside the crate).

**Public API (`pscat::host`, Experimental like the whole crate API).**
- `Interp::set_utf8_text_check(bool)`: on or off. Off by default; off is the
  one branch in `begin_show` #1412 already had. Off again drops the notes.
- `Interp::utf8_text_findings() -> Utf8TextReport { fonts: Vec<Utf8TextFinding>,
  more_fonts: usize }`, `Utf8TextFinding { font, characters, more_characters,
  line, file }`. All `#[non_exhaustive]`, plain `String`/`char`/`usize` fields.
- Bounds as consts: `MAX_UTF8_TEXT_FONTS` (5), `MAX_UTF8_TEXT_CHARS` (6),
  `MAX_UTF8_TEXT_NAME_BYTES` (128), aliases of the numbers the lint already used.

**One location model.** `line`/`file` are what the lint reports: the line of the
last source token scanned (where execution was, as `ErrorDetails::line`), which
for a show in a procedure is the line that called it. The issue pointed at the
`TraceFrame` conventions; they are used for the types and file naming (`file` is
a `run` file's name, else the name given to `set_main_file_name`), not for a
"where the procedure was written" lookup: a procedure whose last element is the
`show` has left the execution stack before it runs, so a lookup from the top
frame would name the caller's element as the show's. The trace now keeps
`line` plus the run file and main name separately instead of a pre-formatted
string; the lint message is built from them and is byte-for-byte what #1412
printed, including for a named main file (the CLI names every file argument, and
a main-program line stays bare; `tests/cli.rs`
`lint_utf8_message_names_run_files_only_even_for_a_named_main_file`).

**What Codex has flagged before, and the answer.**
- *Uncharged work.* The check scans at most 4096 bytes per show (a longer string
  is skipped before any scan) and now copies at most 128 bytes of a `FontName`.
  Before this change `font_name` copied the whole `FontName` string on every
  qualifying show (a 1 MiB name, once per show, uncharged); the failing-before
  test is `a_huge_font_name_is_cut`. The site (line, file strings) is built only
  for a font not seen yet, so a loop of shows allocates nothing for it (it
  allocated a message string per show before). The per-show work is constant and
  below the show's own, which the interpreter charges per glyph, so it is not
  separately charged: `the_check_changes_no_output_and_no_step_count` shows the
  step count is the same on and off.
- *Unbounded list.* 5 fonts x 6 characters x 128-byte names, plus a count.
- *Behaviour change when off.* None: `utf8_text_findings()` is empty, no
  allocation, and `the_check_changes_no_output_and_no_step_count` compares pixels,
  stacks and steps on and off.

**Lifetime.** The findings are the last program's: `begin_source` (so `run_str`)
clears them, and a run that errors keeps what it showed before the error.

**Not done.** `Utf8TextFinding` carries data, not the lint's wording; a host
words its own message (the fix is `/PSCatUnicode true` in a copy of the font).
`kshow`, strings over 4096 bytes and Type 3 fonts are not examined (#1412).
