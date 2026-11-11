# 776: tracked image drivers for the identity check

No tracked `.ps` file called `image`, `colorimage` or `imagemask`, so
`scripts/identity_check.sh` proved nothing for a change to the image code
(#730 and #739 each fell back on hand-written programs).

## What was added

Four drivers in `selftest/drivers/` (strict-linted by `scripts/selftest.sh`
and `tests/selftest.rs` like the others, 260x50 pages, 4 pages each, all data
generated inline by a small byte-pattern procedure, no fixtures):

- `image_formats.ps`: dict `image` at 1, 2, 4, 8 bits in Gray, RGB, CMYK and
  Indexed (a page each), plus a 16-bit cell.
- `image_sources.ps`: DataSource as a string, a procedure (one row per call),
  an inline `currentfile` ASCIIHexDecode filter, string-backed ASCIIHex,
  ASCII85 and RunLength filters; Level 1 `image`; `colorimage` single source
  at 1, 4 and 8 bits, Gray/RGB/CMYK.
- `image_masks.ps`: `imagemask` operand and dict forms, both polarities,
  `/Interpolate true` on a mask; ImageType 3 with Decode `[1 0]`, `[0 1]`,
  none, a coarser mask, inline filter sources, a rotated mask matrix, CMYK
  and Indexed images.
- `image_matrix.ps`: `/Interpolate` absent, false and true (magnified,
  minified, one axis only, Indexed, CMYK, ImageType 3); rotated, skewed and
  mirrored CTMs; rotated, skewed and positive-y ImageMatrix; a clip.

## Making the identity check pick them up

The check picks files by `--loader` (a file that loads a lib) or `--files`.
These drivers load no lib, so no loader would find them. They carry
`%%IdentityCorpus: always` in the header, and in two-binary mode
(`--base-bin/--head-bin`, the standard form for a `src/` change) the script
adds every tracked driver with that line. A single shared binary leaves them
out: with one binary only the files can differ, and these never do.
`tests/identity_check.rs` covers both.

PNG output only is compared (as everywhere in the check), so the SVG and PDF
writers' image paths are not covered by it.

## Not renderable yet, pinned as probes

The engine refuses four things today; each is wrapped in a `stopped`
probe on a page with live images, so when someone implements one the cell
starts to draw and the check reports DIFFERS (review it with
`--expect-diff`). The last one is a bare dict that only pins today's
`limitcheck`; it cannot start to draw and needs a well-formed dict when type 1
lands:

- `BitsPerComponent 16` (`rangecheck`) in `image_formats.ps`, all four spaces.
- `colorimage` with `multi true`, separate component sources (`limitcheck`).
- `/MultipleDataSources true` in the dict form (`typecheck`).
- `ImageType 3` with `InterleaveType 1` (`limitcheck`).

## Bite test

Two one-line mutants of `src/image.rs`, each built in a scratch worktree of the
branch and run as the base side of `identity_check.sh --base-bin/--head-bin`
against the branch binary (only `examples/sierpinski.ps` plus the always-on
drivers, a negative control that rotates a gray image):

- `cell_at`: `.floor()` to `.round()` on `ix`. All 16 pages of the four
  drivers DIFFER; `sierpinski.ps` (no image) stays identical; control differs.
- `smooth_rgb`: the bilinear sample position `f - 0.5` to `f - 0.4`. Only the
  four pages of `image_matrix.ps` DIFFER (the only driver that sets
  `/Interpolate true` on colour data, its ImageType 3 cell included); the
  control does not use `/Interpolate`, so it reports CONTROL FAILED, as it should.
- Two identical binaries: 17 of 17 identical (the renders are deterministic).
