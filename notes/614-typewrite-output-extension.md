## typewrite: -o picks its format by extension, and refuses other names (issue #614, 2026-10-21)

The #600 port kept `scripts/typewrite.sh`'s rule: `.pdf` wrote a PDF and any
other name wrote a PNG, so `-o memo.svg` or `-o memo.txt` put PNG bytes into a
misnamed file. `photo-etch` already rejected unknown extensions and wrote SVG.

**Decision (by standards, matching photo-etch):** `-o` takes `.png`, `.svg` or
`.pdf`, case-insensitive. Anything else, or no extension, fails with the usage
text and `--out must end in .png, .svg, or .pdf, got: NAME`, before any input
is read and before anything is written. `--ps` renders nothing, so `-o` is not
checked then. `output_format` moved from `photo_etch.rs` to `src/subcmd/mod.rs`
and both subcommands use it (same message as before for photo-etch).

**SVG works through the same renderer.** A page that overflows splits into
`out-001.svg`, `out-002.svg`, ... like PNG. It is large: the ribbon's weave
is vector clipping, about 40 KB a character, so a full memo page is ~38 MB and
a 12-character line 0.5 MB. Quick Look draws the short one correctly.
The docs say to prefer PNG or PDF for a full page.

**Behaviour change:** an extensionless `-o` (or `.txt`, `.jpg`) used to write a
PNG and now fails. `handwrite` still writes PNG whatever the name (out of scope).

No render changed: `typewrite` output is byte-identical to the base binary's
for PNG and PDF (checked with a base and a head binary).
