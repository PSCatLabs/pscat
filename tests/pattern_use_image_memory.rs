//! The memory cap and the SVG images made for uses of a Type 1 shading
//! pattern in another context (#1368, Codex round 1 on PR #1475, R2 and R3).
//! The cap counts the whole process's heap, so this lives in a test binary of
//! its own with one test, as `limits_memory.rs` does.

use pscat::{Interp, Limits, PsError};

#[global_allocator]
static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;

const MIB: usize = 1 << 20;

/// Run `f` while another thread watches the heap: the most it grew past
/// where it started, as that thread saw it, and `f`'s result. A copy made
/// before a refusal is freed by the time anyone looks, so this is how a test
/// sees one.
fn peak_growth<R>(f: impl FnOnce() -> R) -> (usize, R) {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let base = pscat::memtrack::live_bytes();
    let (stop, peak) = (
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicUsize::new(0)),
    );
    let watcher = {
        let (stop, peak) = (stop.clone(), peak.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let grown = pscat::memtrack::live_bytes().saturating_sub(base);
                peak.fetch_max(grown, Ordering::Relaxed);
                std::thread::sleep(std::time::Duration::from_micros(50));
            }
        })
    };
    let r = f();
    stop.store(true, Ordering::Relaxed);
    watcher.join().expect("watcher");
    (peak.load(Ordering::Relaxed), r)
}

/// An `n` x `n` table of noise as a Type 1 shading's function: a
/// smooth-edged image that PNG compresses poorly (the more so the larger `n`).
fn noise_table(n: usize) -> String {
    let mut seed = 5u32;
    (0..n * n * 3)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            format!("{:02x}", seed >> 24)
        })
        .collect()
}

/// `pattern` is the page pattern `grad`, `tiles` the program that uses it.
fn run(
    page: u32,
    pattern: &str,
    tiles: &str,
    cap: usize,
) -> (usize, Result<(), PsError>, String, Vec<u8>) {
    let (peak, r, svg, px, _) = run_full(page, pattern, tiles, cap);
    (peak, r, svg, px)
}

/// What a run left: the heap's peak, how it ended, the SVG, the page's
/// pixels, the PDF and how many operands are on the stack.
struct Ran {
    peak: usize,
    r: Result<(), PsError>,
    svg: String,
    px: Vec<u8>,
    pdf: Vec<u8>,
    depth: usize,
}

/// [`run`], with the operand stack as well.
fn run_full(
    page: u32,
    pattern: &str,
    tiles: &str,
    cap: usize,
) -> (usize, Result<(), PsError>, String, Vec<u8>, usize) {
    let ran = run_with(page, pattern, tiles, cap, false);
    (ran.peak, ran.r, ran.svg, ran.px, ran.depth)
}

/// [`run_full`], recording a PDF too when `pdf` is set.
fn run_with(page: u32, pattern: &str, tiles: &str, cap: usize, pdf: bool) -> Ran {
    let mut it = Interp::with_page(page, page).expect("page");
    it.gfx_mut().enable_svg();
    if pdf {
        it.gfx_mut().enable_pdf();
    }
    it.run_source(format!("1 0 0 setrgbcolor 0 0 {page} {page} rectfill {pattern}").as_bytes())
        .expect("setup");
    it.set_limits(Limits::none().with_max_memory(cap))
        .expect("the allocator is installed");
    let (peak, r) = peak_growth(|| it.run_source(tiles.as_bytes()));
    let svg = it.gfx().svg_pages().expect("svg on").concat();
    Ran {
        peak,
        r,
        svg,
        px: it.gfx().pixmap.data().to_vec(),
        pdf: it.gfx().pdf_document().unwrap_or_default(),
        depth: it.operand_stack().len(),
    }
}

fn grad(size: u32) -> String {
    grad_n(size, 64)
}

/// [`grad`] with an `n` x `n` table.
fn grad_n(size: u32, n: usize) -> String {
    format!(
        "<< /PatternType 2 /Shading << /ShadingType 1 /ColorSpace /DeviceRGB \
         /Matrix [{size} 0 0 {size} 0 0] /Function << /FunctionType 0 /Domain [0 1 0 1] \
         /Range [0 1 0 1 0 1] /Size [{n} {n}] /BitsPerSample 8 /DataSource <{}> >> >> >> \
         matrix makepattern /grad exch def",
        noise_table(n)
    )
}

