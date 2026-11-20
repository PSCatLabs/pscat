//! SVG export — Stage 9 task 4.
//!
//! A recorder that mirrors the paint pipeline: paths are already in
//! device space when they're painted, so every `fill`/`stroke` maps
//! to one SVG element with the same coordinates the raster got
//! (glyphs included — they arrive as outlines). Clips become
//! `clipPath` defs; the *intersection* of nested clips is expressed
//! by wrapping the element in one `<g clip-path=…>` per link of the
//! clip chain. Images are embedded as base64 PNG `<image>` elements
//! carrying the exact device transform the rasterizer used.
//!
//! Text set in the standard fonts can be written as `<text>` instead
//! (#1119, [`SvgText::Text`], `pscat --svg-text`): one element per run
//! of glyphs on a baseline (`crate::textrun`), each glyph at its own
//! `x`, so the layout is pscat's whatever face the viewer substitutes.
//!
//! One SVG document per page, mirroring the multi-page model: the
//! recorder ends a page when showpage/copypage snapshots one, and a
//! pending lazy erase clears the body exactly when it clears the
//! canvas.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::rc::Rc;

use tiny_skia::FillRule;

use crate::gfx::{BlendMode, ClipNode, Composite};
use crate::pattern::{Ink, PatternTile, SvgTile};
use crate::textrun::{GlyphText, Placement, TextRun};

/// How SVG output writes text set in the standard fonts (#1119).
///
/// Set with `Gfx::set_svg_text` (`interp.gfx_mut().set_svg_text(..)`) or
/// `pscat --svg-text`. Only SVG output reads it: PNG and PDF draw glyph
/// outlines either way (real text in PDF is #1026).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SvgText {
    /// Every glyph is its outline, a `<path>`: exact in any viewer. The
    /// default, and the only form before #1119.
    #[default]
    Paths,
    /// Glyphs of the twelve built-in standard faces (Helvetica, Times and
    /// Courier, four styles each) become `<text>` elements: selectable,
    /// searchable and far smaller, drawn by the viewer in a
    /// metric-compatible face (Arial, Times New Roman, Courier New or
    /// Liberation), each glyph at pscat's position. Other fonts keep their
    /// outlines.
    Text,
}

pub struct SvgRecorder {
    width: u32,
    height: u32,
    body: String,
    defs: String,
    /// ClipNode identity (its address) → def id, per page (defs reset
    /// with the page). The `Rc` is held alongside the id so the node
    /// stays alive and its address can't be recycled for a *different*
    /// clip while the entry exists: keyed by a bare address, a freed
    /// node's id was handed to whichever later clip landed on the same
    /// address, which clipped the wrong shape and made the output vary
    /// from run to run (issues #258, #261). `PdfRecorder::open_clip` does
    /// the same for the same reason.
    clip_ids: HashMap<usize, (Rc<ClipNode>, usize)>,
    next_clip: usize,
    /// Next `<linearGradient>`/`<radialGradient>` id (issue #20's `shfill`).
    next_grad: usize,
    pages: Vec<String>,
    /// Omit the white backdrop rect (`--transparent`).
    transparent: bool,
    /// The text run still taking glyphs (#1119), with its clip-group
    /// open/close strings and leftover blend style, fixed when the run
    /// opened. Written to `body` when anything else is drawn, so paint
    /// order holds; `document` writes it after the body, so a page that
    /// ends in text is whole before the run closes.
    run: Option<(TextRun, String, String, String)>,
    /// Prefix of every def id: empty on the page, unique per capture of a
    /// pattern's tile (#1038), whose defs end up beside the page's.
    prefix: String,
    /// Recording an uncoloured pattern's tile (#1038): every colour is
    /// `currentColor`, which the `<pattern>` sets per use.
    uncolored: bool,
    /// Pattern defs on this page (#1038), by tile identity and, for an
    /// uncoloured tile, colour; the `Rc` keeps the address from being
    /// reused while the entry exists, as in `clip_ids`.
    /// Keyed by tile, uncoloured colour and, for a pen-space stroke, the
    /// pen's linear map's bits (the `<pattern>` is placed through its inverse).
    pattern_ids: HashMap<PatternKey, (Rc<PatternTile>, usize)>,
    /// Tiles whose content group is already in this page's defs.
    tile_groups: HashMap<usize, Rc<PatternTile>>,
    next_pattern: usize,
}

pub(crate) type Chain = Option<Rc<ClipNode>>;

/// `shfill`'s geometry, mirroring `crate::shading::ShadingKind` without
/// pulling that module's PsFunction/parsing machinery in here.
#[derive(Clone, Copy)]
pub(crate) enum ShKind {
    Axial {
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
    },
    Radial {
        x0: f32,
        y0: f32,
        r0: f32,
        x1: f32,
        y1: f32,
        r1: f32,
    },
}

impl SvgRecorder {
    pub fn new(width: u32, height: u32) -> Self {
        SvgRecorder {
            width,
            height,
            body: String::new(),
            defs: String::new(),
            clip_ids: HashMap::new(),
            next_clip: 0,
            next_grad: 0,
            pages: Vec::new(),
            transparent: false,
            run: None,
            prefix: String::new(),
            uncolored: false,
            pattern_ids: HashMap::new(),
            tile_groups: HashMap::new(),
            next_pattern: 0,
        }
    }

    /// A recorder for a pattern's tile (#1038): its def ids start with
    /// `prefix`, and an uncoloured tile is written in `currentColor`.
    pub(crate) fn capture(width: u32, height: u32, prefix: String, uncolored: bool) -> Self {
        let mut rec = Self::new(width, height);
        rec.prefix = prefix;
        rec.uncolored = uncolored;
        rec
    }

