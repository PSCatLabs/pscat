//! The limits and TilingType 2's laid-out cells (#1332, Codex round 1 on
//! PR #1372). The memory cap counts the whole process's heap, so this lives
//! in a test binary of its own with one test, as `limits_memory.rs` does.
//!
//! A fractional TilingType 2 pattern paints from its cells laid out at
//! `floor(i * step)` over the canvas. When a limit keeps that layout from
//! being made, the pattern paints from its whole-pixel period instead: the
//! cells of a 7.3 px step are then 7 px apart, so the fifth starts at 28,
//! not 29.

use pscat::{Interp, Limits, PsError};

#[global_allocator]
static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;

const MIB: usize = 1 << 20;

/// A pattern of `bw` x `bh` cells on `xs` x `ys` steps, painted over the
/// `fill` rectangle (`x y w h`).
fn program(bw: u32, bh: u32, xs: &str, ys: &str, fills: &[&str]) -> String {
    let mut src = format!(
        "<< /PatternType 1 /PaintType 1 /TilingType 2 /BBox [0 0 {bw} {bh}] \
         /XStep {xs} /YStep {ys} \
         /PaintProc {{ pop 0 setgray 0 0 {bw} {bh} rectfill }} >> matrix makepattern setpattern\n"
    );
    for f in fills {
        src.push_str(&format!("{f} rectfill\n"));
    }
    src
}

fn run(w: u32, h: u32, limits: Limits, src: &str) -> (Interp, Result<(), PsError>) {
    let mut it = Interp::with_page(w, h).expect("page");
    it.set_limits(limits).expect("the allocator is installed");
    let r = it.run_source(src.as_bytes());
    (it, r)
}

/// Where the ink starts along the first inked row at or below `from`.
fn starts(it: &Interp, from: u32) -> Vec<u32> {
    let pm = &it.gfx().pixmap;
    let ink = |x: u32, y: u32| pm.pixel(x, y).is_some_and(|p| p.red() < 128);
    let y = (from..pm.height())
        .find(|&y| (0..pm.width()).any(|x| ink(x, y)))
        .expect("an inked row");
    (0..pm.width())
        .filter(|&x| ink(x, y) && (x == 0 || !ink(x - 1, y)))
        .collect()
}

/// Every x where an inked run starts along row `y`.
fn run_starts(it: &Interp, y: u32) -> Vec<u32> {
    let pm = &it.gfx().pixmap;
    let ink = |x: u32| pm.pixel(x, y).is_some_and(|p| p.red() < 128);
    (0..pm.width())
        .filter(|&x| ink(x) && (x == 0 || !ink(x - 1)))
        .collect()
}

const LATTICE: [u32; 6] = [0, 7, 14, 21, 29, 36];
const PERIOD: [u32; 6] = [0, 7, 14, 21, 28, 35];

