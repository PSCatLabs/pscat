# #249 — Move dev and historical docs out of the repo root

Part of epic #246, carrying out the layout approved in `docs/dev/DOCS_PLAN.md`
(#247). Docs-only: no render, library or interpreter behaviour changed.

**Where everything went** (all with `git mv`, in a commit of pure renames so
`git log --follow` works):

| From | To |
|---|---|
| `HANDOFF.md`, `ARCHITECTURE.md`, `VM.md`, `FONTS.md`, `CAPABILITIES.md` | `docs/dev/` |
| `docs/PS_LIBRARY_COUPLING.md`, `docs/SELFTEST.md`, `docs/GS_CHECK_INVENTORY.md` | `docs/dev/` |
| `docs/DRYMEDIA.md` + `docs/drymedia/`, `docs/PAINT_LOADING.md`, `docs/CANVAS_SURFACE.md`, `docs/WATERCOLOR.md` + `docs/watercolor_prototypes/` | `docs/dev/design/` |
| `docs/TYPEWRITER.md` + `docs/typewriter/` | `docs/dev/design/` (landed after the plan; the plan's rule for new design docs) |
| the loose `docs/*.png` evidence images | `docs/dev/design/`; the two `drycharcoal_*` ones into `docs/dev/design/drymedia/` |
| `INIT.md`, `ROADMAP.md`, `NOTES.md` | `docs/archive/` |
| `docs/keepin_it_real_shirt.md`, `docs/groovy_font_choice.md`, `docs/fonts/` | `docs/archive/` |

`SDLC.md`, `AGENTS.md`, `CLAUDE.md`, `README.md` and `notes/` stay at the root.
After the move `docs/` holds only `guides/`, `dev/` and `archive/`.

**How the references were fixed.** One script (run once, not committed) did
both kinds of rewrite over every tracked file except `notes/*` (history),
`docs/archive/*` (kept as written) and the plan's appendix:
- Markdown links, images and `<a href>`/`<img src>` were resolved against the
  linking file's old location, mapped through the move table, and written back
  relative to its new location (this is what re-bases links inside moved docs
  and the `../paintkit_*.png` embeds in `painting.md`).
- Path mentions in prose, comments and strings (`NOTES.md`, `docs/DRYMEDIA.md`,
  `docs/drymedia/...`) and `github.com/.../blob|tree/main/...` URLs were
  rewritten by exact path. Bare names of design docs (`DRYMEDIA.md 6.2`) were
  left alone: they name a doc, not a path.

Then by hand: `CLAUDE.md` and `AGENTS.md` (read `docs/dev/HANDOFF.md` first;
`INIT.md` and `ROADMAP.md` are history, so the "read INIT.md in full" and
"follow its stages in order" instructions were reworded), the contributor
index in `docs/dev/README.md`, the five new rows in `docs/archive/README.md`,
`site/contribute.html`, and the status line of `DOCS_PLAN.md`. The
typewriter tools under `docs/dev/design/typewriter/tools/` computed the repo root as
`../..` from their folder; the folder is now two levels deeper, so it is
`../../../..`.

**Checked.** `scripts/check_md_links.py` and `scripts/check_site_links.py` on a
fresh `_site` both report 0 broken; a grep for every old path finds nothing
outside `notes/`, `docs/archive/` and `DOCS_PLAN.md`'s appendix (which is the
before-the-moves reference map, kept as it was).

**Not done.** The optional split of the stage log at the top of `HANDOFF.md`
into an archive file: another agent was editing `HANDOFF.md`, and a split would
have broken rename tracking when merging. It stays one file.

**Gotcha for the next mover.** `notes/` fragments keep the old paths on
purpose; do not rewrite them.
