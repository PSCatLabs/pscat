# #683: the plugin icon

The owner picked candidate D of four (comment on #683, 2026-10-26): "That
better represents that it is postscript related. A reminds people too much
of Napster". D is a sheet of paper whose top corners are cat ears, with a
turned-up corner and `%!` (Courier-Bold) as its face. The rejected A, B and C
candidates and the contact sheet stay on branch
`feature/683-plugin-icon-pscat-logo` (commit 6b74021) for history; this PR
brings over only D.

## Files

- `art/logo/pscat-logo-d.ps`: the source (CC0). Its header has the three
  render commands: 384 pt page, `--dpi 96` for 512 px, `--dpi 9` for 48 px,
  `--dpi 192` for the 1024 px icon.
- `art/logo/renders/pscat-logo-d-{512,48}.png`: renders for docs.
- `plugin/.claude-plugin/icon.png`: the 1024 px icon.

## Where the manifests expect the icon

- **Claude Code / Anthropic's directory**: the top-level `icon` field in
  `plugin/.claude-plugin/plugin.json`, a `./` path to an image inside the
  plugin. The directory reads it for the listing; Claude Code ignores it at
  load time. It is not allowed in a marketplace entry (`claude plugin
  validate` reports it as unknown there). Source: Plugin manifest
  reference, "Directory listing fields",
  https://code.claude.com/docs/en/plugins-reference. `claude plugin
  validate` (2.1.309) also looks for a file by path: its `ICON_MISSING`
  warning says it checks `icon`, then `.claude-plugin/icon.png` or `.svg`,
  `assets/icon.png` or `.svg`. So the file alone would be found; the field is
  set anyway so nothing depends on the fallback. Requirements, from the same
  warning: a square PNG or JPEG, 512 to 2048 px a side, under 2 MB, no SVG
  or WebP, and it may become the listing icon only once, at the first save
  or submit in the developer portal.
- **Codex**: `extensions.com.openai.interface.composerIcon` and `.logo` in
  the root `plugin/plugin.json`, `./` paths relative to the plugin root.
  Source: https://developers.openai.com/codex/plugins/build. That page
  suggests `./assets/` but gives no size; both point at the same 1024 px
  file. (Third-party pages say the composer icon shows at 32 px; unverified.)

## Font

The brief expected #757 (the embedded standard 35) to change the `%!`
glyphs. It did not: `Courier-Bold` was already one of the 12 Liberation
builtins (`LiberationMono-Bold.ttf`, `src/font.rs`) before #757, which
embedded only the other 23 names. Re-rendering on main gives PNGs
byte-identical to the 2026-10-23 candidates, so D is unchanged.
