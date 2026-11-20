# #1338: `[shadowed-operator]` names the line a `def` is written on

Follow-up to #1109 (`notes/1109-shadowed-operator-line-and-systemdict.md`), which reported the line the
scanner had reached: for a `def` inside a procedure or loop, the line that ran it (`gallery/hundred_lines.ps`
said 261 for a `def` on 254, the closing `} for`).

Now the message names the `def`'s own line (254). The wording is unchanged (`when the program reached line N`),
only the number moves, so tests and `pscat-mcp` matching on the prefix still match.

## How

Two pieces, both off the hot path.

1. **Lines are kept at scan time, only under `--lint`.** `Interp::trace_operator_shadows()` already switches the
   shadow trace on; it now owns a `lint::BodyLines` table. `scan_procedure` (the one function that builds a
   procedure body from tokens) gets the table from `next_item`'s split borrow and, for a body scanned from the
   main program or a `run` file (the sources that have lines), records one source line per element, keyed by the
   address of the body's storage and holding a `Weak` so the address is not reused. A plain render passes
   `None`: one `Option` check per scanned `{`, nothing per token or per executed element. Capped at 4M elements.
2. **The running `def` is found lazily**, only when a shadow is first recorded (a loop redefining the name a
   million times still pays once: `note_shadow` lends the trace out of the interpreter and hands `ShadowTrace::note`
   a closure that runs only for a new entry; it also no longer clones the `run` file name on every call). The interpreter tags no object, and a procedure frame whose
   last element runs is popped before it does (tail calls), so `running_def_site` looks at what the execution
   stack still says:
   - a procedure frame on top: element `pc - 1`, the `def` itself or the call that is running a body that ends in one;
   - a `for`/`repeat`/`loop`/`forall` frame: its body (the `def` is that body's last element);
   - the scanner: the name of the token it is running, read back from the source text on demand
     (`Lexer::token_name`), resolved to a procedure.
   From an element, `def_leaves` follows a name to the procedure it is bound to and its last element (to depth 6), and
   an operator (`if`, `ifelse`, `exec`, `stopped`, ...) to the procedure literals just before it.
   Under a scanner token that is an operator, `{ ... /x 1 def } if` in the program text, the branch is on no frame
   and no element; `BodyLines::nearest_before` picks the body scanned just before the token whose last
   element is a `def` of that name. That body is already freed, so the table keeps, per body, the literal names it
   spells when it ends in `def`/`store`.

A `def` counts only if its body spells `/name` before it, and the answer is used only when exactly one `def`
is found. Otherwise (`/set { exch def } def` called as `3 /ln set`, an `exec` of a procedure built at run time, two
branches that both define the name) the line is the one that ran it, as before. For `/set`, that is the more
useful line anyway.

## Cost

Release binaries, base (`origin/feature/0.7.1`) and head built from the same tree, `/usr/bin/time -l` instructions
retired (user seconds, three runs each, are within noise; instructions are the reliable number):

| program | mode | base | head |
| --- | --- | --- | --- |
| 13.5M-token flat file (`1 2 add pop 3 4 mul pop` x 1.5M) | plain | 17 534M | 17 099M |
| same | `--lint` | 19 255M | 18 830M |
| `0 1 3000000 { /x exch def } for` | plain | 3 319M | 3 319M |
| same | `--lint` | 3 517M | 3 520M |
| same on an operator's name (`/ln`, the worst case) | `--lint` | 4 393M | 4 409M (+5 per `def`) |
| `/f { 1 pop } def 0 1 10000000 { pop f } for` | plain | 11 622M | 11 622M |
| same | `--lint` | 11 622M | 11 622M |
| 3M calls of a procedure ending in `/x 2 def` | `--lint` | 5 903M | 5 906M |

A plain render adds one `Option` check per scanned `{` (a procedure literal), nothing per token, per executed element, in
`proc_step` or in `def`. (The flat file is 2% fewer instructions at head: code layout, not a change in that path.)
`--lint` pays a `Vec<u32>` per scanned procedure body (4 bytes per element, 4M elements at most) and one hash insert.

## Not done

- `store` and `def` only; `put` into systemdict still has no line of its own.
- A tail `def` reached through `exec` of a procedure the program built at run time reports the caller's line.
- #1081's `[stack-leak]` line has the same limit (a leak made inside a loop is credited to the closing `} for`).
