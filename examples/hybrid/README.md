# Hybrid examples: an image model paints, pscat sets the exact layer

Worked examples for [pscat with image models](../../docs/guides/pscat-with-image-models.md):

| Program | Page | Background (frozen) | Image model alone (for comparison) |
|---|---|---|---|
| [`sky_talk.ps`](sky_talk.ps), a public-talk poster | A4, 595 x 842 pt | [`sky_talk_bg.jpg`](sky_talk_bg.jpg) | [`sky_talk_model.jpg`](sky_talk_model.jpg) |
| [`menu_card.ps`](menu_card.ps), a café menu | US Letter, 612 x 792 pt | [`menu_card_bg.jpg`](menu_card_bg.jpg) | [`menu_card_model.jpg`](menu_card_model.jpg) |
| [`ferry_timetable.ps`](ferry_timetable.ps), a ferry timetable (#999) | A4, 595 x 842 pt | [`ferry_bg.jpg`](ferry_bg.jpg) | [`ferry_model.jpg`](ferry_model.jpg) |

Render from the repository root (the programs read their JPEGs by a relative
path):

The title and label faces (Abril Fatface and Jost on the poster, Playfair
Display and Cinzel on the menu) are catalog faces. A checkout has them; an
installed pscat needs `pscat fonts install --pack extras` first, or they
stand in as Helvetica.

```sh
pscat --page 595x842 --pdf sky_talk.pdf examples/hybrid/sky_talk.ps
pscat --page 612x792 --pdf menu_card.pdf examples/hybrid/menu_card.ps
pscat --page 595x842 --dpi 144 --png sky_talk.png examples/hybrid/sky_talk.ps
```

`/PscatOnly true def` before a program swaps the painting for art that
pscat draws itself (a screen-printed vector sky; a flat-ink botanical print
of the pears), keeping the exact layer:

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
pscat sheet -o docs/guides/hybrid/ferry_compare.png --page 595x1032 \
  --dpi 48 --no-sizes --sweep Panel=1,2,3 \
  --label "Harbour Ferry: (a) image model alone, (b) pscat alone, (c) hybrid" \
  examples/hybrid/compare_ferry.ps
```

The facts are checked by `tests/hybrid.rs`: the QR codes of (b) and (c) are
read back by two independent decoders from the PNG and the PDF export, every
string of the brief is painted exactly, and the decoders' verdict on the image
model's own QR codes is pinned. The text of (a) was checked by reading the
image at full size against the brief, string by string.

## Where the images came from

All four JPEGs were made on 2026-11-04 by **OpenAI's image model, through the
Codex CLI 0.166 `image_generation` tool**, signed in with the project
owner's ChatGPT account. OpenAI's terms assign the output to the user; the
owner approved committing them. As files under `examples/`, they fall under
that directory's existing CC0 1.0 mapping in `licenses/assets.json`
([`LICENSE-CC0`](../../LICENSE-CC0)).

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

## The repeat runs (2026-11-05)

For [issue #999](https://github.com/PSCatLabs/pscat/issues/999) the
whole-poster and whole-menu instructions above were run again, six times
each, and a harder example was added. Everything was run on 2026-11-05 with
OpenAI's image model, through the Codex CLI 0.166 `image_generation` tool,
signed in as before. Each run was one command in its own empty scratch
directory:

```sh
codex exec --skip-git-repo-check -s workspace-write -- "<instruction>" < /dev/null
```

- **The instructions** were the two whole-poster instructions above, verbatim,
  with one sentence appended: "Do not check, edit or post-process the image
  after generating it." (In #995 Codex had started to patch its own QR code.)
- **The prompt that reached the image tool** is Codex's rewrite of the
  instruction, and it differed from run to run. Some rewrites added detail
  that the instruction did not have, such as the bars' percentage heights or
  "a full four-module white quiet zone". Each is quoted below exactly as
  Codex printed it.
- **No post-processing.** Each saved PNG was checked byte for byte against
  the original in `~/.codex/generated_images/` and is identical.
- **What is committed.** The repeat images are not committed. Their checked
  results are in the guide's "Is it right every time?" section. Only the
  ferry timetable's background (`ferry_bg.jpg`, JPEG quality 80) and its
  first whole-timetable run (`ferry_model.jpg`, quality 90, used as panel
  (a), with no picking) are committed. Both decoders give the same verdict on
  `ferry_model.jpg` as on its PNG original, and `tests/hybrid.rs` pins that
  verdict.

**`ferry_bg.jpg`** (original 1055 x 1491 px PNG). The instruction:

> Use your image generation tool to create the following image. A loose watercolour painting on cream cold-pressed paper for the header of a ferry timetable, portrait orientation (A4 proportions, 1 by 1.414). Only the top third of the sheet is painted: a small blue-and-white passenger ferry crossing a calm harbour on a clear winter morning, a red-and-white lighthouse on a low headland at the right, a few gulls, soft wet-in-wet washes of blue, teal and sand. The painting fades out softly into the paper; the bottom two-thirds of the sheet stay bare cream paper with only the faintest wash, so a timetable can be printed over it. Visible paper grain. No text, no letters, no numbers, no logos, no frame or border. Save it as ferry_bg.png in the current directory at full resolution; do not resize or recompress it. Print the path and pixel dimensions, and print verbatim the exact prompt you passed to the image generation tool. Do not check, edit or post-process the image after generating it.

The prompt Codex passed to the tool:

> A loose watercolour painting on cream cold-pressed paper for the header of a ferry timetable, portrait orientation (A4 proportions, 1 by 1.414). Only the top third of the sheet is painted: a small blue-and-white passenger ferry crossing a calm harbour on a clear winter morning, a red-and-white lighthouse on a low headland at the right, a few gulls, soft wet-in-wet washes of blue, teal and sand. The painting fades out softly into the paper; the bottom two-thirds of the sheet stay bare cream paper with only the faintest wash, so a timetable can be printed over it. Visible paper grain. No text, no letters, no numbers, no logos, no frame or border.

**`ferry_model.jpg`** and the other four whole-timetable runs (each 1055 x 1491 px). The instruction, the same for all five:

> Use your image generation tool to create the following image. Design a complete A4 portrait ferry timetable as one image. Background: a loose watercolour of a small passenger ferry crossing a harbour, with a lighthouse, across the top of the sheet, on cream paper. The timetable must contain exactly this text, spelled exactly as given, with every time and price exactly as given: the title "Harbour Ferry" and the line "Winter timetable from Tuesday 1 December 2026". A table of departures, Monday to Saturday, with one row per stop and one column per sailing: "Harbourside Pier" 07:15, 08:45, 10:15, 11:45, 13:15, 14:45, 16:15, 17:45; "Mill Lane Steps" 07:22, 08:52, 10:22, 11:52, 13:22, 14:52, 16:22, 17:52; "Lighthouse Point" 07:31, 09:01, 10:31, 12:01, 13:31, 15:01, 16:31, 18:01; "Gull Island" 07:44, 09:14, 10:44, 12:14, 13:44, 15:14, 16:44, 18:14; "Westbay" 07:56, 09:26, 10:56, 12:26, 13:56, 15:26, 16:56, 18:26. A fares table with columns "Single" and "Return": "Adult" 3.40 and 6.20; "Child (5–15)" 1.70 and 3.10; "Concession" 2.30 and 4.20; "Bicycle" 1.00 and 1.80. The small print "Children under 5 travel free. Tickets on board, card or cash." and "Sailings may be cancelled in high winds; check the board at each pier." Include a scannable QR code that encodes exactly https://pscatlabs.github.io/pscat/hybrid.html, with the caption "Scan: how this timetable was made" beside it. Save it as ferry_model.png in the current directory at full resolution; do not resize or recompress it. Print the path and pixel dimensions, and print verbatim the exact prompt you passed to the image generation tool. Do not check, edit or post-process the image after generating it.

**The edits.** Two runs asked the model to change one line of an image it was given, with the image attached (`--image input.png`): sky run 1 and menu run 1. The instructions:

> Use your image generation tool to edit the attached poster image (input.png). Change only the date line from "Thursday 3 December 2026 · 7:30 pm" to "Friday 4 December 2026 · 7:30 pm". Change nothing else in the image. Save the result as sky_edit.png in the current directory at full resolution; do not resize or recompress it. Print the path and pixel dimensions, and print verbatim the exact prompt you passed to the image generation tool. Do not check, edit or post-process the image after generating it.

> Use your image generation tool to edit the attached menu image (input.png). Change only the price of "Flat white" from 3.80 to 3.95. Change nothing else in the image. Save the result as menu_edit.png in the current directory at full resolution; do not resize or recompress it. Print the path and pixel dimensions, and print verbatim the exact prompt you passed to the image generation tool. Do not check, edit or post-process the image after generating it.

The prompt Codex passed to the image tool, for each run:

<details><summary>sky-1 (started 2026-11-05 09:40:38 PST)</summary>

```text
Create a complete A4 portrait event poster as one finished image, at the highest available native resolution, with an A4 width-to-height ratio of 210:297. The entire flat poster fills the image, with no surrounding mockup, frame, wall, or props.

Background: a painterly oil painting of an evening sky over a calm sea, deep blue above fading to peach and gold at the horizon. Preserve rich oil-paint brushwork and a tranquil atmosphere. Integrate beautifully legible, professionally typeset event information over the painting with a clear hierarchy, generous margins, and strong contrast.

The poster must contain exactly the following text, spelled exactly as given, with no additional wording:
"Why Is the Sky Blue?"
"A free public talk on light, air and colour"
"Dr. Imogen Hartley, atmospheric physicist"
"Thursday 3 December 2026 · 7:30 pm"
"Harbourside Library, Lecture Room 2"
"I ∝ 1/λ⁴"
"Scattering relative to red light (700 nm)"
"400 nm 9.4×"
"450 nm 5.9×"
"500 nm 3.8×"
"550 nm 2.6×"
"600 nm 1.9×"
"700 nm 1.0×"
"Scan: how this poster was made"

Make "Why Is the Sky Blue?" the prominent title, and "A free public talk on light, air and colour" its subtitle. Present the speaker, date and time, and venue clearly. Display the Rayleigh scattering formula exactly as "I ∝ 1/λ⁴", with the proportionality sign, Greek lambda, and superscript four.

Include a small, precise vertical bar chart titled exactly "Scattering relative to red light (700 nm)". It must have six bars of equal width on a shared zero baseline, in this left-to-right order, labelled exactly "400 nm 9.4×", "450 nm 5.9×", "500 nm 3.8×", "550 nm 2.6×", "600 nm 1.9×", and "700 nm 1.0×". Each bar's height must be proportional to its numerical value: 9.4, 5.9, 3.8, 2.6, 1.9, 1.0. Thus, relative to the first bar, the heights must be 100%, 62.766%, 40.426%, 27.660%, 20.213%, and 10.638%. Colour the bars violet, blue, cyan, green, orange, and red respectively. Keep all six labels readable and unobscured; they may each break onto two lines. Do not add axis wording or numerical tick labels.

Include a real, scannable QR code encoding exactly this URL, with no trailing spaces or added characters: https://pscatlabs.github.io/pscat/hybrid.html
Render the QR code as a crisp, undistorted square of solid black modules on a clean white background, with a full four-module white quiet zone on all sides and enough size to scan reliably. Its encoded data must be the exact URL above, not a decorative approximation. Place the exact caption "Scan: how this poster was made" beside the QR code. Do not print the URL elsewhere.

All elements, including the painting, text, formula, chart, and QR code, must be part of this single finished poster image. No watermark, logo, invented text, or omitted text.
```

</details>

<details><summary>sky-2 (started 2026-11-05 09:42:21 PST)</summary>

```text
Create a complete A4 portrait event poster as one finished image, at full native resolution, ideally 2480 × 3508 pixels with the A4 width-to-height ratio. Use case: ads-marketing and scientific-educational.

Background: a painterly oil painting of an evening sky over a calm sea, deep blue above fading to peach and gold at the horizon. Give the composition refined event-poster typography, generous margins, excellent readability and a clear visual hierarchy. Integrate the information, small chart and QR code elegantly into this single poster.

The poster must contain exactly the following text, spelled and punctuated exactly as written, with no additional text:
Title: "Why Is the Sky Blue?"
Subtitle: "A free public talk on light, air and colour"
Speaker: "Dr. Imogen Hartley, atmospheric physicist"
Date and time: "Thursday 3 December 2026 · 7:30 pm"
Venue: "Harbourside Library, Lecture Room 2"
Rayleigh scattering formula: "I ∝ 1/λ⁴"
Small bar chart title: "Scattering relative to red light (700 nm)"
Six bar labels, in this left-to-right order:
"400 nm 9.4×"
"450 nm 5.9×"
"500 nm 3.8×"
"550 nm 2.6×"
"600 nm 1.9×"
"700 nm 1.0×"
QR caption: "Scan: how this poster was made"

The small bar chart must have exactly six bars, with a shared zero baseline and linear heights exactly proportional to 9.4, 5.9, 3.8, 2.6, 1.9 and 1.0, respectively. Use violet, blue, cyan, green, orange and red for the corresponding wavelengths. Keep every complete bar label legible; each label may be split into wavelength and value on two lines. Do not add axis text or tick labels.

Include a real scannable QR code that encodes exactly this URL: https://pscatlabs.github.io/pscat/hybrid.html
Render the QR as a crisp, precise black-and-white square matrix with standard finder patterns and a sufficient uninterrupted white quiet zone on all four sides. Place the caption "Scan: how this poster was made" beside the QR code. The URL is the encoded payload only; do not print the URL as visible text.

Render all required text literally; the descriptors Title, Subtitle, Speaker, Date and time, Venue, Rayleigh scattering formula, Small bar chart title, Six bar labels and QR caption are instructions, not text to print. No quotation marks around the displayed text. No additional words, logos, watermarks or border mockup. Deliver the complete flat poster image, with opaque background.
```

</details>

<details><summary>sky-3 (started 2026-11-05 09:43:54 PST)</summary>

```text
Create a complete, polished A4 portrait event poster as one image, with an A4 aspect ratio (210:297), at the highest available native resolution.

Background: a painterly oil painting of an evening sky over a calm sea, deep blue above fading to peach and gold at the horizon. Use expressive oil paint texture and an elegant, carefully balanced editorial poster layout. Keep all typography crisp, legible, and beautifully spaced, with strong contrast against the painting.

The poster must contain exactly the following text, spelled exactly as given, with no additional text:
Title: "Why Is the Sky Blue?"
Subtitle: "A free public talk on light, air and colour"
Speaker: "Dr. Imogen Hartley, atmospheric physicist"
Date and time: "Thursday 3 December 2026 · 7:30 pm"
Venue: "Harbourside Library, Lecture Room 2"
Rayleigh scattering formula: "I ∝ 1/λ⁴"
Chart title: "Scattering relative to red light (700 nm)"
Six bar labels:
"400 nm 9.4×"
"450 nm 5.9×"
"500 nm 3.8×"
"550 nm 2.6×"
"600 nm 1.9×"
"700 nm 1.0×"
QR caption: "Scan: how this poster was made"

The words Title, Subtitle, Speaker, Date and time, Venue, Rayleigh scattering formula, Chart title, Six bar labels, and QR caption are instructions only; do not print those instructional labels or quotation marks.

Place the main title prominently in the upper portion, with the subtitle beneath it, and arrange the speaker and event details in a clear hierarchy. Include the formula clearly. Include a small, precise six-bar chart with a shared zero baseline and equal-width bars, in wavelength order from 400 nm to 700 nm. Each bar's height must be proportional to its numerical value: 9.4, 5.9, 3.8, 2.6, 1.9, 1.0. Give each bar its exact combined wavelength and value label as specified above, with enough space to read every label. Use violet, blue, cyan, green, orange, and red bar colours in wavelength order.

Include a real, scannable QR code encoding exactly https://pscatlabs.github.io/pscat/hybrid.html . Render the QR code with precise square black modules on a plain opaque white square, with a generous white quiet zone, no stylization, no texture, and no painting over the code. Put the exact caption "Scan: how this poster was made" beside the QR code. The URL must be encoded in the QR code, not printed elsewhere on the poster.

Deliver only the finished flat poster image, edge to edge, without a mockup, frame, surrounding room, watermark, or extra text.
```

</details>

<details><summary>sky-4 (started 2026-11-05 09:45:27 PST)</summary>

```text
Create one complete A4 portrait event poster as a single image, at the highest available native resolution, with an A4 aspect ratio (210:297). Use case: scientific-educational event poster.

Background: a painterly oil painting of an evening sky over a calm sea, deep blue above fading to peach and gold at the horizon. Integrate refined, highly legible event typography and a small scientific chart into this painting. Keep all text clear, well spaced, and within generous page margins. The result is the finished flat poster itself, with no mockup, frame, or surrounding scene.

The poster must contain exactly the following text, spelled and punctuated exactly as given, with no additional prose or branding:
Title: "Why Is the Sky Blue?"
Subtitle: "A free public talk on light, air and colour"
Speaker: "Dr. Imogen Hartley, atmospheric physicist"
Date and time: "Thursday 3 December 2026 · 7:30 pm"
Venue: "Harbourside Library, Lecture Room 2"
Rayleigh scattering formula: "I ∝ 1/λ⁴"
Chart title: "Scattering relative to red light (700 nm)"
Six bar labels:
"400 nm 9.4×"
"450 nm 5.9×"
"500 nm 3.8×"
"550 nm 2.6×"
"600 nm 1.9×"
"700 nm 1.0×"
QR caption: "Scan: how this poster was made"

Make the title prominent near the top, followed by the subtitle. Place the speaker, date and venue in a readable hierarchy. Include the formula as a clear scientific accent. Include a small vertical bar chart with six bars in wavelength order from left to right. All bars must begin on the same baseline and their heights must be strictly proportional to 9.4, 5.9, 3.8, 2.6, 1.9 and 1.0 respectively. Use the exact corresponding label below each bar; labels may wrap onto two lines without changing their text. The chart title must appear above the chart. Color the bars violet, blue, cyan, green, orange and red respectively.

Include a real, scannable, square black-and-white QR code encoding exactly this URL: https://pscatlabs.github.io/pscat/hybrid.html . Preserve the correct QR module pattern, three finder squares, crisp square modules and a clean white quiet zone; no decorative distortion or painting over the code. Place the exact caption "Scan: how this poster was made" beside the QR code. The URL is the QR payload, not additional printed text.

Render all the specified text verbatim, preserving the question mark, middle dot, multiplication signs, proportionality symbol, Greek lambda and superscript 4. Do not print the field labels such as "Title:", "Speaker:" or "QR caption:". Generate the entire finished poster in this one image.
```

</details>

<details><summary>sky-5 (started 2026-11-05 09:47:57 PST)</summary>

```text
Create a complete, finished A4 portrait event poster as one image, at the highest available native resolution, with the A4 aspect ratio (210:297). Use case: scientific-educational event poster.

Background: a painterly oil painting of an evening sky over a calm sea, deep blue above fading to peach and gold at the horizon. Keep the painting atmospheric and richly textured, with a calm sea and a luminous horizon. Integrate elegant, highly legible typography, generous margins, and a clear hierarchy into the finished poster.

The poster must contain exactly the following text, spelled exactly as given. Do not add any other text:
Why Is the Sky Blue?
A free public talk on light, air and colour
Dr. Imogen Hartley, atmospheric physicist
Thursday 3 December 2026 · 7:30 pm
Harbourside Library, Lecture Room 2
I ∝ 1/λ⁴
Scattering relative to red light (700 nm)
400 nm 9.4×
450 nm 5.9×
500 nm 3.8×
550 nm 2.6×
600 nm 1.9×
700 nm 1.0×
Scan: how this poster was made

Make "Why Is the Sky Blue?" the prominent title and "A free public talk on light, air and colour" the subtitle. Clearly present the speaker, date/time, and venue. Include the Rayleigh scattering formula exactly as "I ∝ 1/λ⁴".

Include a small, clean, precisely drawn bar chart titled "Scattering relative to red light (700 nm)". It must have exactly six vertical bars, in this order, with these labels and values: "400 nm 9.4×", "450 nm 5.9×", "500 nm 3.8×", "550 nm 2.6×", "600 nm 1.9×", "700 nm 1.0×". All bars share one baseline, and their heights must be proportional to 9.4, 5.9, 3.8, 2.6, 1.9, and 1.0 respectively. Use violet, blue, cyan, green, orange, and red for the six bars respectively. Keep all chart text readable and ensure it fits.

Include a real, scannable QR code encoding exactly https://pscatlabs.github.io/pscat/hybrid.html with the caption "Scan: how this poster was made" beside it. The QR code must have accurate black square modules on an opaque white square, a full four-module white quiet zone, crisp edges, and sufficient size for scanning. Encode that exact URL in the QR code; do not print the URL as additional text. Keep the QR code unobstructed and free of painting texture or decoration.

Render all poster text exactly, including punctuation, the middle dot, the multiplication signs, the Greek lambda, and the superscript 4. Deliver the entire poster edge to edge as one flat image, without an external frame, mockup, watermark, or additional text.
```

</details>

<details><summary>sky-6 (started 2026-11-05 09:49:33 PST)</summary>

```text
Create a complete A4 portrait event poster as one finished image, at full resolution in PNG format. Use an A4 portrait aspect ratio (210:297).

Background: a painterly oil painting of an evening sky over a calm sea, deep blue above fading to peach and gold at the horizon. Combine the painting with polished, highly legible event-poster typography and a clear, carefully aligned layout. Keep all text comfortably within the page margins.

The poster must contain exactly the following text, spelled exactly as given, with no additional text:
Title: "Why Is the Sky Blue?"
Subtitle: "A free public talk on light, air and colour"
"Dr. Imogen Hartley, atmospheric physicist"
"Thursday 3 December 2026 · 7:30 pm"
"Harbourside Library, Lecture Room 2"
Rayleigh scattering formula: "I ∝ 1/λ⁴"
Bar chart title: "Scattering relative to red light (700 nm)"
Six bar labels:
"400 nm 9.4×"
"450 nm 5.9×"
"500 nm 3.8×"
"550 nm 2.6×"
"600 nm 1.9×"
"700 nm 1.0×"
QR caption: "Scan: how this poster was made"

Include a small bar chart with exactly six bars, in wavelength order from 400 nm to 700 nm, with each bar's height proportional to its value on a common zero baseline: 9.4, 5.9, 3.8, 2.6, 1.9, 1.0. Associate each bar unambiguously with its exact label above. Use violet, blue, cyan, green, orange, and red respectively. Make the chart title and labels readable.

Include a real, scannable QR code encoding exactly https://pscatlabs.github.io/pscat/hybrid.html . Render the QR modules sharply in pure black on a solid white square with a sufficient white quiet zone, without painterly distortion, decoration, or overlapping marks. Place the caption "Scan: how this poster was made" beside the QR code.

Render the complete poster directly in this single image. Preserve the exact wording, symbols, punctuation, spelling, and numerical values. Do not print the field names such as Title or Subtitle, quotation marks, or the URL as additional text.
```

</details>

<details><summary>menu-1 (started 2026-11-05 09:40:38 PST)</summary>

```text
Create a complete café menu card as ONE finished image, US Letter portrait (8.5:11 aspect ratio), flat full-page artwork with no surrounding scene or mockup. Use the highest available native resolution.

Background: a loose watercolour of exactly two pears and pear leaves on cream paper. Keep the watercolour soft and the menu text clearly legible, with generous spacing and refined café typography. Arrange the title at the top, followed by the address, then the three menu sections. Align prices neatly while preserving every word and price exactly.

The menu must contain exactly the following text, spelled and punctuated exactly as given. Do not add any other visible text:

The Lantern Pear
Café · 14 Mill Lane

Breakfast, 8–11
Buttermilk pancakes, maple butter 9.50
Shakshuka with sourdough 11.25
Bircher muesli, poached pear 7.75

Lunch, 12–3
Roast squash and farro salad 12.50
Leek and Gruyère tart 13.00
Soup of the day with bread 8.25

Drinks
Flat white 3.80
Pot of tea 3.20
Pear and ginger soda 4.10

Scan: how this menu was made

Include a real, scannable QR code encoding exactly https://pscatlabs.github.io/pscat/hybrid.html with the caption "Scan: how this menu was made" beside it. The URL is the QR payload only and must not appear as additional printed text. Render the QR code with sharp black square modules on solid white, a sufficient clear quiet zone, and no watercolour or decoration over it. Preserve the exact accents, middle dot, en dashes, capitalization, wording, and decimal prices. No extra labels, currency symbols, logos, or watermarks.
```

</details>

<details><summary>menu-2 (started 2026-11-05 09:42:21 PST)</summary>

```text
Create one complete café menu card as a single finished image, US Letter portrait (8.5:11 aspect ratio), at full native resolution. The card fills the entire image, viewed straight on, with no mockup, surrounding tabletop, perspective, or cropped edges.

Background: a loose watercolour of exactly two pears and pear leaves on cream paper. Keep the watercolour soft and place it so all menu lettering is clear and readable. Use an elegant, balanced café menu layout with generous margins, tasteful typography, and clearly aligned prices.

The menu must contain exactly the following text, spelled exactly as written, preserving every accent, punctuation mark, en dash, and price. Do not add other text. Set the café name prominently, the address below it, then the three sections and their items in this order. Prices may be aligned in a right-hand column while retaining the exact item text and values.

The Lantern Pear
Café · 14 Mill Lane

Breakfast, 8–11
Buttermilk pancakes, maple butter 9.50
Shakshuka with sourdough 11.25
Bircher muesli, poached pear 7.75

Lunch, 12–3
Roast squash and farro salad 12.50
Leek and Gruyère tart 13.00
Soup of the day with bread 8.25

Drinks
Flat white 3.80
Pot of tea 3.20
Pear and ginger soda 4.10

Scan: how this menu was made

Include a real, scannable QR code that encodes exactly https://pscatlabs.github.io/pscat/hybrid.html and no other content. Render the QR code with crisp black square modules on a clean white square, a sufficient white quiet zone on all four sides, no artistic distortions, and no illustration behind it. Place the exact caption "Scan: how this menu was made" beside the QR code. The encoded URL must not appear as additional printed text. Keep the entire QR code and all menu text within the page margins.
```

</details>

<details><summary>menu-3 (started 2026-11-05 09:43:54 PST)</summary>

```text
Create a complete café menu card as one finished image, US Letter portrait (8.5 × 11 inches, 17:22 aspect ratio), at full native resolution, ideally 2550 × 3300 pixels. The artwork fills the image; no surrounding scene, mockup, perspective, or cropped edges.

Background: a loose watercolour of exactly two pears and pear leaves on cream paper. Keep the painting soft and airy so that all menu text remains exceptionally clear. Use elegant, readable café typography, a strong title, well-spaced section headings, and neatly aligned prices. Compose a polished printable menu with generous margins.

The menu must contain exactly the following text, spelled exactly as given, preserving punctuation, accents, en dashes, and every price. Do not add any other wording. Each menu item and its price should appear together on a single row.

The Lantern Pear
Café · 14 Mill Lane

Breakfast, 8–11
Buttermilk pancakes, maple butter 9.50
Shakshuka with sourdough 11.25
Bircher muesli, poached pear 7.75

Lunch, 12–3
Roast squash and farro salad 12.50
Leek and Gruyère tart 13.00
Soup of the day with bread 8.25

Drinks
Flat white 3.80
Pot of tea 3.20
Pear and ginger soda 4.10

Include a real, scannable QR code encoding exactly this URL: https://pscatlabs.github.io/pscat/hybrid.html
Render the QR code with crisp black square modules on solid white, correct finder patterns, and a generous uninterrupted white quiet zone. It must be large enough to scan reliably on a printed US Letter menu. Place this exact caption beside it:
Scan: how this menu was made

The URL is the QR payload only and must not appear as visible menu text. Ensure all requested text appears once, with no omissions, substitutions, extra text, currency symbols, or watermark.
```

</details>

<details><summary>menu-4 (started 2026-11-05 09:45:27 PST)</summary>

```text
Use case: ads-marketing
Asset type: complete café menu card, one flat image.
Create a finished US Letter portrait menu (8.5 × 11 inches, aspect ratio 17:22), at full native resolution, ideally 2550 × 3300 pixels. Fill the image with the menu itself, viewed straight on, with no mockup, surrounding scene, or perspective.
Background: a loose watercolour of exactly two pears and pear leaves on cream paper. Keep the watercolour soft and the text highly legible. Use elegant café typography, generous spacing, clear section headings, and neatly aligned prices. Arrange the three sections vertically with room for the QR code and its caption at the bottom.
Render exactly the following visible text, spelled exactly as written, preserving accents, punctuation, en dashes, and all prices. Do not add any other visible text:
The Lantern Pear
Café · 14 Mill Lane

Breakfast, 8–11
Buttermilk pancakes, maple butter 9.50
Shakshuka with sourdough 11.25
Bircher muesli, poached pear 7.75

Lunch, 12–3
Roast squash and farro salad 12.50
Leek and Gruyère tart 13.00
Soup of the day with bread 8.25

Drinks
Flat white 3.80
Pot of tea 3.20
Pear and ginger soda 4.10

Scan: how this menu was made

Include a real, scannable QR code encoding exactly https://pscatlabs.github.io/pscat/hybrid.html with no extra characters. Use a correct QR module matrix, crisp square black modules on solid white, and a clear four-module white quiet zone. Do not stylize or distort the QR code. Place the exact caption "Scan: how this menu was made" beside it. The URL is encoded in the QR code only, not printed as additional text. Keep all menu text and the complete QR code comfortably within the page margins.
```

</details>

<details><summary>menu-5 (started 2026-11-05 09:47:57 PST)</summary>

```text
Create a complete, finished café menu card as one flat image, US Letter portrait, aspect ratio 8.5:11. Render at 2176 × 2816 pixels if supported, at full native resolution.

Background: a loose watercolour of exactly two pears and pear leaves on cream paper. Keep the watercolour gentle and airy, with clear cream space behind all lettering. Use refined, readable café typography, a prominent café name, balanced spacing, and neatly aligned prices. Show the entire card straight-on, filling the image, with no perspective, surrounding scene, crop, mockup, or watermark.

The menu must contain exactly the following text, spelled exactly as given, with all accents, punctuation, en dashes, decimal prices, and capitalization preserved. Do not add any other visible text. Each dish and its price belong on one menu row; prices may be aligned in a right-hand column.

The Lantern Pear
Café · 14 Mill Lane

Breakfast, 8–11
Buttermilk pancakes, maple butter 9.50
Shakshuka with sourdough 11.25
Bircher muesli, poached pear 7.75

Lunch, 12–3
Roast squash and farro salad 12.50
Leek and Gruyère tart 13.00
Soup of the day with bread 8.25

Drinks
Flat white 3.80
Pot of tea 3.20
Pear and ginger soda 4.10

Scan: how this menu was made

Include a functional, scannable QR code that encodes exactly this URL: https://pscatlabs.github.io/pscat/hybrid.html
Place the caption "Scan: how this menu was made" beside the QR code. Render the QR code as a precise, high-contrast black square module grid on an untextured light square with a proper quiet zone, no decorative distortion, no illustration overlapping it, and enough size to scan. The URL is the QR payload only and must not appear as extra printed text. Preserve every requested menu line exactly.
```

</details>

<details><summary>menu-6 (started 2026-11-05 09:49:33 PST)</summary>

```text
Create one complete, polished café menu card as a single flat image, US Letter portrait proportions (8.5 × 11 inches), at full native resolution. Show only the menu artwork, edge to edge, with no mockup, perspective, surrounding objects, or cropped content.

Background: a loose watercolour of exactly two pears and pear leaves on cream paper. Keep the watercolour delicate and positioned so all menu text is clear and easy to read. Use elegant café typography, generous margins, balanced spacing, and a clear hierarchy. Align prices neatly while preserving the exact item wording and price values below.

The menu must contain exactly the following text, spelled exactly as given. Preserve accents, punctuation, the middle dot, en dashes, capitalization, and all prices. Do not add any other text. The quotation marks and layout instructions are not printed.

The Lantern Pear
Café · 14 Mill Lane

Breakfast, 8–11
Buttermilk pancakes, maple butter 9.50
Shakshuka with sourdough 11.25
Bircher muesli, poached pear 7.75

Lunch, 12–3
Roast squash and farro salad 12.50
Leek and Gruyère tart 13.00
Soup of the day with bread 8.25

Drinks
Flat white 3.80
Pot of tea 3.20
Pear and ginger soda 4.10

Include a real, scannable QR code that encodes exactly this URL:
https://pscatlabs.github.io/pscat/hybrid.html
The URL itself must not appear as printed text. Make the QR code crisp black modules on a clean white square with an adequate quiet zone, large enough to scan reliably. Keep watercolour and text outside its quiet zone. Beside the QR code, print exactly this caption:
Scan: how this menu was made

Deliver the complete menu as one high-resolution PNG image.
```

</details>

<details><summary>ferry-1 (started 2026-11-05 10:03:07 PST)</summary>

```text
Create a complete A4 portrait ferry timetable as one finished image, at full native resolution. Use a flat, straight-on page layout with no surrounding scene or mockup. Background: cream paper with a loose watercolour of a small passenger ferry crossing a harbour, with a lighthouse, across the top of the sheet. Arrange the information below in clear, beautifully typeset, highly legible tables, with generous margins and careful alignment. Preserve every character, time, price, and punctuation exactly. Do not add currency symbols or any extra text.

Title:
Harbour Ferry

Subtitle:
Winter timetable from Tuesday 1 December 2026

Departure table heading:
Monday to Saturday

Departure table: exactly five stop rows, with the stop name at left and exactly eight sailing columns. Each line below is one row; vertical bars indicate column boundaries and should be rendered as table layout rather than literal text:
Harbourside Pier | 07:15 | 08:45 | 10:15 | 11:45 | 13:15 | 14:45 | 16:15 | 17:45
Mill Lane Steps | 07:22 | 08:52 | 10:22 | 11:52 | 13:22 | 14:52 | 16:22 | 17:52
Lighthouse Point | 07:31 | 09:01 | 10:31 | 12:01 | 13:31 | 15:01 | 16:31 | 18:01
Gull Island | 07:44 | 09:14 | 10:44 | 12:14 | 13:44 | 15:14 | 16:44 | 18:14
Westbay | 07:56 | 09:26 | 10:56 | 12:26 | 13:56 | 15:26 | 16:56 | 18:26

Fares table: an unlabeled first column for passenger categories, then columns headed exactly Single and Return. Exactly four fare rows:
[blank] | Single | Return
Adult | 3.40 | 6.20
Child (5–15) | 1.70 | 3.10
Concession | 2.30 | 4.20
Bicycle | 1.00 | 1.80

Small print, verbatim:
Children under 5 travel free. Tickets on board, card or cash.
Sailings may be cancelled in high winds; check the board at each pier.

Include a scannable QR code that encodes exactly https://pscatlabs.github.io/pscat/hybrid.html. Render the QR code as sharp black square modules on a plain white square, with a sufficient quiet zone for reliable scanning. Beside the QR code place this exact caption:
Scan: how this timetable was made

The complete page, including illustration, all five timetable rows and eight sailing columns, all fares, both small-print sentences, QR code and caption must fit within the A4 portrait sheet. Do not omit or duplicate any entry.
```

</details>

<details><summary>ferry-2 (started 2026-11-05 10:03:07 PST)</summary>

```text
Create a complete A4 portrait ferry timetable as ONE finished image at full resolution, with an A4 portrait aspect ratio. Use case: infographic-diagram. Show the entire flat sheet, edge to edge, without perspective, mockup surroundings, or cropping.

Design: cream paper, with a loose watercolour of a small passenger ferry crossing a harbour, with a lighthouse, across the top of the sheet. Below and integrated with this illustration, use clear, carefully typeset typography, generous margins, and readable aligned tables. The illustration must not obscure any text. Every word, time, and price below must appear exactly as supplied. Do not add other text, currency symbols, or extra sailings.

Title:
Harbour Ferry

Subtitle:
Winter timetable from Tuesday 1 December 2026

Departures heading:
Monday to Saturday

Departures table: exactly five stop rows, with one column for the stop name and eight columns for the eight sailings. Read each row left to right. Do not print the pipe separators; they indicate table cells.
Harbourside Pier | 07:15 | 08:45 | 10:15 | 11:45 | 13:15 | 14:45 | 16:15 | 17:45
Mill Lane Steps | 07:22 | 08:52 | 10:22 | 11:52 | 13:22 | 14:52 | 16:22 | 17:52
Lighthouse Point | 07:31 | 09:01 | 10:31 | 12:01 | 13:31 | 15:01 | 16:31 | 18:01
Gull Island | 07:44 | 09:14 | 10:44 | 12:14 | 13:44 | 15:14 | 16:44 | 18:14
Westbay | 07:56 | 09:26 | 10:56 | 12:26 | 13:56 | 15:26 | 16:56 | 18:26

Fares table: an unlabeled first column for the passenger type, then columns headed exactly "Single" and "Return". Exactly these four rows, preserving decimal points, two decimal places, and the en dash in Child (5–15):
Adult | 3.40 | 6.20
Child (5–15) | 1.70 | 3.10
Concession | 2.30 | 4.20
Bicycle | 1.00 | 1.80

Small print, verbatim:
Children under 5 travel free. Tickets on board, card or cash.
Sailings may be cancelled in high winds; check the board at each pier.

Include a real scannable QR code that encodes exactly this URL, with no extra spaces or characters:
https://pscatlabs.github.io/pscat/hybrid.html
The QR code must have crisp black square modules on a plain white square, correct finder patterns and a clear quiet zone. Place it at a readable scanning size near the bottom, with this exact caption beside it:
Scan: how this timetable was made

Prioritize exact text, all 40 departure times, all eight prices, precise table alignment, and the functioning QR code. The result is the complete designed timetable image itself.
```

</details>

<details><summary>ferry-3 (started 2026-11-05 10:05:46 PST)</summary>

```text
Use case: infographic-diagram.
Create one complete A4 portrait ferry timetable as a single finished image, at the highest available native resolution, with an A4 width-to-height ratio. Show the whole sheet straight on, with no surrounding scene or mockup. Cream paper background. Across the top of the sheet, paint a loose watercolour of a small passenger ferry crossing a harbour, with a lighthouse. Below and integrated with this illustration, lay out the timetable with clear, elegant, highly legible typography, generous margins, carefully aligned columns, and restrained table rules. Keep all text clear of the painting.

Render exactly the following text, spelling, punctuation, numbers, times and prices. Do not add other text, currency symbols, sailing numbers, logos, or watermarks.

Title:
Harbour Ferry

Subtitle:
Winter timetable from Tuesday 1 December 2026

Departures heading:
Monday to Saturday

Departures table: one row per stop and eight separate time columns, one per sailing. The vertical bars below indicate column divisions only; do not print the bars.
Harbourside Pier | 07:15 | 08:45 | 10:15 | 11:45 | 13:15 | 14:45 | 16:15 | 17:45
Mill Lane Steps | 07:22 | 08:52 | 10:22 | 11:52 | 13:22 | 14:52 | 16:22 | 17:52
Lighthouse Point | 07:31 | 09:01 | 10:31 | 12:01 | 13:31 | 15:01 | 16:31 | 18:01
Gull Island | 07:44 | 09:14 | 10:44 | 12:14 | 13:44 | 15:14 | 16:44 | 18:14
Westbay | 07:56 | 09:26 | 10:56 | 12:26 | 13:56 | 15:26 | 16:56 | 18:26

Fares table: a left label column and two price columns headed exactly Single and Return. Do not add a fares heading. The vertical bars below indicate column divisions only; do not print the bars.
 | Single | Return
Adult | 3.40 | 6.20
Child (5–15) | 1.70 | 3.10
Concession | 2.30 | 4.20
Bicycle | 1.00 | 1.80

Small print, verbatim:
Children under 5 travel free. Tickets on board, card or cash.
Sailings may be cancelled in high winds; check the board at each pier.

Include a real, scannable QR code that encodes exactly this URL:
https://pscatlabs.github.io/pscat/hybrid.html
The URL is the QR payload, not additional printed text. Render the QR code with crisp black square modules on a plain light background and an adequate clear quiet zone, large enough to scan. Beside the QR code place this caption, verbatim:
Scan: how this timetable was made

The entire timetable, fares, small print, and QR code must fit comfortably on the single portrait sheet. Preserve every listed time and price exactly, with all eight sailings in every departures row. Produce the complete final image in this generation.
```

</details>

<details><summary>ferry-4 (started 2026-11-05 10:05:46 PST)</summary>

```text
Design a complete A4 portrait ferry timetable as one finished image, straight-on and filling the canvas. Use the highest available native resolution with an A4 portrait aspect ratio. Background: cream paper, with a loose watercolour of a small passenger ferry crossing a harbour, with a lighthouse, across the top of the sheet. Keep the timetable and fares crisp, legible, accurately aligned, with generous margins and restrained nautical typography. Fit all content on the single sheet.

The image must contain exactly the following wording, times and prices, spelled exactly as given. Do not add invented text, currency signs, sailing numbers, or extra labels.

Title:
Harbour Ferry

Subtitle:
Winter timetable from Tuesday 1 December 2026

Departure table heading:
Monday to Saturday

Departure table: exactly five stop rows and eight sailing columns, plus the stop-name column. Each line below is one row, with the stop name followed by its eight times in order. The vertical bars are data separators and should not appear as text:
Harbourside Pier | 07:15 | 08:45 | 10:15 | 11:45 | 13:15 | 14:45 | 16:15 | 17:45
Mill Lane Steps | 07:22 | 08:52 | 10:22 | 11:52 | 13:22 | 14:52 | 16:22 | 17:52
Lighthouse Point | 07:31 | 09:01 | 10:31 | 12:01 | 13:31 | 15:01 | 16:31 | 18:01
Gull Island | 07:44 | 09:14 | 10:44 | 12:14 | 13:44 | 15:14 | 16:44 | 18:14
Westbay | 07:56 | 09:26 | 10:56 | 12:26 | 13:56 | 15:26 | 16:56 | 18:26

Fares table: a row-label column and two price columns headed exactly Single and Return. The vertical bars are data separators and should not appear as text:
 | Single | Return
Adult | 3.40 | 6.20
Child (5–15) | 1.70 | 3.10
Concession | 2.30 | 4.20
Bicycle | 1.00 | 1.80

Small print, verbatim:
Children under 5 travel free. Tickets on board, card or cash.
Sailings may be cancelled in high winds; check the board at each pier.

Include a real, scannable QR code that encodes exactly this URL:
https://pscatlabs.github.io/pscat/hybrid.html
Render the QR code with precise sharp black square modules on a clean white square, an adequate clear quiet zone, and sufficient size to scan. Do not stylize, distort, or paint over its modules. Do not print the URL as additional visible text.

Place this exact caption beside the QR code:
Scan: how this timetable was made

Preserve every stop name, every time, every price, all punctuation, the en dash in Child (5–15), and all wording exactly. All text must be fully visible and readable. Generate the entire finished timetable directly as one image.
```

</details>

<details><summary>ferry-5 (started 2026-11-05 10:05:46 PST)</summary>

```text
Create one complete, print-ready A4 portrait ferry timetable as a single high-resolution PNG image, with the whole sheet visible straight-on and no surrounding objects. Use the A4 width-to-height proportion (210:297) and the highest practical native resolution.

Design: cream paper with a loose watercolour illustration across the top showing a small passenger ferry crossing a harbour, with a lighthouse. Keep the illustration clear of the timetable text. Use elegant, highly legible typography, generous margins, a prominent title, carefully aligned tabular numerals, and thin table rules. The departure table must fit all eight sailings across the page with one row per stop and one column per sailing. All text must be sharp, readable, and spelled exactly as specified. Do not add any text beyond the specified title, subtitle, day heading, table entries, small print, and QR caption. Do not add currency symbols.

Title:
Harbour Ferry

Subtitle:
Winter timetable from Tuesday 1 December 2026

Departure table heading:
Monday to Saturday

Departure table: the first field in each following line is the stop name, and the following eight fields are the eight sailing columns in order. Render the data as a properly aligned table, not as running text. Preserve every colon, leading zero, stop name, and time exactly.
Harbourside Pier | 07:15 | 08:45 | 10:15 | 11:45 | 13:15 | 14:45 | 16:15 | 17:45
Mill Lane Steps | 07:22 | 08:52 | 10:22 | 11:52 | 13:22 | 14:52 | 16:22 | 17:52
Lighthouse Point | 07:31 | 09:01 | 10:31 | 12:01 | 13:31 | 15:01 | 16:31 | 18:01
Gull Island | 07:44 | 09:14 | 10:44 | 12:14 | 13:44 | 15:14 | 16:44 | 18:14
Westbay | 07:56 | 09:26 | 10:56 | 12:26 | 13:56 | 15:26 | 16:56 | 18:26

Fares table below the departures, with a blank first-column header and these two column headers:
Single | Return

Fares table rows, with the category in the first column and the two prices in the Single and Return columns respectively. Preserve two decimal places and the en dash in Child (5–15).
Adult | 3.40 | 6.20
Child (5–15) | 1.70 | 3.10
Concession | 2.30 | 4.20
Bicycle | 1.00 | 1.80

Small print, verbatim:
Children under 5 travel free. Tickets on board, card or cash.
Sailings may be cancelled in high winds; check the board at each pier.

Include a real, scannable QR code encoding exactly this URL, with no added characters:
https://pscatlabs.github.io/pscat/hybrid.html
Render the QR code with precise square black modules on a clean white square, an adequate white quiet zone, and sufficient size for reliable scanning. Do not stylize or obscure the QR code. Place this exact caption beside it:
Scan: how this timetable was made

Every specified element must be present within the page. Generate the complete final image in one pass.
```

</details>

<details><summary>edit-sky (started 2026-11-05 10:02:13 PST)</summary>

```text
Edit target: input.png, the attached poster. Change only the date line from "Thursday 3 December 2026 · 7:30 pm" to "Friday 4 December 2026 · 7:30 pm". Match the existing date line’s font, size, weight, colour, baseline, and left alignment. Change nothing else in the image. Preserve all other text, the painting, layout, formula, chart, QR code, colours, and textures exactly. Preserve the full original resolution of 1055 × 1491 pixels; do not resize or recompress the image.
```

</details>

<details><summary>edit-menu (started 2026-11-05 10:02:13 PST)</summary>

```text
Use case: text-localization
Input image: input.png is the edit target.
Change only the price of "Flat white" in the Drinks section from "3.80" to "3.95". Match the existing font, size, color, baseline, spacing, and right alignment exactly. Change nothing else: preserve all other text, prices, artwork, paper texture, colors, layout, borders, and QR code exactly. Preserve the original full pixel dimensions of 1103 × 1426. Do not resize or recompress the image.
```

</details>
