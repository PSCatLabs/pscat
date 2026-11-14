## `resourcestatus` for the Font category (issue #1214, 2026-11-14)

`/Name /Font resourcestatus` returned `false` for every face that was not
already in `FontDirectory`, except the twelve Liberation-backed built-ins:
the check was `font::builtin_index`, which knows nothing of the catalog, the
font packs or the embedded standard-35 faces (`/NewCenturySchlbk-Roman`,
`/Palatino-Roman`, `/Symbol`), all of which `findfont` loads. A program
could not test for a pack face before setting text (a missing `cjk-thai`
pack silently becomes Helvetica).

**Fix.** `font::resolve`'s first three steps (builtin, catalog or wasm
registry, embedded) are now `font::locate`, and `resolve` is `locate` plus
the missing-pack stand-in. `resourcestatus` asks `locate` for a name that is
not yet in `FontDirectory`, so it is true exactly when `findfont` would load
the named face, and false for a name nothing provides and for a pack face
whose pack is missing (where `findfont` substitutes and notes). Resolution
itself is unchanged, so pscat-mcp's pack behaviour is untouched. `locate`
loads the face (as `findfont` does), so a corrupt catalog file reports
false, like the substitution it would cause.

**Face cache (Codex round 1).** `locate` loads the face, and a family name
(`/NotoSansKR` for the file `NotoSansKR-Regular`) missed the cache in
`catalog_fid` (probed under the requested stem, registered under the file's
stem), so every query re-read and leaked the ~10 MB face; `findfont` hid it
behind its `FontDirectory` entry, `resourcestatus` and `has_glyph` did not.
`load_catalog_face` now returns the already-registered face with the file's
own stem before registering a new one. The scan still decides which file
wins, so resolution results are unchanged; only duplicate registrations
(and their FIDs) go away. `tests/font_packs.rs` pins it by FID: 25 status
queries before `findfont` leave the same FID as none (it was 62 against 12).

**Status and size follow gs 10.07.1.** Not loaded, available: `2 -1 true`.
Defined by `definefont`/`defineresource`: `0 -1 true`. Size was a fiction
of 0 for every category; it is -1 ("unknown") now, as gs prints it. An
unknown name: `false`. `undefineresource` of a built-in leaves it `2 -1`.

**Deviation.** gs reports a face `findfont` has loaded from disk as status 1
with the file size; pscat has no record of how a `FontDirectory` entry got
there, so it reports `0 -1`. A program that tests only for the boolean sees
no difference.

**Not done.** `resourceforall` does not exist in pscat, so there is no
listing to keep consistent; `findresource /Font` still substitutes for an
unknown name, as gs does. Other categories keep their generic per-category
registry (gs also reports ColorSpace, Encoding and other built-in instances
that pscat does not model).

Tests: `tests/level2.rs` (standard-35 and unknown names, definefont,
undefineresource, a program compared against gs where gs is installed) and
`tests/font_packs.rs` (a bare install against a temp `PSCAT_HOME`, then an
installed pack).
