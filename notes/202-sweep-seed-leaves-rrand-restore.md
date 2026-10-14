## `--sweep-seed` leaves an `rrand` restore alone (issue #202, 2026-10-14)

Alpine Lake replays its scene in the lake by logging `rrand` at each painting
step and restoring it with `srand` (`lkdo`). `--sweep-seed` forced every
`srand` to the swept value, so each restore replaced the logged stream with the
forced seed and the reflection stopped matching the scene above. The same
save-with-`rrand`/restore-with-`srand` idiom is in artkit `scatter`, hatchkit,
fillkit (incl. `fkunwind`), lettering, surfacekit and paintkit's knife
(`rrand tplrr ne { tplrr srand } if`); all were silently reset under a sweep.

**Choice: option (a), in `ops/arith.rs`, memory-free.** While a seed override
is armed, `rrand` returns `state | 2^40`; `srand` of a value in
`[2^40, 2^40 + 2^31)` is a restore (state = low 31 bits), left alone and not
counted as the override firing (so the "never called srand" sweep warning is
still truthful). Everything else, literal seeds and seeds a library derives by
arithmetic from `rrand` (`rrand 7919 mul srand`), is still forced, so a sweep
still varies the composition. With no override `rrand` returns the plain state:
default renders are unchanged (proved with `identity_check.sh --base-bin
--head-bin`).

Why not the others. (b) a new save/restore operator would need a new name,
a `where` fallback in every library so Ghostscript still runs them, and edits
to each restore site (including paintkit's). (a) fixes every site at once with
no library change. (c) leaves seven libraries and a gallery piece broken under
a sweep. A set of "seen rrand states" was rejected as unbounded memory.

Trade-offs. Under an override `rrand` prints a tagged number (about 1.1e12);
nothing in lib/ uses the printed value other than to hand it back. A seed in
the tag window passed to `srand` is treated as a restore: 2^31 values of 2^64.

Stale text left alone: `lib/paintkit.ps` (`pwsrand` rationale, ~line 5795)
still says an `rrand` restore is reset by a sweep; it is outside this PR's
files (doc-comment work in progress there), tracked as a follow-up.
