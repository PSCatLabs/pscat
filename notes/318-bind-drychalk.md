## Bind drychalk; drycrayon and the older kits measured and left alone (issue #318, 2026-10-13)

#307 bound artkit, paintkit and drykit (`} bind def` on every procedure)
and left the rest for a follow-up. This measures each of them. Only
drychalk gains, so only drychalk is bound; `dcscratch` still pays.

All numbers are gs 10.07.1, `-dNOSAFER -sDEVICE=png16m -r144`, CPU `user`
seconds, the best of 3, on a shared machine (load average 6 to 7), so trust
the ratios rather than the second digit. A run that exits nonzero or
writes to stderr is reported as an error, never as a time.

### drychalk and drycrayon

| file | before | bound |
|---|---|---|
| `examples/drychalk_specimen.ps` | 2.77 | 2.26 |
| `examples/drycrayon_specimen.ps` | 2.94 | 2.91 |
| `examples/drycrayon_motion_demo.ps` | 2.08 | 2.05 |

drychalk gains 18%: its own procedures (`dcchalk`, the board painters,
`dcmin`/`dcmax`) run hot. drycrayon gains 1%, which is inside the noise:
the specimen's time is in drykit's strokes, which #307 already bound, not
in drycrayon's own procedures. Binding it would only add a file to the
identity run, so it stays `} def`. (Its procedures are correct as written;
`tests/libbind.rs` does not load it.)

### Does `dcscratch` still pay?

Yes. The hypothesis was that, with the lookups bound, the private
dictionary on top of the dictionary stack would cost more per operator
lookup than it saves on the scratch names. Measured on the bound
drychalk, `examples/drychalk_specimen.ps`, by really removing it (the
`begin`/`end` pairs in `dcchalk` and `dcboard`, the `end`/`begin` around
the `/Ghost` call, and the `dcLevels` dial moved to userdict), not by
defining `/dcscratch userdict def`, which would put userdict on the stack
twice:

| | CPU s (best of 3, run twice) |
|---|---|
| with `dcscratch` | 2.22, 2.25 |
| without, names in userdict | 3.08, 2.73 |

About 20% faster with it. The drychalk and drykit names the specimen
defines while it runs (a couple of hundred) are enough to hurt userdict's
hash table, and with bound procedures few operator lookups remain to be
slowed by the private dictionary. Left as it was, with a comment saying
so. (The difference from drykit and paintkit in #307, where the private
dictionary lost, is that those bind their procedures *and* define far
fewer names at run time per call.)

### The other kits

Triage in the base tree, best of 3, user seconds, on the example or
gallery piece that loads each library:

| library | file | gs |
|---|---|---|
| hatchkit | `examples/hatching.ps` | 0.17 |
| stipplekit | `examples/stippling.ps` | 0.20 |
| surfacekit | `examples/surfacekit.ps` | 0.14 |
| printkit | `examples/printkit.ps` | 0.15 |
| pagekit | `examples/template_card.ps`, `template_poster.ps` | 0.06 |
| dataviz, graph | `examples/dataviz.ps`, `graphing.ps` | 0.13 |
| etching | `examples/etching_demo.ps` | 0.18 |
| halftonekit | `examples/halftone.ps`, `gallery/out_of_register.ps` | 0.16, 0.33 |
| strokeglyphs, handscript | `examples/strokeglyphs.ps`, `handwriting.ps` | 0.08, 0.07 |
| fillkit, lettering, headline, jersey, apparel | examples and gallery | not measurable |
| hangul | `examples/hangul_handscript.ps` | not measurable |

Everything measurable is at or under 0.33 s, most of it gs startup, so
binding cannot move it. fillkit, lettering, headline, jersey and apparel
(and hangul) stop in gs on a missing font (`lettering-font-not-found`):
they use pscat's bundled fonts, so there is no gs time to improve. A
hot-path win in pscat is a different question and was not measured
(pscat looks operators up without the dictionary-stack cost). These are
left as `} def` rather than bound on a guess.

### Audit (drychalk)

- No name drychalk defines is in `systemdict` in gs or pscat, so no
  procedure is bound to the wrong thing by a forward reference.
- Nothing tracked redefines an operator drychalk uses after it loads.
  The only operator-named `def`s are `gallery/ring_of_type.ps` (`/stop`)
  and `gallery/hundred_lines.ps` (`/ln`) as variables, neither loading
  drychalk, and a prototype under `docs/` that defines `setcolor` after
  loading artkit and paintkit only.
- No drychalk procedure is meant to be overridden. Its `/Ghost` procedure
  is a caller's argument, not a library name.
- `dcnewscratch` (`{ 1200 dict }`) is bound too, since `dict` is an
  operator; `dcscratch` itself is a dictionary, not a procedure.

### Identity (pscat)

`scripts/identity_check.sh`, base tree at the merge-base (a detached
worktree on the rebase point), one binary, a control that redefines `gt`
after drychalk loads and calls `dcmin` (differs between the trees, as it
must):
- `--loader lib/drychalk.ps`: 1 of 1 identical (`examples/drychalk_specimen.ps`;
  `tests/drychalk.rs` covers the rest);
- `--loader lib/artkit.ps --gallery-still`: 82 of 82 identical across 66
  files, and `gallery/alpine_lake.ps` equals its committed still in both
  trees.

### Tests

`tests/libbind.rs` now loads drychalk after the three it already covered;
its deliberately unbound control still proves the walk can tell, and a
temporarily unbound `dcmin` makes both the pscat and the gs test fail.

### Left over

- `lib/drycharcoal.ps` belongs to open PR #315 and is not bound: a
  follow-up once it merges (issue filed).
- A pscat-side measurement of the font-using kits, if anyone wants to
  know whether bind helps there.
