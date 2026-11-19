//! Tiling patterns (#1038): `makepattern`, `setpattern` and the
//! `/Pattern` colour space, PatternType 1, in PNG, SVG and PDF.
//!
//! Each picture renders three ways and is held against the raster:
//! Ghostscript rasterising our PDF, resvg rasterising our SVG, and (for
//! the semantics) Ghostscript running the PostScript itself. The PDF must
//! hold a real `/Pattern` resource, not a raster or a flat colour.

use std::process::Command;

mod common;

use common::pdf_text;
use pscat::Interp;
use tiny_skia::Pixmap;

const SIZE: u32 = 400;
const BLOCK: u32 = 10;

/// A diagonal hatch whose lines cross the tile's edges, filling a circle.
const HATCH: &str = "
<< /PatternType 1 /PaintType 1 /TilingType 1
   /BBox [0 0 8 8] /XStep 8 /YStep 8
   /PaintProc { pop 0 0 0.6 setrgbcolor 1.5 setlinewidth
                -1 -1 moveto 9 9 lineto stroke
                -1 7 moveto 1 9 lineto stroke
                7 -1 moveto 9 1 lineto stroke } >> matrix makepattern /hatch exch def
hatch setpattern
200 200 150 0 360 arc fill
";

/// One uncoloured pattern of dots in two colours. The PaintProc's own
/// colour is ignored.
const UNCOLOURED: &str = "
<< /PatternType 1 /PaintType 2 /TilingType 1
   /BBox [0 0 12 12] /XStep 12 /YStep 12
   /PaintProc { pop 1 0 0 setrgbcolor 6 6 4 0 360 arc fill } >> matrix makepattern /dots exch def
[/Pattern /DeviceRGB] setcolorspace
0.8 0.1 0.1 dots setcolor 20 20 170 360 rectfill
0.1 0.3 0.8 dots setpattern 210 20 170 360 rectfill
";

/// A step of 7.3 pt (not a whole pixel), a cell larger than its step,
/// a stroke and text painted with patterns.
const MIXED: &str = "
<< /PatternType 1 /PaintType 1 /TilingType 1
   /BBox [0 0 7.3 7.3] /XStep 7.3 /YStep 7.3
   /PaintProc { pop 0.9 0.4 0 setrgbcolor 0 0 3.65 3.65 rectfill } >> matrix makepattern /frac exch def
frac setpattern 10 210 180 180 rectfill
<< /PatternType 1 /PaintType 1 /TilingType 1
   /BBox [-6 -6 16 16] /XStep 10 /YStep 10
   /PaintProc { pop 0 0.5 0 setrgbcolor 5 5 7 0 360 arc 0.7 setlinewidth stroke } >> matrix makepattern /big exch def
big setpattern 210 210 180 180 rectfill
frac setpattern 12 setlinewidth 30 30 moveto 170 170 lineto stroke
/Helvetica-Bold findfont 60 scalefont setfont big setpattern 215 120 moveto (Pat) show
";

struct Outputs {
    png: Pixmap,
    svg: String,
    pdf: Vec<u8>,
}

fn render(src: &str) -> Outputs {
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.gfx_mut().enable_svg();
    it.gfx_mut().enable_pdf();
    it.run_source(src.as_bytes())
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    let svg = it.gfx().svg_pages().expect("svg on").remove(0);
    Outputs {
        png: it.gfx().pixmap.clone(),
        svg,
        pdf: it.gfx().pdf_document().expect("pdf on"),
    }
}

/// gs's raster of `input` at `SIZE` px, rendered at `k` times the
/// resolution and box-averaged down.
fn gs_png(input: &[u8], ext: &str, name: &str) -> Pixmap {
    gs_png_at(input, ext, name, 1)
}

