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
}