    /// The captured tile: its defs and its body.
    pub(crate) fn into_tile(mut self) -> SvgTile {
        self.flush_text();
        SvgTile {
            defs: self.defs,
            body: self.body,
        }
    }

    /// The paint attribute value for `ink`: a colour, or a reference to the
    /// pattern def, written into this page's defs on first use. `pen` is the
    /// linear map of a pen-space stroke's own `transform` (#1196), which a
    /// `userSpaceOnUse` pattern would otherwise inherit.
    fn ink_attr(&mut self, ink: &Ink, pen: Option<tiny_skia::Transform>) -> String {
        match ink {
            Ink::Rgb(_) if self.uncolored => "currentColor".to_string(),
            Ink::Rgb(rgb) => Self::color(*rgb),
            Ink::Pattern(tile, rgb, place) => {
                self.pattern_ref(tile, *rgb, place.as_ref().map(|p| p.matrix), pen)
            }
        }
    }

    /// `url(#…)` of `tile`'s `<pattern>` (in `rgb`, if uncoloured).
    ///
    /// Each `<pattern>` is one cell of the lattice, `XStep` x `YStep` at the
    /// pattern-space origin (no `x`/`y`: viewers disagree on whether those
    /// move the content with the cell), holding the tile's content (the captured elements
    /// under the capture-to-pattern matrix, clipped to the `/BBox`) at every
    /// whole step that reaches the cell, so a BBox bigger than the step
    /// wraps as the raster's period does. The captured defs and the BBox
    /// clip go in the page's defs once per tile. Its
    /// `patternTransform` is the tile's adjusted pattern matrix, the one
    /// the raster paints with. An uncoloured tile's content is in
    /// `currentColor`, set by a `color` on the cell's group. For a stroke
    /// drawn in pen space under `pen` (its element `transform`), the
    /// `patternTransform` is `pen`'s inverse times that matrix, so the
    /// pattern lands where it does on the page, as in the raster.
    /// `place` stands in for the tile's matrix when it was made in another
    /// context (#1300): its matrix in this recorder's coordinates.
    fn pattern_ref(
        &mut self,
        tile: &Rc<PatternTile>,
        rgb: (f32, f32, f32),
        place: Option<tiny_skia::Transform>,
        pen: Option<tiny_skia::Transform>,
    ) -> String {
        let Some(st) = &tile.svg else {
            return "none".to_string();
        };
        let tkey = Rc::as_ptr(tile) as usize;
        let colour = tile.uncolored.then(|| Self::color(rgb));
        let pen_bits = pen.map(|l| [l.sx, l.ky, l.kx, l.sy].map(f32::to_bits));
        let key = (tkey, colour.clone(), pen_bits);
        if let Some((_, id)) = self.pattern_ids.get(&key) {
            return format!("url(#{}p{id})", self.prefix);
        }
        let prefix = self.prefix.clone();
        let tid = tile.id;
        let [bx0, by0, bx1, by1] = tile.spec.bbox.map(|v| v as f32);
        if let std::collections::hash_map::Entry::Vacant(e) = self.tile_groups.entry(tkey) {
            e.insert(tile.clone());
            let _ = write!(
                self.defs,
                "{}<clipPath id=\"{prefix}pb{tid}\" clipPathUnits=\"userSpaceOnUse\">\
                 <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath>",
                st.defs,
                fmt_precise(bx0),
                fmt_precise(by0),
                fmt_precise(bx1 - bx0),
                fmt_precise(by1 - by0),
            );
        }
        let id = self.next_pattern;
        self.next_pattern += 1;
        let (ax, ay) = (tile.spec.xstep.abs() as f32, tile.spec.ystep.abs() as f32);
        let matrix = place.unwrap_or(tile.geom.matrix);
        let placement = match pen.and_then(|l| l.invert()) {
            Some(inv) => inv.pre_concat(matrix),
            None => matrix,
        };
        // The content is written out at each step rather than `<use>`d:
        // resvg drops `<use>` copies shifted out of the cell's own box.
        let inv = svg_matrix(tile.geom.capture.invert().unwrap_or_default());
        let mut uses = String::new();
        let (xs, ys) = cell_lattice(cell_steps(bx0, bx1, ax), cell_steps(by0, by1, ay));
        for j in ys {
            for i in xs.clone() {
                uses.push_str("<g");
                if i != 0 || j != 0 {
                    let _ = write!(
                        uses,
                        " transform=\"translate({} {})\"",
                        fmt_precise(i as f32 * ax),
                        fmt_precise(j as f32 * ay)
                    );
                }
                let _ = write!(
                    uses,
                    "><g clip-path=\"url(#{prefix}pb{tid})\"><g transform=\"{inv}\">{}</g></g></g>",
                    st.body
                );
            }
        }
        let content = match &colour {
            Some(c) => format!("<g color=\"{c}\">{uses}</g>"),
            None => uses,
        };
        let _ = write!(
            self.defs,
            "<pattern id=\"{prefix}p{id}\" patternUnits=\"userSpaceOnUse\" \
             width=\"{}\" height=\"{}\" patternTransform=\"{}\">{content}</pattern>",
            fmt_precise(ax),
            fmt_precise(ay),
            svg_matrix(placement),
        );
        self.pattern_ids.insert(key, (tile.clone(), id));
        format!("url(#{prefix}p{id})")
    }

