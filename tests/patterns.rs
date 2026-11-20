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
    gs_png_with(input, ext, name, k, &[])
}

fn gs_png_with(input: &[u8], ext: &str, name: &str, k: u32, extra: &[&str]) -> Pixmap {
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
        .args(extra)
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

/// An embedded PNG as an SVG of one rect per run of like pixels: resvg is
/// built without raster-image decoding (`default-features = false`), so
/// the tests hand it each image (an imagemask's stencil, #1299) as vector
/// art with the same pixels, sharp-edged as `image-rendering="pixelated"`.
fn png_as_svg(
    _mime: &str,
    data: std::sync::Arc<Vec<u8>>,
    opts: &resvg::usvg::Options,
) -> Option<resvg::usvg::ImageKind> {
    let pm = Pixmap::decode_png(&data).ok()?;
    let (w, h) = (pm.width(), pm.height());
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
         shape-rendering=\"crispEdges\">"
    );
    for y in 0..h {
        let mut x = 0;
        while x < w {
            let p = pm.pixel(x, y)?;
            let mut end = x + 1;
            while end < w && pm.pixel(end, y)? == p {
                end += 1;
            }
            if p.alpha() > 0 {
                let c = p.demultiply();
                svg.push_str(&format!(
                    "<rect x=\"{x}\" y=\"{y}\" width=\"{}\" height=\"1\" \
                     fill=\"rgb({},{},{})\" fill-opacity=\"{}\"/>",
                    end - x,
                    c.red(),
                    c.green(),
                    c.blue(),
                    f32::from(c.alpha()) / 255.0
                ));
            }
            x = end;
        }
    }
    svg.push_str("</svg>");
    let tree = resvg::usvg::Tree::from_str(&svg, opts).ok()?;
    Some(resvg::usvg::ImageKind::SVG(tree))
}

