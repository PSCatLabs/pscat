## The chalk problem list on the public API (issue #1157, 2026-11-12)

`pscat::chalkboard::ChalkError::problems() -> &[Problem]` gives a library
user the list `pscat chalk` and `chalkboard_math` report since #1080: the
same problems, order and places. `chalkboard::Problem` is public and
`#[non_exhaustive]`: public fields `token`, `line`, `column`, `offset`, and
`code()` / `message()` (precomputed, so `chalk.rs` is untouched).

The list needs the program's printed output, which `chalkboard::explain(&PsError)` never sees,
so the facade also gets `Driver::explain(printed, err)`, a thin wrapper of
the hidden `chalk::Driver::explain`. A caller runs with
`interp.capture_stdout()` and passes `interp.captured_stdout()`.
`driver()` delimiter problems already carry the list with no run.

`problems()` is empty for config, empty-input and render errors, and for the
position-less error `chalkboard::explain` alone returns, as the MCP tool
gives no `problems` array for those either.

`ChalkError` changed from a tuple struct to a struct with a private field
(it was already opaque), holding the converted `Vec<Problem>`. The hidden
`chalk::ChalkError::Problems` is unchanged and `#[non_exhaustive]`, for
pscat-mcp through 0.7.0. Tests: `the_library_problem_list_is_the_cli_list`
in `tests/chalk.rs` and the doctests on `ChalkError::problems`.
