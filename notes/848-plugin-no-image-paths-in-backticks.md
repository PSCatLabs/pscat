# 848: no bundled-image paths in backticks (directory checklist)

The directory checklist page moved: the live URL is
<https://claude.com/docs/plugins/pre-submission-checklist> (the
`/docs/directory/plugins/...` URL in the issue is a 404). Under "Files in the
plugin folder", which "apply to every file in the plugin folder, including
images and documents", the row reads (result: "Held for a reviewer"):

> To show a bundled image in the README, use Markdown image syntax. Don't
> refer to bundled images or fonts from commands, hooks, or scripts, or write
> their paths in backticks or a code block.

## What counts

- The rule is about **bundled** images and fonts. `plugin/` bundles none today,
  so main had no live violation of the letter. `paint.md:221` named a repo
  path (`docs/dev/design/paintkit_massing_demo.png`), which a plugin user
  cannot open anyway and which a validator may not tell from a bundled path.
  It is now a Markdown link to the GitHub blob URL.
- Output names (`out.png`, `grid.png`, `gs.png`, `out-001.png`, the bare
  `.png`) are **not** bundled files, so they stay. Rewording them would make
  the skills worse (they are the commands a user runs) for no checklist gain.
  If the portal's Validate ever holds on them, the fix is a reword there.

## The rule in `scripts/plugin_check.py`

`_check_asset_paths` scans every `.md` under the plugin directory: inline
backtick spans (any run length) and fenced blocks (``` and ~~~). A token ending
in an image or font extension is reported if it names a bundled file (by path
or basename) or contains a `/` (a path outside the bundle). Bare output names,
`https://` URLs and Markdown `![](...)` images pass. Indented (4-space) code
blocks are not scanned: list continuation lines in the skills are indented
that deep. Run only from `check_plugin`, since the Codex check sees the same
directory and would report twice.

Negative control: before the `paint.md` edit the rule reported exactly
`paint.md:221` and nothing else; after it, clean.

## Not done

The checklist row also covers commands, hooks and scripts that name bundled
images or fonts. None exist; the scan covers Markdown only.