    /// One glyph as text (#1119): continue the open run, or close it and
    /// open a new one. A glyph with no placement (a collapsed font matrix)
    /// paints nothing in the raster either, so it is dropped.
    pub(crate) fn glyph(
        &mut self,
        g: &GlyphText,
        rgb: (f32, f32, f32),
        comp: Composite,
        chain: &Chain,
    ) {
        let Some(p) = Placement::of(g) else {
            return;
        };
        if let Some((run, ..)) = &mut self.run
            && run.accepts(g, &p, rgb, comp, chain)
        {
            run.push(g, &p);
            return;
        }
        self.flush_text();
        let (open, close, leftover) = self.clip_wrappers(chain, Self::blend_style(comp.blend));
        self.run = Some((
            TextRun::new(g, p, rgb, comp, chain.clone()),
            open,
            close,
            leftover,
        ));
    }

    /// Close the open text run into the body.
    fn flush_text(&mut self) {
        if let Some(run) = self.run.take() {
            Self::write_run(&mut self.body, &run);
        }
    }

    fn write_run(
        out: &mut String,
        (run, open, close, leftover): &(TextRun, String, String, String),
    ) {
        let Some(face) = crate::textrun::standard_face(run.fid) else {
            return;
        };
        let mut xs = String::new();
        let mut chars = String::new();
        for (i, g) in run.glyphs.iter().enumerate() {
            if i > 0 {
                xs.push(' ');
            }
            xs.push_str(&fmt_f32(g.x as f32));
            match g.ch {
                '&' => chars.push_str("&amp;"),
                '<' => chars.push_str("&lt;"),
                '>' => chars.push_str("&gt;"),
                c => chars.push(c),
            }
        }
        let transform = if run.upright() {
            String::new()
        } else {
            let f = |v: f64| fmt_frame(v);
            format!(
                " transform=\"matrix({} {} {} {} 0 0)\"",
                f(run.frame[0]),
                f(run.frame[1]),
                f(run.frame[2]),
                f(run.frame[3]),
            )
        };
        let _ = write!(
            out,
            "{open}<text xml:space=\"preserve\"{transform} x=\"{xs}\" y=\"{}\" \
             font-family=\"{}\" font-size=\"{}\"{}{} fill=\"{}\"{}{leftover}>{chars}</text>{close}",
            fmt_f32(run.y as f32),
            face.css_family,
            fmt_f32(run.size as f32),
            if face.bold {
                " font-weight=\"bold\""
            } else {
                ""
            },
            if face.italic {
                " font-style=\"italic\""
            } else {
                ""
            },
            Self::color(run.rgb),
            Self::opacity_attr(run.comp, false),
        );
    }

    pub(crate) fn set_transparent(&mut self, transparent: bool) {
        self.transparent = transparent;
    }

