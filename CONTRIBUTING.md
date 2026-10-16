# Contributing to pscat

Thank you for wanting to help. pscat is a PostScript interpreter written in
Rust, plus a library of PostScript art tools. Bug reports, fixes, new art
libraries, fonts, docs and gallery pieces are all welcome.

By taking part you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md).
To report a security problem, read [`SECURITY.md`](SECURITY.md) first and do not
use a public issue.

## Licensing of contributions

**Inbound = outbound.** What you contribute is licensed the way the project
licenses the same kind of thing:

| You contribute | Licence |
|---|---|
| Code: Rust, scripts, PostScript libraries (`lib/`), tests, build and CI files | **MIT OR Apache-2.0**, at the user's choice |
| Original creative resources: gallery pieces (images and the `.ps` programs that draw them), examples, site assets, art you made for the project | **CC0-1.0** (public-domain dedication) |
| Fonts | They **keep their upstream licence**; the font's own licence file travels with it |
| Documentation | The same as code, unless it is an original creative piece (then CC0-1.0) |

By opening a pull request you state that you wrote the contribution (or have the
right to submit it) and that you license it as in the table. There is no
separate contributor agreement and no copyright assignment: you keep your
copyright.

Please do not submit:

- anything copied from a project whose licence does not allow that (including
  Ghostscript, which is AGPL, and its fonts and code);
- a font without a licence that allows redistribution, with its source and
  licence stated in the PR;
- creative work you are not willing to put in the public domain under CC0-1.0
  (keep it out of the repository, or open an issue to talk it over first).

The licence files at the repository root and the notices file for third-party
material are the authority on exact terms. The licensing work
([#563](https://github.com/jeromebanks/postscript_interpreter/issues/563)) is
what puts them there.

## Getting started

```sh
cargo build --release          # the interpreter and the MCP server
cargo run --release -- examples/<some>.ps   # watch a program draw in a window
cargo run --release -- --png out.png examples/<some>.ps   # no window
```

[`docs/guides/getting-started.md`](docs/guides/getting-started.md) covers the
flags. Before changing code, read
[`docs/dev/HANDOFF.md`](docs/dev/HANDOFF.md) (where things live and the
gotchas that have cost time before) and
[`docs/dev/ARCHITECTURE.md`](docs/dev/ARCHITECTURE.md). The rest of the
contributor notes are indexed in [`docs/dev/README.md`](docs/dev/README.md).
[`AGENTS.md`](AGENTS.md) is the full list of working conventions; it is written
for coding agents, and everything in it applies to people too.

## The workflow

The full lifecycle is [`SDLC.md`](SDLC.md). In short:

1. **Open an issue first** for anything bigger than a typo: what you want to
   change and why, even in a few sentences. The templates in
   `.github/ISSUE_TEMPLATE/` cover a bug, a feature and a font request. For a
   large change, say what you plan before you build it.
2. **Branch** from `main`: `feature/<issue>-<slug>`, or `fix/<issue>-<slug>`
   for a bug.
3. **Write the change** and its tests, in small commits.
4. **Run the quality gate** before opening the PR. It is exactly what CI runs:

   ```sh
   PSCAT_PROFILE=ci ./scripts/selftest.sh \
     && cargo test --profile ci \
     && cargo clippy --all-targets -- -D warnings \
     && cargo fmt --all -- --check
   ```

   The test suite runs a lot of PostScript through the interpreter, so use the
   `ci` profile (optimized, with the checks a debug build has); a plain
   `cargo test` works but is many times slower.
5. **Open a pull request** that says what changed and why, ends with
   `Closes #<issue>` on its own line, and has a test plan. The PR template
   asks for these.
6. **Review.** Every change gets an independent review before it merges. Fix
   what the reviewer finds, or say why you will not.
7. **Merge.** Squash-merge, after CI is green and the review is clean.
   **Pull requests that touch a path listed in
   [`.github/CODEOWNERS`](.github/CODEOWNERS)** (CI workflows, `Cargo.toml` and
   `Cargo.lock`, release scripts, licence files and plugin manifests) are merged
   by the repository owner, never by an agent; see "Who merges" in `SDLC.md`.

### What a good change looks like

- **No `unwrap()` or `expect()` on anything derived from program input.** A
  bad PostScript file is an error the interpreter reports, not a panic.
- **Tests live with the code.** PostScript library changes get a `%%SelfTest`
  block next to the code ([`docs/dev/SELFTEST.md`](docs/dev/SELFTEST.md));
  rendering changes get a pixel test in `tests/`.
- **Comments say why**, not what.
- **Keep the docs true to the code as it is now.** User-facing how-to goes in
  the matching page of [`docs/guides/`](docs/guides/README.md); the README is a
  short front door. Add a short note about your change in
  `notes/<issue>-<slug>.md` (format in [`notes/README.md`](notes/README.md)).
- **Art needs eyes.** If your change alters how something looks, put the
  before and after images in the PR (drag them into the description), not only
  the code.
- **Check your links**: `python3 scripts/check_md_links.py`.

### Adding a font

Open a **font request** issue first (the template asks for the licence and
where the font comes from). Only fonts with a licence that allows redistribution
can go in. See [`docs/dev/FONTS.md`](docs/dev/FONTS.md) and
[`docs/guides/fonts.md`](docs/guides/fonts.md) for how the catalog works.

## Using AI tools

Contributions written with AI assistance are fine and are held to the same bar:
you are responsible for what you submit, including its licence and that you
could explain it. This repository is itself largely developed with coding
agents ([`AGENTS.md`](AGENTS.md), [`CLAUDE.md`](CLAUDE.md)).

## Questions

Open an issue. There is no chat channel.