fn resvg_png(svg: &str) -> Pixmap {
    let opts = resvg::usvg::Options {
        image_href_resolver: resvg::usvg::ImageHrefResolver {
            resolve_data: Box::new(png_as_svg),
            ..Default::default()
        },
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_data(svg.as_bytes(), &opts).expect("parse SVG");
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
    assert!(text.contains("/Pattern << /P"), "page resource: {text}");
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

/// TilingType 2 (no distortion, #1332): a 3 pt cell keeps its 3 px size
/// on steps that are no whole pixels (7.3 x 9.7), and the spacing varies
/// instead. The cell's edges are whole pixels, so gs (which paints
/// pattern cells aliased) and pscat agree pixel for pixel, and the y flip
/// (a negative device step, `floor` of it) is checked beside x.
const TYPE2: &str = "
<< /PatternType 1 /PaintType 1 /TilingType 2
   /BBox [0 0 3 3] /XStep 7.3 /YStep 9.7
   /PaintProc { pop 0.1 0.2 0.7 setrgbcolor 0 0 3 3 rectfill } >> [1 0 0 1 13 21] makepattern /t2 exch def
t2 setpattern
10 10 380 380 rectfill
<< /PatternType 1 /PaintType 2 /TilingType 2
   /BBox [0 0 2 2] /XStep 5.6 /YStep 4.45
   /PaintProc { pop 0 0 2 2 rectfill } >> matrix makepattern /t2u exch def
[/Pattern /DeviceRGB] setcolorspace 0.8 0.1 0.1 t2u setcolor
100 100 100 100 rectfill
";

#[test]
fn type_2_cells_keep_their_size_and_the_spacing_varies() {
    let out = render(TYPE2);
    assert_real_pattern(&out.pdf, 1);
    // The SVG carries the true step (the unadjusted matrix). Its raster is
    // not held against ours here: resvg's differs by 5.3 block-mean, as if
    // it rounded a `<pattern>` tile to whole pixels (7.3 -> 7), and hayro's
    // PDF raster by 5.4 likewise.
    assert!(
        out.svg.contains("width=\"7.3\" height=\"9.7\""),
        "{}",
        out.svg
    );
    if common::gs_available() {
        // gs rasterises our PDF with the spacing and cell size the PDF
        // means, and that is its raster of the program.
        assert_close(
            "type2: our PDF via gs at 8x",
            &out.png,
            &gs_png_at(&out.pdf, "pdf", "type2", 8),
            3.0,
        );
        let ps = format!("%!PS\n{TYPE2}\nshowpage\n");
        assert_close(
            "type2: our PDF via gs against the program in gs",
            &gs_png(ps.as_bytes(), "ps", "type2"),
            &gs_png(&out.pdf, "pdf", "type2"),
            0.5,
        );
        // And our raster is gs's raster of the program, cell for cell.
        let gs = gs_png_aliased(TYPE2, "type2");
        let bad = strong_mismatch(&out.png, &gs);
        assert!(bad < 0.0005, "pixels differing from gs: {bad}");
    }
    let ink = |x: u32, y: u32| {
        let p = out.png.pixel(x, y).expect("pixel");
        p.blue() > 150 && p.red() < 100
    };
    let starts = |at: &dyn Fn(u32) -> bool, from: u32, to: u32| -> Vec<i32> {
        (from..to)
            .filter(|&v| at(v) && !at(v - 1))
            .map(|v| v as i32)
            .collect()
    };
    // Across a row of cells (the lowest row: device rows 376-378 for the
    // origin at user y 21): cell i starts at 13 + floor(i * 7.3).
    let xs = starts(&|x| ink(x, 377), 14, 120);
    let want_x: Vec<i32> = (1..)
        .map(|i| 13 + (f64::from(i) * 7.3 + 1e-6).floor() as i32)
        .take_while(|&x| x < 120)
        .collect();
    assert_eq!(xs, want_x, "x starts");
    // Down a column of cells (x 13-15): the step is -9.7 in device y, so
    // cell j's top is 376 + floor(-9.7 j).
    let ys = starts(&|y| ink(14, y), 11, 380);
    let mut want_y: Vec<i32> = (0..)
        .map(|j| 376 + (f64::from(j) * -9.7 + 1e-6).floor() as i32)
        .take_while(|&y| y > 10)
        .collect();
    want_y.reverse();
    assert_eq!(ys, want_y, "y starts");
    // True size: three whole pixels, no partial ones at the edge.
    assert!(ink(13, 377) && ink(15, 377) && !ink(16, 377));
    assert_real_pattern_tiling_type(&out.pdf, 2);
}

fn assert_real_pattern_tiling_type(pdf: &[u8], tiling: u8) {
    let text = pdf_text(pdf);
    assert!(text.contains(&format!("/TilingType {tiling} ")), "{text}");
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
               matrix makepattern setpattern { 0 0 5 5 rectfill } stopped pop clear \
               1 0 0 setrgbcolor 0 20 20 20 rectfill";
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
               matrix makepattern setpattern 0 0 5 5 rectfill";
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

// --- patterns used across contexts (#1300) --------------------------------

/// A page pattern used inside another pattern's PaintProc, as well as on
/// the page. gs applies a pattern's device matrix unchanged in its
/// pattern accumulator, whose origin is the cell's `/BBox` top-left
/// corner in device space, rounded: so the stripes keep their device size
/// (the outer pattern's 1.5 scale does not reach them) and start at that
/// corner in each cell. A step of 7 (it does not divide the 400 px page)
/// and a fractional corner and translation, so a wrong anchor cannot pass
/// by chance. The content's edges are whole pixels, where gs (which paints
/// pattern cells aliased) and pscat agree pixel for pixel.
const ACROSS: &str = "
<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 7 7] /XStep 7 /YStep 7
   /PaintProc { pop 0.9 0.1 0.1 setrgbcolor 0 0 2 7 rectfill
                0 0.6 0 setrgbcolor 0 0 7 2 rectfill } >> matrix makepattern /stripes exch def
<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [-3.3 -2.6 70.4 61.3] /XStep 90 /YStep 86
   /PaintProc { pop 0.1 0.1 0.7 setrgbcolor -3.3 -2.6 80 70 rectfill
                stripes setpattern 0 0 70 61 rectfill }
>> [1.5 0 0 1.5 4.3 7.6] makepattern /outer exch def
outer setpattern 0 0 400 300 rectfill
stripes setpattern 0 300 400 100 rectfill
";

/// An uncoloured page pattern inside another pattern's PaintProc: the
/// PDF selects it through `/PCS` in the outer pattern's stream.
const ACROSS_UNCOLOURED: &str = "
<< /PatternType 1 /PaintType 2 /TilingType 1 /BBox [0 0 11 11] /XStep 11 /YStep 11
   /PaintProc { pop 2 2 5 5 rectfill } >> matrix makepattern /dots exch def
<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [-3.3 -2.6 70.4 61.3] /XStep 90 /YStep 86
   /PaintProc { pop 0.1 0.1 0.7 setrgbcolor -3.3 -2.6 80 70 rectfill
                [/Pattern /DeviceRGB] setcolorspace 1 0.8 0 dots setcolor 0 0 70 61 rectfill }
>> [1.5 0 0 1.5 4.3 7.6] makepattern /outer exch def
outer setpattern 0 0 400 400 rectfill
";

/// The same page pattern inside a pattern turned 30 degrees: gs's
/// accumulator is device-aligned, so the stripes stay upright on the page
/// inside the turned cells.
const ACROSS_TURNED: &str = "
<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 12 12] /XStep 12 /YStep 12
   /PaintProc { pop 0.9 0.1 0.1 setrgbcolor 0 0 3 12 rectfill
                0 0.6 0 setrgbcolor 0 0 12 2 rectfill } >> matrix makepattern /stripes exch def
<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 60 60] /XStep 100 /YStep 100
   /PaintProc { pop 0.1 0.1 0.7 setrgbcolor 0 0 60 60 rectfill
                stripes setpattern 6 6 48 48 rectfill }
