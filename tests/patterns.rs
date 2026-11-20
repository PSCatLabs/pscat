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

// --- shading patterns (#1298) -----------------------------------------------

/// A two-colour ramp, red to blue.
const RAMP: &str =
    "/ramp << /FunctionType 2 /Domain [0 1] /C0 [0.9 0.1 0.1] /C1 [0.1 0.2 0.9] /N 1 >> def\n";

/// The issue's picture: text filled with an axial shading pattern. The
/// pattern is made under a translation and used under a scale, so a
/// pattern that follows the CTM at use, not at `makepattern`, shows.
const SHADED_TEXT: &str = "
gsave 0 40 translate
<< /PatternType 2 /Shading << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [20 0 380 0]
   /Function ramp /Extend [true true] >> >> matrix makepattern /grad exch def
grestore
grad setpattern
/Helvetica-Bold findfont 110 scalefont setfont 20 170 moveto (Shade) show
gsave 2 2 scale 10 20 190 30 rectfill grestore
";

/// A radial pattern with a `/Background` and no `/Extend`, a Type 1
/// (function-based) pattern cut by its `/BBox`, a stroke, and a shading
/// whose axis collapses (it paints only its background, as in gs).
const SHADED_MIX: &str = "
<< /PatternType 2 /Shading << /ShadingType 3 /ColorSpace /DeviceRGB /Coords [100 300 10 100 300 80]
   /Function ramp /Background [1 0.85 0.2] >> >> matrix makepattern setpattern
20 220 160 160 rectfill
<< /PatternType 2 /Shading << /ShadingType 1 /ColorSpace /DeviceRGB /Matrix [140 0 0 140 230 230]
   /Function << /FunctionType 0 /Domain [0 1 0 1] /Range [0 1 0 1 0 1] /Size [2 2] /BitsPerSample 8
                /DataSource <ff0000 00ff00 0000ff ffffff> >> /BBox [240 240 360 360] >> >>
matrix makepattern setpattern
220 220 160 160 rectfill
<< /PatternType 2 /Shading << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 0 200]
   /Function ramp /Extend [true true] >> >> [1 0 0 1 0 20] makepattern setpattern
14 setlinewidth 1 setlinecap 30 40 moveto 120 180 lineto 180 40 lineto stroke
<< /PatternType 2 /Shading << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [300 0 300 0]
   /Function ramp /Background [0.3 0.7 0.3] >> >> matrix makepattern setpattern
220 20 160 160 rectfill
";

/// Every output of a shading-pattern picture against our PNG, and gs's
/// run of the program against gs's raster of our PDF. The PDF holds a
/// real `/PatternType 2` pattern and no image. With `text`, gs's run of
/// the program draws gs's own Helvetica (its glyphs are not pscat's), so
/// it is held only against our PNG, at the looser bound.
fn check_shading(name: &str, src: &str, text: bool, hayro: bool) -> Outputs {
    let src = format!("{RAMP}{src}");
    let out = render(&src);
    let pdf = pdf_text(&out.pdf);
    assert!(pdf.contains("/PatternType 2 "), "{pdf}");
    assert!(pdf.contains("/Pattern cs /P"), "no pattern fill: {pdf}");
    assert!(
        !pdf.contains("/Subtype /Image"),
        "a raster stands in: {pdf}"
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
            &format!("{name}: our PDF via gs"),
            &out.png,
            &gs_png(&out.pdf, "pdf", name),
            3.0,
        );
        let ps = format!("%!PS\n{src}\nshowpage\n");
        let program = gs_png(ps.as_bytes(), "ps", name);
        assert_close(
            &format!("{name}: the program in gs"),
            &out.png,
            &program,
            3.0,
        );
        if text {
            return out;
        }
        assert_close(
            &format!("{name}: our PDF via gs against the program in gs"),
            &program,
            &gs_png(&out.pdf, "pdf", name),
            0.5,
        );
    }
    out
}

