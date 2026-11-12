## pscat with an image model: two hybrid examples and a checked comparison (issue #995, 2026-11-04)

**What shipped.** `examples/hybrid/`: an A4 talk poster (`sky_talk.ps`) and a
US Letter café menu (`menu_card.ps`), each drawing a frozen image-model
background (JPEG, via `/DCTDecode`) and setting the exact layer on top: type,
a chart computed from the Rayleigh formula (poster), prices (menu), and a QR
code from `lib/qr.ps`. `/PscatOnly true` swaps the painting for a paintkit one,
for the "pscat alone" panel. Three-way comparison sheets
(`docs/guides/hybrid/*_compare.png`) come from `pscat sheet --sweep
Panel=1,2,3` over `compare_*.ps`, which typeset the checked facts under each
panel. Guide `docs/guides/pscat-with-image-models.md`, site page
`site/hybrid.html`, two gallery pieces (with the raw background as their
reference image) and a "pscat with an image model" specimen group holding the
sheets; a short section in the psart skill; real links from the when-to-use
guide and page.

**Owner decision (2026-11-04).** The images are made by OpenAI's image model
through the owner's Codex CLI (ChatGPT account; OpenAI's terms assign output
rights to the user). The agent ran Codex itself. Prompts, dates, tool version
and conversion settings are in `examples/hybrid/README.md`. The JPEGs sit
under `examples/**`, so `licenses/assets.json` already maps them to CC0 with
the rest of `examples/`; no notices change was made (THIRD_PARTY_NOTICES.md is
an owner path).

**What the comparison found, honestly.** OpenAI's image model, asked for each
whole poster with the brief, got **every string and number right** in both
(20 and 24 strings; chart bars within 2% of the data, measured in pixels). Its
QR codes fail: rqrr and rxing both locate the symbol and both fail its error
correction, on the original PNGs and on the committed JPEGs. The guide says so
and argues the hybrid on what is left: a scanning QR code, data-driven numbers,
edits without regeneration, vector print output, reproducibility. One attempt
per whole-poster prompt, no retries or picking.

**Gotchas met.**
- `codex exec` waits on stdin when stdin is not a terminal: run it with
  `< /dev/null`.
- Codex's image skill verifies its own output: after the whole-poster
  generations it found the QR codes did not decode and began compositing a
  real QR code into the image. Those runs were stopped and the tool's original
  output (in `~/.codex/generated_images/<session>/`) was used, so panel (a) is
  the image model alone. Its log does not record the exact prompt it passed to
  the tool for those two runs; for the backgrounds it printed the prompt, which
  was unchanged.
- rqrr can report a second, undecodable "grid" in a whole painted poster, so
  `read_rqrr` (exactly one grid) fails; `tests/hybrid.rs` decodes every grid
  and requires exactly the URL among the ones that decode.
- At 48 dpi, the landscape cookbook's sky bands (`2` pt apart, `2.5` pt tall)
  showed light horizontal seams; 4 pt tall bands removed them. Worth a look
  in the cookbook if low-dpi thumbnails matter.

**Deferred / proposed.** A notices line for model-generated example assets
(owner path); `pscat sheet` captions show `Panel=1` for a sweep, and a
per-value caption would read better on comparison sheets.
