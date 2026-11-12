//! The art catalog: what this build of pscat offers a program, as
//! `pscat --capabilities` prints it and the plugin's
//! `describe_art_capabilities` tool returns it.

/// The catalog as one JSON object, the payload `pscat --capabilities`
/// prints:
///
/// - `pscat_version`: this crate's version;
/// - `catalog_signature`: a hash of the entries' names, kinds and
///   availability, which changes when the catalog does;
/// - `font_packs`: the pinned font-pack release, the installed packs (each
///   with its directory `path` on this machine, and whether it is the
///   pinned release) and any override directories;
/// - `capabilities`: one entry per font, Type 3 face, palette, page
///   template, library procedure (artkit, the style packs and the rest)
///   and native operator, sorted by name then kind. Each has `name`,
///   `kind`, `description`, `parameters`, `source`, `load` (the `run` lines
///   that bring it into scope), `example` and `availability`.
///
/// The fonts are found by scanning the font directories on disk, so call
/// this in the process (and environment) whose renders it should describe.
/// A server that shows the payload to clients may want to remove the
/// `path` keys under `font_packs`, which name directories on the host.
///
/// ```
/// let catalog = pscat::catalog::capabilities();
/// assert_eq!(catalog["pscat_version"], env!("CARGO_PKG_VERSION"));
/// let entries = catalog["capabilities"].as_array().expect("a list");
/// assert!(entries.iter().any(|c| c["name"] == "Helvetica" && c["kind"] == "font"));
/// ```
pub fn capabilities() -> serde_json::Value {
    crate::capabilities::payload_json()
}
