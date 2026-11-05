## Hybrid, reversed: pscat sets the layout, the image model renders it, pscat sets the exact layer (issue #1001, 2026-11-05)

**What shipped.** `examples/hybrid/alpine_trail.ps`, a 2:3 travel poster (576
x 864 pt) in three stages from one program: `/Stage /Reference` draws a flat
colour block-in at known coordinates (`alpine_trail_ref.png`, 1024 x 1536 px
at 128 dpi, committed exactly as sent); the image model turned it into a
photograph (`alpine_trail_photo.jpg`, frozen); the default stage places that
photograph at exactly 128 dpi with no crop and lays the exact layer on at
the reference's coordinates: summit and col labels on leader lines ending on
the summits, a dashed route along the program's own trail and ridge, the hut
label, and in the foreground shade the layout left empty an elevation
profile, stats computed from it (ascent, DIN 33466 walking time), bus times
and a QR code. `/Season /Autumn` puts the same layer on a second photograph
from the same reference; `/PscatOnly true` paints paintkit over the block-in.
`compare_alpine_trail.ps` draws its own comparison and process panels (it does
not share #995's `compare_panel.ps`, which #998 was editing at the same time).
Guide section "pscat sets the layout, the image model renders it", a row in
the when-to-use decision table (guide and site page), a site section on
`site/hybrid.html`, a gallery piece and two specimens.

**Measured layout fidelity** (OpenAI's image model, Codex CLI 0.166, 2026-11-05;
pinned by `tests/hybrid_reverse.rs` on the committed files). Ridge: mean 1.2
px (summer) and 1.1 px (autumn) over 1024 columns, 95% within 4 px, max 7 px.
Shade edge: median 4 and 5 px. Hut roof: 3.8 px and 14.1 px left (the autumn
hut grew). Big outlines hold to about a pixel; small objects drift more, so
the exact layer anchors to big edges and points at small ones with leaders.

**Reference format.** Flat colour with the light in it (1.2 px) beat the same
shapes as black outlines (4 px, systematically high). The model keeps crude
shapes too: a first, rougher block-in came back as geometric pyramids with a
hard shadow band, so the block-in has to be drawn as the picture should be.
Depth maps and paintkit renders as references were not tried.

**The comparison, honestly.** The image model alone, asked for the whole
poster from the same brief (one attempt, no picking), spelled all 26 strings
right, arrows and en dash included, as in #995. Its chart points are up to 0.1
km and 30 m off and its QR code does not decode (rqrr finds it and fails its
error correction; rxing does not read it). The case for the reversed hybrid is
the QR code, exact data, a composition the program owns and can reuse for a
second photograph, and edits without regeneration; not spelling.

**Gotchas.** `codex exec -i FILE "prompt"` takes the prompt as a second image
(`-i` is multi-valued); write `--image=FILE`. Codex's image skill rewrites the
instruction into a "sketch-to-render" prompt with its own position estimates;
all prompts are in `examples/hybrid/README.md`. Names `meadow`, `m`, `white`,
`last`, `i`, `k` collide with artkit/paintkit globals: the program avoids them.
