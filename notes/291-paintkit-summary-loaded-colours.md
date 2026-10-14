## paintkit catalog: @summary of pkfan, pkbroad and pktrowel names the loaded colours (issue #291, 2026-10-14)

`pkfan`, `pkbroad` and `pktrowel` take `/Colors` (loaded colours, with bare
pigment names), `/ColorMix` and `/Light`, but their `@summary` lines did not
say so, so an agent that reads only the summary would miss that the presets
can carry several colours. Each summary now has one clause naming `/Colors`,
`/ColorMix` and `/Light`. The `@param` rows already covered these keys, and
`pkdab`'s summary already mentioned `/Colors`.

Comment-only change in `lib/paintkit.ps`: three `% @summary:` lines. Every
changed line starts with `%`, so no render can change. `identity_check.sh`
has no valid `--control` for a comment-only edit, so the PR relies on the
`%`-only diff plus the full gate.

Deferred: `/Touch` is not named in these summaries. It is documented in the
`@param` rows, and adding it would make the summaries long enough to bury the
main point.
