## halftonekit: a line or cross screen fades smoothly at tiny tones (PR #1415, issue #1408, 2026-11-24)

- **What changed / what to look at:** A `/line` or `/cross` halftone at a tone below about 0.0125 (72 dpi, defaults) used to draw every rule at the stroker's 0.1 px floor, so a tone leaving 0 popped a full screen in. Now the rule is shortened instead, so ink tracks tone. Look at the ramp (tones 0 to 0.12, line row on top, cross row below) and the gallery crop: ordinary tones are unchanged.
- **Perceptual check:** the issue has no formal check. Its stated expectation for the repro is "an almost blank page" at `/Tone 0.001`, and "a tone that fades smoothly to zero".
- **Reviewer:** PASS, round 1 (https://github.com/PSCatLabs/pscat/pull/1415#issuecomment-7147764490)

| before | after |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/a957801df7518d757efd2543676f4a449b826af6/docs/dev/review-log/pending/1408-halftonekit-faint-tones/repro-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/a957801df7518d757efd2543676f4a449b826af6/docs/dev/review-log/pending/1408-halftonekit-faint-tones/repro-after.png?raw=true) |
| ![before](https://github.com/PSCatLabs/pscat/blob/a957801df7518d757efd2543676f4a449b826af6/docs/dev/review-log/pending/1408-halftonekit-faint-tones/ramp-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/a957801df7518d757efd2543676f4a449b826af6/docs/dev/review-log/pending/1408-halftonekit-faint-tones/ramp-after.png?raw=true) |
| ![before](https://github.com/PSCatLabs/pscat/blob/a957801df7518d757efd2543676f4a449b826af6/docs/dev/review-log/pending/1408-halftonekit-faint-tones/gallery-crop-before.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/a957801df7518d757efd2543676f4a449b826af6/docs/dev/review-log/pending/1408-halftonekit-faint-tones/gallery-crop-after.png?raw=true) |

Difference between the two gallery crops, amplified 12x (dark = changed; only the tips of the fading ridges):

![diff](https://github.com/PSCatLabs/pscat/blob/a957801df7518d757efd2543676f4a449b826af6/docs/dev/review-log/pending/1408-halftonekit-faint-tones/gallery-crop-diff-x12.png?raw=true)
