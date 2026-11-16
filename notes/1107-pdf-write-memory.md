# #1107: writing a PDF stays inside `--max-memory`

## Problem

`PdfRecorder::finish` cloned every page's content, built every object as its
own `Vec<u8>` in an `objects` list, then built a copy of the whole file in
`out`. The recorder itself was charged to `--max-memory`, but this working
memory was not: a document that fit while running could need several times
its stored size to write. PR #1073's stress case (64 MiB of NUL bytes in
pdfmark strings, each four bytes of file as `\000`) peaked at about 720 MB
resident.

## What was done

Stream, not charge. Every object number is known before any object is built
(`image_base`, `function_base`, `named_base`, `font_base`), so the objects can
be written in file order straight to a sink, which is byte-identical by
construction. Charging would need sizes that cannot be known up front (the
deflate output, the four-fold octal expansion of a string).

- `write_document(trailing_art, probe, sink)` writes through `Out`, which
  hands 64 KiB pieces to the sink and counts for the xref offsets. The
  `objects` list, the output copy, the six page-list clones and
  `img.head.clone()` are gone.
- The open page's content is deflated from the recorder's own string
  (`snapshot_tail` is only what the pending run and groups add after it).
- `PdfValue`, strings, names, `/Info` metadata (DSC `%%Title:`, a UTF-16 hex value that is four bytes of file per byte) and named objects are written straight into the
  sink (`write_literal_string`, `write_name`, ...), so a 4 MiB string of NUL
  bytes is no longer a 16 MiB `String` and then a second copy.
- Under `--max-memory`, `Out` looks at the cap before each piece reaches the
  sink, and `BudgetedVec` (the deflate target, and the `Vec` of
  `pdf_document_limited`) asks it before growing (by half again). The cap is
  `--max-memory` plus `pdf::WRITE_MARGIN` (16 MiB): a run that a limit
  stopped is at its cap and its partial page is still written (as before),
  and the writer's own working memory needs some room. A refusal is
  `PdfWriteError::Memory`. The check is on the tracker's heap count, so a
  `realloc` that briefly holds old and new is not seen, and a growth step
  can take the heap one step past the margin only if it was asked and
  allowed first.
- `finish()` is `finish_limited(.., LimitProbe::NONE)`, so its signature and
  output are unchanged.

## Who gets what

| Caller | Path | A refused write |
| --- | --- | --- |
| `pscat --pdf F` (CLI, and the MCP server's child) | `Interp::write_pdf` into a `BufWriter<File>` | stderr `pscat: cannot write F: ... no file was written` and `%%[ Error: limitexceeded (memory); OffendingCommand: --pdf ]%%`; exit 1. The CLI writes to a temporary file beside the target (following symlinks) and renames it over the target only when the document is complete, so a refusal leaves an existing file, or a symlink's target, as it was and deletes only its own temporary file. A destination that is not a plain file (stdout, `/dev/*`, a FIFO) is written directly. The MCP server sees no output file and the `limit` field, so the call is a `limit_exceeded` with `limit: memory`. |
| Library, in process | `Interp::write_pdf` (a sink that does not keep it) or `Interp::pdf_document_limited` (a `Vec`) | `Err(PdfWriteError::Memory)`; a sink holds a partial document to discard. |
| `Interp::pdf_document` (unchanged signature) | `finish` | never refused (it cannot say so); it still holds far less than before. |
| wasm/npm `ps_pdf` | `pdf_document` | the wasm build has no tracking allocator, so no memory limit exists there; the writer's peak is lower. |

## Measured (release build, macOS, `/usr/bin/time -l` maximum resident set size)

| Program | before | after |
| --- | --- | --- |
| #1073's stress: 64 MiB of NUL bytes in a pdfmark array, 256 MiB file | 629 MB | 79 MB |
| same, `--max-memory 100` (the run fits; before, the limit did not apply to the write) | 654 MB | 80 MB |
| `gallery/crayon_drawing.ps` (47 MB file, ordinary stroke-heavy art) | 515 MB | 363 MB |
| a small ordinary page | 19 MB | 17 MB |

PDF output is byte-identical (`cmp`) to main's on every `examples/*.ps` and
`gallery/*.ps`, and on crafted programs for several pages with different sizes
and trailing art, an image with an SMask, `shfill`, pdfmark `/DOCINFO`,
`/OBJ` streams with and without `/Filter`, dicts, arrays, forward references
and `/EMBED`, and `--pdf-text` fonts, with and without `--pdf-text`.

## `--png` and `--svg`

They build the whole file in memory and then `std::fs::write` it over the
destination, in place (no temporary file), as before. A failure while
encoding leaves the old file; a failure part way through the write (disk
full) truncates it. They are not changed here.

## Not done

- `pdf_font::font_objects` still returns its five objects of a font as
  `Vec<u8>`s (one font at a time, bounded by the face's subset).
- A single page's raw content text is still held by the recorder (it is the
  recorder's own charged memory); only its packed stream is built at write
  time.