fn gs_png_at(input: &[u8], ext: &str, name: &str, k: u32) -> Pixmap {
    let dir = common::scratch_dir("pscat_patterns");
    let src = dir.join(format!("{name}.{ext}"));
    let out = dir.join(format!("{name}.{ext}.png"));
    std::fs::write(&src, input).expect("write input");
    let status = Command::new("gs")
        .args([
            "-q",
            "-dNOPAUSE",
            "-dBATCH",
            "-dSAFER",
            "-sDEVICE=png16m",
            "-dGraphicsAlphaBits=4",
            "-dTextAlphaBits=4",
            &format!("-g{}x{}", SIZE * k, SIZE * k),
            &format!("-r{}", 72 * k),
        ])
        .arg(format!("-o{}", out.display()))
        .arg(&src)
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected {name}.{ext}");
    let data = std::fs::read(&out).expect("read gs png");
    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&out);
    let big = Pixmap::decode_png(&data).expect("decode png");
    if k == 1 {
        return big;
    }
    let mut small = Pixmap::new(SIZE, SIZE).expect("pixmap");
    let (bw, src_px) = (big.width() as usize, big.data());
    let k = k as usize;
    for y in 0..SIZE as usize {
        for x in 0..SIZE as usize {
            for c in 0..4 {
                let mut sum = 0u32;
                for dy in 0..k {
                    for dx in 0..k {
                        sum += u32::from(src_px[((y * k + dy) * bw + x * k + dx) * 4 + c]);
                    }
                }
                small.data_mut()[(y * SIZE as usize + x) * 4 + c] = (sum / (k * k) as u32) as u8;
            }
        }
    }
    small
}

fn resvg_png(svg: &str) -> Pixmap {
    let tree = resvg::usvg::Tree::from_data(svg.as_bytes(), &resvg::usvg::Options::default())
        .expect("parse SVG");
    let size = tree.size();
    let mut pm = resvg::tiny_skia::Pixmap::new(SIZE, SIZE).expect("pixmap");
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(
            SIZE as f32 / size.width(),
            SIZE as f32 / size.height(),
        ),
        &mut pm.as_mut(),
    );
    let mut out = Pixmap::new(pm.width(), pm.height()).expect("pixmap");
    out.data_mut().copy_from_slice(pm.data());
    out
}

/// hayro, an independent pure-Rust PDF rasteriser, on our PDF: it
/// anti-aliases pattern cells as pscat does, where gs paints them aliased.
fn hayro_png(pdf: &[u8]) -> Pixmap {
    let doc = hayro::Pdf::new(std::sync::Arc::new(pdf.to_vec())).expect("parse PDF");
    let pages = doc.pages();
    let pm = hayro::render(
        &pages[0],
        &hayro::InterpreterSettings::default(),
        &hayro::RenderSettings {
            x_scale: 1.0,
            y_scale: 1.0,
            ..Default::default()
        },
    );
    let (w, h) = (u32::from(pm.width()), u32::from(pm.height()));
    let bytes = pm.take_u8();
    let mut out = Pixmap::new(w, h).expect("pixmap");
    // hayro's pixels are premultiplied over transparent; put them on white.
    for (dst, src) in out.data_mut().chunks_mut(4).zip(bytes.chunks(4)) {
        for c in 0..3 {
            dst[c] = (u16::from(src[c]) + 255 - u16::from(src[3])) as u8;
        }
        dst[3] = 255;
    }
    out
}

/// Mean absolute difference of `BLOCK`-pixel block averages, out of 255.
fn mean_block_diff(a: &Pixmap, b: &Pixmap) -> f64 {
    let down = |pm: &Pixmap| {
        let (w, h) = (pm.width(), pm.height());
        let (bw, bh) = (w / BLOCK, h / BLOCK);
        let mut cells = vec![[0f64; 3]; (bw * bh) as usize];
        let data = pm.data();
        for y in 0..bh * BLOCK {
            for x in 0..bw * BLOCK {
                let i = ((y * w + x) * 4) as usize;
                let cell = &mut cells[((y / BLOCK) * bw + x / BLOCK) as usize];
                for (ch, c) in cell.iter_mut().enumerate() {
                    *c += f64::from(data[i + ch]);
                }
            }
        }
        cells
    };
    let n = f64::from(BLOCK * BLOCK);
    let (da, db) = (down(a), down(b));
    let total: f64 = da
        .iter()
        .zip(&db)
        .map(|(x, y)| (0..3).map(|c| (x[c] - y[c]).abs() / n).sum::<f64>())
        .sum();
    total / (da.len() * 3) as f64
}