#[test]
fn a_shading_pattern_fills_text_in_all_three_outputs() {
    let out = check_shading("shaded text", SHADED_TEXT, true, true);
    // The SVG paints with the gradient itself.
    assert!(out.svg.contains("<linearGradient "), "{}", out.svg);
    assert!(out.svg.contains("fill=\"url(#g"), "{}", out.svg);
    let px = |x: u32, y: u32| out.png.pixel(x, y).expect("pixel");
    // The ramp runs across the page as made, not under the later scale:
    // the bar's left end is red, its right end blue.
    let (l, r) = (px(30, 340), px(370, 340));
    assert!(l.red() > 180 && l.blue() < 60, "left {l:?}");
    assert!(r.blue() > 180 && r.red() < 60, "right {r:?}");
}

#[test]
fn radial_function_background_bbox_and_stroke_patterns_agree_with_gs() {
    let out = check_shading("shaded mix", SHADED_MIX, false, true);
    let px = |x: u32, y: u32| {
        let p = out.png.pixel(x, y).expect("pixel");
        (p.red(), p.green(), p.blue())
    };
    // The radial's background outside its circles and inside the first.
    assert_eq!(px(10, 10), (255, 255, 255));
    assert_eq!(px(25, 25), (255, 217, 51));
    assert_eq!(px(25, 175), (255, 217, 51));
    assert_eq!(px(100, 100), (255, 217, 51));
    // The Type 1 pattern only inside its BBox.
    assert_eq!(px(225, 175), (255, 255, 255));
    assert_ne!(px(300, 100), (255, 255, 255));
    // The collapsed axis paints its background.
    assert_eq!(px(300, 300), (77, 179, 77));
}

/// Strokes in pen space (an anisotropic CTM, #1196) with a plain axial
/// pattern (SVG: the gradient, placed through the pen's inverse) and with
/// one that has a background (SVG: a `<pattern>`), a radial cut to its
/// `/BBox`, and a Type 1 pattern whose background shows outside its
/// Domain, inside its `/BBox` (gs paints it there).
const SHADED_PEN: &str = "
<< /PatternType 2 /Shading << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 0 200]
   /Function ramp /Extend [true true] >> >> matrix makepattern /plain exch def
<< /PatternType 2 /Shading << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [40 0 160 0]
   /Function ramp /Background [0.2 0.7 0.3] >> >> matrix makepattern /backed exch def
gsave 1 3 scale plain setpattern 6 setlinewidth 30 10 moveto 90 60 lineto stroke
backed setpattern 110 10 moveto 180 60 lineto stroke grestore
<< /PatternType 2 /Shading << /ShadingType 3 /ColorSpace /DeviceRGB /Coords [300 100 10 300 100 70]
   /Function ramp /Extend [true false] /BBox [215 25 385 175] >> >> matrix makepattern setpattern
210 20 180 160 rectfill
<< /PatternType 2 /Shading << /ShadingType 1 /ColorSpace /DeviceRGB /Matrix [100 0 0 100 250 250]
   /Function << /FunctionType 0 /Domain [0 1 0 1] /Range [0 1 0 1 0 1] /Size [2 2] /BitsPerSample 8
                /DataSource <ff0000 00ff00 0000ff ffffff> >>
   /Background [0.2 0.7 0.3] /BBox [220 220 380 380] >> >> matrix makepattern setpattern
210 210 180 180 rectfill
";

#[test]
fn pen_space_strokes_and_a_type1_background_agree_with_gs() {
    let out = check_shading("shaded pen", SHADED_PEN, false, true);
    let px = |x: u32, y: u32| {
        let p = out.png.pixel(x, y).expect("pixel");
        (p.red(), p.green(), p.blue())
    };
    // The Type 1 background outside the Domain, inside the BBox; nothing
    // outside the BBox.
    assert_eq!(px(230, 170), (51, 179, 77));
    assert_eq!(px(215, 170), (255, 255, 255));
    // The plain stroke takes the gradient itself in SVG, through the pen.
    assert!(out.svg.contains("<linearGradient "), "{}", out.svg);
}

/// A radial whose circles are not nested and whose end does not extend,
/// cut to a `/BBox`: the raster paints a point's smaller root where its
/// larger one is past the end (`two_root_layer`), as gs and PDF do. SVG
/// cannot ask for the smaller root (the gap `shfill` has too), so only the
/// raster and the PDF are held against gs.
const SHADED_TWO_ROOTS: &str = "
<< /PatternType 2 /Shading << /ShadingType 3 /ColorSpace /DeviceRGB /Coords [160 200 20 260 200 60]
   /Function ramp /Extend [true false] /BBox [60 60 340 340] >> >> matrix makepattern setpattern
