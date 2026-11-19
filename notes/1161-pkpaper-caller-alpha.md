## `pkpaper` honours the caller's `setalpha` (issue #1161, 2026-11-18)

Since #1075 `pkwash` multiplies each layer's alpha by the caller's `setalpha`;
`pkpaper` still reset to `1 setalpha /Normal setblendmode`, so
`gsave 0.5 setalpha ... grestore` faded a wash but left the paper ground and
its grain at full strength under it. One rule now covers both (the issue's
recommended option).

- `pkpaper` reads `currentalpha` once at entry into `pvca` (same guard as
  `pkwash`: `pwhasalpha pkalphaok and`, else 1). The ground fill is painted at
  `pvca`, and each grain mark at `/Alpha` times `pvca`.
- **Alpha 1 is unchanged.** `x * 1.0` is exact, so a ground made under the
  default alpha is byte-identical (identity check over the paintkit corpus in
  the PR).
- **Blend stays the call's own.** The ground is still `/Normal` whatever the
  caller set; `/Blend` is still a grain option.
- **Fallback path** (plain Ghostscript, `pkalphaok` false) paints opaque
  flattened tones and has no alpha to fade, as for `pkwash`.
- Docs: header comment, `@summary` and `/Alpha` parameter,
  `docs/guides/painting.md`. Test: `tests/pkpaper_caller_alpha.rs`.