fn assert_close(what: &str, ours: &Pixmap, theirs: &Pixmap, max: f64) {
    let mean = mean_block_diff(ours, theirs);
    eprintln!("{what}: mean block diff {mean:.3}");
    if mean > max
        && let Some(dir) = std::env::var_os("PSCAT_PATTERN_DUMP")
    {
        let dir = std::path::PathBuf::from(dir);
        let name = what.replace([' ', ':'], "_");
        for (pm, side) in [(ours, "ours"), (theirs, "theirs")] {
            if let Ok(png) = pscat::png::encode(pm, 1.0) {
                let _ = std::fs::write(dir.join(format!("{name}_{side}.png")), png);
            }
        }
    }
    assert!(mean <= max, "{what}: mean block diff {mean:.2} > {max}");
}

/// The PDF holds a real tiling pattern, painted by name, and no
/// image stands in for it.
fn assert_real_pattern(pdf: &[u8], paint_type: u8) {
    let text = pdf_text(pdf);
    assert!(
        text.contains(&format!("/PatternType 1 /PaintType {paint_type} ")),
        "{text}"
    );
    assert!(text.contains("/Pattern << /P0 "), "page resource: {text}");
    assert!(text.contains(" scn "), "no pattern fill: {text}");
    assert!(
        !text.contains("/Subtype /Image"),
        "a raster stands in: {text}"
    );
}

/// `hayro`: also hold hayro's raster of the PDF against ours. hayro paints
/// only one copy of a cell whose `/BBox` is larger than its step (gs paints
/// the overlapping copies, as the PLRM and pscat do), so a picture with
/// such a cell skips it.
fn check_all(name: &str, src: &str, paint_type: u8, hayro: bool) -> Outputs {
    let out = render(src);
    assert_real_pattern(&out.pdf, paint_type);
    assert!(out.svg.contains("<pattern "), "{}", out.svg);
    assert_close(
        &format!("{name}: SVG via resvg"),
        &out.png,
        &resvg_png(&out.svg),
        3.0,
    );
    if hayro {
        assert_close(
            &format!("{name}: PDF via hayro"),
            &out.png,
            &hayro_png(&out.pdf),
            3.0,
        );
    }
    if common::gs_available() {
        // gs's raster of our PDF against our PNG. gs paints pattern cells
        // without anti-aliasing (GraphicsAlphaBits does not reach them), so
        // at 72 dpi its cells are heavier (8-9 here, 3.1 at 4x); rendered at
        // 8x and averaged down, the aliasing goes and the PDF's meaning
        // remains.
        assert_close(
            &format!("{name}: our PDF via gs at 8x"),
            &out.png,
            &gs_png_at(&out.pdf, "pdf", name, 8),
            3.0,
        );
        // And gs paints our PDF exactly as it paints the program itself:
        // the PDF means what the PostScript means.
        let ps = format!("%!PS\n{src}\nshowpage\n");
        assert_close(
            &format!("{name}: our PDF via gs against the program in gs"),
            &gs_png(ps.as_bytes(), "ps", name),
            &gs_png(&out.pdf, "pdf", name),
            0.5,
        );
    }
    out
}

#[test]
fn a_hatch_fills_a_circle_in_all_three_outputs() {
    let out = check_all("hatch", HATCH, 1, true);
    // Inside the circle the hatch's ink, outside it white.
    let px = |x: u32, y: u32| out.png.pixel(x, y).expect("pixel");
    assert_eq!(px(10, 10).red(), 255);
    let inked = (150..250)
        .flat_map(|y| (150..250).map(move |x| (x, y)))
        .filter(|&(x, y)| u16::from(px(x, y).blue()) > u16::from(px(x, y).red()) + 40)
        .count();
    assert!(inked > 1500, "too little hatch: {inked}");
}