20 20 360 360 rectfill
";

#[test]
fn a_radial_pattern_paints_its_smaller_root_as_gs_does() {
    let src = format!("{RAMP}{SHADED_TWO_ROOTS}");
    let out = render(&src);
    assert!(pdf_text(&out.pdf).contains("/PatternType 2 "));
    if common::gs_available() {
        let ps = format!("%!PS\n{src}\nshowpage\n");
        let program = gs_png(ps.as_bytes(), "ps", "two roots");
        assert_close("two roots: the program in gs", &out.png, &program, 3.0);
        assert_close(
            "two roots: our PDF via gs",
            &out.png,
            &gs_png(&out.pdf, "pdf", "two roots"),
            3.0,
        );
    }
}

#[test]
fn setalpha_reaches_a_shading_pattern() {
    // `setalpha` is pscat's, so gs runs only our PDF (its `ca`); hayro
    // ignores `ca` on a shading pattern and sits this one out.
    let src = format!(
        "{RAMP}0 0.5 0 setrgbcolor 100 0 200 400 rectfill \
         << /PatternType 2 /Shading << /ShadingType 2 /ColorSpace /DeviceRGB \
         /Coords [20 0 380 0] /Function ramp /Extend [true true] >> >> matrix makepattern \
         setpattern 0.5 setalpha 20 100 360 200 rectfill"
    );
    let out = render(&src);
    let p = out.png.pixel(200, 200).expect("pixel");
    assert!(p.green() > 40 && p.green() < 200, "not translucent: {p:?}");
    assert_close(
        "alpha shading: SVG via resvg",
        &out.png,
        &resvg_png(&out.svg),
        3.0,
    );
    if common::gs_available() {
        assert_close(
            "alpha shading: our PDF via gs",
            &out.png,
            &gs_png(&out.pdf, "pdf", "alpha shading"),
            3.0,
        );
    }
}

#[test]
fn an_imagemask_paints_a_shading_pattern() {
    let src = format!(
        "{RAMP}<< /PatternType 2 /Shading << /ShadingType 2 /ColorSpace /DeviceRGB \
         /Coords [20 0 380 0] /Function ramp /Background [0 0.6 0] >> >> matrix makepattern setpattern \
         gsave 20 20 translate 170 170 scale stencil grestore \
         gsave 300 210 translate 30 rotate 120 120 scale stencil grestore"
    );
    check_masked("masked shading", &src, "/Pattern cs /P0 scn", true);
}

/// A page shading pattern inside a tiling pattern's PaintProc keeps its
/// device placement, as gs places it in its pattern accumulator (#1300).
const SHADED_ACROSS: &str = "
<< /PatternType 2 /Shading << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 60 0]
   /Function ramp /Extend [true true] >> >> matrix makepattern /grad exch def
<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 80 80] /XStep 100 /YStep 100
   /PaintProc { pop grad setpattern 10 10 60 60 rectfill }
>> [1 0 0 1 10 10] makepattern setpattern
0 0 400 400 rectfill
";

#[test]
fn a_page_shading_pattern_paints_inside_a_paintproc_as_gs_places_it() {
    let src = format!("{RAMP}{SHADED_ACROSS}");
    let out = check_outputs("shaded across", &src, 1, false);
    if common::gs_available() {
        let ps = format!("%!PS\n{src}\nshowpage\n");
        assert_close(
            "shaded across: the program in gs",
            &out.png,
            &gs_png(ps.as_bytes(), "ps", "shaded across"),
            3.0,
        );
    }
}

const SHADING: &str = "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 100 0] \
                       /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> >>";

