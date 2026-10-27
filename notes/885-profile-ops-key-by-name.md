## `--profile-ops` keys operator rows by name (issue #885, 2026-10-26)

Follow-up to #881. The operator table used the operator's function address as
its key, so operators that share one function (`readonly`/`noaccess`/
`executeonly`, `rcheck`/`wcheck`) had their time merged under one
`a/b/c` row, and an operator object built outside systemdict's own table
(`filenameforall`'s `copy`/`exec`) would have had no label.

### What changed

- `Profiler::record_op` takes the operator's `&'static str` name. The map key
  is the string's (address, length), hashed with a small multiply hash
  (`AddrHasher`): SipHash on the pair cost about 12% more CPU on `fib` under
  `--profile-ops`, the multiply hash costs nothing measurable. `report` merges
  rows whose text is equal, in case one name was registered through two
  string literals.
- The timed loop carries the name in a side field, `Interp::op_name`, written
  only by `proc_step::<true>`. `ProcStep::Op` still holds just the function,
  so the plain loop, which shares `proc_step`, is unchanged (#737 measured a
  wider enum as costly). The two other routes (`execute_name_timed`, an
  operator object met by `next_item`) read `op.name` directly.
- No systemdict walk any more (`set_ops` takes no argument) and no
  `(unnamed operator)` label.

`exec` of an operator object (`arr /executeonly load exec`) calls the
function directly and is not timed, before or after this change; so the
`next_item` arm that passes `op.name` has no PostScript route to test.

### Visible effect

Rows are one per registered name, so an alias that used to share a row with
another name has its own: `readonly`, `noaccess` and `executeonly` are three
rows. The JSON shape is unchanged (`name`, `calls`, `time_ms`), and a probe
over systemdict found no operator whose registered name differs from its key,
so no single-name row changed its label.

### Overhead (CPU seconds, minimum / median, interleaved, `bench_compare.sh`)

"control" is the base binary run again: the noise floor.

| file | flags | base | head | control |
|---|---|---|---|---|
| bound `fib 27` | `--profile-ops` | 0.22 / 0.22 | 0.22 / 0.22 | 0.22 / 0.22 |
| unbound `fib 27` | `--profile-ops` | 0.24 / 0.24 | 0.23 / 0.23 | 0.24 / 0.24 |
| `fib 30` | `--profile-ops` | 1.34 / 1.36 | 1.26 / 1.37 | 1.35 / 1.38 |
| `gallery/chalkboard_lecture.ps` | `--profile-ops` | 13.98 / 14.16 | 14.51 / 14.80 | 14.60 / 15.23 |
| `fib 30` | none | 0.31 / 0.32 | 0.32 / 0.32 | 0.31 / 0.33 |
| bound `fib 30` | none | 0.15 / 0.16 | 0.16 / 0.16 | 0.16 / 0.16 |
| `fib 30` | `--profile` | 0.50 / 0.51 | 0.50 / 0.50 | 0.50 / 0.51 |

Every head result is inside the control's spread: no measurable change. With
SipHash on the (address, length) key, `fib 27` under `--profile-ops` was about
12% slower (0.23 to 0.26 s), which is why the hasher is custom.