>> [0.866 0.5 -0.5 0.866 60 10] makepattern /outer exch def
outer setpattern 0 0 400 400 rectfill
";

/// gs's raster of `ps` at 72 dpi with no anti-aliasing, the way gs
/// paints pattern cells anyway.
fn gs_png_aliased(ps: &str, name: &str) -> Pixmap {
    let dir = common::scratch_dir("pscat_patterns");
    let src = dir.join(format!("{name}.aliased.ps"));
    let out = dir.join(format!("{name}.aliased.png"));
    std::fs::write(&src, format!("%!PS\n{ps}\nshowpage\n")).expect("write input");
    let status = Command::new("gs")
        .args(["-q", "-dNOPAUSE", "-dBATCH", "-dSAFER", "-sDEVICE=png16m"])
        .arg(format!("-g{SIZE}x{SIZE}"))
        .arg("-r72")
        .arg(format!("-o{}", out.display()))
        .arg(&src)
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected {name}");
    let data = std::fs::read(&out).expect("read gs png");
    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&out);
    Pixmap::decode_png(&data).expect("decode png")
}

/// The share of pixels where `a` and `b` differ by more than half the
/// range in some channel: a misplaced stripe, not an anti-aliased edge.
fn strong_mismatch(a: &Pixmap, b: &Pixmap) -> f64 {
    let bad = a
        .data()
        .chunks(4)
        .zip(b.data().chunks(4))
        .filter(|(p, q)| (0..3).any(|c| p[c].abs_diff(q[c]) > 128))
        .count();
    bad as f64 / f64::from(SIZE * SIZE)
}

/// Every output of a picture whose PDF gs reads with different edge rules
/// than the program (gs's PDF interpreter paints a pattern cell's
/// whole-pixel rect edges a pixel heavier, with or without nesting, so the
/// pictures' 0.5 bound against gs's run does not apply): resvg's SVG,
/// hayro's PDF, and gs's PDF at 8x against our PNG.
fn check_outputs(name: &str, src: &str, paint_type: u8, hayro: bool) -> Outputs {
    let out = render(src);
    assert_real_pattern(&out.pdf, paint_type);
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
    }
    out
}

#[test]
fn a_page_pattern_paints_inside_another_patterns_paintproc() {
    // hayro paints a pattern inside a pattern stream resampled (blurred,
    // with the same placement), so it sits this one out.
    let out = check_outputs("across", ACROSS, 1, false);
    // The inner stripes are there: red inside the outer cells.
    let red = (0..400)
        .flat_map(|x| (100..400).map(move |y| (x, y)))
        .filter(|&(x, y)| {
            let p = out.png.pixel(x, y).expect("pixel");
            p.red() > 200 && p.green() < 60 && p.blue() < 60
        })
        .count();
    assert!(
        red > 5_000,
        "the inner pattern is missing: {red} red pixels"
    );
    // A copy of the stripes' pattern stream under its own `/Matrix`.
    let text = pdf_text(&out.pdf);
    assert!(
        text.matches("/BBox [0 0 7 7]").count() >= 2,
        "no second stripes pattern for the outer stream: {text}"
    );
    if common::gs_available() {
        // The placement itself, pixel for pixel against gs's run of the
        // program: only the outer cells' fractional edges (anti-aliased
        // here, aliased in gs) may differ.
        let gs = gs_png_aliased(ACROSS, "across");
        let bad = strong_mismatch(&out.png, &gs);
        eprintln!("across: strong mismatch against gs {bad:.4}");
        assert!(bad < 0.005, "across: {bad:.4} of the pixels differ from gs");
        // A stripe a pixel off would fail it: shifting ours by one row
        // does.
        let mut shifted = out.png.clone();
        let w = SIZE as usize * 4;
        let src = out.png.data()[..w * (SIZE as usize - 1)].to_vec();
        shifted.data_mut()[w..].copy_from_slice(&src);
        assert!(strong_mismatch(&shifted, &gs) > 0.05);
    }
}

