## `Interp::with_page_scaled` refuses an unusable scale (issue #1133, 2026-11-11)

A NaN or 0 scale built a 1x1 canvas (`(points * scale).round() as u32` is 0,
then `.max(1)`), so a host that passed a bad scale got a blank render instead
of a failure; a negative scale mirrored the page. It now returns `None` for a
scale that is not finite and above zero, the contract it already had for an
over-size page.

- **Guard.** A private `usable_scale(scale)` helper in `src/interp.rs` and a
  three-line early return, so `Interp::new_scaled` (held PR #1126, which has the
  same check inline) can call the helper later without either change moving.
  It was not made `pub`: that would be new API.
- **Callers audited.** Every in-tree caller already passes `dpi / 72.0` with
  `dpi` clamped to 9..1200 (`--dpi`, `subcmd::dpi`, chalk's `layout.dpi`,
  the MCP `dpi` argument), so none relied on the old behaviour. The
  wasm entry points `ps_begin_scaled` and `ps_begin_vector` already returned -1
  for such a scale before calling in, and `web/pscat.js` turns -1 into a
  `RangeError` naming the page size and scale; they are unchanged. The
  library's `Option` is the only place a Rust host sees it.
- Test: `tests/with_page_scaled.rs`.
