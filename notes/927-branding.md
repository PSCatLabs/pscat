## Branding: one tagline, the logo, and a document beside the art (issue #927, 2026-10-30)

The owner's call (comment on #927, 2026-10-30): follow ChatGPT's branding
advice. pscat's tagline is **"PostScript for precision and imagination."**;
the PSCatLabs org's is **"Where precision meets imagination."**. Earlier
decisions in the issue body still hold: documents and art are two equal
halves, the live window is a feature rather than the headline, pscat aims to
replace Ghostscript for most work and uses it as its correctness reference,
and no speed, memory or large-document claims until #929 publishes numbers.

### What changed

- **Descriptions.** `Cargo.toml`, `npm/package.json`, `server.json` (96 of
  the MCP Registry's 100 characters, checked by `test_release_stages.py`),
  both plugin manifests, the marketplace entry and Codex's `shortDescription`
  all lead with the tagline. The plugin text keeps its second sentence about
  using an installed pscat or downloading one: directory reviewers want it.
  Crate, npm and Registry text is frozen per release, so it reaches those
  listings with 0.7.0.
- **README and site.** The README opens with the logo (linked to its `.ps`,
  captioned as drawn by pscat), the tagline, ChatGPT's expanded pitch
  (checked against the code: page templates, dataviz, typewriter, PDF
  metadata, wasm, MCP), the Ghostscript stance and a document+art pair. The
  site's home page has the same hero, a new "Documents" card, `og:`/Twitter
  tags pointing at the social preview, and the logo as favicon.
  "Not a Ghostscript replacement" is gone from both.
- **Ghostscript wording.** CI does not install gs, so the gs-driven tests run
  only locally; many `ghostscript_accepts_*` tests check only that gs
  accepts a file (`docs/dev/GS_CHECK_INVENTORY.md`). The claim is the
  narrower one that holds: test expectations are pinned against
  Ghostscript's output.
- **"Watch it draw".** Only the headline uses changed (crate description,
  site title/h1, README opening, getting-started's first line). Every place
  it describes the window or the playground as a feature stays.

### Assets

- `art/brand/social-preview.ps` -> `art/brand/renders/social-preview.png`,
  1280x640, for the repo's social preview and the site's `og:image`. It
  pastes no images: it runs the logo, `examples/template_certificate.ps` and
  `gallery/evening_stream.ps` each in a clipped, scaled card under
  save/restore with `showpage` stubbed. paintkit renders correctly under the
  scaled CTM (compared by eye with the gallery render). About 13 s to render.
- `art/brand/renders/certificate.png`: the certificate example cropped by
  page size, `pscat --page 612x580 --dpi 144 --png
  art/brand/renders/certificate.png examples/template_certificate.ps`. It is
  the document half of the README, site and org-profile pairs.
- The document specimen: Field Notes reads as a journal page (art), and
  `examples/dataviz_paper.ps` has captions that collide with its bars, so
  the plain, crisply typeset certificate won.
- `scripts/build_site.sh` copies the logo renders, the preview and the
  certificate into `_site/assets/brand/`.

### For the owner

`docs/brand/org-profile-README.md` is the draft PSCatLabs profile (absolute
URLs, for a `PSCatLabs/.github` repo). The org and repo settings (name,
description, URL, avatar, topics, social preview upload) are listed with
exact values in the PR's Decision block; none were changed by the agent.

### Deferred

- A lab variant of the logo for the org avatar (the owner's org-scope
  comment raised it); the avatar uses logo d as-is for now.
- Comparative performance wording, until #929.