#[test]
fn an_uncoloured_page_pattern_paints_inside_another_patterns_paintproc() {
    let out = check_outputs("across uncoloured", ACROSS_UNCOLOURED, 1, false);
    let text = pdf_text(&out.pdf);
    assert!(text.contains("/PaintType 2 "), "{text}");
    assert!(text.contains("/PCS cs 1 0.8 0 /P"), "{text}");
}

#[test]
fn a_page_pattern_stays_upright_inside_a_turned_pattern() {
    check_all("across turned", ACROSS_TURNED, 1, true);
}

/// A pattern made inside another's PaintProc and used after it, on the
/// page: gs places it by the matrix it got in the accumulator, read as
/// page device coordinates (scaled 1.5 with the outer pattern, anchored at
/// the outer origin less the accumulator's). gs 10.07.1 crashes running
/// this with its default pattern cache ("Unrecoverable error");
/// `-dMaxPatternBitmap=1` paints it, and that is the reference.
const MADE_INSIDE: &str = "
<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [-3.3 -2 63 57.3] /XStep 400 /YStep 400
   /PaintProc { pop 0.1 0.1 0.7 setrgbcolor -3.3 -2 80 70 rectfill
     << /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 10 10] /XStep 10 /YStep 10
        /PaintProc { pop 0.9 0.1 0.1 setrgbcolor 0 0 3 10 rectfill
                     0 0.6 0 setrgbcolor 0 0 10 2 rectfill } >> [1 0 0 1 2 1] makepattern
     userdict /inner 3 -1 roll put }
>> [1.5 0 0 1.5 4.3 7.6] makepattern /outer exch def
outer setpattern 0 0 30 30 rectfill
inner setpattern 50 50 300 300 rectfill
";

#[test]
fn a_pattern_made_inside_a_paintproc_paints_on_the_page() {
    let out = render(MADE_INSIDE);
    assert_close(
        "made inside: SVG via resvg",
        &out.png,
        &resvg_png(&out.svg),
        3.0,
    );
    assert_close(
        "made inside: PDF via hayro",
        &out.png,
        &hayro_png(&out.pdf),
        3.0,
    );
    let red = (50..350)
        .flat_map(|x| (50..350).map(move |y| (x, y)))
        .filter(|&(x, y)| {
            let p = out.png.pixel(x, SIZE - 1 - y).expect("pixel");
            p.red() > 200 && p.green() < 60
        })
        .count();
    assert!(
        red > 10_000,
        "the inner pattern is missing: {red} red pixels"
    );
    if common::gs_available() {
        assert_close(
            "made inside: our PDF via gs at 8x",
            &out.png,
            &gs_png_at(&out.pdf, "pdf", "made_inside", 8),
            3.0,
        );
        let ps = format!("%!PS\n{MADE_INSIDE}\nshowpage\n");
        assert_close(
            "made inside: our PDF via gs against the program in gs",
            &gs_png_with(
                ps.as_bytes(),
                "ps",
                "made_inside",
                1,
                &["-dMaxPatternBitmap=1"],
            ),
            &gs_png(&out.pdf, "pdf", "made_inside"),
            0.5,
        );
    }
}

// --- one def, one id (#1343) ------------------------------------------------

/// A page pattern whose capture writes a def of its own (its `rectclip`).
/// `rectclip` only reaches the SVG as a `<clipPath>` when the capture has one
/// to write, so each program below clips the inner stripes.
const CLIPPED_STRIPES: &str = "
<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 7 7] /XStep 7 /YStep 7
   /PaintProc { pop gsave 0 0 5 7 rectclip 0.9 0.1 0.1 setrgbcolor 0 0 7 7 rectfill grestore
                0 0.6 0 setrgbcolor 0 0 7 2 rectfill } >> matrix makepattern /stripes exch def
";

/// A pattern whose `PaintProc` paints with `stripes` (and clips itself).
fn outer_using_stripes(name: &str, matrix: &str) -> String {
    format!(
        "<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 70 61] /XStep 90 /YStep 86
            /PaintProc {{ pop 0.1 0.1 0.7 setrgbcolor 0 0 70 61 rectfill
              gsave 5 5 50 50 rectclip stripes setpattern 0 0 70 61 rectfill grestore }}
         >> {matrix} makepattern /{name} exch def\n"
    )
}