    fn rule_attr(rule: FillRule) -> &'static str {
        match rule {
            FillRule::EvenOdd => "evenodd",
            _ => "nonzero",
        }
    }

    /// Register the whole clip chain, return the group-open prefix, the
    /// matching close suffix, and whatever of `blend_style` the wrappers
    /// didn't absorb (i.e. all of it when there is no chain, so the
    /// caller puts it on the painted element itself).
    ///
    /// `mix-blend-mode` goes on the *outermost* wrapper rather than on
    /// the element, and this is load-bearing rather than stylistic. A
    /// non-`none` `clip-path` establishes a stacking context, so an
    /// element declaring the blend *inside* the group composites
    /// against transparent black instead of against the page — i.e.
    /// renders plain source-over, silently diverging from the raster.
    /// **Confirmed empirically, not reasoned about**: the same scene
    /// (a Multiply fill inside a circular clip over a yellow ground)
    /// exported both ways and rasterized in Chrome gives rgb(51,92,46)
    /// — exactly pscat's own raster — with the declaration on the
    /// group, and rgb(51,102,230), the unblended blue, with it on the
    /// element. That case is reachable by default, not exotic:
    /// `lib/paintkit.ps`'s `pkwash` paints its edge pooling and its
    /// granulation inside a `clip`. Opacity stays on the element: it's
    /// a paint attribute and doesn't interact with isolation.
    fn clip_wrappers(&mut self, chain: &Chain, blend_style: &str) -> (String, String, String) {
        let mut ids = Vec::new();
        let mut node = chain.clone();
        while let Some(n) = node {
            let key = Rc::as_ptr(&n) as usize;
            let id = match self.clip_ids.get(&key) {
                Some(&(_, id)) => id,
                None => {
                    let id = self.next_clip;
                    self.next_clip += 1;
                    self.clip_ids.insert(key, (n.clone(), id));
                    let _ = write!(
                        self.defs,
                        "<clipPath id=\"{prefix}c{id}\" clipPathUnits=\"userSpaceOnUse\">\
                         <path d=\"{}\" clip-rule=\"{}\"/></clipPath>",
                        n.path.to_svg_data(),
                        Self::rule_attr(n.rule),
                        prefix = self.prefix,
                    );
                    id
                }
            };
            ids.push(id);
            node = n.parent.clone();
        }
        let mut open = String::new();
        for (i, id) in ids.iter().enumerate() {
            // i == 0 is the outermost emitted group, the one that
            // composites into the page.
            let style = if i == 0 { blend_style } else { "" };
            let _ = write!(open, "<g clip-path=\"url(#{}c{id})\"{style}>", self.prefix);
        }
        let leftover = if ids.is_empty() { blend_style } else { "" };
        (open, "</g>".repeat(ids.len()), leftover.to_string())
    }

    /// The `fill-opacity`/`stroke-opacity` attribute for one painted
    /// element (issue #47). Empty at full opacity, so a program that
    /// never touches `setalpha` exports exactly the SVG it did before.
    /// The blend mode is *not* here — see `clip_wrappers` for where it
    /// goes and why.
    fn opacity_attr(c: Composite, stroking: bool) -> String {
        if c.alpha >= 1.0 {
            return String::new();
        }
        format!(
            " {}-opacity=\"{}\"",
            if stroking { "stroke" } else { "fill" },
            fmt_f32(c.alpha),
        )
    }

    fn blend_style(blend: BlendMode) -> &'static str {
        match blend {
            BlendMode::Normal => "",
            BlendMode::Multiply => " style=\"mix-blend-mode:multiply\"",
        }
    }

    fn color(rgb: (f32, f32, f32)) -> String {
        let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("#{:02x}{:02x}{:02x}", c(rgb.0), c(rgb.1), c(rgb.2))
    }

    pub(crate) fn fill(
        &mut self,
        d: &str,
        rule: FillRule,
        ink: &Ink,
        comp: Composite,
        chain: &Chain,
    ) {
        self.flush_text();
        let paint = self.ink_attr(ink, None);
        let (open, close, leftover) = self.clip_wrappers(chain, Self::blend_style(comp.blend));
        let _ = write!(
            self.body,
            "{open}<path d=\"{d}\" fill=\"{paint}\" fill-rule=\"{}\"{}{leftover}/>{close}",
            Self::rule_attr(rule),
            Self::opacity_attr(comp, false),
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn stroke(
        &mut self,
        d: &str,
        pen: Option<(tiny_skia::Transform, usize)>,
        ink: &Ink,
        width: f32,
        cap: tiny_skia::LineCap,
        join: tiny_skia::LineJoin,
        miter_limit: f32,
        dash: Option<(&[f32], f32)>,
        comp: Composite,
        chain: &Chain,
    ) {
        self.flush_text();
        // The pen's linear part, as the path's `transform` below writes it.
        let pen_linear =
            pen.map(|(l, _)| tiny_skia::Transform::from_row(l.sx, l.ky, l.kx, l.sy, 0.0, 0.0));
        let paint = self.ink_attr(ink, pen_linear);
        let (open, close, leftover) = self.clip_wrappers(chain, Self::blend_style(comp.blend));
        let cap = match cap {
            tiny_skia::LineCap::Round => "round",
            tiny_skia::LineCap::Square => "square",
            _ => "butt",
        };
        let join = match join {
            tiny_skia::LineJoin::Round => "round",
            tiny_skia::LineJoin::Bevel => "bevel",
            _ => "miter",
        };
        // `d` is in pen space when the CTM is anisotropic (#1196,
        // `Gfx::stroke_space`): SVG strokes in the element's own user
        // space, so a `transform` on the path itself gives the elliptical
        // pen. The clip groups around it stay in device space.
        // Its width and dashes are pen units too, written with the same
        // extra decimals as `d` (`PenSpace::extra_decimals`).
        let num = |v: f32| match pen {
            Some((_, extra)) => fmt_nonzero(v, 3 + extra),
            None => fmt_f32(v),
        };
        let transform = match pen {
            Some((l, _)) => format!(
                " transform=\"matrix({} {} {} {} 0 0)\"",
                fmt_frame(f64::from(l.sx)),
                fmt_frame(f64::from(l.ky)),
                fmt_frame(f64::from(l.kx)),
                fmt_frame(f64::from(l.sy)),
            ),
            None => String::new(),
        };
        let mut extra = String::new();
        if let Some((pattern, phase)) = dash {
            let list: Vec<String> = pattern.iter().map(|d| num(*d)).collect();
            let _ = write!(
                extra,
                " stroke-dasharray=\"{}\" stroke-dashoffset=\"{}\"",
                list.join(" "),
                num(phase)
            );
        }
        let _ = write!(
            self.body,
            "{open}<path{transform} d=\"{d}\" fill=\"none\" stroke=\"{paint}\" stroke-width=\"{}\" \
             stroke-linecap=\"{cap}\" stroke-linejoin=\"{join}\" stroke-miterlimit=\"{}\"{extra}{}{leftover}/>{close}",
            num(width),
            fmt_f32(miter_limit),
            Self::opacity_attr(comp, true),
        );
    }

    /// An image (or imagemask) as embedded PNG: `png` is the encoded
    /// W×H source raster, `t` the device transform mapping source
    /// pixel coordinates to device space.
    ///
    /// `interpolate` (`/Interpolate true`, #730) emits
    /// `image-rendering="auto"`, the viewer's smooth scaling; otherwise
    /// `pixelated` keeps the hard cell edges (the SVG default would smooth).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn image(
        &mut self,
        png: &[u8],
        w: usize,
        h: usize,
        t: [f32; 6],
        interpolate: bool,
        comp: Composite,
        chain: &Chain,
    ) {
        // `setalpha` / `setblendmode` reach images since #766. The blend
        // goes on the outermost wrapper (see `clip_wrappers`); an image
        // takes `opacity`, not `fill-opacity`. Nothing is emitted at the
        // default, so existing exports are unchanged.
        self.flush_text();
        let (open, close, leftover) = self.clip_wrappers(chain, Self::blend_style(comp.blend));
        let opacity = if comp.alpha >= 1.0 {
            String::new()
        } else {
            format!(" opacity=\"{}\"", fmt_f32(comp.alpha))
        };
        let _ = write!(
            self.body,
            "{open}<image width=\"{w}\" height=\"{h}\" preserveAspectRatio=\"none\" \
             image-rendering=\"{}\"{opacity}{leftover} \
             transform=\"matrix({} {} {} {} {} {})\" \
             xlink:href=\"data:image/png;base64,{}\"/>{close}",
            if interpolate { "auto" } else { "pixelated" },
            fmt_f32(t[0]),
            fmt_f32(t[1]),
            fmt_f32(t[2]),
            fmt_f32(t[3]),
            fmt_f32(t[4]),
            fmt_f32(t[5]),
            base64(png),
        );
    }

    /// An imagemask painted with a pattern (#1299): the stencil `png`
    /// (opaque white where it paints, transparent elsewhere) becomes a
    /// `<mask>` def, and the image's footprint, written in device space
    /// with no `transform` so the `userSpaceOnUse` pattern lands where the
    /// raster puts it, is filled with `ink` through it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn stencil(
        &mut self,
        png: &[u8],
        w: usize,
        h: usize,
        t: [f32; 6],
        ink: &Ink,
        comp: Composite,
        chain: &Chain,
    ) {
        self.flush_text();
        // Masks share the gradients' counter: the letter keeps ids apart.
        let id = self.next_grad;
        self.next_grad += 1;
        let prefix = self.prefix.clone();
        let _ = write!(
            self.defs,
            "<mask id=\"{prefix}m{id}\"><image width=\"{w}\" height=\"{h}\" \
             preserveAspectRatio=\"none\" image-rendering=\"pixelated\" \
             transform=\"matrix({} {} {} {} {} {})\" \
             xlink:href=\"data:image/png;base64,{}\"/></mask>",
            fmt_f32(t[0]),
            fmt_f32(t[1]),
            fmt_f32(t[2]),
            fmt_f32(t[3]),
            fmt_f32(t[4]),
            fmt_f32(t[5]),
            base64(png),
        );
        let (wf, hf) = (w as f32, h as f32);
        let mut d = String::new();
        for (i, (x, y)) in [(0.0, 0.0), (wf, 0.0), (wf, hf), (0.0, hf)]
            .into_iter()
            .enumerate()
        {
            let dx = t[0] * x + t[2] * y + t[4];
            let dy = t[1] * x + t[3] * y + t[5];
            let _ = write!(
                d,
                "{}{} {}",
                if i == 0 { "M" } else { "L" },
                fmt_precise(dx),
                fmt_precise(dy)
            );
        }
        d.push('Z');
        let paint = self.ink_attr(ink, None);
        let (open, close, leftover) = self.clip_wrappers(chain, Self::blend_style(comp.blend));
        let _ = write!(
            self.body,
            "{open}<path d=\"{d}\" fill=\"{paint}\" mask=\"url(#{prefix}m{id})\"{}{leftover}/>{close}",
            Self::opacity_attr(comp, false),
        );
    }

    /// `shfill` (issue #20): a native `<linearGradient>`/`<radialGradient>`
    /// def, painted as a full-page rect (clipped like any other fill).
    /// `gradientUnits="userSpaceOnUse"` plus `gradientTransform` set to
    /// the CTM mirrors the same trick `Gfx::shfill` uses for the raster
    /// path — Coords stay in user space, the CTM does the mapping.
    /// SVG's matrix(a b c d e f) is the same convention as the CTM's
    /// own [a b c d tx ty] (`ops/matrix.rs`), so `ctm` here is passed
    /// through as-is: `[sx, ky, kx, sy, tx, ty]`.
    ///
    /// An end `extend` leaves off (#1040) is drawn as SVG can: the gradient
    /// runs on past it (`extended_for_svg`) and is transparent there, so the
    /// padding beyond is transparent too. SVG paints only a point's larger
    /// root, so a radial whose circles are not nested, its end not extended,
    /// leaves a point whose larger root is past the end unpainted where the
    /// raster and PDF paint its smaller root (the circles-not-nested gap
    /// below already applies to such a radial).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn shfill(
        &mut self,
        kind: &ShKind,
        ctm: [f32; 6],
        stops: &[(f32, f32, f32, f32)],
        extend: [bool; 2],
        region_d: &str,
        comp: Composite,
        chain: &Chain,
    ) {
        self.flush_text();
        // Lengthening the gradient past an end (`extended_for_svg`) can
        // overflow f32 for geometry near f32::MAX, and `inf` is no SVG
        // number. Then it is done at an eighth of the size, the transform
        // multiplying it back (a power of two, so exact); if even that is
        // not finite, nothing is written, as the PDF writes nothing.
        let mut ctm = ctm;
        let mut extended = extended_for_svg(kind, stops, extend);
        if !sh_finite(&extended.0) {
            extended = extended_for_svg(&sh_scaled(kind, 0.125), stops, extend);
            for v in &mut ctm[..4] {
                *v *= 8.0;
            }
        }
        if !sh_finite(&extended.0) || !ctm.iter().all(|v| v.is_finite()) {
            return;
        }
        let (kind, stops) = &extended;
        let id = self.next_grad;
        self.next_grad += 1;
        let stop_tags = |stops: &[SvgStop]| {
            let mut tags = String::new();
            for &(pos, r, g, b, a) in stops {
                let _ = write!(
                    tags,
                    "<stop offset=\"{}\" stop-color=\"{}\"{}/>",
                    fmt_precise(pos),
                    Self::color((r, g, b)),
                    if a < 1.0 { " stop-opacity=\"0\"" } else { "" },
                );
            }
            tags
        };
        let matrix = format!(
            "matrix({} {} {} {} {} {})",
            fmt_precise(ctm[0]),
            fmt_precise(ctm[1]),
            fmt_precise(ctm[2]),
            fmt_precise(ctm[3]),
            fmt_precise(ctm[4]),
            fmt_precise(ctm[5]),
        );
        match *kind {
            ShKind::Axial { x0, y0, x1, y1 } => {
                let _ = write!(
                    self.defs,
                    "<linearGradient id=\"{prefix}g{id}\" gradientUnits=\"userSpaceOnUse\" \
                     x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" spreadMethod=\"pad\" \
                     gradientTransform=\"{matrix}\">{}</linearGradient>",
                    fmt_precise(x0),
                    fmt_precise(y0),
                    fmt_precise(x1),
                    fmt_precise(y1),
                    stop_tags(stops),
                    prefix = self.prefix,
                );
            }
            ShKind::Radial {
                x0,
                y0,
                r0,
                x1,
                y1,
                r1,
            } => {
                // SVG's radialGradient is (cx,cy,r) for the outer/end
                // circle plus an optional (fx,fy,fr) focal circle for
                // the start — exactly ShadingType 3's two-circle
                // model, with one added constraint SVG enforces that
                // PostScript doesn't: fr <= r. PostScript allows a
                // *shrinking* radial (r0 > r1, e.g. a burst that
                // contracts toward its center — confirmed against gs),
                // which would need fr > r to represent directly and
                // gets silently clamped/degenerated by SVG if emitted
                // that way. Swap which circle is "outer" vs "focal"
                // when that happens, and reverse the stops to
                // compensate (offset' = 1 - offset, order reversed so
                // they stay non-decreasing) — caught by a Codex
                // review; every hand-tested example here only grows.
                //
                // Documented gap SVG's model can't fully absorb even
                // with that swap: SVG's cone construction only renders
                // *faithfully* when the focal circle is entirely
                // inside the outer one (distance(centers) + focal_r <=
                // outer_r) — an off-center two-circle shading that
                // fails that (a real, valid ShadingType 3; PostScript
                // has no such constraint) isn't detected or worked
                // around here, so its SVG export can visibly diverge
                // from the raster (browsers vary in how they degrade
                // it). Caught by a Codex review pointing out this
                // file's *own* two-circle test/example geometry was
                // itself uncontained — both were replaced with a
                // contained one; a genuinely faithful fallback for the
                // uncontained case (e.g. rasterizing just that
                // gradient into an embedded PNG, the way `image`
                // already embeds bitmap content) is out of scope here.
                let (outer_x, outer_y, outer_r, focal_x, focal_y, focal_r, shrinking) = if r0 > r1 {
                    (x0, y0, r0, x1, y1, r1, true)
                } else {
                    (x1, y1, r1, x0, y0, r0, false)
                };
                let tags = if shrinking {
                    let mut reversed: Vec<SvgStop> = stops
                        .iter()
                        .map(|&(p, r, g, b, a)| (1.0 - p, r, g, b, a))
                        .collect();
                    reversed.reverse();
                    stop_tags(&reversed)
                } else {
                    stop_tags(stops)
                };
                // fx/fy (the *point*) are plain SVG 1.1 and always
                // emitted, even when the focal point coincides with
                // the outer center (SVG already defaults them to
                // cx/cy there, so this is a no-op, not a behavior
                // change) — omitting them whenever focal_r happened to
                // be 0 was wrong whenever the focal point *wasn't*
                // also the outer circle's center, silently recentering
                // the gradient on the wrong point (also caught by a
                // Codex review: every hand-tested example here used a
                // concentric burst, which never exposed it). Only `fr`
                // (SVG2, less consistently supported) stays gated on
                // focal_r > 0 — the common burst-from-a-point case
                // needs nothing beyond fx/fy to render identically
                // everywhere.
                let mut extra = format!(
                    " fx=\"{}\" fy=\"{}\"",
                    fmt_precise(focal_x),
                    fmt_precise(focal_y)
                );
                if focal_r > 0.0 {
                    let _ = write!(extra, " fr=\"{}\"", fmt_precise(focal_r));
                }
                let _ = write!(
                    self.defs,
                    "<radialGradient id=\"{prefix}g{id}\" gradientUnits=\"userSpaceOnUse\" \
                     cx=\"{}\" cy=\"{}\" r=\"{}\"{extra} spreadMethod=\"pad\" \
                     gradientTransform=\"{matrix}\">{tags}</radialGradient>",
                    fmt_precise(outer_x),
                    fmt_precise(outer_y),
                    fmt_precise(outer_r),
                    prefix = self.prefix,
                );
            }
        }
        let (open, close, leftover) = self.clip_wrappers(chain, Self::blend_style(comp.blend));
        let _ = write!(
            self.body,
            "{open}<path d=\"{region_d}\" fill=\"url(#{}g{id})\"{}{leftover}/>{close}",
            self.prefix,
            Self::opacity_attr(comp, false),
        );
    }

    /// The page size from now on (`setpagedevice`, #909); finished pages
    /// keep theirs.
    pub(crate) fn set_size(&mut self, width: u32, height: u32) {
        self.flush_text();
        self.width = width;
        self.height = height;
    }

    /// The lazy showpage erase (or an explicit erasepage): the new
    /// page starts blank.
    pub(crate) fn erase(&mut self) {
        self.run = None;
        self.body.clear();
        self.defs.clear();
        self.clip_ids.clear();
        self.pattern_ids.clear();
        self.tile_groups.clear();
    }

    fn document(&self) -> String {
        let mut body = std::borrow::Cow::Borrowed(self.body.as_str());
        if let Some(run) = &self.run {
            let mut b = self.body.clone();
            Self::write_run(&mut b, run);
            body = std::borrow::Cow::Owned(b);
        }
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" \
             xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
             width=\"{w}pt\" height=\"{h}pt\" viewBox=\"0 0 {w} {h}\">\
             {backdrop}{defs}{body}</svg>\n",
            backdrop = if self.transparent {
                String::new()
            } else {
                format!(
                    "<rect width=\"{}\" height=\"{}\" fill=\"#ffffff\"/>",
                    self.width, self.height
                )
            },
            w = self.width,
            h = self.height,
            defs = if self.defs.is_empty() {
                String::new()
            } else {
                format!("<defs>{}</defs>", self.defs)
            },
        )
    }

    /// showpage/copypage: snapshot the page document. (The body is
    /// *not* cleared here — the erase is lazy, same as the canvas.)
    pub(crate) fn end_page(&mut self) {
        self.flush_text();
        self.pages.push(self.document());
    }

    /// All finished pages, plus the live body if it holds unshown art.
    pub fn finish(&self, trailing_art: bool) -> Vec<String> {
        let mut pages = self.pages.clone();
        if pages.is_empty() || trailing_art {
            pages.push(self.document());
        }
        pages
    }
}

