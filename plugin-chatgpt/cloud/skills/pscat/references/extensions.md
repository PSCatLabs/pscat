<!-- cloud: replace "Read this for pscat-only operators (translucency, native noise," -->
Read this for pscat-only operators (translucency and native noise). Back to
[SKILL.md](../SKILL.md).
<!-- cloud: sub "so hand someone a
`--pdf` or `--png`, not a `.ps`," -->
so hand someone a
PDF or PNG, not a `.ps`,
<!-- cloud: sub "For a heavy per-pixel noise
render, see `--bands` in [bands-profile.md](bands-profile.md)." -->
For a heavy per-pixel noise
render, see [bands-profile.md](bands-profile.md).
<!-- cloud: drop "## Save and reload a big numeric array: writereals / readreals" -->
<!-- cloud: drop "## Photo → engraving" -->
<!-- cloud: drop "## Watch it draw (needs a display)" -->
<!-- cloud: replace "## Capabilities (structured, for agents)" -->
## Capabilities (structured, for agents)

`describe_art_capabilities` is the machine-readable inventory of what this build
actually has: fonts, Type 3 faces, artkit's palettes, `lib/pagekit.ps`'s
templates, and the library's major procedures with their parameters.
`get_renderer_info` gives the renderer's version. Prefer them over guessing a
name from prose (the skill files included).
