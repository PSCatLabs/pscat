<!-- cloud: replace-file -->
# The hosted pscat tools in full

Read this for what each of the seven tools takes and returns, and the limits
that apply. Back to [SKILL.md](../SKILL.md).

The server has exactly these tools: `render_postscript`, `validate_postscript`,
`describe_art_capabilities`, `get_renderer_info`, `handwrite`, `chalkboard_math`
and `review_sheet`. Every one is read-only. Call no other pscat tool name.

## Limits

Programs run in a sandbox: only the built-in `lib/` can be read (for example
`(lib/artkit.ps) run`), nothing can be written, and there is no network. Each
call has limits on source size, pixels per page and per side, pages, output size
and time; `get_renderer_info` reports the ones in force. A render that exceeds
one fails with an error that names it (for example `output_too_large` or
`limit_exceeded`), so keep programs small: lower the `dpi`, draw fewer
objects, and do less work per pixel. There are no render jobs, so a render that
needs longer than the limit cannot be done here.

## `render_postscript`

`source` is the program. Optional: `width` and `height` in points (default
612x792), `dpi` (9 to 1200, default 72), `format` (`png`, `svg` or `pdf`) and
`halftone` (PNG only). PNG gives one image per page; SVG one document per page
(small ones come back as text, larger ones as base64 blobs); PDF one document
with every page. The text part of the reply gives each page's size in points and
pixels, the dpi, and what the program printed. A PostScript error returns its
name, the offending command and the line instead of an image.

## `validate_postscript`

`source` only. It scans the program, runs it with the same limits as a render,
discards the output and returns the error if there is one, lint findings (a blank
page, an unbalanced `gsave`, a leak on the operand stack), the page count and
what the program printed. Use it to check a program before an expensive render,
or to bisect an error.

## `describe_art_capabilities` and `get_renderer_info`

`describe_art_capabilities` with no arguments gives an index of every kind of
thing the build has. `kind` (`font`, `type3_face`, `palette`, `template`,
`procedure`, `operator` or `dial`) lists that kind's names, 100 at a time
(`offset`, `limit`); `names` (up to 20) returns full entries with parameters, the
library that defines each, the `run` lines to load it and an example. Prefer it
to guessing a name from prose. `get_renderer_info` takes no arguments and reports
the renderer and server versions, the output formats, the limits and the
installed font packs.

## `handwrite` and `chalkboard_math`

See [handwriting-chalk.md](handwriting-chalk.md).

## `review_sheet`

See [sheets.md](sheets.md).

## Reading files and showing the result

Programs cannot read your files or the workspace, and no tool writes a file, so
there is no output path: the image comes back in the reply. The `pscat` skill's
"Show the result to the user" says what to put in your final message.