#[test]
fn hatch_tiles_have_no_seams() {
    // Every 8x8 tile of the raster inside the fill is the same pixels:
    // no tile edge is lighter, darker or shifted.
    let out = render(HATCH);
    let pm = &out.png;
    let tile = |tx: u32, ty: u32| -> Vec<u8> {
        let mut v = Vec::new();
        for y in 0..8 {
            for x in 0..8 {
                let p = pm.pixel(tx + x, ty + y).expect("pixel");
                v.extend([p.red(), p.green(), p.blue()]);
            }
        }
        v
    };
    let first = tile(160, 160);
    for ty in (160..232).step_by(8) {
        for tx in (160..232).step_by(8) {
            assert_eq!(tile(tx, ty), first, "tile at {tx},{ty} differs");
        }
    }
}

#[test]
fn an_uncoloured_pattern_paints_in_two_colours() {
    let out = check_all("uncoloured", UNCOLOURED, 2, true);
    let reddish = |x: u32, y: u32| {
        let p = out.png.pixel(x, y).expect("pixel");
        (p.red(), p.green(), p.blue())
    };
    // A dot centre in each half (y down: user y 30 is row 370).
    assert_eq!(reddish(30, 370), (204, 26, 26));
    assert_eq!(reddish(222, 370), (26, 77, 204));
    // The PDF sets the colour with the pattern, in [/Pattern /DeviceRGB].
    let text = pdf_text(&out.pdf);
    assert!(text.contains("/PCS [/Pattern /DeviceRGB]"), "{text}");
    assert!(text.contains("/PCS cs 0.8 0.1 0.1 /P0 scn"), "{text}");
    // The pattern's own content carries no colour.
    assert!(out.svg.contains("currentColor"), "{}", out.svg);
}

#[test]
fn fractional_steps_wide_cells_strokes_and_text_agree_across_outputs() {
    check_all("mixed", MIXED, 1, false);
}

#[test]
fn pattern_free_programs_write_no_pattern() {
    let out = render("0 0 1 setrgbcolor 10 10 100 100 rectfill");
    assert!(!pdf_text(&out.pdf).contains("/Pattern"));
    assert!(!out.svg.contains("<pattern"));
}

fn eval(src: &str) -> String {
    let mut it = Interp::with_page(50, 50).expect("page");
    it.run_source(src.as_bytes())
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    let mut out = String::new();
    while let Ok(o) = it.pop() {
        out = format!("{} {out}", o.repr());
    }
    out.trim().to_string()
}

const DICT: &str = "<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 10 10] \
                    /XStep 10 /YStep 10 /PaintProc { pop 0 0 5 5 rectfill } >>";

#[test]
fn makepattern_errors_are_the_ones_gs_gives() {
    let cases = [
        ("/PaintProc", "undefined"),
        ("/PaintType 3 def", "rangecheck"),
        ("/TilingType 4 def", "rangecheck"),
        ("/BBox [0 0 0 10] def", "rangecheck"),
        ("/BBox [10 10 0 0] def", "rangecheck"),
        ("/BBox [0 0 10] def", "rangecheck"),
        ("/XStep 0 def", "rangecheck"),
        ("/PatternType 3 def", "rangecheck"),
        ("/PaintProc 5 def", "typecheck"),
    ];
    for (edit, want) in cases {
        let edit = if edit.ends_with("def") {
            format!("dup begin {edit} end")
        } else {
            format!("dup {edit} undef")
        };
        let src = format!(
            "{DICT} {edit} matrix {{ makepattern }} stopped {{ $error /errorname get }} if"
        );
        let got = eval(&src);
        assert!(got.ends_with(&format!("/{want}")), "{edit}: {got}");
    }
    assert!(
        eval(&format!(
            "{DICT} [1 0 0 1 0] {{ makepattern }} stopped {{ $error /errorname get }} if"
        ))
        .ends_with("/typecheck")
    );
    // A negative step is a step.
    assert_eq!(
        eval(&format!(
            "{DICT} dup /XStep -10 put matrix makepattern type"
        )),
        "dicttype"
    );
}

