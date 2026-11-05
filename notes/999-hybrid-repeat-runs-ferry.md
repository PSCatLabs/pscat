## Hybrid guide: a repeat-run accuracy test and a harder example (issue #999, 2026-11-05)

**Why.** In #995 the image model got every word and number of both whole
posters right and failed only on the QR codes. That was one sample. This
measures how often it is right, with checked facts.

**What was run (2026-11-05, OpenAI's image model through the Codex CLI 0.166
`image_generation` tool).**
- The #995 whole-poster and whole-menu instructions, six times each, with one
  sentence appended ("Do not check, edit or post-process the image after
  generating it.").
- Two one-line edits, each given its source image: the poster's date and the
  menu's Flat white price.
- A new, denser brief: an A4 ferry timetable (40 times, 8 fares, small print,
  QR), five whole-page runs, plus one background for the hybrid.
- All 20 tool prompts are verbatim in `examples/hybrid/README.md`. Every saved
  PNG is byte-identical to the tool's original in `~/.codex/generated_images/`.

**Results.**
- Text and numbers: 0 errors in all 17 whole-page images. Every string and
  number was read on 2x crops; en dashes were measured against digits.
- QR codes: 0 of 17 decode (rqrr fails error correction every time; rxing
  fails its checksum 16 times and finds nothing once).
- Bar heights: within 5% in five of six poster runs. Run 1 was 6–15% too tall
  on the 450–700 nm bars. One pixel is 3–4% of the 700 nm bar.
- Edits: both came back right, but the image was redrawn: 42% (poster) and
  20% (menu) of the pixels outside the edited line changed by more than 8
  levels, the QR modules included.
- The committed #995 poster's bars were re-measured against the 400 nm bar
  (5.91, 3.80, 2.61, 1.87, 1.02); the guide now states that reading.

**The ferry example.** `examples/hybrid/ferry_timetable.ps` computes the
times from the departures and each stop's minutes out; `tests/hybrid.rs`
checks the painted copy against the brief typed out separately (64 strings)
and pins that the image model's `ferry_model.jpg` QR does not decode. A
`compare_ferry` specimen and sheet; `/PscatOnly` draws a flat-ink harbour.
Blind review (fresh agent, unlabelled): hybrid > pscat alone > image model
alone overall; the image model's table read slightly larger at a distance.

**Method notes for a re-run.** Score against a checklist written before
opening any image. Pass the image to `codex exec` as `--image FILE --`
before the prompt: `-i FILE "prompt"` swallows the prompt as a second image.
Measuring bars by pixel needs per-image baselines; a naive saturation scan is
fooled by the sunset painting.