/// Every `id="..."` of an SVG document is unique, and every `url(#...)` names
/// one that is defined: a def written twice repeats an id (invalid SVG), and
/// one dropped leaves a reference resvg draws as nothing.
fn assert_ids_unique_and_resolved(what: &str, svg: &str) {
    let mut ids = Vec::new();
    let mut rest = svg;
    while let Some(i) = rest.find(" id=\"") {
        rest = &rest[i + 5..];
        let end = rest.find('"').expect("closing quote");
        ids.push(&rest[..end]);
    }
    let mut seen = std::collections::HashSet::new();
    for id in &ids {
        assert!(
            seen.insert(*id),
            "{what}: id {id:?} is written twice:\n{svg}"
        );
    }
    let mut rest = svg;
    while let Some(i) = rest.find("url(#") {
        rest = &rest[i + 5..];
        let end = rest.find(')').expect("closing paren");
        assert!(
            seen.contains(&rest[..end]),
            "{what}: {:?} is not defined",
            &rest[..end]
        );
    }
}

/// Run `body` after `CLIPPED_STRIPES` and check the SVG's ids and that the
/// stripes' clip is written once.
fn check_unique_defs(what: &str, body: &str) -> Outputs {
    let out = render(&format!("{CLIPPED_STRIPES}{body}"));
    assert!(out.svg.contains("<clipPath"), "{what}: no clip written");
    assert_ids_unique_and_resolved(what, &out.svg);
    out
}

#[test]
fn a_page_pattern_used_on_the_page_and_in_a_tile_writes_its_defs_once() {
    let outer = outer_using_stripes("outer", "[1.5 0 0 1.5 4.3 7.6]");
    // On the page first, then inside the tile.
    check_unique_defs(
        "page then tile",
        &format!(
            "stripes setpattern 0 300 400 100 rectfill {outer}
             outer setpattern 0 0 400 300 rectfill"
        ),
    );
    // Inside the tile first, then on the page.
    check_unique_defs(
        "tile then page",
        &format!(
            "{outer} outer setpattern 0 0 400 300 rectfill
             stripes setpattern 0 300 400 100 rectfill"
        ),
    );
}

#[test]
fn two_tiles_that_share_a_page_pattern_write_its_defs_once() {
    // The stripes are never painted on the page itself.
    let a = outer_using_stripes("a", "[1 0 0 1 4.3 7.6]");
    let b = outer_using_stripes("b", "[0.866 0.5 -0.5 0.866 60 10]");
    check_unique_defs(
        "siblings",
        &format!("{a} {b} a setpattern 0 0 200 400 rectfill b setpattern 200 0 200 400 rectfill"),
    );
}

#[test]
fn nested_contexts_write_each_def_once() {
    // stripes inside Q's PaintProc, Q made inside R's, R on the page, and
    // stripes on the page too.
    let out = check_unique_defs(
        "three levels",
        "<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 90 90] /XStep 400 /YStep 400
            /PaintProc { pop 0.1 0.1 0.7 setrgbcolor 0 0 90 90 rectfill
              << /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 30 30] /XStep 30 /YStep 30
                 /PaintProc { pop 0.9 0.9 0.2 setrgbcolor 0 0 30 30 rectfill
                   gsave 3 3 20 20 rectclip stripes setpattern 0 0 30 30 rectfill grestore }
              >> [1 0 0 1 1.5 2.5] makepattern setpattern 10 10 70 70 rectfill }
         >> [1.5 0 0 1.5 4.3 7.6] makepattern setpattern 0 0 300 300 rectfill
         stripes setpattern 0 300 400 100 rectfill",
    );
    // The stripes' clip, the tile Q's and R's own: three `rectclip`s.
    assert!(out.svg.matches("<clipPath").count() >= 3);
}

#[test]
fn a_pattern_made_inside_a_tile_writes_its_defs_once() {
    check_unique_defs(
        "made inside",
        "<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [-3.3 -2 63 57.3] /XStep 400 /YStep 400
            /PaintProc { pop 0.1 0.1 0.7 setrgbcolor -3.3 -2 80 70 rectfill
              << /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 10 10] /XStep 10 /YStep 10
                 /PaintProc { pop gsave 0 0 3 10 rectclip 0.9 0.1 0.1 setrgbcolor 0 0 10 10 rectfill
                              grestore 0 0.6 0 setrgbcolor 0 0 10 2 rectfill } >> [1 0 0 1 2 1] makepattern
              userdict /inner 3 -1 roll put }
         >> [1.5 0 0 1.5 4.3 7.6] makepattern /outer exch def
         outer setpattern 0 0 30 30 rectfill
         inner setpattern 50 50 300 300 rectfill",
    );
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