#[test]
fn colour_space_readbacks_match_gs() {
    assert_eq!(
        eval(&format!(
            "{DICT} matrix makepattern /p exch def /DeviceGray setcolorspace p setpattern \
             currentcolorspace currentcolor p eq"
        )),
        "[/Pattern [/DeviceGray]] true"
    );
    assert_eq!(
        eval("[/Pattern /DeviceRGB] setcolorspace currentcolorspace currentcolor"),
        "[/Pattern [/DeviceRGB]] null"
    );
    let up = DICT.replace("/PaintType 1", "/PaintType 2");
    assert_eq!(
        eval(&format!(
            "{up} matrix makepattern /u exch def [/Pattern /DeviceRGB] setcolorspace \
             1 0 0.5 u setcolor currentcolor pop"
        )),
        "1.0 0.0 0.5"
    );
    // The copy is a new dictionary with /Implementation, as in gs.
    assert_eq!(
        eval(&format!(
            "{DICT} dup matrix makepattern dup /Implementation known 3 1 roll eq"
        )),
        "true false"
    );
    // A device colour operator leaves the pattern space.
    assert_eq!(
        eval(&format!(
            "{DICT} matrix makepattern setpattern 0.5 setgray currentcolorspace"
        )),
        "[/DeviceGray]"
    );
    // Not a pattern: undefined; not a dictionary: typecheck.
    assert!(
        eval("[/Pattern] setcolorspace << >> { setcolor } stopped { $error /errorname get } if")
            .ends_with("/undefined")
    );
    assert!(
        eval("[/Pattern] setcolorspace 5 { setcolor } stopped { $error /errorname get } if")
            .ends_with("/typecheck")
    );
}

#[test]
fn the_null_pattern_paints_nothing() {
    let out = render("0.5 setgray [/Pattern] setcolorspace 0 0 400 400 rectfill");
    assert!(out.png.pixels().iter().all(|p| p.red() == 255));
}

#[test]
fn an_error_in_the_paintproc_leaves_the_page_as_it_was() {
    let mut it = Interp::with_page(50, 50).expect("page");
    it.gfx_mut().enable_svg();
    it.gfx_mut().enable_pdf();
    let src = "1 0 0 setrgbcolor 0 0 20 20 rectfill \
               << /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 10 10] \
                  /XStep 10 /YStep 10 /PaintProc { pop 0 0 5 5 rectfill nosuchname } >> \
               matrix { makepattern } stopped pop clear \
               currentrgbcolor 0 20 20 20 rectfill";
    it.run_source(src.as_bytes())
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    let pm = &it.gfx().pixmap;
    assert_eq!((pm.width(), pm.height()), (50, 50));
    // Both squares red on the page: the colour and CTM came back.
    for (x, y) in [(5, 45), (5, 15)] {
        let p = pm.pixel(x, y).expect("pixel");
        assert_eq!((p.red(), p.green(), p.blue()), (255, 0, 0), "{x},{y}");
    }
    let pdf = pdf_text(&it.gfx().pdf_document().expect("pdf"));
    assert!(!pdf.contains("/Pattern"), "{pdf}");
    assert_eq!(
        it.gfx().svg_pages().expect("svg")[0]
            .matches("<path")
            .count(),
        2
    );
}

#[test]
fn a_limit_inside_the_paintproc_leaves_every_output_on_the_page() {
    let mut it = Interp::with_page(60, 60).expect("page");
    it.gfx_mut().enable_svg();
    it.gfx_mut().enable_pdf();
    it.set_limits(pscat::Limits::none().with_max_steps(5000))
        .expect("limits");
    let src = "1 0 0 setrgbcolor 0 0 30 30 rectfill \
               << /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 10 10] \
                  /XStep 10 /YStep 10 /PaintProc { pop 0 0 5 5 rectfill { } loop } >> \
               matrix makepattern";
    assert!(
        it.run_source(src.as_bytes()).is_err(),
        "the step limit stops it"
    );
    let pm = &it.gfx().pixmap;
    assert_eq!((pm.width(), pm.height()), (60, 60));
    let p = pm.pixel(5, 55).expect("pixel");
    assert_eq!((p.red(), p.green(), p.blue()), (255, 0, 0));
    let svg = &it.gfx().svg_pages().expect("svg")[0];
    assert!(svg.contains("fill=\"#ff0000\""), "{svg}");
    assert!(!svg.contains("t1_"), "{svg}");
    let pdf = pdf_text(&it.gfx().pdf_document().expect("pdf"));
    assert!(pdf.contains("1 0 0 rg"), "{pdf}");
    assert!(!pdf.contains("/Pattern"), "{pdf}");
}

