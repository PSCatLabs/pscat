## `pdfmark`: `/DOCINFO`, `/OBJ` + `/PUT`, `/EMBED` (issue #1032, 2026-11-07)

`pdfmark` is a systemdict operator (`src/ops/pdfmark.rs`). Without PDF
recording it is `cleartomark`, as Ghostscript 10 is on `png16m`. Without a
mark it is `unmatchedmark`. With `--pdf` (or the wasm build's `pdf: true`)
it applies:

| Type | Operands | Notes |
| --- | --- | --- |
| `/DOCINFO` | `key value …` | into `/Info`; see precedence below |
| `/OBJ` | `/_objdef {n} /type /stream\|/dict\|/array` | defining a name twice: `rangecheck` |
| `/PUT` | `{stream} string\|file\|dict`, `{dict} dict`, `{array} index value` | a stream's `/Length` is always computed |
| `/APPEND` | `{array} value` | |
| `/CLOSE` | `{n}` | accepted, no effect |
| `/EMBED` | `/Name string /FS dict` | both required (gs gives `rangecheck` without `/Name`) |

All other types (`/ANN`, `/OUT`, `/DEST`, `/PAGE`, …), and marks that name
a reserved object (`{Catalog}`, `{DocInfo}`, `{ThisPage}`, `{PrevPage}`,
`{NextPage}`, `{DocOpen}`, `{PageN}`), are consumed and have no effect. On
an error the operands stay on the stack. A forward `{name}` reference makes
an empty dict, as gs does. If `/OBJ` later defines that name, it can still
choose the type.

## Precedence (decided by gs 10.07.1 pdfwrite)

A file with `%%Title: DSC Title` and `%%For: DSC Author` and then
`[ /Title (Mark Title) /Producer (MyProd) … /DOCINFO pdfmark`: gs wrote
`/Title(Mark Title)`, `/Author(DSC Author)`, and its own `/Producer`. pscat
does the same:

- A key set by a mark replaces the DSC value. The DSC value stays for keys
  no mark sets. A later mark replaces an earlier one, key by key.
- `/Producer` is always `(pscat)`.
- The order is `/Producer`, `/Title`, `/Author`, then other marked keys in
  the order they were first set. With no pdfmark, `/Info` and every object
  number are unchanged from before.
- No `/CreationDate` is added: output stays deterministic. A program can
  set one itself.

## Text strings (one deliberate difference from gs)

gs copies `/DOCINFO` string bytes into the PDF unchanged, so UTF-8 `café`
reads as `cafÃ©` (PDFDocEncoding). pscat writes valid non-ASCII UTF-8 as a
UTF-16BE hex string, as it already does for `%%Title:` (#8). Bytes that
start with `FE FF` (the program's own UTF-16) pass through as hex. Other
non-UTF-8 bytes are written byte for byte as octal escapes. Printable
ASCII stays a literal string. Only `/DOCINFO` values are text strings. The
`/EMBED` `/Name` and every string inside `/FS` are byte strings, written
exactly.

## Names

A PDF name byte outside the regular characters is written `#XX`, including
`#` itself. So the PostScript name `/text#2Fplain` (whose text really
contains `#2F`) becomes `/text#232Fplain`, which is what gs writes. Spell a
MIME subtype as `(text/plain) cvn`.

## Streams, files and the browser build

- `/PUT` of a file reads it to its end. `file` opened it, so the read roots
  (`src/read_roots.rs`, `--read-root`) were already checked. A file outside
  them never gets this far: `file` raises `invalidfileaccess`
  (`tests/pdf_pdfmark.rs`).
- `currentfile /ASCIIHexDecode filter` (or `/ASCII85Decode`) embeds inline
  data and needs no file system. pscat has no `SubFileDecode`.
- On a non-PDF device that inline data is not consumed, so it runs as
  program text. gs behaves the same way. Strings are the portable spelling.
- Stream data is Flate-compressed unless the program gave a `/Filter`; with
  a `/Filter` the data is taken as already encoded.
- The total stream data per document is capped at 64 MiB (`limitcheck`),
  because a decoding filter read to its end can grow without bound.
  Operands nested more than 32 deep (or a value that contains itself) are a
  `limitcheck`, and so is a mark whose operands hold more than 2^20
  objects in all, counting a shared array once per place it appears. The
  execution limits are only looked at between operators, and an array
  that holds one array twice, doubled 25 times, would otherwise be 2^26
  copies inside one `pdfmark`.
- A mark that fails leaves the document as it was: a missing `/PUT` target
  is only created once the mark is known to succeed.
- WASM parity: strings and `currentfile` data give byte-identical PDFs from
  the CLI and the wasm build (`wasm_pdfmark_document_matches_the_cli`). A
  named file cannot be opened in the browser build (it has no file system),
  so there `file` fails before `pdfmark` runs.
- PostScript dict keys are written sorted. A dict's own iteration order
  follows interned name ids, which can differ between hosts.

## Layout

Named objects come after the shadings and before `/Info`. When something is
embedded, the Catalog gets `/Names << /EmbeddedFiles << /Names [ … ] >> >>`:
one root node, keys sorted bytewise, a repeated name replacing the earlier
entry, `/FS` written inline (as gs does). The embedded-file stream gets
only the dict entries the program gave it; gs adds no `/Type
/EmbeddedFile` either.

## Checked

- pypdf 7 in strict mode reads the issue's program: metadata
  `{'/Title': 'Mark Title', '/Creator': 'café', …}` and attachment
  `tune.abc` = the `/PUT` bytes.
- gs renders the PDF with `-dPDFSTOPONERROR` (`gs_reads_the_attachment_document`).
