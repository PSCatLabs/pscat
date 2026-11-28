## Parameter, cache, job-control, overprint and smoothness operators (issue #1270, 2026-11-28)

Twenty-three PLRM operators that a prolog calls near its first line were
`undefined`. All are defined now, each as the cheapest honest behaviour: what
the PLRM says is stored is stored and read back, what pscat does not have
(printers, job servers, glyph caches, separations) answers as gs 10.07.1
answers for a plain non-persistent job, and what would change a limit or reach
outside the sandbox is refused. Every value below was measured with
`gs -q -dNODISPLAY -dNOSAFER -dBATCH -dNOPAUSE`; `tests/params.rs` pins 626
cases from `tests/data/params_cases.txt`, one line each, and reruns them in gs
when it is installed (the file is gs's output; the test fails if gs's changes).
`src/ops/params.rs` is the implementation, `src/ops/graphics.rs` the
overprint and smoothness pair, `src/ops/vm.rs` the shared-VM names.

### How gs does it, and so how pscat does

In gs most of these are PostScript procedures over a few C parameters
(`Resource/Init/gs_lev2.ps`, `gs_dps2.ps`), so the operators agree with each
other, and pscat keeps those links:

| operator | is, in gs | here |
|---|---|---|
| `setcachelimit n` | user parameter `MaxFontItem` | the same parameter |
| `currentcacheparams` `mark size lower upper` | system `MaxFontCache`, user `MinFontCompress`, `MaxFontItem` | the same three |
| `setcacheparams` | sets those three | sets those three |
| `cachestatus` | `0 MaxFontCache 0 200 0 5000 MaxFontItem` | the same; nothing is cached |
| `ucachestatus` / `setucacheparams` | `mark 0 0 0 0 MaxUPathItem`; the setter only ever raises it (`max`) | the same |
| `vmreclaim n` | 1 and 2 ignored, over 2 `rangecheck`, 0 to -2 set user `VMReclaim` | the same |
| `setvmthreshold n` | user `VMThreshold` (-1 is the default 8000000, 0 is stored as 1) | the same |
| `setshared`, `currentshared`, `shareddict` | `setglobal`, `currentglobal`, `globaldict` | the same objects |
| `startjob` | with no password that opens a job, `exit_bool password startjob` replaces both operands by `false`; the first must be a boolean | the same, always |
| `exitserver` (in `serverdict`) | a procedure over `startjob`: `invalidaccess`, the operand consumed | an operator of that name in `serverdict`, the same |
| `setoverprint` / `setsmoothness` | gstate fields | gstate fields (`GraphicsState::overprint`, `::smoothness`) |

**Save and restore.** User parameters are local VM in gs: `restore` brings
them back (`JobName`, `VMReclaim`, `MaxFontItem`, `MaxUPathItem`...). System
parameters are not rolled back (`MaxFontCache` and every `Max*Cache` survive a
`restore`). Here the user set is an `Rc<Vec<PVal>>` held by each `SaveRecord`
and copied on the first write after a `save`, so a `save` costs one pointer.

**Overprint and smoothness.** `initgraphics`, `showpage` and `setpagedevice`
reset smoothness to its initial 0.02 and leave overprint alone; only a
`grestore` or `grestoreall` back to the initial state clears overprint
(measured, and pinned by cases). Smoothness is clamped to 0..1 and read back
as the shortest decimal of the `f32` it is held in, as the miter limit is.
Nothing is simulated: overprint is stored and read back only (the CMYK
pipeline owns simulating it).

### The parameter dictionaries

`setuserparams` and `setsystemparams` take a dictionary, look up each
parameter pscat has (they never walk the program's dictionary, so a
million-entry operand costs the same as an empty one), check **all** of them
and only then apply any, as gs does (probed: a bad value anywhere leaves every
other key unset, whatever the key order). A key pscat does not have is ignored,
as gs ignores any unknown key; a string key counts as the name. The first bad
value, in the table's name order, gives the error with the dictionary left on
the stack: `typecheck` for the wrong type (a real for an integer too),
`rangecheck` for a value outside the range gs accepts, `invalidaccess` for a
`noaccess` string.

The tables are the PLRM's parameters that gs also has. Types and ranges, from
gs (`typecheck` for anything but the type; the others `rangecheck` outside):

| kind | parameters | accepted |
|---|---|---|
| any integer | `HalftoneMode`, `MaxFormItem`, `MaxPatternItem`, `MaxScreenItem`, `MaxSuperScreen`, `MaxUPathItem`; system `Cur*` (but `CurFontCache`), `Max*Cache`, `MaxSourceList`, `MaxDisplayList`, `MaxDisplayAndSourceList`, `MaxScreenStorage`, `MaxImageBuffer`, `RamSize`, `JobTimeout`, `WaitTimeout` | any 64-bit integer |
| 0 to 2^32-1 | `MinScreenLevels` | stored as is |
| 0 to 2^32-1, reads back 0 | user `JobTimeout`, `WaitTimeout` | gs does not implement the timeouts |
| -2^31 to 2^32-1 | `MaxFontItem`, `MinFontCompress` | a 32-bit signed value clamped at 0: -1 and 2^31 and over read back 0 |
| `VMThreshold` | -1 (default 8000000), 0 (stored 1), 1 and over | below -1 `rangecheck` |
| `VMReclaim` | -2, -1, 0 | else `rangecheck` |
| `MaxFontCache` | 0 to 2^32-1 | stored between 100000 and 100000000 |
| boolean | `AccurateScreens`, `IdiomRecognition`, `DoPrintErrors` | |
| string | `JobName`, `PrinterName`, `LicenseID`, `CurInputDevice`, `CurOutputDevice` | stored as a copy; read back as a fresh copy |
| ignored (no check) | `PageCount`, `RealFormat`, `ByteOrder`, `BuildTime`, `Revision`, `CurFontCache` | gs ignores a write to them |

