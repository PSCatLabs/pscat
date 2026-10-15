# 437 -- recount the font catalog figures

Docs and site text only. No renders change.

## What was on disk (origin/main 14b4983)

- `fonts/catalog/`: 67 outline font files (`.ttf`/`.otf`, no `.ttc`) in 51
  family directories: 6 TeX Gyre families (21 files), the URW symbols
  directory (2 files), and 44 other families (44 files).
- `pscat --fonts` with `PSCAT_ROOT` set to the worktree lists the reachable
  names, which include aliases, so it is not a face count.
- `fonts/`: the 12 bundled Liberation faces (not in the catalog).
- `lib/fonts/*.ps`: 7 Type 3 display faces. Adding `lib/handscript.ps` and
  `lib/typewriter.ps` makes 9 Type 3 faces.

## Changed

| File | Old | New |
| --- | --- | --- |
| `site/fonts.html` | a 62-file runtime catalog | a 67-file runtime catalog |
| `site/guides.html` | the 61-face runtime catalog | the 67-face runtime catalog |
| `docs/guides/fonts.md` | catalog of 61 outline faces | catalog of 67 outline faces |
| `docs/guides/fonts.md` | 61 libre outline faces | 67 libre outline faces |
| `docs/guides/README.md` | the 61-face catalog | the 67-face catalog |
| `docs/guides/getting-started.md` | the 61-face catalog | the 67-face catalog |

## Checked and left alone

- "38 curated families" (site shelf and guide) and "40 more display and
  text families" (site intro) match the layout: 44 non-standard directories
  minus NotoSans JP/KR/Thai and NanumBrushScript = 40, which is the 38-family
  shelf plus Chewy and BagelFatOne.
- "nine Type3 faces" (site) is right: 7 in `lib/fonts/` plus handscript and
  typewriter.
- `.claude/skills/psart/SKILL.md` says "seven Type 3 display faces": the
  seven `lib/fonts/` display faces, so correct as written.
- `fonts/catalog/README.md` has no numeric face or file count to fix.
- `notes/412-*.md` and `docs/archive/` are historical; not edited.
- "35 families" in `gallery/manifest.json` describes the standard-35
  specimen, not the catalog.
