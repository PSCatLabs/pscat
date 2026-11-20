# #1324: shfill with an extreme /BBox no longer panics

`shfill` of an axial or radial shading (ShadingType 2 or 3) with a `/BBox` such as
`[-1e10 -1e10 1e10 1e10]` panicked in tiny-skia's anti-aliased `fill_path`
(`alpha_runs.rs`), which cannot take corners around 1e9 device pixels away. It
predates 0.7.1 (rc.4 and rc.5 do it too); #1322 fixed the same class for Type 1.

`Gfx::bbox_in_guard` cuts the box's four device corners (`clip_polygon`, in f64) to the
page grown by its own size on every side before `paint_shfill` sees it. A box whose
corners are all inside that guard is not touched, so ordinary renders are bit-identical;
a cut box still covers the same page pixels (the guard is outside the page, and an
extreme box has no edge on it). A box wholly beyond the guard becomes empty and paints
nothing, as gs. SVG and PDF keep the box as given: they never rasterise it.

Pinned by `tests/shfill_extreme_bbox.rs`: ±1e10, ±1e9, ±1e5 and a one-sided extreme box
give the pixels of a page-sized box, for both types, also under a rotated CTM, and a box
far off the page paints nothing.
