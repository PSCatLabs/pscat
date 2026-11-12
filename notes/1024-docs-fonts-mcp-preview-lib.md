# 1024, 1046, 1055, 1056 (part 1): docs for the ArtColony report batch

One docs PR for four issues reported while building ArtColony/sheet_music.

- **#1024 own fonts.** `pscat --help` (under `--fonts`) and `pscat fonts help` now
  say that a `.ttf`/`.otf`/`.ttc` in `fonts/catalog/<dir>/` is found by
  `/<FileStem> findfont`; `docs/guides/fonts.md` and the pscat skill say the same
  (one directory level, case-insensitive stem, a `.ttc` gives its first face).
  Left out: the npm README line (`npm/` belongs to another lane) and having
  `--fonts` print the directories it searched.
- **#1046 ImageType 3.** `plugin/skills/pscat/references/extensions.md` no longer
  implies a soft mask: an `ImageType 3` mask is a 1-bit stencil (8 bits is
  `rangecheck`, `InterleaveType` 1 and 2 `limitcheck`), `/SMask` in an ImageType 1
  dict is ignored, and the one alpha an image has is `setalpha`. A per-pixel soft
  alpha for images stays an enhancement nobody has asked for yet.
- **#1055 preview.** The MCP image block reaches the model, not necessarily the
  user. The server instructions and the four image tools' descriptions now say so
  (`PREVIEW_NOTE` in `src/mcp_server.rs`), `references/mcp.md`, the pscat and psart
  skills and `plugin/README.md` too. No new `path` field: without `out_path` the
  pages live in a scratch directory that is deleted, so a returned path would
  dangle; with `out_path` the reply already ends in `wrote <absolute path>`. The
  stable binary path is `$CACHE/bin/pscat` since #919.
- **#1056 part 1.** `pscat --help` ends with an Environment block (`PSCAT_HOME`,
  `PSCAT_LIB`, `PSCAT_TRACE_RESOURCES`) and the search order;
  `docs/guides/getting-started.md` and the pscat skill document the per-user
  `lib/` for your own libraries and that `PSCAT_LIB` is one directory subject to
  `--read-root`. Parts 2 (a current-file-name operator) and 3 (making another
  plugin's library readable by `pscat-mcp`) are owner decisions and are not done.

Also in the skills: the ellipse builders and the grestore-discards-the-path habit
(#1053), and accented Latin in `sgpath` (#1051). Both describe code in open PRs
(#1072, #1068), so this PR merges after them.
