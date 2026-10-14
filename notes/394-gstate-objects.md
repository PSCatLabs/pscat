# #394: Level 2 gstate objects, an exact clip capture (2026-10-14)

## Decision

The issue offered two routes to an exact capture of the clip in force:
an exact `clippath` (the intersection of the chain, with its rule), or
Level 2 `gstate` objects. This slice takes `gstate`:

- An exact `clippath` needs polygon boolean operations on curves (the
  intersection of an even-odd ring and a rotated square is a new path,
  not any of the inputs). pscat has none, and the answer would still be
  a path, which `clip` would re-rasterise.
- A gstate holds the graphics state itself. The clip inside it is the
  live chain's `Rc` (the mask, and each link's path and rule), so
  `setgstate` puts back the very mask that was in force. Exact by
  construction, nested and `eoclip` links included, at no raster cost.
- gs has the same operators, so programs that use them run in both.

## What shipped

- `Value::GState(Rc<RefCell<GraphicsState>>)` (`src/object.rs`): type
  `gstatetype`, `cvs` gives `--nostringval--`, `==` prints `-gstate-`,
  `eq` and dict keys go by identity.
- `gstate`, `currentgstate`, `setgstate` (`src/ops/graphics.rs`), and
  `gstate1 gstate2 copy` (`src/ops/data.rs`). `setgstate` leaves the
  gsave stack alone (PLRM), so a `grestore` after it still pops what the
  matching `gsave` pushed.
- `currentgstate` and `copy` overwrite a gstate's contents; both are
  journaled (`Interp::journal_gstate`, `JEntry::GState`), so `restore`
  undoes them, as in gs.
- Not modelled: local/global VM for gstates, `invalidaccess` on a
  read-only gstate.

Every behaviour above was pinned against gs 10.07.1 first (`type`, `==`,
`cvs`, `eq`, path and CTM restored by `setgstate`, `currentgstate`
returning its operand, `copy`, the typecheck error names, and the
`save`/`restore` rollback). pscat printed the same 22 lines.

## Evidence

- `tests/gstate.rs`
  `setgstate_brings_back_a_nested_eoclip_exactly`: an even-odd ring
  nested in a rotated square, captured with `gstate`, left, replaced by
  another clip, then put back with `setgstate`: the fill is
  byte-identical to the fill under the clips themselves (and the same
  via `currentgstate`). Controls: replaying through `clippath ... clip`
  (a mask trace) differs at the edges, and a nonzero ring fills the
  hole.
- `ghostscript_agrees_on_a_replayed_nested_eoclip`: gs's own replay is
  byte-identical to its direct clip; pscat's replay is 5.4 grey levels
  per 10x10 block from gs (gs clips without anti-aliasing, so edges
  differ pixel for pixel whatever the route), against 36 for the
  nonzero control.
- Exporters: a page that fills under the gstate, then under a different
  clip, then `setgstate`s the old clip back and fills again. `--pdf`
  rasterised by gs is byte-identical to gs rendering the program
  directly. `--svg` reuses the same `clipPath` ids for the restored
  chain, with `clip-rule="evenodd"` on the ring.
- Cost guard (one run, not a benchmark): 10,000 each of `currentgstate`,
  `gstate` and `setgstate` under a nested clip with a 200-segment
  current path run in about 0.2 s total, start-up included. The cost is
  the path copy; the mask is shared.
- Identity: see the PR (`scripts/identity_check.sh --base-bin/--head-bin`).

## Not done

- `clippath` stays as it was (#120's rect fast path or a mask trace
  under nested clips, the rule dropped).
- drykit keeping a plain `clip` inside a `dkbatch` (#444). A gstate per
  `dkstroke` would give every record a fresh object, so `dkZsame`'s
  identity grouping would never match; the follow-up describes capturing
  one gstate per clip change instead.
