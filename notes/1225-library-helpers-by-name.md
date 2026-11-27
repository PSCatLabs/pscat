## Library procedures reach their helpers by `//name exec` (issue #1225, 2026-11-26)

`pkliner` called artkit's `shade` by name, looked up when the brush ran. A
caller that kept a colour in a local `/shade` had its own dictionary on the
dictionary stack, so the brush found an array where it wanted a procedure and
failed with `typecheck; OffendingCommand: shade`. `bind` does not help: it
replaces only names that are operators at bind time, and a helper is a
procedure. Every helper a library procedure called by name had the same
hazard (`mix3`, `frnd`, `walkpath`, `scin`, `pkjit`, ...).

### The fix

Inside a library procedure body, a call to a helper that is already defined
when the body is scanned is written `//name exec` instead of `name`. The
scanner replaces `//name` with the procedure itself, so nothing is looked up at
run time and no dictionary the caller has pushed can intercept it. `exec` is
needed because an executable array met inside a procedure is pushed, not run.
This is the idiom `lib/functional.ps`, `lib/aspect.ps`, `lib/warp.ps` and
`lib/kernel.ps` already use. Renders do not change: `scripts/identity_check.sh`
with `--loader lib/artkit.ps` and `--loader lib/paintkit.ps` comes out
byte-identical (numbers in the PR).

The sweep was mechanical (one script over every `lib/*.ps`, using a
tokenizer that skips comments and strings) and applied by rule, not by hand:

| where | rule |
|---|---|
| `lib/paintkit.ps`, `lib/artkit.ps` | every helper procedure with one top-level definition, defined before the use, except paintkit's own public brushes and presets (`@summary` entries such as `pkribbon`, `pkfeather`, `pkflat`, `pigmix`), see below |
| the other libraries | calls into artkit (and into another library their `@requires` line names) whose name is not that library's private prefix (`frnd`, `walkpath`, `scatter`, `scin`, `screct`, `scpath`, `mix3`, `grain`, `fiber`, `hatch`, `noiseinit`, `axialsh`, `showctr`, `pal`, `tfblock`, `rrect`, ...), plus the generic own helpers of `lib/graph.ps` |
| left by name | see below |

Load order: a library now needs what its `@requires` line names loaded first
at load time, not only when a procedure runs (`//name` is scanned then).
`tests/font_packs.rs` and `tests/libbind.rs` loaded lettering and surfacekit
without artkit and were changed to load it first.

Files changed: `artkit`, `paintkit`, `pagekit`, `fillkit`, `lettering`,
`stipplekit`, `surfacekit`, `graph`, `drykit`, `drycrayon`, `drycharcoal`,
`printkit`, `released`, `apparel`.

### What stays by name, and why

- **Forward references** (70 sites in paintkit, a few elsewhere): the helper is
  defined later in the file, so `//name` would be undefined when scanned. Mostly
  the `pu*` (pigment) and `pj*` (jitter, budget) helpers that sit at the end of
  `lib/paintkit.ps`. Their names carry a private prefix. Moving them up is a
  large reorder for no behaviour; a later issue can do it if a clash is ever
  seen.
- **Recursion**: a procedure calling itself.
- **Data and state**: `wkmaxsegs` (a limit a program may raise), `Palettes`,
  `Pigments`, the turtle registers `tx ty th tsp`, `pwwarned` and the other
  `/name ... def` state. `//` would freeze the load-time value.
- **Names two libraries define** (`getopt`, `defaults`, `drawstroke`, `layout`
  in handscript, hangul and etching) and a name defined twice in one file
  (`noise2`): the later definition is meant to win. `lib/handscript.ps`,
  `lib/hangul.ps`, `lib/typewriter.ps` and `lib/etching.ps` are untouched: they
  keep scratch variables (`i`, `n`, `uni`, `mix2`) in the caller's dictionary on
  purpose.
- **Private-prefixed helpers outside paintkit and artkit** (`dk*`, `dc*`, `mk*`,
  `sg*`, `rl*`, ...): a caller does not pick those names. Left for a later pass.
- **The turtle commands** (`fd hop tl tr tpush tpop`), which a program may
  redefine to change what an L-system draws.
- **paintkit's public brushes called by other paintkit procedures**
  (`pkribbon`, `pkfeather`, `pkflat`, `pigmix`, `pkbroad`, ...) and
  **`pktrowel`**: tests (`tests/paintkit.rs` wraps `pkribbon` to record its
  calls; `pktrowelplane`'s `%%SelfTest` replaces `pktrowel` by a counter) and
  programs redefine a public brush by name, and the other brushes built on it
  follow. Those names carry the `pk` prefix, so a caller's local variable does
  not hit them.
- **`frnd`'s one forward reference** was removed by moving the one-line
  `/frnd` above `/jit`; no other definition moved.

### A consequence to know

HANDOFF used to say library names that are not operators stay late-bound, "so
overriding `pkribbon` and the like works". It still works for a program's own
calls. It no longer reaches a *library's* calls to a helper listed above: a
program that redefines `frnd` or `shade` to change what paintkit does has to
patch the procedure's array in place instead
(`/walkpath load 0 { ... } putinterval`, as `tests/paintkit.rs`'s
`pre_walkpath_scans_refuse_...` now does) or edit the library.

### Tests

`tests/lib_helper_shadowing.rs`:

- a static guard over every library above: no name that a file reaches through
  `//name exec` is also reached by a bare name after its definition;
- a behavioural check: a dictionary that defines every such name in the
  repository as a procedure raising `shadowed-<name>-by-caller` is begun, and a
  scene of every paintkit preset plus artkit, surfacekit and pagekit calls
  renders byte for byte what it renders without it;
- the issue's own program, and a control showing the probe raises when a
  procedure does reach `shade` by name.

### Proposed follow-ups (not done here)

- A lint note, as the issue suggests: when an error's OffendingCommand is a name
  the program defined locally and a library also defines, say so.
- Move the `pu*` and `pj*` helper blocks of paintkit above their first use so
  the last 70 forward references can use `//`.