/// The page pattern `grad` of a `size` pt page, used by one tile of
/// `tile` pt whose `PaintProc` paints one point, so the only large thing the
/// cap can meet is the image made for the tile's canvas.
fn one_tile(tile: u32) -> String {
    format!(
        "<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 {tile} {tile}] \
         /XStep {tile} /YStep {tile} /PaintProc {{ pop grad setpattern 0 0 1 1 rectfill }} >> \
         [1 0 0 1 -300 -300] makepattern setpattern 0 0 100 100 rectfill"
    )
}

/// Twelve tiles, each its own context, each using `grad` on a 1000 pt cell.
const TWELVE_TILES: &str = "/tile { /i exch def << /PatternType 1 /PaintType 1 /TilingType 1 \
    /BBox [0 0 1000 1000] /XStep 1000 /YStep 1000 \
    /PaintProc { pop grad setpattern 0 0 1 1 rectfill } >> \
    [1 0 0 1 i 0] makepattern setpattern 0 0 1 1 rectfill } def 0 1 11 { tile } for";

/// What a refused run leaves, and how far the heap went (the peak is what the
/// cap is about: a copy made before a check is freed by the time anyone looks).
#[test]
fn foreign_use_images_never_take_the_heap_past_the_cap() {
    assert!(pscat::memtrack::installed());
    // The margin is for what no check covers (a few strings and tables, a
    // quarter of a MiB at most; a cell's own canvases are checked since
    // #1477, which tightened it from 3 MiB); the faults below are whole
    // copies of an image or a canvas, several MiB.
    let margin = MIB / 2;

    // R2: a 2000 x 2000 cell's image is 16 MB of pixels, and the encoder
    // copies them and writes about as much again, none of which a cap used
    // to see before it was allocated.
    let pattern = grad(2000);
    let program = one_tile(2000);
    let (_, r, quiet, quiet_px) = run(1000, &pattern, "", 400 * MIB);
    assert_eq!(r, Ok(()));
    let mut refused = 0;
    for cap_mib in [41, 43, 46, 60] {
        let cap = cap_mib * MIB;
        let (peak, r, svg, px) = run(1000, &pattern, &program, cap);
        assert!(
            peak <= cap + margin,
            "cap {cap_mib} MiB: the heap grew {:.2} MiB past its start",
            peak as f64 / MIB as f64
        );
        match r {
            Ok(()) => assert_eq!(svg.matches("<image ").count(), 1, "cap {cap_mib} MiB"),
            Err(PsError::MemoryExceeded) => {
                refused += 1;
                assert_eq!(svg, quiet, "cap {cap_mib} MiB: the SVG is as it was");
                assert!(px == quiet_px, "cap {cap_mib} MiB: the page is as it was");
            }
            Err(e) => panic!("cap {cap_mib} MiB: {e:?}"),
        }
    }
    assert!(refused > 0, "some cap refuses the image");
    let (_, r, svg, _) = run(1000, &pattern, &program, 400 * MIB);
    assert_eq!((r, svg.matches("<image ").count()), (Ok(()), 1));

    // R3: twelve contexts each write an image of their own, a PNG kept on
    // the pattern and its base64 in the SVG; the sum, and each write's
    // transient copies, stay under the cap, which stops the run partway.
    let pattern = grad(1000);
    let (_, r, all, _) = run(1000, &pattern, TWELVE_TILES, 800 * MIB);
    assert_eq!(r, Ok(()));
    assert_eq!(all.matches("<image ").count(), 12, "one image a context");
    for cap_mib in [32, 48] {
        let cap = cap_mib * MIB;
        let (peak, r, svg, _) = run(1000, &pattern, TWELVE_TILES, cap);
        assert_eq!(r, Err(PsError::MemoryExceeded), "cap {cap_mib} MiB");
        assert!(
            peak <= cap + margin,
            "cap {cap_mib} MiB: the heap grew {:.2} MiB past its start",
            peak as f64 / MIB as f64
        );
        let n = svg.matches("<image ").count();
        assert!(n < 12, "cap {cap_mib} MiB: stopped after {n}");
    }

    // R3: strokes under eight pen matrices in one 1000 pt cell: eight
    // `<pattern>`s, one image. Eight copies of a 4 MiB base64 string would
    // take the heap past a cap that holds the image once.
    let strokes: String = (2..10)
        .map(|k| {
            format!(
                "gsave 1 {k} scale 8 setlinewidth 50 {y} moveto 500 {y} lineto stroke grestore ",
                y = 20 + k
            )
        })
        .collect();
    let pens = format!(
        "<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 1000 1000] /XStep 1000 \
         /YStep 1000 /PaintProc {{ pop grad setpattern {strokes} }} >> [1 0 0 1 -300 -300] \
         makepattern setpattern 0 0 100 100 rectfill"
    );
    let (_, r, all, _) = run(1000, &pattern, &pens, 800 * MIB);
    assert_eq!(
        (r, all.matches("<image ").count()),
        (Ok(()), 1),
        "one image, not one a pen"
    );
    let (peak, r, svg, _) = run(1000, &pattern, &pens, 40 * MIB);
    assert_eq!(r, Ok(()), "the image once fits");
    assert_eq!(svg.matches("<image ").count(), 1);
    assert!(
        peak <= 40 * MIB + margin,
        "the heap grew {:.2} MiB past its start under a 40 MiB cap",
        peak as f64 / MIB as f64
    );

    // #1477: a tile's own canvases. A 2000 x 2000 cell holds its capture
    // canvas (16 MiB), the clip mask and the first fill's coverage mask
    // (4 MiB each) and, folded at the end of the `PaintProc`, its period
    // (16 MiB) and, for a TilingType 2 lattice, its oriented cell (16 MiB
    // more). A cap in between must refuse the cell before the one that does
    // not fit is made, and leave the page, the SVG and the operands as an
    // unmade pattern leaves them.
    for tiling in [1, 2] {
        // A step that is no whole number of pixels makes a TilingType 2
        // tile a lattice, which keeps a cell of its own.
        let step = if tiling == 2 { "1999.5" } else { "2000" };
        let cell = format!(
            "<< /PatternType 1 /PaintType 1 /TilingType {tiling} /BBox [0 0 2000 2000] \
             /XStep {step} /YStep {step} /PaintProc {{ pop 0 setgray 0 0 1 1 rectfill }} >> matrix \
             makepattern setpattern 0 0 100 100 rectfill"
        );
        let (_, r, quiet, quiet_px, _) = run_full(1000, "", "", 400 * MIB);
        assert_eq!(r, Ok(()));
        let (_, r, _, made_px, _) = run_full(1000, "", &cell, 400 * MIB);
        assert_eq!(r, Ok(()), "TilingType {tiling} with room");
        assert!(made_px != quiet_px, "the pattern paints");
        let (mut refused, mut fitted, mut depth) = (0, 0, None);
        for cap_mib in (20..=76).step_by(2) {
            let cap = cap_mib * MIB;
            let (peak, r, svg, px, left) = run_full(1000, "", &cell, cap);
            assert!(
                peak <= cap + margin,
                "TilingType {tiling}, cap {cap_mib} MiB: the heap grew {:.2} MiB past its start",
                peak as f64 / MIB as f64
            );
            match r {
                Ok(()) => {
                    fitted += 1;
                    assert!(
                        px == made_px,
                        "cap {cap_mib} MiB: the pattern paints as it does"
                    );
                }
                Err(PsError::MemoryExceeded) => {
                    refused += 1;
                    assert_eq!(svg, quiet, "cap {cap_mib} MiB: the SVG is as it was");
                    assert!(px == quiet_px, "cap {cap_mib} MiB: the page is as it was");
                    // The same operands whichever way it was refused.
                    assert_eq!(
                        *depth.get_or_insert(left),
                        left,
                        "TilingType {tiling}, cap {cap_mib} MiB: operands put back"
                    );
                }
                Err(e) => panic!("TilingType {tiling}, cap {cap_mib} MiB: {e:?}"),
            }
        }
        assert!(
            refused > 0 && fitted > 0,
            "the ladder crosses the cell's size"
        );
    }

    // #1477: `makepattern` of a Type 1 shading makes the SVG's image on the
    // canvas it runs on (1000 x 1000, 4 MiB of pixels), and the encoder
    // copies the pixels and writes its output. A cap that fits the layer but
    // not the copy refuses the `makepattern`, whole: its operands stay, the
    // page and the SVG as they were, and the heap never passes the cap.
    let made = format!("{} grad pop", grad_n(2000, 512));
    let (_, r, quiet, quiet_px, _) = run_full(1000, "", "", 400 * MIB);
    assert_eq!(r, Ok(()));
    let (_, r, svg, _, _) = run_full(1000, "", &made, 400 * MIB);
    assert_eq!(r, Ok(()), "with room it is made");
    assert_eq!(
        svg, quiet,
        "an image is written at a mark, not at makepattern"
    );
    let (mut refused, mut fitted) = (0, 0);
    for cap_kib in (4 * 1024..=30 * 1024).step_by(1024) {
        let cap = cap_kib * 1024;
        let (peak, r, svg, px, left) = run_full(1000, "", &made, cap);
        assert!(
            peak <= cap + margin,
            "cap {cap_kib} KiB: the heap grew {:.2} MiB past its start",
            peak as f64 / MIB as f64
        );
        match r {
            Ok(()) => fitted += 1,
            Err(PsError::MemoryExceeded) => {
                refused += 1;
                assert_eq!(svg, quiet, "cap {cap_kib} KiB: the SVG is as it was");
                assert!(px == quiet_px, "cap {cap_kib} KiB: the page is as it was");
                assert_eq!(left, 2, "cap {cap_kib} KiB: the dict and the matrix stay");
            }
            Err(e) => panic!("cap {cap_kib} KiB: {e:?}"),
        }
    }
    assert!(
        refused > 0 && fitted > 0,
        "the ladder crosses the image's size"
    );

    // #1477: the image `makepattern` made is written inline into the SVG at
    // the first mark that paints with it, as base64 and then a copy of it
    // for each group around it. A cap that fits the pattern but not that
    // write refuses the mark, whole: the page and the SVG as they were.
    let used = format!("{} grad setpattern 0 0 100 100 rectfill", grad_n(1000, 512));
    let (_, r, quiet, quiet_px, _) = run_full(1000, "", "", 400 * MIB);
    assert_eq!(r, Ok(()));
    let (_, r, svg, _, _) = run_full(1000, "", &used, 400 * MIB);
    assert_eq!(r, Ok(()), "with room it is painted");
    assert_eq!(svg.matches("<image ").count(), 1, "the image is written");
    let (mut refused, mut fitted, mut first_fit) = (0, 0, None);
    for cap_mib in (16..=48).step_by(2) {
        let cap = cap_mib * MIB;
        let (peak, r, svg, px, _) = run_full(1000, "", &used, cap);
        assert!(
            peak <= cap + margin,
            "cap {cap_mib} MiB: the heap grew {:.2} MiB past its start",
            peak as f64 / MIB as f64
        );
        match r {
            Ok(()) => {
                fitted += 1;
                first_fit.get_or_insert(cap_mib);
            }
            Err(PsError::MemoryExceeded) => {
                refused += 1;
                if svg != quiet {
                    // A refusal at the pattern's own making leaves both so;
                    // one at the mark must too.
                    panic!("cap {cap_mib} MiB: the SVG is not as it was");
                }
                assert!(px == quiet_px, "cap {cap_mib} MiB: the page is as it was");
            }
            Err(e) => panic!("cap {cap_mib} MiB: {e:?}"),
        }
    }
    assert!(
        refused > 0 && fitted > 0,
        "the ladder crosses the write's size"
    );
    let cap = first_fit.expect("a cap fits") * MIB;
    // The marks after the first write nothing, so a cap that holds the one
    // write holds a hundred marks: they are not held to a second write's room.
    let many = format!(
        "{} grad setpattern 0 1 99 {{ 0 exch 3 1 rectfill }} for",
        grad_n(1000, 512)
    );
    let (_, r, svg, _, _) = run_full(1000, "", &many, cap);
    assert_eq!(
        (r, svg.matches("<image ").count()),
        (Ok(()), 1),
        "100 marks"
    );
    // Strokes under a pen of their own each write a copy of the image into
    // a `<pattern>` of their own: each is held to the room for it, so the
    // cap that holds one copy stops the hundred, and the clock and the
    // steps see each (Codex round 1 on PR #1493, M2).
    let pens = format!(
        "{} grad setpattern 0 0 1 1 rectfill \
         100 {{ 1.01 1 scale 0 0 moveto 1 1 lineto stroke }} repeat",
        grad_n(1000, 512)
    );
    let (peak, r, svg, _, _) = run_full(1000, "", &pens, cap);
    assert_eq!(r, Err(PsError::MemoryExceeded), "the pens do not fit {cap}");
    assert!(
        peak <= cap + margin,
        "the heap grew {:.2} MiB past its start under a {} MiB cap",
        peak as f64 / MIB as f64,
        cap / MIB
    );
    let copies = svg.matches("<image ").count();
    assert!(copies < 3, "{copies} copies of the image under the cap");

    // M1: the png compressor's own buffers (about 320 KB whatever the image)
    // are held too: a 1 x 1 image has no layer to speak of, and a heap 100
    // KB under the cap has room for its pixels but not for the encoder.
    let mut it = Interp::with_page(1, 1).expect("page");
    it.gfx_mut().enable_svg();
    it.set_limits(Limits::none().with_max_memory(MIB))
        .expect("the allocator is installed");
    let one_pixel = "<< /PatternType 2 /Shading << /ShadingType 1 /ColorSpace /DeviceRGB \
         /Matrix [1 0 0 1 0 0] /Function << /FunctionType 0 /Domain [0 1 0 1] \
         /Range [0 1 0 1 0 1] /Size [2 2] /BitsPerSample 8 \
         /DataSource <ff0000 00ff00 0000ff ffffff> >> >> >> matrix makepattern /grad exch def";
    let src = format!("900000 string /big exch def {one_pixel} /done true def");
    let (peak, r) = peak_growth(|| it.run_source(src.as_bytes()));
    assert_eq!(r, Err(PsError::MemoryExceeded), "stops at the cap");
    assert!(
        peak <= MIB + margin,
        "the heap grew {:.2} MiB past its start under a 1 MiB cap",
        peak as f64 / MIB as f64
    );

    // #1477: the end of a `PaintProc` looks at the heap again. This one
    // grows it by 8 MB of its own and leaves three operands behind, so a cap
    // that holds what the capture reserved at its start (content and masks
    // 24 MiB, period 16 MiB; a lattice's cell 16 more) is passed by the
    // period made at its end. The refusal unwinds the capture, with the PDF
    // recorder as it was, and puts the operands back.
    for (tiling, caps) in [(1, 38..=56), (2, 54..=72)] {
        // A step that is no whole number of pixels makes a TilingType 2
        // tile a lattice, which keeps a cell of its own.
        let step = if tiling == 2 { "1999.5" } else { "2000" };
        let cell = format!(
            "<< /PatternType 1 /PaintType 1 /TilingType {tiling} /BBox [0 0 2000 2000] \
             /XStep {step} /YStep {step} /PaintProc {{ pop 8000000 string /big exch def 1 2 3 \
             0 setgray 0 0 1 1 rectfill }} >> matrix makepattern setpattern 0 0 100 100 rectfill"
        );
        let quiet = run_with(1000, "", "", 400 * MIB, true);
        let made = run_with(1000, "", &cell, 400 * MIB, true);
        assert_eq!(made.r, Ok(()), "TilingType {tiling} with room");
        assert!(made.px != quiet.px, "the pattern paints");
        let (mut refused, mut fitted) = (0, 0);
        for cap_mib in caps.step_by(2) {
            let cap = cap_mib * MIB;
            let ran = run_with(1000, "", &cell, cap, true);
            assert!(
                ran.peak <= cap + margin,
                "TilingType {tiling}, cap {cap_mib} MiB: the heap grew {:.2} MiB past its start",
                ran.peak as f64 / MIB as f64
            );
            match ran.r {
                Ok(()) => {
                    fitted += 1;
                    assert!(ran.px == made.px, "cap {cap_mib} MiB: paints as it does");
                    assert_eq!(ran.depth, made.depth, "cap {cap_mib} MiB: the stack");
                }
                Err(PsError::MemoryExceeded) => {
                    refused += 1;
                    assert_eq!(ran.svg, quiet.svg, "cap {cap_mib} MiB: the SVG as it was");
                    assert!(ran.px == quiet.px, "cap {cap_mib} MiB: the page as it was");
                    assert!(ran.pdf == quiet.pdf, "cap {cap_mib} MiB: the PDF as it was");
                    // The operands of `rectfill`, and not what the `PaintProc`
                    // left above them.
                    assert_eq!(ran.depth, 4, "cap {cap_mib} MiB: operands put back");
                }
                Err(e) => panic!("TilingType {tiling}, cap {cap_mib} MiB: {e:?}"),
            }
        }
        assert!(
            refused > 0 && fitted > 0,
            "the ladder crosses the cell's size"
        );
    }
}
