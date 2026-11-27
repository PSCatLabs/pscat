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

/// A 64 x 64 table of noise as a Type 1 shading's function: a smooth-edged
/// image that PNG compresses poorly.
fn noise_table() -> String {
    let mut seed = 5u32;
    (0..64 * 64 * 3)
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
    let mut it = Interp::with_page(page, page).expect("page");
    it.gfx_mut().enable_svg();
    it.run_source(format!("1 0 0 setrgbcolor 0 0 {page} {page} rectfill {pattern}").as_bytes())
        .expect("setup");
    it.set_limits(Limits::none().with_max_memory(cap))
        .expect("the allocator is installed");
    let (peak, r) = peak_growth(|| it.run_source(tiles.as_bytes()));
    let svg = it.gfx().svg_pages().expect("svg on").concat();
    (peak, r, svg, it.gfx().pixmap.data().to_vec())
}

fn grad(size: u32) -> String {
    format!(
        "<< /PatternType 2 /Shading << /ShadingType 1 /ColorSpace /DeviceRGB \
         /Matrix [{size} 0 0 {size} 0 0] /Function << /FunctionType 0 /Domain [0 1 0 1] \
         /Range [0 1 0 1 0 1] /Size [64 64] /BitsPerSample 8 /DataSource <{}> >> >> >> \
         matrix makepattern /grad exch def",
        noise_table()
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
    // The margin is for what no check of this change covers (a cell's own
    // canvas, a few strings and tables, a quarter of a MiB at most); the
    // faults below are whole copies of an image, several MiB.
    let margin = MIB;

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
            peak <= cap + 3 * margin,
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
}
