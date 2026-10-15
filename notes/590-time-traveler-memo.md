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
  (`/Strike 4`, red `/Ink`, `/Bleed 0.2` of black from `/above`), then the black
  ink restored. The body has `/Bleed 0.12` of red from `/below`; at these values
  the bleed is rare and small, as NBS C431 says most ribbons show none.
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

## #547

Renders do not depend on #547 (copy-body erosion): it acts on `/Copies` above 0,
and a `/Generation 0` page types with `/Copies 0`.

## Commands

    { printf '2 2 scale\n'; cat gallery/time_traveler_memo.ps; } > /tmp/hi.ps
    pscat --page 1224x2016 --png gallery/renders/time_traveler_memo.png /tmp/hi.ps
    gs -q -dNOSAFER -dBATCH -dNOPAUSE -sDEVICE=png16m -r72 -g612x1008 \
       -sOutputFile=gs.png gallery/time_traveler_memo.ps </dev/null    # ok
