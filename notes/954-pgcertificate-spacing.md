## pgcertificate spacing laid out by measured ink (issue #954, 2026-10-30)

The branding PR #953 uses `examples/template_certificate.ps` as its
document specimen, and its perceptual review failed on three defects in
`pgcertificate` (`lib/pagekit.ps`): the title's cap top sat 0.02h under
the inner rule; the signer names floated 0.03h above their rules while
the captions under them touched; and a two-line citation left a band of
about 0.3h empty above the signatures, because every element sat at a
fixed fraction of h whatever the copy.

What changed (interface unchanged: same keys, same leftover contract):

- **Title** cap top 0.07h below the inner rule.
- **Signature block**, from the bottom: caption baseline 0.06h above the
  inner rule, rule 0.022h above the caption's cap top, name baseline
  0.016h above the rule. Names are centred over their rules (they were
  left-aligned at the rule's start while the captions were centred) and
  fitted to the rule's width, 0.28w.
- **The middle** has two groups, the caption + awardee cluster and the
  citation, and three gaps: title to cluster, cluster to citation,
  citation to the signer names. Each gap runs from the upper element's
  baseline to the lower one's cap top (balancing by ink, not by leading
  boxes, which leave a leading minus a cap height of air above a
  tfblock's first line). Every gap starts at 0.05h, and the space left
  over is split equally. The citation's line count comes from `tfwrap`,
  which wraps with the same `tflinebreak` as `tfflow`, so the two agree.
  The count is clamped to what fits at the minimum gaps, and `tfblock`
  gets a box that holds exactly that many baselines, so the rest comes
  back as leftover. An empty citation collapses its slot, and the
  cluster sits midway between title and names.
- **Fitted sizes first.** The title and awardee shrink to fit their width,
  using `pgzfitmax`'s rule (shrink only, never enlarge). The fitted size
  is now worked out before anything is placed, so a long, shrunk name
  gets a slot sized to its real ink instead of floating in its nominal
  one.
- **Sizes for README width, stepping down for long copy.** At about 380 px
  wide the old 0.02h captions were about 6 px tall, so "This certifies
  that" went from 0.024h to 0.032h, the names from 0.02h to 0.03h, and the
  captions to 0.024h italic. The citation takes the largest of four steps
  (size / leading / measure: 0.033h/0.05h/0.66w, 0.029h/0.044h/0.72w,
  0.026h/0.039h/0.76w, 0.023h/0.035h/0.78w) whose wrapped lines all fit
  at the minimum gaps. The specimen's two sentences get the first step,
  about 60 characters a line, which also avoids its old one-word last
  line. A long citation shrinks before any of it is cut.

**Capacity.** The first version of this fix used one size only. That
held 408 characters of a test body at 532x500, against the old layout's
684, so the first perceptual review failed it for cutting a citation
that used to fit. With the steps, a 532x500 certificate holds 899
characters before anything goes to the leftover. The minimum gap went
from 0.05h to 0.06h after the same review found an overflowing citation
crowding the awardee and the names.

Cap height and descender are Times' own proportions (0.66 and 0.22 of
the size), not measured per glyph. That is close enough for spacing, and
it keeps the template on the standard 14 fonts as before.

Tests (`tests/pagekit.rs`, the `certificate_*` set) scan rows of ink on
a 72 dpi render: the title's padding under the inner rule, the name sitting
3 to 12 px above its rule with the caption at least 7 px below, the
largest blank band no more than 1.6 times the next for a short, a longer
and an empty citation, and a long-everything certificate with an
overflowing citation keeping all ink inside its rectangle, out of the
border band and clear of the title. All four failed on the old layout.
Only `pgcertificate` changed: an identity check over every file that
loads `lib/pagekit.ps` differs only in `examples/template_certificate.ps`.

Not done here: signer names still shrink on one line, so a very long
presenter name gets small; a two-line name option would be its own
change.
