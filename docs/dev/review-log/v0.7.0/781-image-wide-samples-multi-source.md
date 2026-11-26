## image: 16-bit samples, colorimage multi-source, /MultipleDataSources (PR #787, issue #781, 2026-10-25)

- **What changed / what to look at:** three image features that used to be refused now draw: 16-bit (and 12-bit) samples, `colorimage` with `multi true`, `/MultipleDataSources true`. The only changed pixels are the rightmost cell of each page of the test drivers (rows are pages). Wide samples are reduced to 8 bits in PNG, SVG and PDF alike.
- **Perceptual check:** the issue has none. Its intent: "Implement them in the PNG, SVG and PDF outputs ... review the cells ... that start drawing."
- **Reviewer:** PASS, round 1 (https://github.com/PSCatLabs/pscat/pull/787#issuecomment-6406839526). Known, not from this change: CMYK tints differ slightly from Ghostscript, and Ghostscript paints the left cell of an ImageType 3 image black.

| before | after |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/f837f52bb65a126e6aa644b2ef73674d73c0a054/docs/dev/review-log/pending/781-image-wide-samples-multi-source/formats-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/f837f52bb65a126e6aa644b2ef73674d73c0a054/docs/dev/review-log/pending/781-image-wide-samples-multi-source/formats-after.png?raw=true) |
| ![before](https://github.com/PSCatLabs/pscat/blob/f837f52bb65a126e6aa644b2ef73674d73c0a054/docs/dev/review-log/pending/781-image-wide-samples-multi-source/sources-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/f837f52bb65a126e6aa644b2ef73674d73c0a054/docs/dev/review-log/pending/781-image-wide-samples-multi-source/sources-after.png?raw=true) |

New test sheet `image_planes.ps` (ours, then Ghostscript at 4x):

| ours | Ghostscript |
|---|---|
| ![ours](https://github.com/PSCatLabs/pscat/blob/f837f52bb65a126e6aa644b2ef73674d73c0a054/docs/dev/review-log/pending/781-image-wide-samples-multi-source/planes-after.png?raw=true) | ![gs](https://github.com/PSCatLabs/pscat/blob/f837f52bb65a126e6aa644b2ef73674d73c0a054/docs/dev/review-log/pending/781-image-wide-samples-multi-source/planes-gs.png?raw=true) |
