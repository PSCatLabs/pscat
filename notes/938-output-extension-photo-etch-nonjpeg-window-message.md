## Output-name check, photo-etch non-JPEG message, headless message (issues #938, #939, #941, 2026-10-29)

Three small CLI fixes from a claude.ai QA pass over 0.7.0-rc.1.

**#938.** `--png`, `--svg`, `--pdf` and `handwrite -o` write one format
whatever the name says, so `--png out.jpg` left a PNG named `.jpg`. They now
call `subcmd::check_output_name`, built on `output_format` (the helper
`typewrite`, `photo-etch` and `chalk` already use): the right extension, no
extension and an unknown one (`out.dat`) pass; another of `.png/.svg/.pdf`, or
one of a short list of other image and document types (`OTHER_KNOWN_EXTENSIONS`
in `subcmd/mod.rs`), exits 1 naming the extension and writes nothing. The flag
CLI checks at the end of `parse_args`, so every mode (sweep, lint, bands)
hits it before anything runs. A multi-page run's `out-001.png` keeps the
extension, so checking the typed name is enough. Not covered: `--contact-sheet`
(always a PNG; same bug class, not in the ticket) and `pscat sheet -o`.

**#939.** `photo-etch` reads the photo's first 12 bytes in Rust before the
dimensions pass and, when they are not `FF D8` (all `et-scan` checks), exits 1
with `PHOTO is a PNG; only JPEG photos are supported (convert it to JPEG
first)` (also GIF, BMP, WebP, TIFF, ISO media, PDF, empty, other). PNG input
was not added: the interpreter has no PNG decoder reachable from PostScript
(`lib/etching.ps` samples through `DCTDecode`), so it means a new decode path
and a tone-grid entry point, not a small change. Proposed as a follow-up.

**#941.** The no-window message (`src/window_stub.rs`) now leads with
`pscat --png out.png file.ps`. Only the Linux musl release archives are built
`--no-default-features` (`.github/workflows/release.yml`, `docs/dev/RELEASING.md`
"Platforms"); the macOS and Windows archives have the window, and the plugin
launcher downloads `*-apple-darwin` on macOS. So the pscat skill says the
plugin's Linux binary is headless, not that every plugin download is.
