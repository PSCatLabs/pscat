//! `lib/kernel.ps`, the `kernelimage` shim for other interpreters (issue
//! #1066).
//!
//! Under pscat the native operators win: loading the shim defines neither
//! name. To test the shim itself here, the tests load it with `where`
//! answering false for `/kernelimage` and `/kernelcompile` (the only way the
//! file asks), so it defines both in userdict, where they shadow
//! systemdict's. Every picture is then rendered both ways and must match
//! byte for byte: pscat's reals are 64-bit in both paths, and the shim
//! quantizes as `src/kernel/mod.rs::quantize` does.
//!
//! The Ghostscript check renders the same programs with the shim under `gs`
//! (32-bit reals) and compares them with pscat's native render. It runs
//! only where `gs` is installed (CI has none).

use pscat::Interp;

mod common;

const SHIM: &str = include_str!("../lib/kernel.ps");

/// `where` answering false for the two kernel names, for the shim's load.
const MASK: &str = "/where { dup /kernelimage eq 1 index /kernelcompile eq or \
                    { pop false } { //where } ifelse } bind def\n";
const UNMASK: &str = "\nuserdict /where undef\n";

fn interp(w: u32, h: u32, shim: bool) -> Interp {
    let mut it = Interp::with_page(w, h).expect("page");
    if shim {
        it.run_str(&format!("{MASK}{SHIM}{UNMASK}"))
            .unwrap_or_else(|e| panic!("masked shim load: {}", it.error_report(&e)));
        it.run_str("/kernelimage load type /arraytype eq { } { kernel-shim-not-defined } ifelse")
            .expect("the masked load defines the shim");
    }
    it
}

/// The page's RGBA bytes, or the error's name.
fn render(prog: &str, w: u32, h: u32, shim: bool) -> Result<Vec<u8>, String> {
    let mut it = interp(w, h, shim);
    match it.run_str(prog) {
        Ok(()) => Ok(it.pages()[0].rgba8()),
        Err(e) => Err(e.name().to_string()),
    }
}

/// A program painting `pixel` (defined by `body`) over a `w` x `h` page.
fn program(body: &str, w: u32, h: u32, ncomp: u32) -> String {
    format!(
        "/W {w} def /H {h} def\n{body}\n\
         gsave W H scale W H [W 0 0 H neg 0 H] /pixel load {ncomp} kernelimage grestore showpage"
    )
}

#[track_caller]
fn same(body: &str, ncomp: u32) {
    let (w, h) = (70, 9);
    let prog = program(body, w, h, ncomp);
    let native = render(&prog, w, h, false);
    let shim = render(&prog, w, h, true);
    assert!(native.is_ok(), "native failed: {native:?}\n{prog}");
    assert!(native == shim, "native and shim renders differ: {shim:?}\n{prog}");
}

#[track_caller]
fn same_error(body: &str, ncomp: u32, err: &str) {
    let prog = program(body, 70, 9, ncomp);
    assert_eq!(render(&prog, 70, 9, false), Err(err.to_string()), "native\n{prog}");
    assert_eq!(render(&prog, 70, 9, true), Err(err.to_string()), "shim\n{prog}");
}

#[test]
fn under_pscat_the_native_operators_win() {
    let mut it = Interp::new();
    it.run_str(SHIM).expect("load");
    it.run_str(
        "/kernelimage load type /operatortype eq \
         /kernelcompile load type /operatortype eq \
         userdict /kernelimage known not userdict /kernelcompile known not",
    )
    .expect("probe");
    let got: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(got, ["true"; 4], "operator, operator, not in userdict twice");
}

#[test]
fn the_masked_load_defines_both_and_kernelcompile_says_no() {
    let mut it = interp(4, 4, true);
    it.run_str("{ pop pop 0 } 1 kernelcompile").expect("kernelcompile");
    let st = it.operand_stack();
    assert_eq!(st.len(), 2);
    assert_eq!(st[0].repr(), "false");
    assert!(st[1].repr().contains("lib/kernel.ps"), "{}", st[1].repr());
}

#[test]
fn the_mandelbrot_from_the_guide() {
    same(MANDEL, 3);
}

#[test]
fn gray_clamps_below_and_above() {
    same(
        "/pixel { /j exch def /i exch def i 0.03 mul 0.5 sub j 0.2 mul add } bind def",
        1,
    );
}

#[test]
fn cmyk_and_integer_components() {
    same(
        "/pixel { /j exch def /i exch def i 2 mod j 3 mod 1 sub i j add 4 mod 2 sub i 70 div } bind def",
        4,
    );
}

#[test]
fn rounding_boundaries() {
    // k + 0.5 over 255 lands each sample on a rounding boundary.
    same(
        "/pixel { /j exch def /i exch def i 0.5 add 255 div  i j add 255 div  j 0.5 sub 255 div } bind def",
        3,
    );
}

#[test]
fn infinity_and_nan_components() {
    same(
        "/pixel { /j exch def /i exch def \
           i 3 mod 0 eq { 1e308 10 mul } { i 3 mod 1 eq { 1e308 10 mul neg } { 1e308 10 mul dup sub } ifelse } ifelse \
         } bind def",
        1,
    );
}

