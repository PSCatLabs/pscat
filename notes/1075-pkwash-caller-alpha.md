## `pkwash` honours the caller's `setalpha` (issue #1075, 2026-11-11)

`pkwash` set `1 setalpha` itself inside its `gsave` and then each layer's own
alpha, so `gsave 0.6 setalpha ... pkwash grestore` was a no-op: the PNGs at
alpha 1 and 0.2 were byte-identical. It now reads `currentalpha` once at entry
(before the neutralizing `gsave`) into `pwca`, and `pwmark`, the one place a
layer, the rim and the grain set their alpha, multiplies by it.

- **Alpha 1 is unchanged.** `x * 1.0` is exact, so a wash under the default
  alpha is byte-identical (the identity check over the paintkit corpus, in the
  PR). Halving is exact too, which the test uses: caller alpha 0.5 with
  `/Alpha 0.8` equals caller alpha 1 with `/Alpha 0.4`.
- **Only the real-alpha path.** `pwca` is read when both `pwhasalpha` and
  `pkalphaok` hold. The flattened fallback (plain Ghostscript, or
  `pkalphaok` set false by hand) paints opaque colours and has no alpha to
  fade, and the forced-fallback preview must keep ignoring ambient state as
  it does for blend mode (PR #109).
- **Blend mode stays the wash's own** (`/Blend`), so `pkwash` is still not
  wrappable in the `pkwet` sense; the comment there now says alpha is the
  exception.
- **Not changed: `pkpaper`** also sets its own alpha for grain marks and
  ignores the caller's. The issue names `pkwash` only.
- Docs: the header comment, `@summary` and `/Alpha` parameter (so
  `--capabilities` says it), `docs/guides/painting.md`, `psart/references/paint.md`.
- Test: `tests/pkwash_caller_alpha.rs`.
