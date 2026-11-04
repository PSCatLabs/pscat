# Hybrid examples: an image model paints, pscat sets the exact layer

Two worked examples for [pscat with image models](../../docs/guides/pscat-with-image-models.md):

| Program | Page | Background (frozen) | Image model alone (for comparison) |
|---|---|---|---|
| [`sky_talk.ps`](sky_talk.ps), a public-talk poster | A4, 595 x 842 pt | [`sky_talk_bg.jpg`](sky_talk_bg.jpg) | [`sky_talk_model.jpg`](sky_talk_model.jpg) |
| [`menu_card.ps`](menu_card.ps), a café menu | US Letter, 612 x 792 pt | [`menu_card_bg.jpg`](menu_card_bg.jpg) | [`menu_card_model.jpg`](menu_card_model.jpg) |

Render from the repository root (the programs read their JPEGs by a relative
path):

```sh
pscat --page 595x842 --pdf sky_talk.pdf examples/hybrid/sky_talk.ps
pscat --page 612x792 --pdf menu_card.pdf examples/hybrid/menu_card.ps
pscat --page 595x842 --dpi 144 --png sky_talk.png examples/hybrid/sky_talk.ps
```

`/PscatOnly true def` before a program swaps the painting for one that
paintkit paints, keeping the exact layer:

```sh
{ echo '/PscatOnly true def'; cat examples/hybrid/sky_talk.ps; } | \
  pscat --page 595x842 --dpi 144 --png sky_pscat.png -
```

## The comparison sheets

`compare_sky_talk.ps` and `compare_menu_card.ps` draw one panel each of (a)
the image model alone, (b) pscat alone and (c) the hybrid, with the checked
facts typeset under it (`compare_panel.ps` does the drawing). `pscat sheet`
lays the three panels side by side:

```sh
pscat sheet -o docs/guides/hybrid/sky_talk_compare.png --page 595x1032 \
  --dpi 48 --no-sizes --sweep Panel=1,2,3 \
  --label "Why Is the Sky Blue?: (a) image model alone, (b) pscat alone, (c) hybrid" \
  examples/hybrid/compare_sky_talk.ps
pscat sheet -o docs/guides/hybrid/menu_card_compare.png --page 612x982 \
  --dpi 48 --no-sizes --sweep Panel=1,2,3 \
  --label "The Lantern Pear: (a) image model alone, (b) pscat alone, (c) hybrid" \
  examples/hybrid/compare_menu_card.ps
```

The facts are checked by `tests/hybrid.rs`: the QR codes of (b) and (c) are
read back by two independent decoders from the PNG and the PDF export, every
string of the brief is painted exactly, and the decoders' verdict on the image
model's own QR codes is pinned. The text of (a) was checked by reading the
image at full size against the brief, string by string.

## Where the images came from

All four JPEGs were made on 2026-11-04 by **OpenAI's image model, through the
Codex CLI 0.166 `image_generation` tool**, signed in with the project
owner's ChatGPT account. OpenAI's terms assign the output to the user, and the
owner dedicates them, like the rest of `examples/`, to the public domain under
CC0 1.0 ([`LICENSE-CC0`](../../LICENSE-CC0)).

Each was made by one command, run in an empty scratch directory:

```sh
codex exec --skip-git-repo-check -s workspace-write "<instruction>" < /dev/null
```

The instructions, verbatim (the generation prompt is everything between
"create the following image." and "Save it as"):

**`sky_talk_bg.jpg`** (original 1024 x 1536 px PNG; Codex reported passing
the prompt to its image tool unchanged)

> Use your image generation tool to create the following image. A painterly oil painting of a wide evening sky over a calm sea and a low dark headland, portrait orientation (2:3). Deep clear blue at the top of the sky, fading through soft lavender to warm peach and gold near the horizon; a few soft cumulus clouds in the middle catch the last light. Visible brushwork and canvas texture. Keep the top third open, calm sky with little detail, and the bottom quarter a dark, quiet sea, so the picture works as a poster background with type laid over it. No text, no letters, no numbers, no logos, no people, no frame. Save it as sky_bg.png in the current directory at full resolution; do not resize or recompress it. Print the path and pixel dimensions, and print verbatim the exact prompt you passed to the image generation tool.

**`menu_card_bg.jpg`** (original 1102 x 1427 px PNG; prompt passed unchanged)

