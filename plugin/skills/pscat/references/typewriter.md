# Typewritten pages and released documents

Read this for typed letters, memos and pages, or an aged "released document"
look. Back to [SKILL.md](../SKILL.md).

## Typewritten pages (text → PNG/PDF)

`-o` takes `.png`, `.svg` or `.pdf` (format by extension; anything else is an
error). SVG pages are large (~40 KB a character): prefer PNG or PDF.

```sh
pscat typewrite letter.txt -o letter.png
pscat typewrite --text "Dear Sir," -o short.png
pscat typewrite --reflow --spacing 2 --page-numbers bottom -o draft.pdf draft.txt
pscat typewrite --memo-to "All Staff" --memo-from "Records" \
    --memo-subject "Carbon copies" --memo-date "Oct 14, 1962" --height fit -o memo.png body.txt
```

The /Typewriter font: per-key quirks, carriage drift, per-strike
variation, ribbon ink; word wrap, tab stops, `--spacing`, `--indent`, a
memo header. Markup in the text: `<u>underlined</u>`, `<x>crossed
out</x>`, `<o>old|new</o>`, `<r>redacted</r>`. Same `--machine`/`--seed`,
same page. `--height fit` sizes the page to the text; longer text makes
more pages. Non-ASCII becomes ASCII or `?`. `--ps out.ps` writes a driver
that also runs in Ghostscript (`gs -dNOSAFER`). From PostScript:
`(lib/typewriter_outlines.ps) run (lib/typewriter.ps) run` then
`<< /Text (...) /Memo << /To (...) >> /Seed 3 >> tw-type showpage`.

## Released documents

A "released document" look (aged paper, scan skew, photocopy
generations, stamps, redaction bars, margin notes, staples, punch holes):
`lib/released.ps` after artkit and surfacekit (and handscript for notes):
`{ << /Text (...<r>name</r>...) /Redactor { rel-redact } >> tw-type } <<
/Age 0.6 /Generation 3 /Stamps [ << /Lines [ (RECEIVED) (14 MAR 1964) ] /X
462 /Y 700 >> ] >> rel-page showpage`. Every effect is optional, off at 0,
and seeded. Invent stamps and names; never reproduce real seals.
