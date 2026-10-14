## walkpath: bound the source-segment count (issue #110, 2026-10-13)

`walkpath` flattens the path and runs `wkmeasure` over every segment before it emits a stop. Every
preset budget (pkspray's 150,000, pkwash's boundary samples, pkdry's bristle x sample product) counts
what is *deposited*, so a path of hundreds of thousands of tiny or zero-length segments passes them all
and still costs seconds. Reproduced on this machine (release build): 200,000 segments of 0.0007 pt,
`pkribbon` 1.11 s; 300,000 zero-length segments plus one real one, bare `walkpath` 0.60 s.

**The fix.** `lib/artkit.ps` gains `/wkmaxsegs 50000 def`. `wkmeasure`, which already visits every
segment, counts moveto, lineto and closepath visits and raises
`walkpath-segment-count-exceeds-safety-limit` the moment the count passes the limit. It runs `cleartomark`
first, so the `[` mark and the pathforall operands do not leak into a `stopped` caller. The count is
taken during the measuring pass, so the caller's proc has not run and no preset has drawn. The same run
that took 1.11 s is now refused in 0.1 s including path construction.

**Why 50,000.** The largest per-call segment count any tracked `.ps` file feeds `walkpath`, measured by
instrumenting `wkmeasure` and rendering all 107 tracked non-lib `.ps`/`.eps` files plus every
`%%SelfTest` block (`selftest.sh`):

| resolution | maximum | where |
|---|---|---|
| 72 dpi | 1,726 | `examples/paintkit_motion_demo.ps` |
| 288 dpi (4x any shipped build; `flattenpath` works in device space) | 3,167 | same file |
| 72 dpi, selftest blocks | 402 | `selftest/drivers/paintkit.ps` |
| 72 dpi | 319 | `gallery/alpine_lake.ps` (about 53,000 walkpath calls, none large) |
| 72 dpi | 268 | `examples/paintkit_dry_demo.ps` |

The limit is about 16x the largest count at 4x the shipped resolution. The drykit specimen never calls
`walkpath` at all (`lib/drykit.ps` and `tests/drykit.rs` have no match), so it is not in the table.

**alongpath, pathtext, ctext, et-draw: decided not to guard.** Written into `alintern`'s header comment.
`walkpath`'s bound exists because its callers promise a deposit budget "checked before any drawing
starts" and a source segment that emits no stop slips past it. `alongpath` makes no such promise: its
stamp count is already bounded only by the caller's pitch (0.0001 on a 500 pt line is five million
stamps and nothing refuses that), so a segment bound would cap one cost of an interface whose main cost
is uncapped. It would also risk refusing legitimately dense `charpath` outlines. The residual: `pathtext`
keeps walking segments after its last glyph is placed. `et-draw` is not a path walk (it samples a decoded
image; `lib/etching.ps` has no `pathforall`).

**Tests.**
- `%%SelfTest: walkpath-bounds-its-segment-count` (`lib/artkit.ps`): exactly `wkmaxsegs` segments walks, one
  more is refused, many two-segment subpaths are refused (consecutive bare movetos collapse in the path, so
  they cannot be used), the proc never ran, and a small path walks after a refusal.
- `tests/artkit.rs`: the same boundary through the Rust API, plus an empty operand stack after the guard.
- `tests/artkit.rs`, `every_walkpath_preset_refuses_a_huge_segment_count_promptly`: all 11 walkpath-based
  presets (pkribbon, pknib, pkdry, pkspray, pkoil, pktrowel, pkfan, pkbroad, pkliner, pkwash, pkdab) get a
  400,000 zero-length-segment path, must raise the guard name, draw no ink, and finish inside three times the
  time a legal 40,000-segment walk takes (self-relative, so it holds on a slow machine; every time is the
  best of three, because the first version, a single timing against twice the calibration, failed once under
  the full suite's parallel load: 514 ms against a calibration inflated to 190 ms). Checked to fail:
  with the limit raised past the path, `pkribbon` succeeds; with the limit set to 399,990 (a
  count-then-check shape) it fails on the timing assertion (471 ms against a 263 ms bound).

**Cost on legal paths.** The counter adds three small operations per segment. 100 walks of a 40,000-segment
path: 47.4 s base against 48.9 s branch (runs 46.9 / 47.4 / 47.8 against 50.9 / 52.8 / 43.0, so within noise).
50,000 walks of a 3-segment path: 14.2 s base against 14.3 s branch. A full `alpine_lake` render takes about
100 s and varied 96 to 116 s between runs on either tree, which is more than any effect here.

**Byte identity.** `scripts/identity_check.sh` (21 paintkit-loading files, 30 pages, with a control that
trips the limit and so differs between trees, and `alpine_lake` equal to its committed still in both): all
identical. The same render of all 107 tracked files (125 PNGs, including the artkit-only ones identity_check
does not cover) in a base and a head tree: all byte-identical.

**What the bound does not reach.** `pkribbon` and `pknib` run their own `pathforall` over the whole path
(`pkscanclosed` at `lib/paintkit.ps:159`, and a single-subpath check in `pknib`) before they call
`walkpath`, so the guard cannot fire first. Refusal time, with path construction subtracted (best of five,
release build): `pkribbon` 0.094 s at 400,000 zero-length segments and 0.261 s at 1,600,000; `pknib` the
same; `pkdry`, `pkwash` flat (0.07 s and 0.07 s, the start-up cost of those presets). That is linear in
segment count at about 0.16 microseconds per segment, roughly 17x cheaper than a `walkpath` pass (2.7
microseconds), and the caller has already paid O(n) to build such a path. So the claim here is that
`walkpath`'s own traversal is bounded, not that every preset is bounded end to end. Filed as a follow-up.
(Whether a preset opens a `gsave`/`begin` before its first `walkpath` call and so leaves it open when the
guard fires is the same exposure the existing deposit-count guards already have, since they too raise from
inside a `walkpath` proc; this change does not alter it.)

New top-level name `wkmaxsegs` added to `ARTKIT_INTERNAL` in `src/capabilities.rs`. Deliberately not
edited: `README.md`, `HANDOFF.md` and `tests/paintkit.rs`, which held PRs #285 and #279 also change, and
`lib/drykit.ps`.