/// A pattern fill under `setalpha` keeps its pattern: the 16-bit fill
/// shadow (#731) composites a flat colour, so it must not take a pattern
/// fill's translucent or edge runs (they turned a flat grey once both
/// landed on feature/0.7.1).
#[test]
fn a_translucent_pattern_fill_keeps_its_pattern() {
    let out = render(&HATCH.replace("hatch setpattern", "hatch setpattern 0.5 setalpha"));
    let px = |x: u32, y: u32| out.png.pixel(x, y).expect("pixel");
    let (mut white, mut blue) = (0, 0);
    for y in 150..250 {
        for x in 150..250 {
            let p = px(x, y);
            if p.red() >= 250 && p.green() >= 250 && p.blue() >= 250 {
                white += 1;
            } else if u16::from(p.blue()) > u16::from(p.red()) + 40 {
                blue += 1;
            }
        }
    }
    assert!(white > 1500, "the hatch's gaps stay white: {white}");
    assert!(blue > 1000, "the hatch's lines stay blue: {blue}");
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
    // Every image is a mask's stencil: no coloured PNG stands in.
    assert_eq!(
        out.svg.matches("<image ").count(),
        out.svg.matches("<mask ").count(),
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
    // The frame's bottom edge (the stencil's last row, device rows
    // 370-380) shows both stripes, and never the green set before the
    // pattern.
    let row: Vec<_> = (20..190).map(|x| px(x, 375)).collect();
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
    let paper = px(60, 295);
    assert_eq!((paper.red(), paper.green(), paper.blue()), (255, 255, 255));
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
    let out = render("[/Pattern] setcolorspace 400 400 scale 2 1 8 [2 0 0 1 0 0] {<00ff>} image");
    let p = out.png.pixel(100, 200).expect("pixel");
    assert_eq!((p.red(), p.green(), p.blue()), (0, 0, 0));
    assert!(pdf_text(&out.pdf).contains("/Subtype /Image"));
}

#[test]
fn fills_after_a_patterned_imagemask_paint_as_without_it() {
    // The stencil's coverage goes through the mask fills reuse: a fill
    // after it, over the same pixels, must not pick any of it up.
    let circle = "1 setgray 0 0 400 400 rectfill 0 setgray 150 150 90 0 360 arc fill \
                  0.3 setgray 210 140 moveto 330 330 lineto 2.3 setlinewidth stroke";
    let after = render(&format!(
        "{STENCIL}{MASKED_HATCH} gsave 20 20 translate 360 360 scale stencil grestore \
         0 setgray {circle}"
    ));
    let alone = render(circle);
    assert!(after.png.data() == alone.png.data(), "stale coverage");
}

// --- the PaintProc runs at the pattern's first use (#1341) ------------------

/// A coloured pattern `pp` whose PaintProc counts its runs in `n`.
const COUNTED: &str = "/n 0 def \
    << /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 10 10] /XStep 10 /YStep 10 \
       /PaintProc { pop /n n 1 add def 0 0 5 5 rectfill } >> matrix makepattern /pp exch def ";

/// A page of `src`, for pixel and `n` checks.
fn run_page(src: &str) -> Interp {
    let mut it = Interp::with_page(50, 50).expect("page");
    it.run_source(src.as_bytes())
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    it
}

fn inked(it: &Interp) -> usize {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|p| p.red() < 128)
        .count()
}

#[test]
fn the_paintproc_runs_once_at_the_first_paint() {
    // gs 10.07.1: not at makepattern, setcolorspace or setcolor; once at the
    // first fill, not again for a second fill or a gsave level, and not at
    // all for a pattern that is set and never painted.
    let got = eval(&format!(
        "{COUNTED} n /Pattern setcolorspace pp setcolor n \
         0 0 40 40 rectfill n 0 0 40 40 rectfill \
         gsave pp setcolor 0 0 10 10 rectfill grestore n \
         {COUNTED} /pp2 pp def n"
    ));
    assert_eq!(got, "0 0 1 1 0", "{got}");
    // A pattern made and never set runs nothing, whatever else is painted.
    assert_eq!(
        eval(&format!("{COUNTED} 0 setgray 0 0 40 40 rectfill n")),
        "0"
    );
}

#[test]
fn a_paintproc_may_name_a_pattern_made_after_its_own() {
    // The case that was `undefined`: the outer PaintProc names `late`,
    // which does not exist until after the outer makepattern.
    let outer = |inner_first: bool| {
        let stripes = "<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 7 7] \
             /XStep 7 /YStep 7 /PaintProc { pop 0.9 0.1 0.1 setrgbcolor 0 0 2 7 rectfill \
             0 0.6 0 setrgbcolor 0 0 7 2 rectfill } >> matrix makepattern /late exch def\n";
        let big = "<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [-3.3 -2.6 70.4 61.3] \
             /XStep 90 /YStep 86 /PaintProc { pop 0.1 0.1 0.7 setrgbcolor -3.3 -2.6 80 70 rectfill \
             late setpattern 0 0 70 61 rectfill } >> [1.5 0 0 1.5 4.3 7.6] makepattern /outer exch def\n";
        let page = "outer setpattern 0 0 400 300 rectfill late setpattern 0 300 400 100 rectfill";
        if inner_first {
            format!("{stripes}{big}{page}")
        } else {
            format!("{big}{stripes}{page}")
        }
    };
    let early = render(&outer(true));
    let late = render(&outer(false));
    assert!(
        early.png.data() == late.png.data(),
        "the order the patterns are made in changes the page"
    );
    assert!(
        late.png
            .pixels()
            .iter()
            .any(|p| p.red() > 200 && p.green() < 60),
        "the inner stripes are missing"
    );
    if common::gs_available() {
        let bad = strong_mismatch(&late.png, &gs_png_aliased(&outer(false), "late"));
        assert!(bad < 0.005, "late: {bad:.4} of the pixels differ from gs");
    }
}

