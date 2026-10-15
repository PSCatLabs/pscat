# 544: capabilities.rs module docs brought in line with `parameters`

Comment-only change in `src/capabilities.rs`. No behaviour or renders change.

- The "Migrated" bullet no longer says `lib/paintkit.ps` is the only migrated
  file: any `lib/*.ps` carrying `% @` tags is migrated.
- The `parameters` paragraph now describes the generic rule (dict-driven APIs
  with named keys), not the old list of templates plus `hs-*`/`scatter`. Rows
  come from `% @param:` tags or hand-written `Param` lists, as in
  docs/dev/CAPABILITIES.md (#545).
- The `catalog()` doc comment drops "paintkit so far".
- The opening paragraph's list of procedure sources now names the tagged kits
  generically.