#[test]
fn setalpha_reaches_a_pattern() {
    let src = "<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 10 10] \
               /XStep 10 /YStep 10 /PaintProc { pop 0 0 0 setrgbcolor 0 0 10 10 rectfill } >> \
               matrix makepattern setpattern 0.5 setalpha 0 0 400 400 rectfill";
    let out = render(src);
    let p = out.png.pixel(200, 200).expect("pixel");
    assert!((126..=129).contains(&p.red()), "{}", p.red());
    assert!(pdf_text(&out.pdf).contains("/ca 0.5"));
    assert!(out.svg.contains("fill-opacity=\"0.5\""));
}

/// A pattern made inside another's PaintProc, and used there: the inner
/// pattern's matrix is relative to the outer pattern's space in PDF.
const NESTED: &str = "
<< /PatternType 1 /PaintType 1 /TilingType 1
   /BBox [0 0 40 40] /XStep 40 /YStep 40
   /PaintProc { pop
     << /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 6 6] /XStep 6 /YStep 6
        /PaintProc { pop 0.8 0.2 0.1 setrgbcolor 0 0 3 3 rectfill } >> matrix makepattern
     setpattern 0 0 20 20 rectfill
     0.1 0.3 0.7 setrgbcolor 20 20 20 20 rectfill } >> matrix makepattern
setpattern 20 20 360 360 rectfill
";

#[test]
fn a_pattern_inside_a_paintproc_places_alike_in_all_outputs() {
    check_all("nested", NESTED, 1, true);
}

#[test]
fn sethsbcolor_leaves_a_pattern_space_for_devicergb_as_in_gs() {
    assert_eq!(
        eval(&format!(
            "{DICT} matrix makepattern setpattern 0.5 0.5 0.5 sethsbcolor currentcolorspace \
             currentcmykcolor"
        )),
        "[/DeviceRGB] 0.75 0.5 0.5 0.0"
    );
}

/// A pattern set before an anisotropic CTM, painted by a thick stroke
/// (drawn in pen space, #1196): the stripes stay where `makepattern` put
/// them on the page, as gs paints them, in PNG and SVG alike (Codex round
/// 1 on #1301: the pen transform also moved the pattern).
const PEN_STROKE: &str = "
<< /PatternType 1 /PaintType 1 /TilingType 1
   /BBox [0 0 20 20] /XStep 20 /YStep 20
   /PaintProc { pop 0.8 0.1 0.1 setrgbcolor 0 0 10 20 rectfill } >> matrix makepattern /stripes exch def
stripes setpattern
gsave 3 1 scale 30 setlinewidth 10 60 moveto 120 340 lineto stroke grestore
gsave [1 0.4 0 1 0 0] concat 24 setlinewidth 40 20 moveto 300 120 lineto stroke grestore
340 340 40 40 rectfill
";

#[test]
fn a_pattern_stroke_under_an_anisotropic_ctm_stays_in_place() {
    check_all("pen stroke", PEN_STROKE, 1, true);
}

/// A cell 100 steps wide whose art sits in its first step: every copy that
/// reaches the cell is in the SVG, so it is not blank (Codex round 1 on
/// #1301: a 64-step cap kept steps -99..-36 and lost the one at 0).
const WIDE_CELL: &str = "
<< /PatternType 1 /PaintType 1 /TilingType 1
   /BBox [0 0 100 1] /XStep 1 /YStep 1
   /PaintProc { pop 0.1 0.2 0.8 setrgbcolor 0 0 1 1 rectfill } >> matrix makepattern /wide exch def
gsave 4 4 scale wide setpattern 5 5 40 40 rectfill grestore
";