#[test]
fn the_paintproc_starts_from_the_state_makepattern_had() {
    // gs 10.07.1: the line width and colour at makepattern (3, red), not at
    // the first use (7, green).
    let got = eval(
        "/lw 0 def /col [0 0 0] def \
         << /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 10 10] /XStep 10 /YStep 10 \
            /PaintProc { pop /lw currentlinewidth def /col [currentrgbcolor] def } >> \
         1 0 0 setrgbcolor 3 setlinewidth matrix makepattern /pp exch def \
         0 1 0 setrgbcolor 7 setlinewidth /Pattern setcolorspace pp setcolor \
         0 0 40 40 rectfill lw cvi col 0 get cvi col 1 get cvi currentlinewidth cvi",
    );
    assert_eq!(got, "3 1 0 7", "{got}");
}

#[test]
fn the_paintproc_sees_the_operands_and_the_operator_still_does() {
    // gs: 7 8 under rectfill's four operands and the dictionary (7 deep);
    // after the fill the stack is as it was.
    let got = eval(
        "/seen 0 def << /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 10 10] \
            /XStep 10 /YStep 10 /PaintProc { count /seen exch def pop 0 0 5 5 rectfill } >> \
         matrix makepattern /pp exch def /Pattern setcolorspace pp setcolor \
         7 8 0 0 40 40 rectfill seen",
    );
    assert_eq!(got, "7 8 7", "{got}");
    // A PaintProc that eats operands does not take the operator's with it.
    let got = eval(
        "<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 10 10] \
            /XStep 10 /YStep 10 /PaintProc { clear } >> \
         matrix makepattern /pp exch def /Pattern setcolorspace pp setcolor \
         7 8 0 0 40 40 rectfill",
    );
    assert_eq!(got, "7 8", "{got}");
}

#[test]
fn a_paintproc_that_fails_runs_again_at_the_next_use() {
    // gs 10.07.1: the failed first fill leaves the pattern unmade, and the
    // next fill runs the PaintProc a second time.
    let it = run_page(
        "/n 0 def << /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 10 10] \
            /XStep 10 /YStep 10 /PaintProc { pop /n n 1 add def n 1 eq { 1 0 div } if \
            0 0 5 5 rectfill } >> matrix makepattern setpattern \
         { 0 0 50 50 rectfill } stopped clear n 1 ne { stop } if \
         0 0 50 50 rectfill n 2 ne { stop } if",
    );
    assert!(inked(&it) > 0, "the second run painted nothing");
}

