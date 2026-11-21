# #1162: wasm `ps_page_count` uses `Interp::page_count`

`ps_page_count` in `src/wasm.rs` carried its own copy of the page-count rule
(pages shown, plus one when nothing was shown or the canvas holds trailing
art). It now calls `Interp::page_count` (`src/page.rs`).

Finding: the two rules were identical, and both match `Interp::pages`,
`svg_pages` and `pdf_document` (all use `pages.is_empty() ||
has_trailing_art()`), so behaviour is unchanged. The change removes the
chance of drift between the npm `Renderer.pageCount()` and the library.

`tests/wasm.rs::wasm_page_count_matches_the_library` runs five programs (empty,
one showpage, a trailing unshown page, two pages, two showpages after art)
through the wasm build under node and asserts the counts equal the native
`Interp::page_count`, and equal `[1, 1, 2, 2, 2]`.
