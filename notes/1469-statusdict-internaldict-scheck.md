## statusdict, serverdict, internaldict, scheck, GlobalFontDirectory, SharedFontDirectory (issue #1469, 2026-11-27)

Six PLRM names a prolog probes (`/statusdict where`, `1183615869 internaldict`)
were `undefined`; gs 10.07.1 has all six. They are defined small: what a
probe needs, not a printer. Every value below was measured with
`gs -dNOSAFER -dNODISPLAY` and is pinned by `tests/statusdict_internaldict.rs`
(which reruns the probes in gs when it is installed).

**What gs shows, and what pscat does**

| name | gs | pscat |
|---|---|---|
| `statusdict` | `dicttype`, in systemdict, local VM (`gcheck` false), `rcheck` and `wcheck` true, not executable, 87 entries; takes `put`, `begin`/`def`, is rolled back by `restore`; not on the dictionary stack | the same, with no entries |
| `serverdict` | as `statusdict`, 6 entries (`exitserver`, `execjob`, `setrealdevice`, `.jobsave`, `.jobsavelevel`, `.adminjob`) | the same, with no entries; `exitserver` stays #1270's |
| `GlobalFontDirectory`, `SharedFontDirectory` | one dictionary under both names (`eq`), in systemdict, global VM (`gcheck` and `scheck` true), `wcheck` false (a `put` or `def` is `invalidaccess`), 0 entries at start, fills as fonts load | one empty dictionary under both names, `gcheck`/`scheck` true; writable, and font definitions stay in `FontDirectory` |
| `scheck` | answers exactly as `gcheck` for every object tried (simple, string, array, dict, userdict, systemdict, globaldict, save, a composite made in global mode) | `gcheck`'s answer under its own operator name (`$error /command` says `scheck`) |
| `internaldict` | an operator; `1183615869 internaldict` is one writable local dictionary, the same on every call, not systemdict/userdict/globaldict/statusdict, 4 entries (`.execute_scheduled_inits`, `setweightvector`, `CCRun`, `.delayed_init_queue`); any other key, of any type, is `invalidaccess` (not `typecheck`) with the key left on the stack; no operand is `stackunderflow` | the same dictionary behaviour, empty, nothing executed; only the integer key (or a real exactly equal to it) opens it |

**statusdict contents.** The 87 gs entries fall in four groups: tray procedures
(`lettertray`, `a4tray`, `legaltray`, ...), setters (`setpapertray`,
`setpagetype`, `setduplexmode`, `setpageparams`, `setdefaulttimeouts`, ...),
read-outs (`product` = `(GPL Ghostscript)`, `revision` 10071, `buildtime`,
`ramsize`, `byteorder`, `pagecount`, `processcolors`, ...) and job state
(`jobname`, `jobtimeout`, `manualfeed`, ...). None is defined here: they are
a printer, not a probe's need, and `letter` and `a4` (named in the issue) are
not statusdict members in gs either (`statusdict /letter known` is false in gs).
A prolog guards them with `known` or `where`; an unguarded one fails on any
non-printer device. `product` and `revision` would be the first to add if a
real file needs them.

**Where pscat differs from gs**
- The three dictionaries are empty; gs's hold entries (counts above).
- `GlobalFontDirectory` is writable and does not fill as fonts load
  (`/Helvetica findfont pop GlobalFontDirectory length` is 2 in gs, 0 here).
  pscat has one VM and one `FontDirectory`, which is also writable where gs's
  is read-only, so a second read-only rule for one dictionary would touch every
  write path (`put`, `def`, `store`, `undef`, `copy`, the fused paths) for a
  probe nobody makes.
- `internaldict` opens for a real within single precision of the key in gs
  (`1183615870.0`, `1183615869.5`); here only the exact value does.

**Resource limits.** Four empty `Dict`s (no table memory until a program
writes) and six more `systemdict` keys. Startup heap, `Interp::with_page(10,
10)`, minimum of 50 constructions under `TrackingAllocator`: 49092 bytes before,
49604 after (+512). `systemdict length` 332 -> 338 (the table had room, no
resize); `vmstatus` and `countdictstack` (3) are unchanged, since none of them
is on the dictionary stack. A program's writes into them are charged like any
dict. `Drop for Interp` empties all four (`PlrmDicts`), so a dictionary that holds
itself does not leak (`dropping_an_interp_frees_its_dictionaries`).

**Deferred.** `exitserver` (#1270). Filling `GlobalFontDirectory` from
`definefont`, and making it read-only, if a program needs either.
