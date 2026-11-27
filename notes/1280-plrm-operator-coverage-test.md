## PLRM operator coverage test (issue #1280, 2026-11-27)

First bullet of #1280 only: a test that checks `systemdict` against the PLRM operator list and
keeps a checked-in allowlist of the gaps. `languagelevel` still reports 2 (it moves to 3 when no
`missing` line is left, and that waits on the owner), and the corpus sweep is a follow-up.

**The list.** `tests/data/plrm_operators.txt` holds the 384 operator entries of the PLRM third
edition, section 8.2 "Operator Details" (the 8.1 "Operator Summary" is a subset: it leaves out
the Display PostScript names and the procedure-set members, so it cannot be the universe).
`scripts/plrm_operator_index.py` derives it from the Adobe PDF (sha256 in its header) by font:
8.2 entry names are 9.8 pt bold Myriad in the left margin, 8.1 operator names are 8.7 pt bold
Myriad under 11.3 pt section headings. It drops the 28 entries annotated `(error)`. The
script needs PyMuPDF and is not run by the build or the tests.

**Ghostscript cross-check** (10.07.1, `systemdict {pop ==} forall`). 354 of the 384 names are in
gs. The 30 that gs lacks are all PLRM entries that live in a procedure set (CIDInit,
BitmapFontInit, Trapping, ColorRendering), in `userdict` (`UserObjects`) or in `serverdict`
(`exitserver`); the PLRM decides, and they are recorded in the gap list, not dropped. The 187
non-dot names gs has and the PLRM does not (switches like `NOPAUSE`, the PDF interpreter, `rsearch`)
are outside the list.

**Today.** 292 of 384 are defined. 55 are `missing` (46 tracked by #1218, #1267-#1272; 9 had no issue
when this landed: `countexecstack`, `execstack`, `dictstack`, `statusdict`, `GlobalFontDirectory`,
`SharedFontDirectory`, `scheck`, `serverdict`, `internaldict`) and 37 are `omitted` with a reason: the CIDInit/BitmapFontInit/Trapping/
ColorRendering procedure-set members, `UserObjects`, and the interactive-executive names
(`executive`, `echo`, `prompt`, `start`, which the PLRM says are undefined in products without an
executive).

**How it fails.** A PLRM operator that is neither defined nor listed prints `+ name: newly missing`.
A listed operator that is now defined prints `- name: ... delete its line`. Both name the file to
edit. `exitserver` is probed in `serverdict`, where the PLRM and gs put it (`HOME_DICT`).
