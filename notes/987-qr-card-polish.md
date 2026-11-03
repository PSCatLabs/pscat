# #987: business card polish, white stock and the front lockup

Two refinements from the perceptual review of `gallery/pscat_card.ps` (#974,
PR #979). Part of #966.

**Stock: white wins.** The brief (`docs/dev/design/QR_CODE.md`, "Business
card target") said "warm white" and also "white edges/no bleed". The card
keeps the paper unprinted and the brief now says white. Printing a warm tint
has two forms, and both are worse than choosing the stock:

- a full tint needs a bleed and a printer's imposition, which the brief
  avoids on purpose;
- a tint held inside the 9 pt inset leaves a white frame, and a trim that
  drifts by 1/32 in (normal for short runs) makes that frame visibly
  uneven.

A tint under the QR code would also lower its contrast. The cream used in
the logo (0.984 0.965 0.918) has a luma near 246, so `tests/qr_card.rs`'s
light-module check (255 within 4) would have to loosen, and the issue rules
that out. Anyone who wants warmth prints on a natural or cream card; the
header comment says so. The tests are unchanged.

**Front: one lockup.** The first front set a 64 pt tile and a 38 pt wordmark
whose top sat well below the tile's, and the wordmark ended at x 210 against
a rule that runs to 234, so the top right read empty. The front now aligns
three edges, measured from the faces' outlines (`charpath pathbbox`, in the
source's comment):

- the tile is 70 pt, at (18, 52);
- Courier-Bold "pscat" at 45 pt from x 99: its ink runs from 102.1 (over the
  tagline's first stroke) to 234, the rule's right end, and from baseline 92
  the t tops out at 122, level with the tile's top;
- the tagline is 13 pt Palatino Italic (was 11), its last baseline on the
  tile's bottom edge.

The block sits 22 pt under the top trim and 19 pt above the rule. The back
is byte-identical (300 dpi PNG compared), so the QR is untouched.

**Still.** Regenerated with the header's `/CardProof` command at 300 dpi.
