## noaccess strings in cvs, cvrs, search, anchorsearch and token (issue #1362, 2026-11-27)

Follow-up to #1321 (`notes/1321-executable-string-gs-parity.md`), from Codex round 2 on #1349.
The issue thought these operators rebuilt their result and lost the flag. gs 10.07.1 does not
return a flagged view at all: it refuses the call (`invalidaccess`) and leaves every operand.
So the fix is a refusal, and no result view needs the flag. Probed with
`gs -q -dNODISPLAY -dNOSAFER -dBATCH -dNOPAUSE -c` (all pinned in `tests/exec_strings.rs`, which
runs gs too when installed):

| program | gs |
|---|---|
| `7 (x) noaccess cvs` | `invalidaccess`, 2 operands left |
| `12345 (x) noaccess cvs`, `(abcdef) noaccess 2 string cvs` | `invalidaccess` (before `rangecheck`) |
| `(7) noaccess 10 string cvs` (the string being converted) | `invalidaccess` |
| `255 16 (x) noaccess cvrs` | `invalidaccess` (before `rangecheck`), 3 left |
| `999 40 (x) noaccess cvrs`, `255 1 (x) noaccess cvrs` | `rangecheck` (radix first) |
| `(a) 10 (xx) noaccess cvrs` | `invalidaccess` (the number is checked after access) |
| `(7) noaccess (z) search`, `(abc) (b) noaccess search` | `invalidaccess`, found or not |
| `(abc) noaccess (a) anchorsearch`, `(abc) (a) noaccess anchorsearch` | `invalidaccess` |
| `(7 8) noaccess token`, `(7 8) noaccess cvx token` | `invalidaccess`, 1 left |
| `(%stdin) (r) file noaccess 20 string cvs` | works (a file is not read) |

Operand order (Codex round 1, probed against gs over every combination of `1`, `(a)` and
`(a) noaccess`): a short stack is `stackunderflow` first (`1 search`, `(a) noaccess search`), then each
operand is checked for its type and then its access in turn. `search` checks its string first and
the seek second, `anchorsearch` the seek first: `1 (a) noaccess search` is `typecheck` and
`(a) noaccess 1 search` is `invalidaccess`, and `anchorsearch` is the other way round. `cvs`,
`cvrs` and `token` already matched gs on the same grid for access (`tests/exec_strings.rs`,
`search_and_anchorsearch_check_their_operands_in_gs_order`).

Implementation: `cvs` and `cvrs` (`src/ops/convert.rs`) and `pop_string`/`token` (`src/ops/data.rs`)
raise `InvalidAccess` before any allocation or limit charge, so a refused call costs nothing against
`--max-steps`/`--max-time`/`--max-memory` (`tests/cvs_limits.rs`); the operand ledger puts the
operands back. A `noaccess` file as `cvs`'s operand is still accepted.

Not done (proposed): `cvi`, `cvr`, `cvn` of a `noaccess` string (gs: `invalidaccess`), a
`readonly` string as the `cvs` buffer (gs: `invalidaccess`; pscat models `readonly` for arrays
only), `rcheck`/`wcheck` and `get`/`getinterval` on a `noaccess` string.
