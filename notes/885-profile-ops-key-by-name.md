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

### Visible effect

Rows are one per registered name, so an alias that used to share a row with
another name has its own: `readonly`, `noaccess` and `executeonly` are three
rows. The JSON shape is unchanged (`name`, `calls`, `time_ms`), and a probe
over systemdict found no operator whose registered name differs from its key,
so no single-name row changed its label.

### Overhead (CPU seconds, best of 9/5/7, interleaved, `bench_compare.sh`)

| file | flags | base | head |
|---|---|---|---|
| bound `fib 27` | `--profile-ops` | 0.220 | 0.220 |
| unbound `fib 27` | `--profile-ops` | 0.240 | 0.230 |
| `fib 30` | `--profile-ops` | 1.340 | 1.260 |
| `fib 30` | none | 0.310 | 0.320 |
| `fib 30` | `--profile` | 0.500 | 0.500 |

With SipHash on the (address, length) key the same runs were about 12% slower
(0.23 to 0.26 s), which is why the hasher is custom.