#[test]
fn shading_makepattern_errors_are_the_ones_gs_gives() {
    // gs 10.07.1, each `{ dict matrix makepattern } stopped`.
    let cases = [
        ("<< /PatternType 2 >>".to_string(), "undefined"),
        ("<< /PatternType 2 /Shading 5 >>".to_string(), "typecheck"),
        (
            "<< /PatternType 2 /Shading << /ShadingType 9 /ColorSpace /DeviceRGB >> >>".to_string(),
            "rangecheck",
        ),
        (
            format!("<< /PatternType 2 /Shading {SHADING} dup /Coords [0 0 1] put >>"),
            "rangecheck",
        ),
        (
            format!("<< /PatternType 2 /Shading {SHADING} dup /Function undef >>"),
            "undefined",
        ),
        (
            format!("<< /PatternType 2 /Shading {SHADING} dup /Background [0 1] put >>"),
            "rangecheck",
        ),
        (
            format!("<< /PatternType 2 /Shading {SHADING} dup /Background [(a) 1 0] put >>"),
            "typecheck",
        ),
        (
            format!("<< /PatternType 2.5 /Shading {SHADING} >>"),
            "rangecheck",
        ),
    ];
    for (dict, want) in cases {
        let got = eval(&format!(
            "{dict} matrix {{ makepattern }} stopped {{ $error /errorname get }} if"
        ));
        assert!(got.ends_with(&format!("/{want}")), "{dict}: {got}");
    }
    // Taken by gs: a real PatternType, any /ExtGState, a /Background that
    // is not an array or out of range, a singular matrix.
    for dict in [
        format!("<< /PatternType 2.0 /Shading {SHADING} >>"),
        DICT.replace("/PatternType 1", "/PatternType 1.0"),
        format!("<< /PatternType 2 /Shading {SHADING} /ExtGState 5 >>"),
        format!("<< /PatternType 2 /Shading {SHADING} dup /Background 5 put >>"),
        format!("<< /PatternType 2 /Shading {SHADING} dup /Background [0 2 0] put >>"),
    ] {
        assert_eq!(
            eval(&format!("{dict} matrix makepattern /Implementation known")),
            "true",
            "{dict}"
        );
    }
    assert_eq!(
        eval(&format!(
            "<< /PatternType 2 /Shading {SHADING} >> [0 0 0 0 0 0] makepattern \
             dup setpattern 0 0 50 50 rectfill /Implementation known"
        )),
        "true"
    );
}

#[test]
fn a_shading_pattern_is_coloured_and_reads_back_as_gs() {
    // A shading pattern takes no components: they stay on the stack.
    assert_eq!(
        eval(&format!(
            "<< /PatternType 2 /Shading {SHADING} >> matrix makepattern /p exch def \
             [/Pattern /DeviceRGB] setcolorspace 0.1 0.2 0.3 p setcolor \
             count currentcolor p eq"
        )),
        "0.1 0.2 0.3 3 true"
    );
    assert_eq!(
        eval(&format!(
            "<< /PatternType 2 /Shading {SHADING} >> matrix makepattern setpattern \
             currentcolorspace"
        )),
        "[/Pattern [/DeviceGray]]"
    );
}

// --- Codex round 1 on PR #1367 ---------------------------------------------

/// A page-sized axial shading pattern, set as the colour.
const PAGE_RAMP: &str = "<< /PatternType 2 /Shading << /ShadingType 2 /ColorSpace /DeviceRGB \
     /Coords [0 0 1000 0] /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> \
     /Extend [true true] >> >> matrix makepattern setpattern\n";