#[test]
fn a_cell_many_steps_wide_keeps_every_copy_in_svg() {
    let out = check_all("wide cell", WIDE_CELL, 1, false);
    // The raster is filled where the rect is.
    let p = out.png.pixel(100, SIZE - 100).expect("pixel");
    assert!(p.blue() > 150 && p.red() < 100, "{p:?}");
}

/// An uncoloured pattern made while a pattern space is current (here,
/// after another pattern was selected) still has its marks: gs paints the
/// second rect blue (Codex round 2 on #1301: the capture inherited the
/// pattern space and painted nothing).
const UNCOLOURED_IN_PATTERN_SPACE: &str = "
<< /PatternType 1 /PaintType 2 /TilingType 1 /BBox [0 0 10 10] /XStep 10 /YStep 10
   /PaintProc { pop 0 0 5 10 rectfill } >> matrix makepattern /a exch def
[/Pattern /DeviceRGB] setcolorspace
1 0 0 a setcolor 20 20 160 360 rectfill
<< /PatternType 1 /PaintType 2 /TilingType 1 /BBox [0 0 10 10] /XStep 10 /YStep 10
   /PaintProc { pop 0 0 5 10 rectfill } >> matrix makepattern /b exch def
0 0 1 b setcolor 220 20 160 360 rectfill
";

#[test]
fn an_uncoloured_pattern_made_in_a_pattern_space_still_paints() {
    let out = check_all(
        "uncoloured in pattern space",
        UNCOLOURED_IN_PATTERN_SPACE,
        2,
        true,
    );
    let blue = (220..380)
        .flat_map(|x| (40..380).map(move |y| (x, y)))
        .filter(|&(x, y)| {
            let p = out.png.pixel(x, y).expect("pixel");
            p.blue() > 200 && p.red() < 60
        })
        .count();
    assert!(
        blue > 20_000,
        "the second pattern is missing: {blue} blue pixels"
    );
}

// --- imagemask painted with a pattern (#1299) ------------------------------

/// A 16x16 stencil: a frame with its diagonals, painting the 1s.
const STENCIL: &str = "
/stencil { 16 16 true [16 0 0 -16 0 16]
  {<ffff c003 a005 9009 8811 8421 8241 8181 8181 8241 8421 8811 9009 a005 c003 ffff>}
  imagemask } def
";

/// A two-colour stripe pattern through the stencil, upright and turned.
const MASKED_HATCH: &str = "
<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 8 8] /XStep 8 /YStep 8
   /PaintProc { pop 0.8 0 0 setrgbcolor 0 0 4 8 rectfill
                0 0 0.8 setrgbcolor 4 0 4 8 rectfill } >> matrix makepattern /hatch exch def
0 1 0 setrgbcolor hatch setpattern
gsave 20 20 translate 170 170 scale stencil grestore
gsave 300 210 translate 30 rotate 120 120 scale stencil grestore
";

/// An uncoloured dot pattern through the stencil, in two colours, one
/// stencil painting its 0s.
const MASKED_DOTS: &str = "
<< /PatternType 1 /PaintType 2 /TilingType 1 /BBox [0 0 10 10] /XStep 10 /YStep 10
   /PaintProc { pop 5 5 3 0 360 arc fill } >> matrix makepattern /dots exch def
[/Pattern /DeviceRGB] setcolorspace
0 0.5 0 dots setcolor gsave 20 20 translate 170 170 scale stencil grestore
0.7 0.2 0 dots setcolor
gsave 210 210 translate 170 170 scale
  16 16 false [16 0 0 -16 0 16]
  {<ffff c003 a005 9009 8811 8421 8241 8181 8181 8241 8421 8811 9009 a005 c003 ffff>}
  imagemask grestore
";

