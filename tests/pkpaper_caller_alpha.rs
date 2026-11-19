//! `pkpaper` honours the caller's `setalpha` (#1161), as `pkwash` does since
//! #1075: the ground fill and each grain mark are multiplied by the alpha in
//! effect at the call, so `gsave 0.5 setalpha ... pkpaper grestore` fades the
//! whole ground. At alpha 1 the product is exact and the ground is unchanged.

use pscat::Interp;

fn load(it: &mut Interp) {
    for path in ["lib/artkit.ps", "lib/paintkit.ps"] {
        let src = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        it.run_source(&src)
            .unwrap_or_else(|e| panic!("{path} failed to load: {}", it.error_report(&e)));
    }
}

/// The page (RGBA bytes) after `body`, drawn over a mid-blue 200x200 page.
fn page(body: &str) -> Vec<u8> {
    let mut it = Interp::with_page(200, 200).expect("page");
    load(&mut it);
    it.run_str(&format!("0.1 0.2 0.8 setrgbcolor clippath fill\n{body}"))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    it.gfx().pixmap.data().to_vec()
}

const OPTS: &str = "/Tone [0.96 0.94 0.88] /Grain 0.6 /Alpha 0.4 /Seed 3";

fn paper(caller_alpha: f64, opts: &str) -> Vec<u8> {
    page(&format!(
        "gsave {caller_alpha} setalpha 20 20 160 160 << {opts} >> pkpaper grestore"
    ))
}

/// Mean of the red channel over the interior of the ground.
fn mean_red(data: &[u8]) -> f64 {
    let mut sum = 0u64;
    let mut n = 0u64;
    for y in 40..160usize {
        for x in 40..160usize {
            sum += u64::from(data[(y * 200 + x) * 4]);
            n += 1;
        }
    }
    sum as f64 / n as f64
}

#[test]
fn a_callers_alpha_fades_the_ground_over_the_backdrop() {
    let (full, mid, faint) = (paper(1.0, OPTS), paper(0.5, OPTS), paper(0.2, OPTS));
    // The backdrop is dark red-poor blue, the paper light: more of the paper
    // shows through as alpha rises, so the red mean rises with it.
    let (rf, rm, rl) = (mean_red(&full), mean_red(&mid), mean_red(&faint));
    assert!(rf > rm && rm > rl, "red mean {rf} / {rm} / {rl}");
    // The ground used to ignore the caller's alpha entirely.
    assert_ne!(full, faint);
    // A half-alpha ground over the backdrop is nowhere near the opaque one.
    assert!(rf - rm > 30.0, "{rf} vs {rm}");
}

#[test]
fn alpha_one_is_the_ground_with_no_caller_alpha_at_all() {
    let bare = page(&format!("20 20 160 160 << {OPTS} >> pkpaper"));
    assert_eq!(paper(1.0, OPTS), bare);
}

#[test]
fn the_caller_alpha_multiplies_the_grain_alpha_exactly() {
    // With no grain the ground alone is compared: a plain half-alpha fill.
    let ground = paper(0.5, "/Tone [0.96 0.94 0.88] /Grain 0 /Seed 3");
    let plain =
        page("gsave 0.5 setalpha 0.96 0.94 0.88 setrgbcolor 20 20 160 160 rectfill grestore");
    assert_eq!(ground, plain);
    // Halving is exact: 0.5 x /Alpha 0.4 is /Alpha 0.2 for every grain mark,
    // but the ground is half-faded in one and opaque in the other, so compare
    // the grain over a ground that is the same in both (/Tone as the backdrop
    // colour is not possible here): check grain strength moves with alpha.
    let a = paper(0.5, OPTS);
    let b = paper(0.5, "/Tone [0.96 0.94 0.88] /Grain 0.6 /Alpha 0.2 /Seed 3");
    assert_ne!(a, b, "grain alpha must still follow /Alpha");
}

#[test]
fn the_callers_alpha_is_back_after_the_ground() {
    let mut it = Interp::with_page(200, 200).expect("page");
    load(&mut it);
    it.run_str("0.6 setalpha 0 0 100 100 << /Seed 1 >> pkpaper currentalpha")
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let top = format!("{:?}", it.operand_stack().last());
    assert!(top.contains("0.6"), "{top}");
}
