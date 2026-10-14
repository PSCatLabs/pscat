## def and load take any non-null key (issue #338, 2026-10-14)

`def` and `load` used a name-only `pop_key` helper, so `1 (one) def` raised
`typecheck` where Ghostscript (and the PLRM) accept any non-null key. `put`,
`get`, `known`, `where`, `store` and `undef` already took every key through
`Dict::classify_key`, so only these two operators were wrong.

What changed:

- `def` (src/ops/dict.rs): pops the key as an `Object`. A name key calls
  `define_name` exactly as before (now matching on a borrowed `&key.value`,
  no `Rc` clone). Any other key calls the new `Interp::define_key`, which
  journals the dict and calls `Dict::put_obj`, so the PLRM key rules come
  for free: a whole-number real is the same key as the integer (`2.0` and
  `2`), a string becomes a name, null is `typecheck`, and `1.5` stays its
  own key.
- `load`: name keys use `load_id` as before; other keys walk the dict stack
  with `Interp::load_key` (`get_obj` per dict). A miss reports
  `undefined` with the key's text (`99 load` -> `Undefined("99")`).
- `define_key` and `load_key` are `#[inline(never)]`, like
  `Dict::put_other`, so the name fast path from #323 does not grow.
- `pop_key` is gone (its only callers were these two).

Verification:

- `tests/eval.rs`: `def_*` tests cover the issue repro, boolean and real
  keys, integral-real normalisation both ways, string -> name, null
  typecheck, load miss text, dict-stack walk and shadowing, and
  save/restore undoing a non-name `def`. Expected values were cross-checked
  against Ghostscript 10.07.1.
- Hot path: `0 1 3000000 { /x exch def x pop /y 1 def y pop } for`, release
  builds of the base commit and this branch (separate target dirs), user
  CPU seconds, five runs each: base 0.44-0.45, branch 0.41-0.43. No slower.
- Byte identity: all 107 tracked `examples/*.ps`, `gallery/*.ps` and
  `selftest/drivers/*.ps` rendered to PNG by both release binaries (the
  base commit's and this branch's; page from `%%BoundingBox`, 72 dpi, 144
  for gallery) and compared with `cmp`: 107 identical, all exit 0. Note
  `scripts/identity_check.sh` renders both trees with ONE binary, so it
  cannot see a `src/` change; it was not used for this proof. The committed
  `gallery/renders/alpine_lake.png` also equals the branch binary's render.

Not done: nothing deferred.

Also: tests/selftest.rs's pressure-guard probe (`row_5_and_10`) had relied on `def` rejecting the leaked number key; it now fails later with `undefined`, same point (the unguarded call still raises, not silent).

Committed stills: of the 27 `gallery/renders/*.png`, 23 equal this harness's
render byte for byte. The other four (`hortus`, `hundred_lines`,
`keepin_it_real`, `varsity_backs`) do not, but the base-commit binary's
render of each is byte-identical to the branch binary's, so the mismatch
predates this change (those stills were made with settings other than this
harness's 144 dpi / `%%BoundingBox` page).
