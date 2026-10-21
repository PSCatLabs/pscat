## wasm32 build warning-free in src/font.rs (issue #626, 2026-10-21)

The issue listed four warnings (unused `mut`, never-constructed `CatalogEncoding`
variants, unread `CatalogFace::key`, unused `ALIASES`). Three of them went away
when #566 made the wasm build register catalog faces at runtime (`register_font_bytes`),
which puts those items in use on wasm32. Only the unused `mut` on `entries` in
`catalog_entries` remained: the filesystem scan that pushes onto it is
`#[cfg(not(target_arch = "wasm32"))]`. It is now
`#[cfg_attr(target_arch = "wasm32", allow(unused_mut))]` on that one `let`, so native
builds still get the `mut` lint and nothing else is silenced.

`cargo check` and `cargo clippy -- -D warnings` for
`--lib --target wasm32-unknown-unknown --no-default-features` are both clean.
No CI step added (workflow files are owner paths); suggested: a wasm32 clippy job
running the command above.
