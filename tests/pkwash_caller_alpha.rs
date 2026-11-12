//! `pkwash` honours the caller's `setalpha` (#1075): each layer's alpha is
//! `/Alpha` times the alpha in effect at the call, so wrapping a wash in
//! `gsave 0.6 setalpha ... grestore` fades it. At alpha 1 the product is
//! exact and the wash is what it always was.

use pscat::Interp;

fn load(it: &mut Interp) {
    for path in ["lib/artkit.ps", "lib/paintkit.ps"] {
        let src = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        it.run_source(&src)
            .unwrap_or_else(|e| panic!("{path} failed to load: {}", it.error_report(&e)));
    }
}

/// The page (RGBA bytes) after `body`, drawn on a white 200x200 page.
fn page(body: &str) -> Vec<u8> {
    let mut it = Interp::with_page(200, 200).expect("page");
    load(&mut it);
    it.run_str(&format!("1 1 1 setrgbcolor clippath fill\n{body}"))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    it.gfx().pixmap.data().to_vec()
}

const BLOB: &str = "0.2 0.4 0.8 setrgbcolor newpath 50 50 moveto 150 50 lineto \
     150 150 lineto 50 150 lineto closepath";

/// Total ink (distance from white) over the page.
fn ink(data: &[u8]) -> u64 {
    data.chunks(4)
        .map(|p| 3 * 255 - u64::from(p[0]) - u64::from(p[1]) - u64::from(p[2]))
        .sum()
}

fn wash(caller_alpha: f64, opts: &str) -> Vec<u8> {
    page(&format!(
        "gsave {caller_alpha} setalpha {BLOB} << {opts} >> pkwash grestore"
    ))
}

#[test]
fn a_callers_alpha_fades_the_wash() {
    let opts = "/Alpha 0.8 /Layers 3 /Grain 0.4 /Seed 7";
    let (full, mid, faint) = (wash(1.0, opts), wash(0.6, opts), wash(0.2, opts));
    assert!(
        ink(&full) > ink(&mid) && ink(&mid) > ink(&faint) && ink(&faint) > 0,
        "ink {} / {} / {}",
        ink(&full),
        ink(&mid),
        ink(&faint)
    );
    // The issue's repro: these used to be identical.
    assert_ne!(full, faint);
}

#[test]
fn alpha_one_is_the_wash_with_no_caller_alpha_at_all() {
    let opts = "/Alpha 0.8 /Layers 3 /Grain 0.4 /Seed 7";
    let bare = page(&format!("{BLOB} << {opts} >> pkwash"));
    assert_eq!(wash(1.0, opts), bare);
}

#[test]
fn the_caller_alpha_multiplies_the_wash_alpha_exactly() {
    // Halving is exact in floating point: 0.5 x /Alpha 0.8 is /Alpha 0.4
    // for every layer, the rim and the grain, so the pixels are equal.
    let a = wash(0.5, "/Alpha 0.8 /Layers 3 /Grain 0.4 /Seed 7");
    let b = wash(1.0, "/Alpha 0.4 /Layers 3 /Grain 0.4 /Seed 7");
    assert_eq!(a, b);
}

#[test]
fn the_callers_alpha_is_back_after_the_wash() {
    let mut it = Interp::with_page(200, 200).expect("page");
    load(&mut it);
    it.run_str(&format!(
        "0.6 setalpha {BLOB} << /Seed 1 >> pkwash currentalpha"
    ))
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let top = format!("{:?}", it.operand_stack().last());
    assert!(top.contains("0.6"), "{top}");
}
