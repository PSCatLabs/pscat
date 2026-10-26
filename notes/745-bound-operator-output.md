# #745: bound `==`, `pstack`, `stack`; one repr writer

**Problem.** After #703 bounded the `--pstack-on-error` dump, `==`, `pstack`
and `stack` still printed without a bound, so a program printing its own
1M-element array filled MCP's 4 MiB cap and got `output_too_large`.
`repr_depth` and `write_bounded` also duplicated the repr logic.

**Fix.**
- `Object::write_repr` (src/object.rs) is the only code that spells an
  object in `==` syntax. `repr()` calls it with `UNBOUNDED_LIMITS` (no size
  cap, depth 32), `repr_bounded(limits)` with any limits. The old
  `repr_depth` and `write_bounded` are gone.
- `OPERATOR_LIMITS` for `==`/`pstack`/`stack` per operand: depth 32, 1024
  elements per array, 4096 bytes per string or name (cut before escaping),
  16 KiB total. Generous: a few hundred reals or a page of text print in
  full. `1000000 array ==` prints about 5 KB ending `... truncated]`.
- `pstack`/`stack` print top first and stop after `OPERATOR_STACK_BYTES`
  (32 KiB, half of `STDOUT_CAPTURE_LIMIT` so a wasm/captured run keeps the
  whole dump) with one `... truncated` line for the items below.
- `stack` uses `Object::text_bounded` (strings and names cut at 4096 bytes).
- The undefined name in `Interp::error_report` and `handleerror` is cut to
  512 bytes (`cap_text`, same ` ... truncated` marker; it has no `; ` or
  ` ]%%`, so the MCP error-line parser still splits cleanly).

**Unchanged on purpose.** `cvs` and `=` digits and text (`format_real` still
gives shortest round-trip digits; fractal_clouds' text cache goes through
`readreals`/`writereals`, #735, not `==`). `=` and `print` are not bounded:
they are a program's deliberate output.

**One visible difference.** `repr()` of arrays nested deeper than 32 used to
end in `...`; it now ends in `... truncated`, the marker the bounded paths
always used. Nothing but a pathological structure reaches it.

**Pinned.** `tests/operator_output_bounds.rs` (literal bytes for ordinary
values through the operators, plus the bounds), `tests/pstack_dump.rs`,
`capped_offending_name_still_parses` (src/mcp_server.rs).
