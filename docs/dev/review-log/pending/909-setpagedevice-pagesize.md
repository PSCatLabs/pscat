## setpagedevice /PageSize sizes the page (PR #930, issue #909, 2026-10-29)

- **What changed / what to look at:** `<< /PageSize [640 400] >> setpagedevice` used to be `undefined` and left a blank page. Now it draws the program's page: the issue's red square on a 640x400 page, both when the caller gives 640x400 and when it gives no size. The two-binary identity check over all 136 examples, gallery and selftest files (178 pages) came out byte-identical.
- **Perceptual check:** "The example produces a red square on a 640×400 white page, with no PostScript error or stack leak. A generated standalone artwork can carry its intended page size without needing an interpreter-specific edit."
- **Reviewer:** PASS, round 1 (https://github.com/PSCatLabs/pscat/pull/930#issuecomment-6505136638)

| before | after |
|---|---|
| ![repro before](https://github.com/PSCatLabs/pscat/blob/f26d7030d4237b18d1cf474e3f3e907a5962a36a/docs/dev/review-log/pending/909-setpagedevice-pagesize/repro-before.png?raw=true) | ![repro after](https://github.com/PSCatLabs/pscat/blob/f26d7030d4237b18d1cf474e3f3e907a5962a36a/docs/dev/review-log/pending/909-setpagedevice-pagesize/repro-after.png?raw=true) |
| ![no page size, before](https://github.com/PSCatLabs/pscat/blob/f26d7030d4237b18d1cf474e3f3e907a5962a36a/docs/dev/review-log/pending/909-setpagedevice-pagesize/nopage-before.png?raw=true) | ![no page size, after](https://github.com/PSCatLabs/pscat/blob/f26d7030d4237b18d1cf474e3f3e907a5962a36a/docs/dev/review-log/pending/909-setpagedevice-pagesize/nopage-after.png?raw=true) |
