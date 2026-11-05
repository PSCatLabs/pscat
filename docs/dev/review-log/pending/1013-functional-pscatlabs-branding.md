## functional examples: PSCatLabs, not ArtColony, in the showcase headers (PR #1017, issue #1013, 2026-11-05)

- **What changed / what to look at:** the small header line on the functional showcase art changes from "ARTCOLONY" to "PSCATLABS" (poster and its colourways: `PSCATLABS PRESENTS / GENERATIVE SESSIONS No. 001`; type and press studies: `PSCATLABS / FUNCTIONAL TYPE STUDY 02/03`). This affects 12 PNGs under `docs/functional-postscript/assets/`; a pixel diff confines every change to that header line.
- **Perceptual check:** the issue has none of its own. Its intent: "replace the text with PSCatLabs wording ... Re-render the affected assets", keeping the layout.
- **Reviewer:** PASS, round 1 (https://github.com/PSCatLabs/pscat/pull/1017#issuecomment-6665174144).

| before | after |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/c4e817499624f6ce9f59b312b285e69ffced0bca/docs/functional-postscript/assets/ultraviolet.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/275142a76eacf751c8b5bd1b5442d5a038476efa/docs/functional-postscript/assets/ultraviolet.png?raw=true) |
| ![before](https://github.com/PSCatLabs/pscat/blob/c4e817499624f6ce9f59b312b285e69ffced0bca/docs/functional-postscript/assets/type-nocturne.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/275142a76eacf751c8b5bd1b5442d5a038476efa/docs/functional-postscript/assets/type-nocturne.png?raw=true) |
| ![before](https://github.com/PSCatLabs/pscat/blob/c4e817499624f6ce9f59b312b285e69ffced0bca/docs/functional-postscript/assets/press-inspected.png?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/275142a76eacf751c8b5bd1b5442d5a038476efa/docs/functional-postscript/assets/press-inspected.png?raw=true) |
