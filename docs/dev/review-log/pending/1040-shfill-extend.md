## shfill: honour /Extend in PNG, SVG and PDF (PR #1239, issue #1040, 2026-11-16)

- **What changed / what to look at:** `shfill` honours `/Extend` per end in PNG, SVG and PDF, and a missing key now paints neither end, as in gs. The two corpus files that relied on the old padding now ask for it. `examples/gradients.ps` is byte-identical; `gallery/varsity_backs.ps` differs only at anti-aliased clip edges.
- **Perceptual check:** "a reviewer confirms each changed render looks intended, not broken."
- **Reviewer:** PASS, round 2 (https://github.com/PSCatLabs/pscat/pull/1239#issuecomment-6928581789). Round 1 failed because the spec-default renders of the two corpus files read as broken; fixed by having them request the extension. polish: the corpus pair shows little of /Extend false; the per-combination tests carry that proof.

| before | after | gs |
|---|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/4e142edfa03073dff4cf77eaab523a91a5786628/docs/dev/review-log/pending/1040-shfill-extend/varsity_backs-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/4e142edfa03073dff4cf77eaab523a91a5786628/gallery/renders/varsity_backs.png?raw=true) | ![gs](https://github.com/PSCatLabs/pscat/blob/4e142edfa03073dff4cf77eaab523a91a5786628/docs/dev/review-log/pending/1040-shfill-extend/varsity_backs-gs.png?raw=true) |