/// A `<pattern>`'s identity: the tile, its colour if uncoloured, and the
/// bits of the pen map a pen-space stroke places it through.
type PatternKey = (usize, Option<String>, Option<[u32; 4]>);

/// `matrix(a b c d e f)` at full precision.
fn svg_matrix(t: tiny_skia::Transform) -> String {
    format!(
        "matrix({} {} {} {} {} {})",
        fmt_precise(t.sx),
        fmt_precise(t.ky),
        fmt_precise(t.kx),
        fmt_precise(t.sy),
        fmt_precise(t.tx),
        fmt_precise(t.ty),
    )
}

/// The whole steps `k` (in units of `step`) at which `[lo, hi]` shifted by
/// `k * step` overlaps the cell `[0, step)`.
fn cell_steps(lo: f32, hi: f32, step: f32) -> std::ops::RangeInclusive<i64> {
    let (lo, hi, step) = (f64::from(lo), f64::from(hi), f64::from(step));
    let first = (-hi / step).floor() as i64 + 1;
    let last = (1.0 - lo / step).ceil() as i64 - 1;
    first..=last
}

/// Both axes' [`cell_steps`], cut to the copy budget the raster's fold has
/// (`crate::pattern::MAX_FOLD_COPIES`), so SVG keeps every copy the PNG
/// draws. `makepattern` refuses a tile over that budget in device pixels;
/// the pattern-space count can still be larger when a step is under a
/// pixel, and then each axis keeps the steps nearest 0.
fn cell_lattice(
    xs: std::ops::RangeInclusive<i64>,
    ys: std::ops::RangeInclusive<i64>,
) -> (std::ops::RangeInclusive<i64>, std::ops::RangeInclusive<i64>) {
    let len = |r: &std::ops::RangeInclusive<i64>| {
        u64::try_from(r.end().saturating_sub(*r.start()).saturating_add(1)).unwrap_or(0)
    };
    let budget = crate::pattern::MAX_FOLD_COPIES;
    let (nx, ny) = (len(&xs), len(&ys));
    if nx.saturating_mul(ny) <= budget {
        return (xs, ys);
    }
    let side = budget.isqrt();
    let (kx, ky) = if nx <= side {
        (nx, budget / nx.max(1))
    } else if ny <= side {
        (budget / ny.max(1), ny)
    } else {
        (side, side)
    };
    (near_zero(xs, kx), near_zero(ys, ky))
}

