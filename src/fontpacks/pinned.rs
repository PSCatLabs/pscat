//! The font-pack index this pscat build installs by default (issue
//! #599). A plain `pscat fonts install` fetches this index tag and
//! refuses it unless its SHA-256 matches, then checks every pack
//! against the index and every file against its pack's manifest, so
//! the same pscat version always gets the same font bytes.
//!
//! `None` means no `fonts-*` release has been pinned yet (development
//! builds, and every build until the first font release). Then
//! `pscat fonts install` says so and asks for `--version` or `--from`;
//! it never falls through to `latest`.
//!
//! Do not edit the constant by hand: `python3 scripts/font_packs.py pin
//! --index <font-index.json>` rewrites it from the released index file
//! (docs/dev/RELEASING-FONTS.md, "Pinning a pscat release").

/// A pinned pack index: its release tag and the SHA-256 of the
/// `font-index.json` asset published under that tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pinned {
    pub tag: &'static str,
    pub sha256: &'static str,
}

pub const PINNED_INDEX: Option<Pinned> = None;
