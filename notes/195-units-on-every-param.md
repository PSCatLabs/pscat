## Units on every @param in the tag-migrated libraries (issue #195, 2026-10-13)

#141 gave every `lib/paintkit.ps` `@param` a leading `[unit]` from a closed
list. This does the same for the rest of the tag-migrated libraries: apparel,
fillkit, halftonekit, headline, hatchkit, jersey, lettering, printkit,
stipplekit, surfacekit. drykit and strokeglyphs already complied (each had its
own copy of the check); hatchkit had one compliant row (`/Mark [proc]`).

**The test is generic now.** `tests/capabilities.rs::every_migrated_parameter_states_its_unit`
walks `capabilities::migrated_files()`, so a library migrated later is covered
without touching the test. It also asserts that a file with `@param` tags had
all of them read (a file whose tags never reach the catalog fails instead of
passing empty). `drykit`'s and `strokeglyphs`'s copies of the unit check are
gone; they keep their "no `/Spread` / `/Density`" checks under new names
(`drykit_files_do_not_reuse_reserved_parameter_names`,
`strokeglyphs_does_not_reuse_reserved_parameter_names`). The paintkit-only
pins (`pkcanvas` units, deprecated aliases not advertised) stay in
`every_paintkit_parameter_has_the_issue_141_shape`.

**Vocabulary additions** (`unit_is_known`): the fixed tokens `string`, `dict`,
`array`; `array of U` for any known `U` (so `[x0 y0 x1 y1]` is `array of
points`); and `U or V` where both are known (`name or array of rgb arrays`,
`count or name`, `0..1 or proc`). No new length unit: user-space lengths are
`points`, as in paintkit. `user units` and `device units` are deliberately not
accepted (`the_unit_vocabulary_stays_closed`).

**fillkit's `/Spread`: documented exception, not a rename.** #141's rule is
`/Spread` = fraction of `/Width`. fillkit's `/Spread` is GIMP's Spread filter:
each sample is taken from a point displaced by up to `n` of the fill box
(`fkrow` adds a +-n offset to the 0..1 sample coordinate), so its unit is
`[fraction of the box]`. Reasons not to rename:
- #141's renames fixed one name with *two meanings inside one library*
  (pkwet, pktrowel, pkdab, pkcrisscross each reused a name differently from
  the others). fillkit has one meaning; the clash is across libraries.
- fillkit is a GIMP-parity set (`/Spread`, `/RGBNoise`, `/HSVNoise` are GIMP's
  names); the name is the point.
- A rename would be a lib API change needing an alias, plus checking lettering's
  `/Fill` pass-through, for no gain the unit does not already give: the
  catalog shows `[fraction of the box]` next to paintkit's `[fraction of /Width]`.
The test pins fillkit `/Spread` to `fraction of the box` and every other
migrated `/Spread` to `fraction of /Width`.

**Doc fixes found on the way.** hatchkit's `/Spacing` said "device units"; the
sweep has no transform and works in the current user space (like `/BBox` and
`/Density`), so it is now `[points]` and the header says user-space units.
jersey bundled several keys into three `@param` lines (`/NameFont /NumberFont
/CaptionFont`, `/Dominance` + `/MaxDominance`, `/Fill /Outline /CaptionFill
...`), so a single unit would have described only the first key; they are now
one row per key (jersey goes from 10 catalog rows to 17, defaults taken from the code).
The split also fixed a real catalog bug: the bundled row's trailing
`(default 6)` was parsed as the default of `/Dominance`, so the catalog said
`/Dominance` defaults to 6 when it is 1.8 (6 is `/MaxDominance`'s).

**Renders are byte-identical** (docs comments only): `scripts/identity_check.sh
--loader <lib>` for each touched library.

**Open dry-media branches.** Checked by copying the PR-branch files into `lib/` and running the new test (then removing them): both pass, and neither advertises `/Load` or `/Spread`. `lib/drycrayon.ps` (#309) and `lib/drychalk.ps`
(#235) were not edited; their `@param` units on the PR branches all fall inside
the existing vocabulary, so the generic test passes on them once they merge.
`lib/drycharcoal.ps` (#241) has no branch yet; the test will check it when it
lands.
