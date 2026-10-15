# #454: cliprestore inside a dkbatch (2026-10-14)

## Decision

drykit's plain-clip tracking (#444, `notes/444-plain-clip-in-a-batch.md`)
shadowed nine names while a `dkbatch` procedure runs. Where the interpreter
has Level 3's `cliprestore`, it is now a tenth: `cliprestore` runs, then
the batch's `/pc` goes to `/dirty`. `clipsave` is not shadowed, since it
changes nothing.

Why `/dirty` and nothing smarter (a stack that `clipsave` pushes `/pc`
onto): `cliprestore` gives back a clip drykit may never have seen, and
Ghostscript scopes it in ways a stack would have to copy (below). Any
transition the shadows cannot follow goes to `/dirty`, never to `null`;
`/dirty` only costs grouping (the next stroke takes one `gstate`), and a
`gstate` is exact. So the rule is always right, whatever the pairing.

## Does pscat get clipsave/cliprestore? Not in this PR

pscat has neither operator (`undefined`); only Ghostscript does. Adding
them was the other way to close #454, and it was left out:

- It is a new public Level 3 operator pair in `src/`, with its own
  semantics to get right. gs's are not a plain stack: the clip stack is
  scoped per `gsave` level (probed: a `cliprestore` inside a deeper `gsave`
  than its `clipsave` puts back the clip from the `gsave`, not the saved
  one; an empty `cliprestore` is a silent no-op at the page level, and
  `save`/`restore` drops pending entries). It touches only the clip, not
  the CTM, colour or path. Matching that is a feature of its own.
- Nothing in the repo uses them, and drykit does not need them: with no
  operator there is nothing to shadow, so a pscat batch is unchanged.
- The issue allows "a gs-only test". The fix is tested in pscat through
  a stand-in pair (below) and in gs for real.

Follow-up filed for the pscat operators: #494.

## What shipped (lib/drykit.ps)

- `dkPcliprestore`: `systemdict /cliprestore get exec dkPdirty`. It goes
  through `systemdict get exec`, not the bare name, so it cannot call
  itself if the interpreter's `cliprestore` is a procedure (the nine
  others rely on `bind` inlining operators; this one does not depend on it).
- `dkPnames` gets the pair only if `systemdict /cliprestore known`
  when drykit loads. `dkPon` and `dkPoff` already loop over the array, so
  install, the "all names resolve to systemdict" rule and take-back follow.
- In gs `cliprestore` and `clipsave` are both `operatortype` in systemdict.

## Evidence

- `tests/drykit.rs` `a_cliprestore_in_a_batch_renders_as_at_once`: three
  programs (clip, stroke, `cliprestore`, stroke; a restore back to an
  outer clip; a stroke before and one after), each byte-identical in a
  batch and drawn at once. pscat has no real pair, so a stand-in is
  installed in systemdict before drykit loads (a gstate saved and put
  back: exact while nothing but the clip changes between the two). Control:
  the same drykit with the one shadow line cut out differs by over 200
  pixels per program, so the check can see the leak.
- `ghostscript_follows_a_cliprestore_in_a_batch` (local only, CI has no
  gs): the same programs through the real operators, png16m at 144 dpi,
  batch against at once, byte-identical; with the shadow line cut out they
  differ.
- The strokes in a program are placed so none overlap another stroke made
  under a different clip: a batch draws levels first-to-last across
  strokes, so crossing strokes legitimately differ from drawing at once
  (as in #444's rows).
- Identity: see the PR. No committed render calls `cliprestore`, and
  pscat's batch path is unchanged (the line is skipped where the operator
  is absent).