> Use your image generation tool to create the following image. A loose watercolour painting on cream cold-pressed paper for the background of a café menu card, portrait orientation (US Letter proportions, about 8.5 by 11). Two ripe pears and a few pear leaves on a sprig sit in the bottom right corner, painted with soft wet-in-wet washes of green, gold and a little rose; a pale sage wash bleeds in from the top left corner. The whole middle and left of the sheet stays almost bare cream paper with only the faintest wash, so menu text can be printed over it. Visible paper grain and soft blooms. No text, no letters, no numbers, no logos, no frame or border. Save it as menu_bg.png in the current directory at full resolution; do not resize or recompress it. Print the path and pixel dimensions, and print verbatim the exact prompt you passed to the image generation tool.

**`sky_talk_model.jpg`** (original 1055 x 1491 px PNG)

> Use your image generation tool to create the following image. Design a complete A4 portrait event poster as one image. Background: a painterly oil painting of an evening sky over a calm sea, deep blue above fading to peach and gold at the horizon. The poster must contain exactly this text, spelled exactly as given: the title "Why Is the Sky Blue?"; the subtitle "A free public talk on light, air and colour"; "Dr. Imogen Hartley, atmospheric physicist"; "Thursday 3 December 2026 · 7:30 pm"; "Harbourside Library, Lecture Room 2". Include the Rayleigh scattering formula "I ∝ 1/λ⁴". Include a small bar chart titled "Scattering relative to red light (700 nm)" with six bars labelled 400 nm 9.4×, 450 nm 5.9×, 500 nm 3.8×, 550 nm 2.6×, 600 nm 1.9× and 700 nm 1.0×, each bar's height in proportion to its value. Include a scannable QR code that encodes exactly https://pscatlabs.github.io/pscat/hybrid.html, with the caption "Scan: how this poster was made" beside it. Save it as sky_model.png in the current directory at full resolution; do not resize or recompress it. Print the path and pixel dimensions, and print verbatim the exact prompt you passed to the image generation tool.

**`menu_card_model.jpg`** (original 1103 x 1426 px PNG)

> Use your image generation tool to create the following image. Design a complete café menu card as one image, US Letter portrait. Background: a loose watercolour of two pears and pear leaves on cream paper. The menu must contain exactly this text, spelled exactly as given, with the prices exactly as given: the name "The Lantern Pear" and the line "Café · 14 Mill Lane". A section "Breakfast, 8–11" with "Buttermilk pancakes, maple butter 9.50", "Shakshuka with sourdough 11.25" and "Bircher muesli, poached pear 7.75". A section "Lunch, 12–3" with "Roast squash and farro salad 12.50", "Leek and Gruyère tart 13.00" and "Soup of the day with bread 8.25". A section "Drinks" with "Flat white 3.80", "Pot of tea 3.20" and "Pear and ginger soda 4.10". Include a scannable QR code that encodes exactly https://pscatlabs.github.io/pscat/hybrid.html, with the caption "Scan: how this menu was made" beside it. Save it as menu_model.png in the current directory at full resolution; do not resize or recompress it. Print the path and pixel dimensions, and print verbatim the exact prompt you passed to the image generation tool.

Notes on the two whole-poster runs, so the comparison can be judged fairly:

- **One attempt each, no picking.** Each "image model alone" panel is the
  first image generated for its instruction. Nothing was retried or chosen
  for looks; a retry would have been allowed only for a tool failure, and
  none happened.
- **The exact tool prompt is not recorded for these two.** Codex's image
  skill normalizes an instruction into a prompt before calling the tool, and
  these runs were stopped (next point) before Codex printed what it passed.
  The instruction above is what went in; the image is what the tool
  returned.
- **Codex then tried to fix the QR code itself.** After generating, it
  checked the QR code, found it did not decode, and began compositing a QR
  code made by a real encoder into the picture. That run was stopped, and
  the committed image is the tool's original output, untouched: the panel
  shows the image model alone, not an agent patching it. (Patching a real
  code in is, of course, the hybrid approach.)

The originals are PNGs; they are committed as JPEGs, which is what pscat's
`/DCTDecode` filter reads, converted with macOS `sips` (`-s format jpeg -s
formatOptions 80` for the two backgrounds, `90` for the two whole-poster
attempts). Both decoders gave the same verdict on the original PNGs as on the
JPEGs: a symbol found, its error correction failed.