### What the sandbox refuses (the deviations)

All of these are `invalidaccess`, with the dictionary left on the stack. Each
is a case in `DEVIATIONS` in `tests/params.rs` with gs's line beside pscat's.

- **Limits.** `MaxOpStack`, `MaxDictStack`, `MaxExecStack`, `MaxLocalVM`
  (user) and `MaxGlobalVM` (system) read back and accept only the value they
  already have. pscat's limits are `--max-steps`, `--max-time`,
  `--max-memory` and a fixed 10000-frame execution stack; a program must not
  move them. `MaxExecStack` reads 10000, pscat's real cap (gs: 5000);
  `MaxOpStack` 300000 and `MaxDictStack` 500 are gs's defaults (pscat has no
  operand-stack cap but memory, and no dictionary-stack cap).
- **Files.** `PermitFileReading`, `PermitFileWriting`, `PermitFileControl`,
  `LockFilePermissions` are refused whatever the value, and are not in
  `currentuserparams` (gs reports `[(*)]`, wide open, which would lie in a
  sandbox). The two resource directories and the path separator
  (`FontResourceDir`, `GenericResourceDir`, `GenericResourcePathSep`) read back
  as fixed `/Resource/Font/`, `/Resource/`, `/` and accept only those, the
  rule gs's own `-dSAFER` applies to the same three keys.
- **Passwords.** `Password`, `StartJobPassword` and `SystemParamsPassword` in a
  `setsystemparams` dictionary are refused: pscat has no job server and no
  password that opens anything. (gs started without `-dSAFER` takes any.)
- **Device parameters.** `setdevparams` on `%os%` (a file system) with a
  non-empty dictionary is refused; gs accepts it and ignores every key. An
  empty dictionary is accepted. `%stdin%`, `%stdout%`, `%stderr%`,
  `%lineedit%` and `%statementedit%` are `/Type /Special` with nothing to set
  and ignore any dictionary, as in gs. `currentdevparams` for `%os%` reports
  fixed sizes, not the host's disk (gs reports the real `Free` and
  `LogicalSize`; under a hosted server that would tell the program about the
  machine), and `Writeable false`.
- **`startjob`, `exitserver`.** There is no job to start or leave. `startjob`
  answers `false` for every password (gs: `false` unless the password opens a
  job, and none does in `-dNOSAFER` either); `exitserver` is `invalidaccess`.

### Other differences

- **Parameters not modelled.** gs-only user parameters (`GridFitTT`,
  `AlignToPixels`, `RenderTTNotdef`, `OverrideICC`, the ICC profile names, the
  comment hooks `ProcessComment`, `ProcessDSCComment`) are ignored when set and
  absent from `currentuserparams`; gs's dictionary has 38 entries, pscat's 20.
  The same for the system's `%diskFontResourceDir` and
  `%diskGenericResourceDir`. The comment hooks are procedures pscat would have
  to call from its scanner, which it does not.
- **Access attributes.** pscat gives strings and dictionaries no read-only or
  `noaccess` state, so a string read from `currentuserparams` can be written to
  (it is a copy, so nothing stored changes), and `noaccess` on the operand
  dictionary is not an `invalidaccess`. Cases pin both.
- **Values past 2^31** in the 32-bit parameters follow gs's wrap (`2147483648
  setcachelimit` reads back 0), and `JobTimeout`/`WaitTimeout` read back 0 as in
  gs. Reals are `typecheck` for every integer parameter, exactly as in gs.
- **`exitserver` with no operand** is `stackunderflow`; gs leaves a `true`
  behind it (its procedure had already pushed one).
- **`serverdict /exitserver get type`** is `operatortype`; gs shows a
  `packedarraytype` procedure. It executes the same.
- **A bad dictionary key order** across two bad values: the first bad one in
  the table's name order wins; gs's is its hash order.
- **Smoothness** is held as an `f32` and read back as its shortest decimal;
  gs prints it with `%g` (six digits), so a value with more than six
  significant digits (`0.123456789`) reads back `0.12345679` here and
  `0.123457` there.

### Resource accounting

- `currentuserparams`, `currentsystemparams` and `currentdevparams` build a
  fresh dictionary each call (gs does too); the dictionary and every string in
  it are charged with `check_alloc` before they are built, and the table walk
  with `poll_limits`.
- `setuserparams` and `setsystemparams` charge the walk of the table (one step
  per eight parameters, plus the string bytes per KiB) and check each string's
  copy against the memory limit before copying it; the whole dictionary is
  validated before anything is stored, so a limit or an error leaves every
  parameter as it was.
- `setcacheparams` and `setucacheparams` look for the mark without popping
  (the ledger would copy every operand it popped), charge the walk down to it
  (one step per 64 operands), check every value, and only then drop the
  operands and the mark in one truncate. `cachestatus` and the other
  multi-value readers ask `check_alloc` for all their operands before pushing
  the first.
- `tests/params_limits.rs` (a million-operand stack under a step cap; every
  call charged; a save loop; a million-entry operand dictionary costs the
  table, not the dictionary) and `tests/params_memory.rs` (a big string
  refused on set and on read).
