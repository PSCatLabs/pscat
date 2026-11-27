## A missing `@requires` prerequisite names the library (issue #1455, 2026-11-27)

#1225 made library procedures call helpers as `//name exec`, which is scanned
when the library loads. A library loaded without its prerequisite therefore
failed at load with the first helper it could not find, `undefined in
pjheight` for drykit without artkit and paintkit: a helper name, not the
library. psanim hit it in a test and asked for the library to be named.

### What changed

Each of the 19 libraries with a `% @requires:` line now has one line per
prerequisite directly under that tag, in the same order, before any code:

```
/pkribbon where { pop } { (drykit needs (lib/paintkit.ps) run first) errordict /undefined get exec } ifelse
```

The result is the real `undefined` error with the sentence as the offending
command, so every way of seeing an error shows it: the CLI prints
`%%[ Error: undefined; OffendingCommand: drykit needs (lib/paintkit.ps) run first; Line: 21 (lib/drykit.ps) ... ]%%`,
`$error /command` holds the string, and Ghostscript says `Error: /undefined in
(drykit needs \(lib/paintkit.ps\) run first)` (checked with `gs -dNOSAFER`).
The first missing prerequisite in load order is the one named.

The probe is a name the prerequisite defines (`walkpath` for artkit, `pkribbon`
for paintkit, `dkstroke` for drykit, `hkline`, `sfshade`, `hllayout`,
`psyletter`, `fp-map`, `aop-before`, `hs-write`, `sgpath`, `TWOutlines`). A
program that defines a variable of the same name before loading a library
passes the probe and gets the old error later; nothing gets worse.

### Why a line in each library, not an interpreter hook

The check is plain PostScript run once at load: no new name in `userdict`, no
extra step in any procedure, no change to `//name` semantics, no `src/` change.
An interpreter hook (reading the tag in `run`) would have had to parse source
on every `run` and would not help a library pasted into a program.

### Keeping the lines honest

`tests/lib_requires_check.rs` reads every `lib/*.ps` `@requires` line with
`selftest::parse_requires`, builds the text the lines must have and compares.
It fails for a library with a tag and no check, a prerequisite added to a tag
without its line, a reordered line, a stray line, or code before the checks. It
also loads each library with 0..n-1 prerequisites and asserts the error names
prerequisite k, loads with all of them and asserts a clean operand stack, and
proves each probe name is defined by its own library and not by anything loaded
before it. A new prerequisite library needs a row in `SENTINELS`.

### Behaviour change to know about

Four libraries used to load on a bare interpreter, because they call their
prerequisites only when a procedure runs: `headline`, `jersey`, `drychalk` and
`mathkit`. They now fail at load like the rest. Every in-repo loader was
scanned (examples, gallery, docs, plugin skills, `src/`, `tests/`, npm and web
scripts) for a load of any `@requires` library without its chain before it; the
one real case was `scripts/test_npm_tarball.sh`, which loaded mathkit alone to
prove `lib` resolves, and now loads handscript and strokeglyphs first. A
correctly loaded library renders identically (`scripts/identity_check.sh
--loader`, numbers in the PR).

### Not done

`pagekit` has no `@requires` line (its prerequisite, artkit, is only in its
prose header) although it uses `//` into artkit, so a bare load still names a
helper. `graph`, `dataviz` and `etching` stand alone. Adding the tag to pagekit
also changes its catalog `load` string, so it is a separate ticket.