#[test]
fn names_resolve_through_the_callers_dicts_not_the_shims() {
    // Every name the shim keeps its state under, defined by the caller:
    // the pixel must see these values, never the shim's.
    let body = "/pixel { /j exch def /i exch def
          N P add M add J add Row add KS add KB add i 1000 div add
          Ready not { 0.5 add } if } bind def
        /Ready false def
        << /N 0.01 /P 0.02 /M 0.03 /J 0.04 /Row 0.05 /KS 0.06 /KB 0.07 >> begin";
    let (w, h) = (70, 9);
    let prog = format!(
        "/W {w} def /H {h} def\n{body}\n\
         gsave W H scale W H [W 0 0 H neg 0 H] /pixel load 1 kernelimage grestore end showpage"
    );
    let native = render(&prog, w, h, false);
    assert!(native.is_ok(), "{native:?}");
    assert!(native == render(&prog, w, h, true), "native and shim differ");
}

#[test]
fn per_pixel_defs_do_not_persist() {
    same(
        "/acc 0.25 def
         /pixel { /j exch def /i exch def
           /acc acc i 0.01 mul add def
           i 2 mod 0 eq { /t 0.5 def } if
           /t where { pop t } { acc } ifelse } bind def",
        1,
    );
}

#[test]
fn wrong_component_counts_and_types_are_errors() {
    same_error("/pixel { pop pop 0.5 0.5 } bind def", 3, "rangecheck");
    same_error("/pixel { pop pop 0.1 0.2 0.3 0.4 } bind def", 3, "rangecheck");
    same_error("/pixel { pop pop (x) } bind def", 1, "typecheck");
    same_error("/pixel { pop pop 0.5 0.5 } bind def", 2, "rangecheck");
}

const MANDEL: &str = "/pixel {
  /j exch def /i exch def
  /cr i W 2 div sub W div 3 mul 0.7 sub def
  /ci H 2 div j sub W div 3 mul def
  /zr 0.0 def /zi 0.0 def /n 0 def
  { zr zr mul zi zi mul add 4 gt n 100 ge or { exit } if
    zr zr mul zi zi mul sub cr add  zr zi mul 2 mul ci add
    /zi exch def /zr exch def /n n 1 add def } loop
  n 100 div dup dup 0.5 mul exch 1 exch sub
} bind def";

const SMOOTH: &str = "/pixel { /j exch def /i exch def
  i W 1 sub div  j H 1 sub div  i j add 5 mul sin 1 add 2 div } bind def";

fn gs_available() -> bool {
    std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// `body`'s 64 x 64 picture with the shim under gs, against pscat's native
/// render: (max per-channel difference, pixels that differ, mean of the
/// per-pixel max difference).
fn gs_against_native(body: &str) -> (u8, usize, f64) {
    let (w, h) = (64u32, 64u32);
    let prog = program(body, w, h, 3);
    let native = render(&prog, w, h, false).expect("native render");
    let dir = common::scratch_dir("pscat-kernel-shim");
    let src = dir.join("shim.ps");
    let png = dir.join("shim.png");
    std::fs::write(&src, format!("{SHIM}\n{prog}\n")).expect("write");
    let out = std::process::Command::new("gs")
        .args(["-q", "-dNOPAUSE", "-dBATCH", "-sDEVICE=png16m", "-r72"])
        .arg(format!("-g{w}x{h}"))
        .arg(format!("-sOutputFile={}", png.display()))
        .arg(&src)
        .output()
        .expect("run gs");
    assert!(
        out.status.success(),
        "gs rejected the shim: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let pix = tiny_skia::Pixmap::decode_png(&std::fs::read(&png).expect("read gs png"))
        .expect("decode gs png");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!((pix.width(), pix.height()), (w, h));
    let gs = pix.data();
    let (mut max, mut n, mut sum) = (0u8, 0usize, 0f64);
    for (a, b) in gs.chunks(4).zip(native.chunks(4)) {
        let d = (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0);
        max = max.max(d);
        n += usize::from(d > 0);
        sum += f64::from(d);
    }
    (max, n, sum / f64::from(w * h))
}

#[test]
fn ghostscript_renders_what_pscat_does() {
    if !gs_available() {
        eprintln!("skipping the gs check: gs not installed");
        return;
    }
    // A smooth picture: only rounding can differ, by at most one level.
    let (max, n, mean) = gs_against_native(SMOOTH);
    eprintln!("smooth: max {max}, {n}/4096 pixels differ, mean {mean:.4}");
    assert!(max <= 1, "smooth: max per-channel difference {max}");
    // The Mandelbrot: gs's 32-bit reals move an escape count by one at a
    // few boundary pixels (one count is 2.55 levels).
    let (max, n, mean) = gs_against_native(MANDEL);
    eprintln!("mandelbrot: max {max}, {n}/4096 pixels differ, mean {mean:.4}");
    assert!(n * 50 <= 4096, "mandelbrot: {n} of 4096 pixels differ");
}
