## Evening Stream: the clouds' wet edge asks the sky at each stroke (issue #491, 2026-10-14)

#403's `pkwet /PerFill` asks the backdrop at each loaded-colour fill instead of once
per pass. Of the gallery's paintkit pieces only Evening Stream's clouds qualify
(`pkbroad` with `/Colors` under a `pkwet`; Alpine Lake's wet marks are sprays, which
`/PerFill` cannot reach). Until now their `/Under` was `evcunder`, the sky at the
cloud's centre: one colour per cloud, so a cloud's top halo was mixed toward a
lighter sky than the one behind it and its foot toward a darker one. Now it is
`{ evskyat }` with `/PerFill true`, so each stroke is mixed toward the sky at its
own place. Only `evcwet` (the shared options) and the two `pkwet` calls in
`evcloud` changed, in `gallery/evening_stream.ps`; no `lib/` change.

![before (left) and after (right), full](491-before-after-full.png)
![before (left) and after (right), the four clouds](491-cloud-compare.png)
![the top edge of the middle-right cloud, before above and after below](491-cloud-top-edge.png)

Close-ups, before left and after right, top to bottom: the left cloud, the middle
one, the right-hand big one and the small one.

- **What it does.** The gain is on the **top halo**: before, a pinkish mauve band
  (the centre sky's colour) sat on the darker violet sky above the body; now it
  settles into it and is barely a band (third image). The foot is a smaller change:
  the grey-mauve fringe under the cream underside is gone, replaced by a pinker, a
  little lighter one that follows the glow. It is a modest change overall, easy to
  miss at the full picture's size; it reads at the 2x close-ups.
- **The cream-peach is unchanged.** `evcbody` and `evclit0` are still toned by
  `evcunder` (kept for that), and the mean of the brightest 15% of pixels in each
  cloud's box is the same before and after: 169 (210,161,136), 169 (209,162,137),
  178 (223,170,142), 175 (218,165,141), left, right big, middle, small right.
- **Measured, honestly.** A 2x render with `evcloud` stubbed out gives the actual
  painted sky. In the outer 5 pt rim of each cloud's own halo, top half and bottom half
  separately, mean colour distance to that cloudless sky (0 to 255, mean of the four
  clouds): top 14.0 before, 13.0 after; bottom 15.3 before, 19.4 after. So the top
  rim is closer to the sky (all but the small cloud, whose top is 11.1 before, 12.1
  after), and the bottom rim is *further* by this number: the fringe is now lighter
  than the sky there (signed luma +10 before, +16 after), where before it was a darker
  grey one that happened to sit nearer in value. I do not read the bottom number as
  a loss of look (the grey band is the thing #433 wanted rid of), but it is not a
  blend into the sky gradient either: what is left there is the cream's own
  contribution, not the backdrop's. Single runs vary by a few points; the pattern
  held over the four clouds except where noted.
- **`/FillLength`: not used.** A cloud's strokes are small circles (radius 3% of the
  width), already one short fill each. `/FillLength 10` moved 50 000 pixels by at most
  14 of 255 and the rim numbers by 0.03 (top 12.97, bottom 19.36), for 2.4 times the
  asks of the procedure.
- **Asks** (count only, not timed): 53 808 calls of `evskyat` across the four clouds
  (128 968 with `/FillLength 10`). `evskyat` draws no random number and only sets its
  own scratch names (`evqx`, `evqy`, `evt`, `evd`), which nothing in `evcloud` reads
  between asks.
- **Tried.** `/Soft` 0.5, 0.75 and 0.9 with the per-fill sky: the rim numbers move
  around without a pattern (bottom 26.1, 18.8, 16.6; top 13.9, 15.3, 14.0), and a
  higher value fattens the halo; `/Soft 0.6`, #433's tuning, kept.
- **Pixels.** 65 110 pixels differ against the still on main (2x); 0 outside the four
  cloud boxes at a 16 pt margin (11 pixels, at most 16 of 255 apart, on the right
  edge of the right-hand big cloud are 1 pt past the 15 pt box of #433, the wet
  halo's reach). Two 2x renders are byte-identical; `--lint` is clean; Ghostscript
  runs the file without errors (correctness only). The 2x still is the site's
  landing hero, so that changes too.
- **Unchanged and left alone.** The brush-stamp rings and polygonal wet edges of
  #384/#433 (#519).