/// Every output of an imagemask under a pattern: the PDF paints a real
/// `/ImageMask` after selecting the pattern in the same `q ... Q`, the
/// SVG fills the footprint with the `<pattern>` through a `<mask>`, and
/// all of them agree with the raster (and gs's run of the program).
fn check_masked(name: &str, src: &str, select: &str, hayro: bool) -> Outputs {
    let src = format!("{STENCIL}{src}");
    let out = render(&src);
    let pdf = pdf_text(&out.pdf);
    assert!(pdf.contains("/ImageMask true"), "{pdf}");
    assert!(
        pdf.lines()
            .any(|l| l.starts_with("q ") && l.contains(select) && l.contains(" Do")),
        "no `{select}` before the stencil's Do: {pdf}"
    );
    assert!(out.svg.contains("<mask "), "{}", out.svg);
    assert!(out.svg.contains("<pattern "), "{}", out.svg);
    assert!(
        !out.svg.contains("<image width"),
        "a coloured PNG stands in"
    );
    assert_close(
        &format!("{name}: SVG via resvg"),
        &out.png,
        &resvg_png(&out.svg),
        3.0,
    );
    if hayro {
        assert_close(
            &format!("{name}: PDF via hayro"),
            &out.png,
            &hayro_png(&out.pdf),
            3.0,
        );
    }
    if common::gs_available() {
        assert_close(
            &format!("{name}: our PDF via gs at 8x"),
            &out.png,
            &gs_png_at(&out.pdf, "pdf", name, 8),
            3.0,
        );
        let ps = format!("%!PS\n{src}\nshowpage\n");
        assert_close(
            &format!("{name}: our PDF via gs against the program in gs"),
            &gs_png(ps.as_bytes(), "ps", name),
            &gs_png(&out.pdf, "pdf", name),
            0.5,
        );
    }
    out
}

#[test]
fn an_imagemask_paints_a_coloured_pattern_in_all_three_outputs() {
    let out = check_masked("masked hatch", MASKED_HATCH, "/Pattern cs /P0 scn", true);
    let px = |x: u32, y: u32| out.png.pixel(x, y).expect("pixel");
    // The frame's bottom edge (device rows 180-190, the stencil's last
    // row) shows both stripes, and never the green set before the pattern.
    let row: Vec<_> = (20..190).map(|x| px(x, 185)).collect();
    assert!(
        row.iter().any(|p| p.red() > 150 && p.blue() < 60),
        "no red stripe"
    );
    assert!(
        row.iter().any(|p| p.blue() > 150 && p.red() < 60),
        "no blue stripe"
    );
    assert!(
        row.iter().all(|p| p.green() < 60 || p.red() > 200),
        "flat green"
    );
    // Inside the frame, off the diagonals: paper.
    assert_eq!(px(60, 105).red(), 255);
}

#[test]
fn an_imagemask_paints_an_uncoloured_pattern_in_its_colour() {
    let out = check_masked("masked dots", MASKED_DOTS, "/PCS cs", true);
    let px = |x: u32, y: u32| out.png.pixel(x, y).expect("pixel");
    let green = (20..190)
        .flat_map(|x| (210..380).map(move |y| (x, y)))
        .filter(|&(x, y)| px(x, y).green() > 100 && px(x, y).red() < 40)
        .count();
    let orange = (210..380)
        .flat_map(|x| (20..190).map(move |y| (x, y)))
        .filter(|&(x, y)| px(x, y).red() > 150 && px(x, y).blue() < 40)
        .count();
    assert!(
        green > 1000 && orange > 1000,
        "green {green}, orange {orange}"
    );
}

#[test]
fn an_imagemask_under_the_null_pattern_paints_nothing() {
    let out = render(&format!(
        "{STENCIL} 0.5 setgray [/Pattern] setcolorspace 20 20 translate 300 300 scale stencil"
    ));
    assert!(out.png.pixels().iter().all(|p| p.red() == 255));
    assert!(!out.svg.contains("<mask") && !out.svg.contains("<image"));
    assert!(!pdf_text(&out.pdf).contains(" Do"));
}

#[test]
fn an_image_still_paints_its_samples_in_a_pattern_space() {
    // gs 10.07: `image` paints its own grey samples under the null pattern.
    let out =
        render("[/Pattern] setcolorspace 0 0 400 400 scale 2 1 8 [2 0 0 1 0 0] {<00ff>} image");
    let p = out.png.pixel(100, 200).expect("pixel");
    assert_eq!((p.red(), p.green(), p.blue()), (0, 0, 0));
    assert!(pdf_text(&out.pdf).contains("/Subtype /Image"));
}
