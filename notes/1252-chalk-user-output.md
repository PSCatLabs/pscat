# #1252: `chalkboard::user_output`

`pscat-mcp` stripped the `CKPROBLEM` check lines from the chalkboard program's
printed output by hand before returning `program_output`. The library now does
it in one place: `chalkboard::user_output(printed: &[u8]) -> Cow<[u8]>` drops
every line that starts with `PROBLEM_LINE_PREFIX` (and its `\n` or `\r\n`),
keeps every other byte, and borrows when there is nothing to drop. It uses the
same line-start test `Driver::problems` parses with, so a last line cut short
by a limit goes too, and `CKPROB` (a cut tag) stays.

`pscat chalk` and the in-tree `chalkboard_math` tool capture the program's
stdout and never return it (`chalk::run_with_limits` drops it; the MCP tool
reads only stderr and the child's stdout for `explain`), so they needed no
call; the tests now pin that neither shows a `CKPROBLEM` line.

Follow-up for pscat-mcp: call `user_output` instead of filtering by prefix.
