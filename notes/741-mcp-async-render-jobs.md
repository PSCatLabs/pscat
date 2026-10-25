# #622 + #741: concurrent MCP calls, cancellation, render jobs

Also closes #725 (a test for the `--timeout` kill backstop), since the kill
path in `run_child` changed.

## What shipped

- **Calls on threads (#622).** `src/mcp_server.rs` reads stdin on the main
  thread and answers `initialize`, `ping`, `tools/list`,
  `notifications/cancelled` and the four job tools inline. Any other
  `tools/call` gets its own thread. At most 4 run at once; past that the call
  gets a `busy` tool error rather than waiting in a queue, because a queued
  call would only use up the client's timeout. A malformed call (no name, an
  unknown tool, arguments that are not an object) is still answered inline,
  so replies to bad input keep their order (`malformed_input_corpus`).
- **Cancellation.** The `inflight` table maps a request id, taken as its JSON
  text, to that call's cancel flag. The flag is registered before the thread
  starts. When the thread finishes it removes its entry and reads the flag
  under the same lock `cancel_request` takes, so there are only two cases:
  - the cancel lands first, and no reply is sent, as the spec asks;
  - the cancel lands after the entry is gone, and it does nothing.

  `run_child` polls the flag along with its timeout, kills the child and
  reaps it.
- **One writer.** `Server::send` builds the line and writes it whole under the
  stdout lock, and nothing else writes to stdout.
- **Jobs (#741).** `start_render` (same schema as `render_postscript`, and
  every argument check made before a job exists), `render_status`,
  `render_result` (the stored `render_postscript` result, so warnings, lint,
  partial pages and `out_path` all behave the same) and `cancel_render`.
  - New flags: `--job-timeout` (default 3600), `--max-jobs` (2) and
    `--job-ttl` (3600). At most 16 jobs are held at once, and the oldest
    finished one is evicted first.
  - Queued jobs start in the order they were asked for (`first_in_line`).
  - Results stay in memory.
- **Progress.** The new CLI flag `pscat --progress` prints
  `pscat: progress pages=N` after each slice in which the page count changed.
  The server's stderr reader parses these lines into the job's counter and
  strips them from the stderr it returns. Rows within one `image` are not
  reported: `image` is a single operator, so reporting them would need a hook
  inside it.
- **No job outlives the server.**
  - When stdin closes, `shut_down` cancels every call and job and joins every
    thread, so children are reaped and scratch dirs removed.
  - For a server that is killed outright, every child gets
    `--exit-with-parent <server pid>`. A thread in the child exits once its
    parent pid is no longer that one. The pid is passed explicitly because the
    server can die before the child first looks, and recording `getppid()` at
    startup lost that race in the first version of the test. In a container
    the server may itself be pid 1, so "parent is 1" cannot mean "orphaned".
  - Unix only. On Windows such a child runs until its own time limit
    (SECURITY.md says so).

## Gotchas found on the way

- **The orphan watcher used `eprintln!`.** The child's stderr is a pipe to the
  server that just died, so the write failed with EPIPE (SIGPIPE is ignored in
  Rust), `eprintln!` panicked, and the watcher thread died instead of ending
  the process. Both new stderr writes in `main.rs` use `let _ = writeln!(...)`.
- **macOS `pgrep -P` does not list zombies** (checked with a deliberate one).
  The tests list children with `ps -A -o pid=,ppid=,stat=` instead, so a
  zombie fails the "no children" check.
- **`SPAWN_LOCK`.** On macOS, std creates a child's pipes with `pipe()`
  followed by a separate close-on-exec step. A fork on another thread in
  between would inherit this child's stdin write end, and the child would
  then never see the end of its program. The lock serialises `spawn`.
- **A PostScript error is a `done` job.** `render_postscript` returns the
  partial page with the error text, not an error result. `failed` is for
  renders with no output, an output cap hit, a timeout, and similar.

## Tests

`tests/mcp.rs`, in the "concurrency, cancellation and render jobs" section.
None of them asserts on elapsed time. A render that has to stay running waits
on a FIFO the test controls, and every wait is a bounded number of looks
(`eventually`).

- A long call runs while `ping` and another call are answered. Cancelling it
  leaves no child (zombies included), and no reply ever comes for it.
- Concurrent large replies interleaved with pings all arrive as clean lines,
  each id exactly once.
- The acceptance test runs on a short time scale: `--timeout 1`, and a job
  gated on a FIFO between its two pages stays `running` past the 1 s call
  limit. It then shows `pages_done` 1, finishes when the gate opens, and its
  result matches `render_postscript` byte for byte.
- `cancel_render` kills and reaps the child. A queued job behind
  `--max-jobs 1` can be cancelled without ever starting. Bad arguments are
  refused at start. TTL expiry works.
- Closing stdin stops every job. A SIGKILLed server leaves no render behind.
- Unit tests:
  - `run_child` against `sleep 60` with a 1 s timeout returns the `timeout`
    error shape (#725); the bound is the counterfactual 60 s;
  - cancelling from another thread kills the child;
  - progress parsing works across split reads;
  - the job table's ordering and purge are covered.

## MCP Tasks

Checked 2026-10-25; sources are in `docs/guides/agents.md` "Render jobs".

- Tasks are experimental in 2025-11-25 (`task` param, `tasks/get`,
  `tasks/result`, `tasks/cancel`, `tasks/list`, `execution.taskSupport`).
- In 2026-07-28 they moved into the `io.modelcontextprotocol/tasks`
  extension: `resultType: "task"`, `tasks/get` carries the result,
  `tasks/result` and `tasks/list` were removed, and `tasks/update` was added.
- Claude Code 2.1.304+ uses the extension (public beta) on 2026-07-28
  connections only.
- Codex 0.164.0 supports neither form.
- pscat-mcp does not speak 2026-07-28 yet (#621), so the jobs are plain tools.
  The mapping is in the guide.

## Follow-ups (proposed, not filed)

- `render_status` with an optional `wait_seconds` (a long poll on the
  condvar) to save agent turns.
- Jobs for `chalkboard_math`, `review_sheet` and `handwrite`, if anyone needs
  them.
- Row progress inside a long `image`/`colorimage`.
- Tasks once #621 lands.
- Unguessable job ids for the hosted server (#689).
