# #590: *The Man from 2026*, a never-released TOP SECRET memo

`gallery/time_traveler_memo.ps`, the typewriter showcase beside *Released in
Part*. A memorandum for the record from an invented Office of Field Inquiry: an
interviewing officer's deadpan account of a man found in a culvert in 1962 who
says he came from 2026 and built his machine by pair-programming with "Claude
Code". The only real names are in his quoted testimony.

## References looked at

- *Memorandum to Holders of NIE 11-8-62*, 8 August 1962 (Wikimedia Commons, a
  US government work): TOP SECRET centred at the head and foot of the sheet, a
  TS control number at the top right, a boxed downgrading group at the foot.
- JFK Assassination Records 104-10297-10106 (May 1961, archives.gov): a file
  memo's layout, the reference number and date under the text, and the
  "Distribution:" list of copies with a hand tick.
- The JCS memo of 11 December 1944 in the Naval Aide's files (DPLA via Commons):
  TOP SECRET typed at the head and the foot.

## Decisions

- **Original, not copy.** The brief is a document that was never released, so
  `/Generation 0`: no copier haze, no release stamps, no case numbers, no
  redactions. It is "Copy 1 of 3", the ribbon original kept by the registry: a
  carbon copy could not carry red typed banners (the advisor's catch).
- **Red typed banners.** Typed after `tw-type` with `tw-set` on the same frame
  (`/Strike 4`, red `/Ink`, `/Bleed 0.85 /BleedInk` black `/BleedSide /above`),
  then the black ink restored with `/Bleed 0`. The first try had `/Bleed 0.2` on
  the banners and 0.12 of red under the body: a pixel diff against `/Bleed 0`
  changed 105 pixels in the whole 2x still, because the `/above` boundary starts
  800 glyph units up and moves in by at most `/Bleed` x 540, so 0.2 never
  reaches a cap, and 0.12 from `/below` (-360 + 65) never reaches a descender.
  0.85 gives a few banner letters a black top (the E of SECRET most), 0.6 almost
  none. The body has no bleed, as NBS C431 says most ribbons show none.
- **One legal page (612 x 1008), not two letter pages.** The gallery takes one
  still per piece, and the 2x still is rendered with `2 2 scale` prepended,
  which a second page (after `showpage`) does not keep. Legal-size paper was
  common for typed government records then; the text is 68 lines.
- **Machine 12**: its only clogged key is `e`. Machine 23 (the first try) clogged
  the capital `O`, which turned TOP into T●P and MEMORANDUM into MEM●RANDUM.
- **No real marking but TOP SECRET** (which the issue asks for). The real
  "GROUP 1 / Excluded from automatic downgrading and declassification" box is
  replaced by an invented "OFI REG. 7 / NOT SUBJECT TO / AUTOMATIC REGRADING".
- **The habit joke** (the typist starts to type 1962 for the year he came from)
  is an x-out followed by 2026, not `<o>1962|2026</o>`: four overstruck digits
  read as "2022".
- Handscript has no slash and draws a poor 4, so the routing date is "8-27" and
  the margin note ends with initials only.

## Evenness (maintainer review)

The first render (`/Wear 1.3 /MachineWear 0.6 /Ribbon 0.6`) swung too far from
letter to letter: very dark strikes beside very light ones, hard to read. Light
strikes come from the strike layer (pressure sd 0.12 and 2% very light, scaled by
/Wear x /StrikeWear) and the key weights (/Wear x /MachineWear), and a dry ribbon
opens the weave more on a light strike. The 2% rate of very light strikes does not
scale with the wear (only their depth does), so a few light letters stay at any
setting. `notes/590-evenness-board.png` sets paragraph 3 at 2x for the first
render and three candidates; the piece now uses F, `/Wear 1.0 /MachineWear 0.4
/StrikeWear 0.5 /Ribbon 0.75`. The line layer is at /Wear 1.0, so baselines and
carriage returns still wander, and the page still reads as typed.

## #547

The renders predate #547 (copy-body erosion), which had no PR when they were
made. #547 is described as acting on a copy's body (`tCopyC` above 0); a probe
printed `TWState /tCopies` as 0 right after this piece's `tw-type`, so it is
expected not to change this page, but that is to be checked when it lands.

## Commands

    { printf '2 2 scale\n'; cat gallery/time_traveler_memo.ps; } > /tmp/hi.ps
    pscat --page 1224x2016 --png gallery/renders/time_traveler_memo.png /tmp/hi.ps
    gs -q -dNOSAFER -dBATCH -dNOPAUSE -sDEVICE=png16m -r72 -g612x1008 \
       -sOutputFile=gs.png gallery/time_traveler_memo.ps </dev/null    # ok
