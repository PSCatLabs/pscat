## Operator identity is by registered name (issue #881, 2026-10-26)

Found during #864. `eq` on two operator objects compared their function
pointers with `fn_addr_eq`. Rust guarantees neither that one function has
one address nor that two functions have two, so the answer could differ
between the test binary and the release binary, and identical-code folding
could make two different operators equal.

### It was already wrong, in every build

Several operators share one function body on purpose, so an address
comparison called them the same operator:

- `readonly`, `noaccess`, `executeonly` (`access_noop`)
- `rcheck`, `wcheck` (`access_check`)
- `max`/`.max` and `min`/`.min` were also one function each.

Ghostscript 10.07.1 (`gs -dNODISPLAY -q`) says all of these are different
operators. This is the deterministic failing test, which no build profile
can hide.

### Semantics, from Ghostscript

| program | gs |
|---|---|
| `/add load /add load eq` | true |
| `/add load /sub load eq` / `ne` | false / true |
| `/readonly load /noaccess load eq` | false |
| `/rcheck load /wcheck load eq` | false |
| `/add load /add eq`, `(add) eq`, `1 eq`, `null eq`, `{add} eq` | false |
| `/myadd /add load def /myadd load /add load eq` | true |
| `/add load cvx /add load cvlit eq` | true (attributes never matter) |
| `<< /add load 1 >> dup /add load 2 put length` | 1 |
| `<< /readonly load 1 /noaccess load 2 >> length` | 2 |

### What changed

An operator is its systemdict name. Every operator object is built by
`ops::op` (name and function together) or, in `filenameforall`, with the
name it is registered under, so equal names mean one operator. A name that
is registered twice (`translate`, `scale`, `rotate`, `handleerror`) is
replaced in systemdict by the later install, and nothing keeps the earlier
object.

- `eq`/`ne` (`objects_equal` in `src/ops/logic.rs`) compare `Operator::name`.
- A dictionary key for an operator is `ExoticKey::OperatorName(name)`
  (`src/object.rs`). Before, it was the function address, so `readonly` and
  `noaccess` collided as one key.
- Fusion matching already worked this way (#864, `runs_operator`).

`tests/operator_identity.rs` pins the table above, and runs under both
`cargo test` and `cargo test --profile ci`.

### Left alone: `--profile-ops`

`--profile-ops` still groups operator time by function address
(`Interp::enable_profile_ops`, `src/profile.rs`). The hot loop sees bare
`fn` pointers, and `set_ops` joins aliased names with `/` in the report.
Both sides read the pointer from the same `Operator` object, so the key is
consistent. The gap: an operator object built at another site (the
`copy`/`exec` pair in `filenameforall`) could carry an address different
from systemdict's, and then its time would be unlabelled. That affects a
profile label only, never program behaviour.