/// Every operator that paints with the current colour, each as a snippet
/// on a 50 pt page with the pattern `pp` current.
const MARKING: [(&str, &str); 18] = [
    ("fill", "0 0 moveto 40 0 lineto 40 40 lineto closepath fill"),
    (
        "eofill",
        "0 0 moveto 40 0 lineto 40 40 lineto closepath eofill",
    ),
    ("stroke", "6 setlinewidth 0 20 moveto 40 20 lineto stroke"),
    ("rectfill", "0 0 40 40 rectfill"),
    ("rectstroke", "6 setlinewidth 3 3 40 40 rectstroke"),
    (
        "show",
        "/Helvetica findfont 40 scalefont setfont 5 5 moveto (H) show",
    ),
    (
        "ashow",
        "/Helvetica findfont 40 scalefont setfont 5 5 moveto 1 0 (H) ashow",
    ),
    (
        "widthshow",
        "/Helvetica findfont 40 scalefont setfont 5 5 moveto 0 0 32 0 0 (H) widthshow",
    ),
    (
        "kshow",
        "/Helvetica findfont 40 scalefont setfont 5 5 moveto { pop pop } (H) kshow",
    ),
    (
        "xshow",
        "/Helvetica findfont 40 scalefont setfont 5 5 moveto (H) [30] xshow",
    ),
    (
        "yshow",
        "/Helvetica findfont 40 scalefont setfont 5 5 moveto (H) [0] yshow",
    ),
    (
        "xyshow",
        "/Helvetica findfont 40 scalefont setfont 5 5 moveto (H) [30 0] xyshow",
    ),
    (
        "awidthshow",
        "/Helvetica findfont 40 scalefont setfont 5 5 moveto 0 0 32 1 0 (H) awidthshow",
    ),
    (
        "glyphshow",
        "/Helvetica findfont 40 scalefont setfont 5 5 moveto /H glyphshow",
    ),
    (
        "show inside a cshow proc",
        "/Helvetica findfont 40 scalefont setfont 5 5 moveto \
         { pop pop 5 5 moveto (H) show } (H) cshow",
    ),
    (
        "a Type 3 glyph painting in the current colour",
        "/T3 << /FontType 3 /FontMatrix [0.04 0 0 0.04 0 0] /FontBBox [0 0 25 25] \
         /Encoding 256 {/A} repeat 256 array astore \
         /BuildChar { pop pop 25 0 setcharwidth 0 0 25 25 rectfill } >> definefont pop \
         /T3 findfont 40 scalefont setfont 5 5 moveto (A) show",
    ),
    (
        "imagemask",
        "gsave 40 40 scale 8 8 true [8 0 0 8 0 0] <ffffffffffffffff> imagemask grestore",
    ),
    (
        "imagemask with a dictionary",
        "gsave 40 40 scale << /ImageType 1 /Width 8 /Height 8 /ImageMatrix [8 0 0 8 0 0] \
         /BitsPerComponent 1 /Decode [1 0] /ImageMask true \
         /DataSource <ffffffffffffffff> >> imagemask grestore",
    ),
];

#[test]
fn every_marking_operator_runs_the_paintproc_first() {
    for (name, snippet) in MARKING {
        let it = run_page(&format!(
            "{COUNTED} /Pattern setcolorspace pp setcolor {snippet} n 1 eq {{ }} {{ stop }} ifelse"
        ));
        assert!(inked(&it) > 0, "{name}: nothing painted");
    }
}

#[test]
fn a_kshow_proc_that_sets_a_pattern_runs_its_paintproc_before_the_next_glyph() {
    let it = run_page(&format!(
        "{COUNTED} /Helvetica findfont 20 scalefont setfont 0 setgray 2 10 moveto \
         {{ pop pop /Pattern setcolorspace pp setcolor }} (HH) kshow \
         n 1 eq {{ }} {{ stop }} ifelse"
    ));
    assert!(inked(&it) > 0);
}

#[test]
fn a_pattern_that_names_itself_stops_short_of_the_memory() {
    // gs 10.07.1 recurses too, to `execstackoverflow` (550 deep). Here the
    // nested canvases are capped (`limitcheck`), so a large cell stops after
    // a few levels, and a small one at the capture depth.
    for (bbox, want) in [("3000", "limitcheck"), ("10", "execstackoverflow")] {
        let got = eval(&format!(
            "/n 0 def << /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 {bbox} {bbox}] \
                /XStep {bbox} /YStep {bbox} /PaintProc {{ pop /n n 1 add def \
                /Pattern setcolorspace pp setcolor 0 0 5 5 rectfill }} >> \
             matrix makepattern /pp exch def /Pattern setcolorspace pp setcolor \
             {{ 0 0 40 40 rectfill }} stopped {{ $error /errorname get }} if"
        ));
        assert!(got.ends_with(&format!("/{want}")), "{bbox}: {got}");
        // And the page is usable afterwards.
        assert_eq!(
            eval(&format!(
                "/n 0 def << /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 {bbox} {bbox}] \
                    /XStep {bbox} /YStep {bbox} /PaintProc {{ pop /Pattern setcolorspace pp setcolor \
                    0 0 5 5 rectfill }} >> matrix makepattern /pp exch def \
                 /Pattern setcolorspace pp setcolor {{ 0 0 40 40 rectfill }} stopped pop clear \
                 0 setgray 0 0 10 10 rectfill 1"
            )),
            "1"
        );
    }
}

#[test]
fn a_nulldevice_fill_runs_it_too_as_gs_does() {
    assert_eq!(
        eval(&format!(
            "{COUNTED} /Pattern setcolorspace pp setcolor gsave nulldevice 0 0 40 40 rectfill grestore n"
        )),
        "1"
    );
}

#[test]
fn a_long_chain_of_unused_patterns_frees_without_recursing() {
    // Each pattern is made with the last as its colour, and none is ever
    // painted, so each holds the one before through its saved state.
    let it = run_page(&format!(
        "{DICT} /d exch def 30000 {{ d matrix makepattern setpattern }} repeat"
    ));
    drop(it);
}