#[test]
fn laid_out_cells_obey_the_limits() {
    assert!(pscat::memtrack::installed());
    let held = vec![1u8; 1 << 20];
    drop(held);

    // Steps: a 100x100 cell on 1.1 px steps over a 1000x1000 page is some
    // 100 million pixels written for a one-pixel fill. The layout's work is
    // charged, so a cap of 1000 steps stops it.
    let src = program(100, 100, "1.1", "1.1", &["0 0 1 1"]);
    let (_, r) = run(1000, 1000, Limits::none().with_max_steps(1000), &src);
    assert_eq!(r, Err(PsError::StepsExceeded));

    // Memory, the row of cells: a 2 x 8000 cell makes a 32 MB row for a
    // 1000x10 canvas. Under an 8 MiB cap the layout is not made and the
    // pattern paints from its period; with room, from the layout.
    let src = program(2, 8000, "7.3", "8000.2", &["0 0 1000 10"]);
    let (it, r) = run(1000, 10, Limits::none().with_max_memory(8 * MIB), &src);
    assert_eq!(r, Ok(()));
    assert_eq!(starts(&it, 0)[..6], PERIOD);
    let (it, r) = run(1000, 10, Limits::none().with_max_memory(200 * MIB), &src);
    assert_eq!(r, Ok(()));
    assert_eq!(starts(&it, 0)[..6], LATTICE);

    // Memory, the cache: the layout of a 1000x1000 page is 4 MB, which fits
    // an 8 MiB cap once but not twice. The second fill finds the first's
    // layout and reuses it before asking for room for another.
    let src = program(3, 3, "7.3", "7.3", &["0 0 1000 400", "0 500 1000 400"]);
    let (it, r) = run(1000, 1000, Limits::none().with_max_memory(8 * MIB), &src);
    assert_eq!(r, Ok(()));
    assert_eq!(starts(&it, 0)[..6], LATTICE, "first fill");
    assert_eq!(starts(&it, 600)[..6], LATTICE, "second fill");

    // The same limits for a glyph painted with the pattern (`show` and the
    // family), which used to bypass them.
    let font = "/Helvetica findfont 20 scalefont setfont 100 100 moveto (A) show\n";
    let src = program(100, 100, "1.1", "1.1", &[]) + font;
    let (_, r) = run(1000, 1000, Limits::none().with_max_steps(1000), &src);
    assert_eq!(r, Err(PsError::StepsExceeded), "show");
    let src = program(100, 100, "1.1", "1.1", &[]) + "0 0 moveto 9 9 lineto stroke\n";
    let (_, r) = run(1000, 1000, Limits::none().with_max_steps(1000), &src);
    assert_eq!(r, Err(PsError::StepsExceeded), "stroke");

    // A glyph 4000 pt tall puts a 360 px bar across a 1000x10 page. Under
    // the 8 MiB cap its cells are 7 px apart (the period), with room 7.3
    // (the layout).
    let bar = "/Helvetica findfont 4000 scalefont setfont 100 -1000 moveto (l) show\n";
    let src = program(2, 8000, "7.3", "8000.2", &[]) + bar;
    let inside = |v: &[u32]| -> Vec<u32> {
        let (lo, hi) = (v[1], v[v.len() - 2]);
        v.iter().copied().filter(|&x| x > lo && x < hi).collect()
    };
    let on = |v: &[u32], f: &dyn Fn(u32) -> u32| -> bool {
        let n = v.len() as u32;
        !v.is_empty() && v.iter().all(|&x| (0..=n * 8).any(|k| f(k) == x))
    };
    let lattice_pos = |k: u32| (f64::from(k) * 7.3 + 1e-6).floor() as u32;
    let period_pos = |k: u32| k * 7;
    let (it, r) = run(1000, 10, Limits::none().with_max_memory(8 * MIB), &src);
    assert_eq!(r, Ok(()), "show under the memory cap");
    let v = inside(&run_starts(&it, 5));
    assert!(
        v.len() > 10 && on(&v, &period_pos) && !on(&v, &lattice_pos),
        "{v:?}"
    );
    let (it, r) = run(1000, 10, Limits::none().with_max_memory(200 * MIB), &src);
    assert_eq!(r, Ok(()), "show with room");
    let v = inside(&run_starts(&it, 5));
    assert!(
        v.len() > 10 && on(&v, &lattice_pos) && !on(&v, &period_pos),
        "{v:?}"
    );

    foreign_layouts_obey_the_limits();
}

/// The page's pattern of `program`, painted by a one-pixel fill (`fill`)
/// inside the PaintProc of an outer pattern whose cell is `w` x `h`, which
/// the page then fills over `page_fill` (#1373): the inner pattern's cells
/// are laid out over the outer pattern's capture canvas.
fn foreign(inner: &str, w: u32, h: u32, fill: &str, page_fill: &str) -> String {
    let inner = inner.replace("setpattern\n", "/inner exch def\n");
    format!(
        "{inner}<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 {w} {h}] \
         /XStep {} /YStep {} /PaintProc {{ pop inner setpattern {fill} rectfill }} >> \
         matrix makepattern setpattern\n{page_fill} rectfill\n",
        2 * w,
        2 * h
    )
}