/// The `n` consecutive steps of `r` nearest 0.
fn near_zero(r: std::ops::RangeInclusive<i64>, n: u64) -> std::ops::RangeInclusive<i64> {
    let (first, last) = (*r.start(), *r.end());
    let n = i64::try_from(n.max(1)).unwrap_or(i64::MAX);
    let centre = 0i64.clamp(first, last.max(first));
    let start = centre.saturating_sub((n - 1) / 2).max(first);
    let end = start.saturating_add(n - 1).min(last);
    end.saturating_sub(n - 1).max(first)..=end
}

fn fmt_f32(v: f32) -> String {
    // Enough precision for device coordinates, no trailing noise.
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// `v` with `decimals` fractional digits, trailing zeros trimmed; a nonzero
/// value that would round to 0 is written at full precision instead, so a
/// thin pen-space stroke width never reads as 0 (#1196).
fn fmt_nonzero(v: f32, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    let t = s.trim_end_matches('0').trim_end_matches('.');
    match t {
        "0" | "-0" if v != 0.0 => fmt_precise(v),
        "-0" => "0".to_string(),
        _ => t.to_string(),
    }
}

/// A text frame's matrix entries, unit-scale: six decimals keep a
/// glyph's position across a page-sized coordinate within a thousandth.
fn fmt_frame(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

/// Full round-trip precision, for values a `gradientTransform` (or
/// this file's own straddle-epsilon stop offsets) can multiply back up
/// by an arbitrary factor — `fmt_f32`'s fixed 3 decimals is fine for
/// device-pixel-space path data, but gradient-*local* coordinates
/// aren't in that space: a shading authored in tiny raw units under a
/// large CTM (or a hairline offset pair marking a stitching
/// discontinuity — see `interior_bound_positions`) would have its
/// meaningful digits truncated away before the transform ever runs
/// (confirmed against gs: `x2="0.0004"` under a `100000`-scale
/// gradientTransform spans 40 real pixels; rounded to `x2="0"` it
/// collapses the whole gradient). `{v}`'s plain Display impl for f32
/// already gives the shortest string that round-trips exactly, so no
/// manual truncation is needed here the way `fmt_f32` needs one.
/// A gradient stop as the SVG writes it: position, RGB, and opacity (1, or
/// 0 for the transparent run past an end that does not extend).
type SvgStop = (f32, f32, f32, f32, f32);

/// The shading for SVG with each end `extend` leaves off made transparent
/// (#1040). SVG pads with the outermost stop's colour, so the gradient is
/// lengthened past that end, in its own t, and given a transparent run
/// there, starting with a zero-width step at the real end: that run's stop
/// is the outermost, so the padding is transparent, and no renderer has to
/// settle two stops at one terminal offset. A radial is run back only as
/// far as a circle of radius 0, past which no circle exists to paint
/// (a growing radial's start, a shrinking one's end), and not at all from a
/// radius of 0. Both ends extended is the shading as it was.
fn extended_for_svg(
    kind: &ShKind,
    stops: &[(f32, f32, f32, f32)],
    extend: [bool; 2],
) -> (ShKind, Vec<SvgStop>) {
    // How far each end runs on, in units of the shading's own t.
    let (before, after) = match *kind {
        ShKind::Axial { .. } => (f32::from(!extend[0]), f32::from(!extend[1])),
        ShKind::Radial { r0, r1, .. } => {
            let dr = r1 - r0;
            let before = match extend[0] {
                true => 0.0,
                false if dr > 0.0 => (r0 / dr).min(1.0),
                false => 1.0,
            };
            let after = match extend[1] {
                true => 0.0,
                false if dr < 0.0 => (r1 / -dr).min(1.0),
                false => 1.0,
            };
            (before, after)
        }
    };
    let mut opaque: Vec<SvgStop> = stops
        .iter()
        .map(|&(t, r, g, b)| (t, r, g, b, 1.0))
        .collect();
    // A hard edge at the end, when it pads (two stops at offset 1): the
    // raster pads with the outermost stop, but renderers differ on two
    // stops at one terminal offset, so the inner one is moved in by a
    // sliver, `EDGE` (as the PDF writer does, #1028), or onto its
    // neighbour if that is nearer (#1041). Only the end needs it: a
    // stitching bound at the domain's start is not interior
    // (`interior_bound_positions`), so no stop of another colour lands at 0,
    // while one just under 1 can round to 1 in f32.
    const EDGE: f32 = 1e-6;
    let n = opaque.len();
    if extend[1] && n >= 3 && opaque[n - 2].0 >= opaque[n - 1].0 {
        opaque[n - 2].0 = (1.0 - EDGE).max(opaque[n - 3].0).min(opaque[n - 1].0);
    }
    if before == 0.0 && after == 0.0 {
        return (*kind, opaque);
    }
    let span = 1.0 + before + after;
    let at = |t: f32| (t + before) / span;
    let mut out = Vec::with_capacity(stops.len() + 4);
    if let Some(&(_, r, g, b)) = stops.first()
        && before > 0.0
    {
        out.push((0.0, r, g, b, 0.0));
        out.push((at(0.0), r, g, b, 0.0));
    }
    out.extend(
        opaque
            .into_iter()
            .map(|(t, r, g, b, a)| (at(t), r, g, b, a)),
    );
    if let Some(&(_, r, g, b)) = stops.last()
        && after > 0.0
    {
        out.push((at(1.0), r, g, b, 0.0));
        out.push((1.0, r, g, b, 0.0));
    }
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let kind = match *kind {
        ShKind::Axial { x0, y0, x1, y1 } => ShKind::Axial {
            x0: lerp(x0, x1, -before),
            y0: lerp(y0, y1, -before),
            x1: lerp(x0, x1, 1.0 + after),
            y1: lerp(y0, y1, 1.0 + after),
        },
        ShKind::Radial {
            x0,
            y0,
            r0,
            x1,
            y1,
            r1,
        } => ShKind::Radial {
            x0: lerp(x0, x1, -before),
            y0: lerp(y0, y1, -before),
            r0: lerp(r0, r1, -before).max(0.0),
            x1: lerp(x0, x1, 1.0 + after),
            y1: lerp(y0, y1, 1.0 + after),
            r1: lerp(r0, r1, 1.0 + after).max(0.0),
        },
    };
    (kind, out)
}

/// Whether every number of `kind` is finite.
fn sh_finite(kind: &ShKind) -> bool {
    match *kind {
        ShKind::Axial { x0, y0, x1, y1 } => [x0, y0, x1, y1].iter().all(|v| v.is_finite()),
        ShKind::Radial {
            x0,
            y0,
            r0,
            x1,
            y1,
            r1,
        } => [x0, y0, r0, x1, y1, r1].iter().all(|v| v.is_finite()),
    }
}

/// `kind` with every length multiplied by `k`.
fn sh_scaled(kind: &ShKind, k: f32) -> ShKind {
    match *kind {
        ShKind::Axial { x0, y0, x1, y1 } => ShKind::Axial {
            x0: x0 * k,
            y0: y0 * k,
            x1: x1 * k,
            y1: y1 * k,
        },
        ShKind::Radial {
            x0,
            y0,
            r0,
            x1,
            y1,
            r1,
        } => ShKind::Radial {
            x0: x0 * k,
            y0: y0 * k,
            r0: r0 * k,
            x1: x1 * k,
            y1: y1 * k,
            r1: r1 * k,
        },
    }
}

fn fmt_precise(v: f32) -> String {
    format!("{v}")
}

/// Standard base64, no external dependency needed for one use.
fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
