## pagekit: pgcertificate spacing, signature block and title padding (PR #956, issue #954, 2026-10-30)

- **What changed / what to look at:** `pgcertificate` lays out its middle by measured ink. The title clears the inner rule by 0.07h. Signer names sit centred just above their rules, with the captions a clear gap below. Three equal gaps replace the empty band above the signatures, and the citation steps down in size before any of it is cut. Look at the README-width render, and at the long-citation cell (bottom left) of the edge cases.
- **Perceptual check:** the issue has no section with that name; its acceptance criteria: "Title, citation, signer names, rules and captions have a clear, balanced hierarchy and no collisions or cramped frame margins." "Certificate is legible and visually credible both at native resolution and at ~380px README display width."
- **Reviewer:** PASS, round 2 (https://github.com/PSCatLabs/pscat/pull/956#issuecomment-6533710658). Round 1 FAILED it (https://github.com/PSCatLabs/pscat/pull/956#issuecomment-6533710368): a one-size citation cut a long body that used to fit, and crowded the name and signatures at minimum gaps. Fixed with size steps and a larger minimum gap.

| specimen before (612x580 at 144 dpi) | after |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/a69837c41cf8b55c51c8141b338f83f5fa010565/docs/dev/review-log/pending/954-pgcertificate-spacing/certificate-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/a69837c41cf8b55c51c8141b338f83f5fa010565/docs/dev/review-log/pending/954-pgcertificate-spacing/certificate-after.png?raw=true) |

| README width (380 px) before | after |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/a69837c41cf8b55c51c8141b338f83f5fa010565/docs/dev/review-log/pending/954-pgcertificate-spacing/certificate-380-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/a69837c41cf8b55c51c8141b338f83f5fa010565/docs/dev/review-log/pending/954-pgcertificate-spacing/certificate-380-after.png?raw=true) |

| edge cases before | after |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/a69837c41cf8b55c51c8141b338f83f5fa010565/docs/dev/review-log/pending/954-pgcertificate-spacing/edge-cases-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/a69837c41cf8b55c51c8141b338f83f5fa010565/docs/dev/review-log/pending/954-pgcertificate-spacing/edge-cases-after.png?raw=true) |
