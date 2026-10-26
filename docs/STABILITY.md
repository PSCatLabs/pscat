# Stability policy

What pscat promises to keep working between releases, what it does not, and how
a change that breaks something is announced. The release process
([#602](https://github.com/PSCatLabs/pscat/issues/602))
refers to the headings below by name, so they are stable too.

## Starting point

These promises begin with the **first published release, 0.6.0**. Until then
the command line, the MCP server and the library layout are still changing
(the resource resolver, the script-to-subcommand port and the MCP safety
bounds all land first), and nothing here is a guarantee about 0.5.x or
`main`.

## Pre-1.0 versioning

pscat follows [semantic versioning](https://semver.org/) with the usual
pre-1.0 reading:

- `0.MINOR.PATCH`. A **patch** release fixes bugs and never breaks a stable
  surface.
- A **minor** release may break a stable surface, **but only after the
  deprecation period below**, and says so in the release notes.
- `1.0.0` is a promise that a breaking change waits for `2.0.0`. It is not
  scheduled.

Tags are `v<version>`. Font packs ship separately on a calendar-versioned train
(`fonts-YYYY.MM`) and are not covered by the interpreter's semver.

## Stability levels

| Level | Meaning |
|---|---|
| **Stable** | Kept working across patch releases, and across minor releases unless deprecated first. |
| **Experimental** | May change or vanish in any release, with no deprecation period. Marked as such in its docs. |
| **Internal** | Not a surface. Do not depend on it. |

## Surfaces

### Stable (from 0.6.0)

- **The PostScript language.** The operators pscat implements follow the
  PostScript Language Reference (3rd ed.). Where pscat differs from it or from
  Ghostscript, that is a bug to fix, not a feature to preserve. The deliberate
  gaps are recorded in [`docs/dev/ARCHITECTURE.md`](dev/ARCHITECTURE.md);
  filling one is not a break.
- **Command-line flags** listed by `pscat --help` and their documented
  meaning, the exit status (0 on success, non-zero on a PostScript error or a
  failed write), and the output formats (`--png`, `--svg`, `--pdf`). Text on
  stderr (warnings, lint lines, error reports) is for people and may be
  reworded.
- **Documented PostScript library procedures.** A procedure counts as
  documented when it appears in the `pscat --capabilities` catalog or is
  described in a page of [`docs/guides/`](guides/README.md). Its name, its
  stack effect and its meaning are kept. A procedure marked `% @internal` in the
  library source, or in neither the catalog nor a guide, is Internal.
- **MCP tool names and input schemas.** The ten tools `render_postscript`,
  `handwrite`, `chalkboard_math`, `review_sheet`, `describe_art_capabilities`,
  `eval_postscript`, `start_render`, `render_status`, `render_result` and
  `cancel_render`, their
  argument names, types and enumerations. A new optional argument or a new
  tool is not a break; renaming or removing one, or making an optional
  argument required, is.
- **The `pscat-mcp` safety behaviour documented in
  [`SECURITY.md`](../SECURITY.md)** is kept or tightened. A fix that closes a
  hole may break a program that relied on it; that is a security fix, not a
  breaking change (see "Exceptions").

### Stable once published (later)

- **The npm / WebAssembly API** (`web/pscat.js`): stable from the release that
  first publishes it as a package
  ([#566](https://github.com/PSCatLabs/pscat/issues/566)).
  The copy in the repo is Experimental until then.

### Experimental

- **The Rust crate API** (`pscat` as a library: `Interp`, `Gfx` and the rest).
  No public Rust item is stable until the crate is published with a defined
  public API
  ([#565](https://github.com/PSCatLabs/pscat/issues/565)).
- **Rendered pixels.** The look of an art library's output (a brush, a paper, a
  font's rasterisation) can change in any release when it is improved. What is
  kept: the same program and seed give the same page *within one build*.
- **Gallery and example programs**, and the website.
- **`--lint` findings**, `--selftest` output, `--spool`, and the live window's
  behaviour (it is a viewer, not an interface).
- **Anything the docs mark experimental.**

### Internal

- Repository scripts (`scripts/`), CI workflows, and the contributor tooling in
  `.claude/`. Some scripts become subcommands in 0.6.0
  ([#600](https://github.com/PSCatLabs/pscat/issues/600));
  the subcommands are then Stable and the scripts are gone.
- The PostScript library's undocumented helpers, the on-disk layout of `lib/`
  and `fonts/`, and the Rust modules.

## Deprecation

A Stable surface is removed or changed incompatibly only in a **minor** release
that comes **at least one minor release after** it was deprecated:

1. In release `0.N`, the surface still works and says it is deprecated and what
   replaces it: a warning on stderr for a flag or a PostScript procedure (once
   per run), a `deprecated` note in the description of an MCP tool or argument,
   and an entry under "Deprecated" in the release notes.
2. In release `0.N+1` or later, it may be removed. The release notes list it
   under "Removed".

A deprecation is never first announced in the same release that removes the
surface.

### Exceptions

- **Security.** A fix may change or remove a surface immediately, with the
  reason in the release notes.
- **Bugs that contradict the language.** An operator that does not follow the
  PostScript Language Reference is fixed in a patch release.
- **Experimental and Internal** surfaces, as above.

## Reporting a break

If a patch release broke something marked Stable, that is a bug: please
[open an issue](https://github.com/PSCatLabs/pscat/issues/new/choose).