/// A blank page of the size of `it`'s: what a stopped run must leave.
fn blank(it: &Interp) -> bool {
    let pm = &it.gfx().pixmap;
    let fresh = Interp::with_page(pm.width(), pm.height()).expect("page");
    pm.data() == fresh.gfx().pixmap.data()
}

/// The limits hold for a layout in another context than the pattern was
/// made in (#1373): the layout is over the capture canvas of the pattern
/// whose PaintProc paints, up to 2^26 pixels, against the lattice's 2^24.
fn foreign_layouts_obey_the_limits() {
    // Steps: 100x100 cells on 1.1 px steps over a 1000x1000 capture, for a
    // one-pixel fill, some 100 million pixels written. A Type 1 inner
    // pattern, which lays nothing out, runs under the same cap.
    let inner = program(100, 100, "1.1", "1.1", &[]);
    let src = foreign(&inner, 1000, 1000, "0 0 1 1", "0 0 1 1");
    let (it, r) = run(1000, 1000, Limits::none().with_max_steps(1000), &src);
    assert_eq!(r, Err(PsError::StepsExceeded), "foreign: steps");
    assert!(blank(&it), "foreign: a stop paints nothing");
    // Nor records anything: the SVG and PDF are those of the program
    // without the fill that stopped.
    let recorded = |src: &str| {
        let mut it = Interp::with_page(1000, 1000).expect("page");
        it.set_limits(Limits::none().with_max_steps(1000))
            .expect("the allocator is installed");
        it.enable_svg();
        it.gfx_mut().enable_pdf();
        let r = it.run_source(src.as_bytes());
        (r, it.svg_pages(), it.gfx().pdf_document())
    };
    let (r, svg, pdf) = recorded(&src);
    assert_eq!(r, Err(PsError::StepsExceeded), "foreign: steps, recording");
    let before = src.trim_end().trim_end_matches("0 0 1 1 rectfill");
    let (r, svg0, pdf0) = recorded(before);
    assert_eq!(r, Ok(()));
    assert_eq!(svg, svg0, "foreign: a stop records no SVG");
    // The page's content stream (the PDF's first) is the same. The inner
    // pattern's own object is in the stopped PDF, unreferenced: its
    // PaintProc ran before the fill stopped, as on the page before #1373.
    let first_stream = |pdf: Option<Vec<u8>>| -> Vec<u8> {
        let pdf = pdf.expect("pdf on");
        let at = |pat: &[u8], from: usize| {
            from + pdf[from..]
                .windows(pat.len())
                .position(|w| w == pat)
                .expect("a stream")
        };
        let start = at(b"stream\n", 0);
        pdf[start..at(b"endstream", start)].to_vec()
    };
    assert_eq!(
        first_stream(pdf),
        first_stream(pdf0),
        "foreign: a stop records nothing on the PDF page"
    );
    let one = src.replace("/TilingType 2", "/TilingType 1");
    let (_, r) = run(1000, 1000, Limits::none().with_max_steps(1000), &one);
    assert_eq!(r, Ok(()), "foreign: Type 1 control");

    // Time: 50x50 cells on 1.1 px steps over the same capture, some 47
    // million pixels, the copies polled against the clock. A ladder of
    // clocks, no fixed budget (Codex round 1 on PR #1480, nitpick C): each
    // run either finishes with the whole page or stops with it blank.
    let inner = program(50, 50, "1.1", "1.1", &[]);
    let src = foreign(&inner, 1000, 1000, "0 0 1 1", "0 0 1 1");
    let (whole, r) = run(1000, 1000, Limits::none(), &src);
    assert_eq!(r, Ok(()));
    let (mut stopped, mut finished) = (0, 0);
    for ms in [1u64, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 4096] {
        let limits = Limits::none().with_max_time(std::time::Duration::from_millis(ms));
        let (it, r) = run(1000, 1000, limits, &src);
        match r {
            Ok(()) => {
                finished += 1;
                assert!(
                    it.gfx().pixmap.data() == whole.gfx().pixmap.data(),
                    "{ms} ms: the whole page"
                );
            }
            Err(PsError::TimeExceeded) => {
                stopped += 1;
                assert!(blank(&it), "{ms} ms: a stop at the clock paints nothing");
            }
            Err(e) => panic!("{ms} ms: {e:?}"),
        }
    }
    assert!(
        stopped > 0 && finished > 0,
        "foreign: time, {stopped} stopped, {finished} finished"
    );

    // A cell made in a PaintProc (so turned into gs's device to be laid
    // out) is charged for the turn before it is made (Codex round 1 on PR
    // #1480, finding B): a 2000 x 2000 cell is some 3,900 steps to turn on
    // top of what its capture and its layout cost, which a 6,000-step cap
    // allowed when the turn was free.
    let turned = |m: &str, canvases: u32| {
        format!(
            "<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 100 100] /XStep 100 /YStep 100
   /PaintProc {{ pop
     << /PatternType 1 /PaintType 1 /TilingType 2 /BBox [0 0 2000 2000] /XStep 4000.3 /YStep 4000.3
        /PaintProc {{ pop 0 0 1 setrgbcolor 0 0 2000 2000 rectfill }} >> {m} makepattern
     /inner exch def
     0 1 {} {{ /n exch def
       << /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 n 100 add 100] /XStep 1000 /YStep 1000
          /PaintProc {{ pop inner setpattern 0 0 1 1 rectfill }} >> matrix makepattern setpattern
       0 0 1 1 rectfill }} for
   }} >> matrix makepattern setpattern
0 0 1 1 rectfill",
            canvases - 1
        )
    };
    let src = turned("[1 0 0 1 0 0]", 1);
    let (it, r) = run(100, 100, Limits::none().with_max_steps(6000), &src);
    assert_eq!(
        r,
        Err(PsError::StepsExceeded),
        "turned: the turn is charged"
    );
    assert!(blank(&it), "turned: a stop paints nothing");
    let (_, r) = run(100, 100, Limits::none().with_max_steps(20_000), &src);
    assert_eq!(r, Ok(()), "turned: with room");
    // With no cell reaching the canvas nothing is turned: five canvases
    // that each would turn the cell again run under 1,000 steps, and paint
    // what they paint with no limits.
    let src = turned("[1 0 0 1 1000 -2900]", 5);
    let (free, r) = run(100, 100, Limits::none(), &src);
    assert_eq!(r, Ok(()));
    let (it, r) = run(100, 100, Limits::none().with_max_steps(1000), &src);
    assert_eq!(r, Ok(()), "no cell reaches: nothing to turn");
    assert!(it.gfx().pixmap.data() == free.gfx().pixmap.data());

    // Memory: a 2 x 8000 cell makes a 32 MB row for a 1000x10 capture.
    // Under an 8 MiB cap the layout is not made and the cells come from the
    // period, 7 px apart; with room, 7.3.
    let inner = program(2, 8000, "7.3", "8000.2", &[]);
    let src = foreign(&inner, 1000, 10, "0 0 1000 10", "0 0 1000 10");
    let (it, r) = run(1000, 10, Limits::none().with_max_memory(8 * MIB), &src);
    assert_eq!(r, Ok(()), "foreign: memory cap");
    assert_eq!(starts(&it, 0)[..6], PERIOD, "foreign: under the cap");
    let (it, r) = run(1000, 10, Limits::none().with_max_memory(200 * MIB), &src);
    assert_eq!(r, Ok(()), "foreign: memory with room");
    assert_eq!(starts(&it, 0)[..6], LATTICE, "foreign: with room");
}
