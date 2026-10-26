# Capabilities catalog: native operators (#754)

`pscat --capabilities` listed library procedures only, so an agent looking
for noise found the `nr*` reference procedures in `lib/noiseref.ps` and never
the native `gnoise2`, `fbm2`, ... operators it should call.

Every entry already carried `kind` (font, type3_face, palette, template,
procedure, dial); #754 adds `operator`. There is no schema version field and
the change is one more `kind` value, so nothing else moved.

Nineteen operators are listed, as hand-written `ENTRIES` rows with
`source: "systemdict"`, an empty `load`, the stack signature as `example`
(the same place a procedure keeps its stack-effect comment) and a one-line
description: the twelve noise operators, `setalpha`/`currentalpha`/
`setblendmode`/`currentblendmode`, `.sameclip`, `writereals`/`readreals`.
The two file operators have an availability string that says they need a
filesystem (absent on wasm) and, for writing, `--allow-write`.

Left out on purpose: PLRM operators (the catalog has no PLRM coverage; the
#740 `renamefile`/`deletefile`/`filenameforall` are PLRM Level 2),
`min`/`max`/`.min`/`.max` (Ghostscript compatibility) and `.permnoise2`
(internal to artkit's `noise2`).

Tests (`tests/capabilities.rs`): the payload lists `gnoise2` as an operator;
every cataloged operator is a real `operatortype` in `systemdict` (so a
library procedure shadowing the name cannot pass); each `nr*` procedure's
summary names its native operator and that operator is cataloged.

The payload grew from about 280 KB to 293 KB, well under pscat-mcp's 4 MiB
child-stream cap. The `describe_art_capabilities` tool description in
`src/mcp_server.rs` still lists only fonts, faces, palettes, templates and
procedures: a one-word follow-up (another lane's file, not touched here).