/// pscat on a 1000 x 1000 page of `src` under `limit` (`--max-steps N` or
/// `--max-memory MIB`): whether it succeeded, and its stdout then stderr. A child
/// process, as the memory limit counts the whole process's heap.
fn limited(src: &str, limit: [&str; 2]) -> (bool, String) {
    use common::CommandDeadline as _;
    let dir = common::scratch_dir("pscat_shading_limits");
    let file = dir.join("limited.ps");
    std::fs::write(&file, src).expect("write source");
    let out = Command::new(env!("CARGO_BIN_EXE_pscat"))
        .args(["--page", "1000x1000", limit[0], limit[1], "--png"])
        .arg(dir.join("limited.png"))
        .arg(&file)
        .stdin(std::process::Stdio::null())
        .output_deadline()
        .expect("spawn pscat");
    (
        out.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

/// A shading pattern's layer under an imagemask or a stroke answers to the
/// step and memory limits, as under a fill (Codex round 1, P1): it is made
/// before anything is recorded or painted, its work held against the step
/// limit and its memory against the cap.
#[test]
fn a_shading_layer_under_imagemask_and_stroke_answers_to_the_limits() {
    let marks = [
        (
            "imagemask",
            "1000 1000 scale 1 1 true [1 0 0 1 0 0] <80> imagemask",
        ),
        (
            "stroke",
            "600 setlinewidth 0 500 moveto 1000 500 lineto stroke",
        ),
        ("rectfill", "0 0 1000 1000 rectfill"),
    ];
    for (op, mark) in marks {
        let src = format!("{PAGE_RAMP}{mark}\n");
        for (limit, what) in [
            (["--max-steps", "1000"], "steps"),
            (["--max-memory", "2"], "memory"),
        ] {
            let (ok, err) = limited(&src, limit);
            assert!(
                !ok && err.contains(&format!("limitexceeded ({what})"))
                    && err.contains(&format!("OffendingCommand: {op}")),
                "{op} under {limit:?}: {err}"
            );
        }
        // And without a limit to meet, it paints.
        let (ok, err) = limited(&src, ["--max-steps", "0"]);
        assert!(ok, "{op} with no limit: {err}");
    }
}

/// A pattern made on one page and reused on a later page of another height
/// keeps its device placement in the PDF, as in the PNG and gs (Codex round
/// 1, P2): the PDF `/Matrix` maps to that page's default space, whose flip
/// the new height moves.
#[test]
fn a_shading_pattern_reused_on_a_taller_page_keeps_its_place_in_pdf() {
    let src = "<< /PageSize [100 100] >> setpagedevice
<< /PatternType 2 /Shading << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 0 100]
   /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> >> >>
matrix makepattern /p exch def
p setpattern 0 0 100 100 rectfill showpage
<< /PageSize [100 200] >> setpagedevice
p setpattern 0 0 100 200 rectfill
";
    // The CLI, which changes the page size as `setpagedevice` asks.
    use common::CommandDeadline as _;
    let dir = common::scratch_dir("pscat_shading_pages");
    let (ps, pdf_path) = (dir.join("pages.ps"), dir.join("pages.pdf"));
    std::fs::write(&ps, format!("%!PS\n{src}showpage\n")).expect("write ps");
    let out = Command::new(env!("CARGO_BIN_EXE_pscat"))
        .arg("--png")
        .arg(dir.join("pages.png"))
        .arg("--pdf")
        .arg(&pdf_path)
        .arg(&ps)
        .stdin(std::process::Stdio::null())
        .output_deadline()
        .expect("spawn pscat");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let png = Pixmap::decode_png(&std::fs::read(dir.join("pages-002.png")).expect("page 2"))
        .expect("decode");
    assert_eq!((png.width(), png.height()), (100, 200));
    let pdf = std::fs::read(&pdf_path).expect("read pdf");
    // Our raster: the shading in device rows 0-100 (blue at the top, red at
    // row 100), nothing below (the shading does not extend).
    let px = |pm: &Pixmap, y: u32| {
        let p = pm.pixel(50, y).expect("pixel");
        (p.red(), p.green(), p.blue())
    };
    assert!(
        px(&png, 5).2 > 200 && px(&png, 95).0 > 200,
        "{:?}",
        px(&png, 5)
    );
    assert_eq!(px(&png, 150), (255, 255, 255));
    // hayro's raster of the PDF's second page.
    let doc = hayro::Pdf::new(std::sync::Arc::new(pdf.clone())).expect("parse PDF");
    let pages = doc.pages();
    let pm = hayro::render(
        &pages[1],
        &hayro::InterpreterSettings::default(),
        &hayro::RenderSettings {
            x_scale: 1.0,
            y_scale: 1.0,
            ..Default::default()
        },
    );
    let (w, h) = (u32::from(pm.width()), u32::from(pm.height()));
    let bytes = pm.take_u8();
    let mut theirs = Pixmap::new(w, h).expect("pixmap");
    for (dst, src) in theirs.data_mut().chunks_mut(4).zip(bytes.chunks(4)) {
        for c in 0..3 {
            dst[c] = (u16::from(src[c]) + 255 - u16::from(src[3])) as u8;
        }
        dst[3] = 255;
    }
    for y in [5, 50, 95, 150, 195] {
        let (a, b) = (px(&png, y), px(&theirs, y));
        let near = |p: u8, q: u8| p.abs_diff(q) <= 6;
        assert!(
            near(a.0, b.0) && near(a.1, b.1) && near(a.2, b.2),
            "row {y}: PNG {a:?}, PDF via hayro {b:?}"
        );
    }
    if common::gs_available() {
        // gs's run of the program and its raster of our PDF, page 2.
        let gs = |input: &std::path::Path, out: &std::path::Path| {
            let status = Command::new("gs")
                .args([
                    "-q",
                    "-dNOPAUSE",
                    "-dBATCH",
                    "-dSAFER",
                    "-sDEVICE=png16m",
                    "-r72",
                ])
                .args(["-dFirstPage=2", "-dLastPage=2"])
                .arg(format!("-o{}", out.display()))
                .arg(input)
                .status()
                .expect("run gs");
            assert!(status.success(), "gs rejected {}", input.display());
            Pixmap::decode_png(&std::fs::read(out).expect("read gs png")).expect("decode")
        };
        let program = gs(&ps, &dir.join("program.png"));
        let ours = gs(&pdf_path, &dir.join("ours.png"));
        for y in [5, 50, 95, 150, 195] {
            let (a, b, c) = (px(&png, y), px(&program, y), px(&ours, y));
            let near = |p: (u8, u8, u8), q: (u8, u8, u8)| {
                p.0.abs_diff(q.0) <= 8 && p.1.abs_diff(q.1) <= 8 && p.2.abs_diff(q.2) <= 8
            };
            assert!(
                near(a, b) && near(c, b),
                "row {y}: PNG {a:?}, gs program {b:?}, our PDF via gs {c:?}"
            );
        }
    }
}

// --- Codex round 2 on PR #1367 ---------------------------------------------

/// A layer the shading paints nothing into still costs its allocation and
/// its `/Background`, which paints every pixel: an axis that collapses
/// with a background, page-sized under an imagemask or a stroke, is held
/// against the step limit like any other (Codex round 2, P1).
#[test]
fn a_background_only_shading_layer_is_charged() {
    let bg = "<< /PatternType 2 /Shading << /ShadingType 2 /ColorSpace /DeviceRGB \
              /Coords [0 0 0 0] /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] \
              /C1 [0 0 1] /N 1 >> /Background [1 0 0] >> >> matrix makepattern setpattern\n";
    for (op, mark) in [
        (
            "imagemask",
            "1000 1000 scale 1 1 true [1 0 0 1 0 0] <80> imagemask",
        ),
        (
            "stroke",
            "600 setlinewidth 0 500 moveto 1000 500 lineto stroke",
        ),
    ] {
        let src = format!("{bg}{mark}\n");
        let (ok, out) = limited(&src, ["--max-steps", "1000"]);
        assert!(
            !ok && out.contains("limitexceeded (steps)")
                && out.contains(&format!("OffendingCommand: {op}")),
            "{op}: {out}"
        );
        let (ok, out) = limited(&src, ["--max-steps", "0"]);
        assert!(ok, "{op} with no limit: {out}");
    }
}

/// An imagemask's layer is held against the steps left when it is made,
/// not when the image began: steps its data-source procedure spent in
/// between count (Codex round 2, P2). At this budget the plain image fits
/// and the one whose procedure loops 20000 times does not, and the limit
/// stops it at the imagemask, before anything after it runs.
#[test]
fn an_imagemask_layer_counts_the_steps_its_data_source_spent() {
    let image = |source: &str| {
        format!(
            "{PAGE_RAMP}1000 1000 scale 1 1 true [1 0 0 1 0 0] {source} imagemask \
             (AFTER_IMAGE) =\n"
        )
    };
    let (ok, out) = limited(&image("{<80>}"), ["--max-steps", "40000"]);
    assert!(ok && out.contains("AFTER_IMAGE"), "plain: {out}");
    let (ok, out) = limited(&image("{20000 {} repeat <80>}"), ["--max-steps", "40000"]);
    assert!(
        !ok && !out.contains("AFTER_IMAGE")
            && out.contains("limitexceeded (steps)")
            && out.contains("OffendingCommand: imagemask"),
        "looping source: {out}"
    );
}
