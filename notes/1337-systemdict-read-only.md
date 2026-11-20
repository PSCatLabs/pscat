# #1337: systemdict is read-only to a program

Ghostscript 10.07.1 (`-dNOSAFER`, and the default too):

```
$ echo 'systemdict /x 1 put' | gs -q -dNODISPLAY -dBATCH -
Error: /invalidaccess in --put--
Operand stack:
   --dict:744/1123(ro)(G)--   x   1
```

`systemdict wcheck` is `false`. The same `invalidaccess` comes from
`systemdict begin /y 2 def end`, `/add 1 store` (the name is found in
systemdict), `systemdict /add undef` and `<< /q 1 >> systemdict copy`.
`/zz 1 store` with `zz` defined nowhere binds in the current dictionary and
works. (The issue's `/x 1 systemdict exch put` is a `typecheck` in gs: the
operands are in the wrong order for `put`.)

## What changed

- `put`, `def` (name and non-name keys, after `systemdict begin`), `store`,
  `undef` and dictionary `copy` refuse systemdict with `invalidaccess`, before
  anything is journalled. The refused `put` and `def` leave their operands, as
  gs does; the operand and dictionary-stack depths are pinned against gs in
  `tests/systemdict_readonly.rs`.
- `def` pays one length compare on the usual path (`def_targets_systemdict`);
  `put` skips its no-ledger fast path for systemdict.
- `systemdict wcheck` is `false`; the other dictionaries still have no access
  attribute (#983).
- Lint: a refused store of a plain value over an operator is kept in
  `Interp::refused_system_stores` (filled only on the refusal path, one entry
  per operator name), and `[shadowed-operator]` reports it from there. Wording:
  `/sub was stored as an integer into systemdict when the program reached line
  2, which would replace the operator sub ...; systemdict is read-only`. A
  program that catches the error with `stopped` still gets the note, and so
  does the failed run. The old end-state comparison with a fresh systemdict
  is gone: it could no longer find anything. `put` now has a line too.

## Who relied on it

`lib/`, `examples/` and `gallery/` have no write into systemdict (searched for
`systemdict ... put/def/begin`, and `store` of an operator's name). All 173
`.ps` files there ran with the base and the head binary with identical exit
codes, output and PNG bytes.

No `wcheck` in `lib/`, `examples/` or `gallery/` either.

## The one divergence this adds

pscat's systemdict has 29 names gs's lacks: `.kernel*`, `.permnoise2`,
`.sameclip`, `fbm2/3`, `gnoise2/2d/3`, `vfbm2/3`, `vnoise2/3`, `ridged2/3`,
`noiseseed`, `kernelarray(s)`, `kernelcompile`, `kernelimage`, `setalpha`,
`currentalpha`, `setblendmode`, `currentblendmode`, `readreals`, `writereals`.
`/fbm2 1 store` binds in userdict in gs (no such name there) and now raises
`invalidaccess` in pscat; `/fbm2 1 def` in userdict is unaffected, and so is
any name in artkit/paintkit (they `def`). A program that `store`s over one of
these names is the only one that behaves differently from gs, and `store`
over an extension operator's name is the shadowing mistake the lint exists for.

The lint wording for a refused store changed (the `/NAME was redefined as`
prefix is kept only for the userdict kind). Nothing in `pscat-mcp` or
`tests/mcp.rs` matches the systemdict wording; they match the userdict one.
