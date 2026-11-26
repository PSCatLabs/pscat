## chalk: a bracket after \\ is row spacing only when it holds a length (PR #945, issue #936, 2026-10-29)

- **What changed / what to look at:** a row that starts with `[` right after a `\\` row break (`a \\ [x,y] = 0`, a commutator) is now drawn in full; before, the bracket group was dropped silently as TeX's row-spacing option. A real spacing option (`\\[4pt]`) still renders byte-identically.
- **Perceptual check:** "`'a \\ [x,y] = 0'` renders both rows in full." (the issue's Acceptance)
- **Reviewer:** PASS, round 1 (https://github.com/PSCatLabs/pscat/pull/945). polish: the faint ghost formulas in the background moved (background texture only).

| acceptance before | acceptance after |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/61b9b6fe9e4304bbfbe435b2f7e572863c433d24/docs/dev/review-log/pending/936-chalk-row-bracket/acceptance-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/61b9b6fe9e4304bbfbe435b2f7e572863c433d24/docs/dev/review-log/pending/936-chalk-row-bracket/acceptance-after.png?raw=true) |

| commutator before | commutator after |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/61b9b6fe9e4304bbfbe435b2f7e572863c433d24/docs/dev/review-log/pending/936-chalk-row-bracket/commutator-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/61b9b6fe9e4304bbfbe435b2f7e572863c433d24/docs/dev/review-log/pending/936-chalk-row-bracket/commutator-after.png?raw=true) |
