//! The pigment palette and `pigmix` (issue #185, `lib/paintkit.ps`,
//! "Pigments"): a Kubelka-Munk mix run per colour channel over a named
//! oil palette, so blue and yellow make green, blue and crimson violet,
//! and white tints instead of only lightening.
//!
//! The guards live in `%%SelfTest` blocks next to the code; this file
//! holds what needs the whole interpreter: reference values pinned
//! against the model, the qualitative claims the issue makes (green,
//! violet, tints), the determinism and cost the issue asks for, the
//! hand-off to the presets' own colour keys, the catalog entries, and a
//! check that Ghostscript computes the same numbers.

mod common;

use common::within_cpu_budget;
use pscat::{Interp, capabilities};
use std::time::Duration;

fn loaded() -> Interp {
    let mut it = Interp::with_page(200, 200).expect("page");
    for path in ["lib/artkit.ps", "lib/paintkit.ps"] {
        let src = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        it.run_source(&src)
            .unwrap_or_else(|e| panic!("{path} failed to load: {}", it.error_report(&e)));
    }
    it
}

/// Runs `src` (which leaves `r g b` on the stack) and returns the three.
fn rgb_of(it: &mut Interp, src: &str) -> [f64; 3] {
    it.run_str(src)
        .unwrap_or_else(|e| panic!("{src}: {}", it.error_report(&e)));
    let b = it.pop_f64().expect("b");
    let g = it.pop_f64().expect("g");
    let r = it.pop_f64().expect("r");
    [r, g, b]
}

fn near(got: [f64; 3], want: [f64; 3], tol: f64, what: &str) {
    for c in 0..3 {
        assert!(
            (got[c] - want[c]).abs() <= tol,
            "{what}: channel {c} is {:.4}, wanted {:.4} (got {got:?})",
            got[c],
            want[c]
        );
    }
}

/// HSV saturation, for "tints lose saturation".
fn saturation(c: [f64; 3]) -> f64 {
    let hi = c[0].max(c[1]).max(c[2]);
    let lo = c[0].min(c[1]).min(c[2]);
    if hi == 0.0 { 0.0 } else { (hi - lo) / hi }
}

const TUBE: [(&str, [f64; 3]); 13] = [
    ("titaniumwhite", [0.95, 0.96, 0.97]),
    ("midnightblack", [0.04, 0.04, 0.05]),
    ("phthaloblue", [0.09, 0.24, 0.55]),
    ("prussianblue", [0.05, 0.17, 0.32]),
    ("alizarin", [0.55, 0.05, 0.24]),
    ("sapgreen", [0.16, 0.38, 0.08]),
    ("cadmiumyellow", [0.99, 0.87, 0.04]),
    ("indianyellow", [0.94, 0.63, 0.07]),
    ("yellowochre", [0.78, 0.56, 0.20]),
    ("vandykebrown", [0.24, 0.14, 0.09]),
    ("darksienna", [0.46, 0.22, 0.10]),
    ("burntumber", [0.32, 0.17, 0.10]),
    ("brightred", [0.82, 0.10, 0.07]),
];

#[test]
fn every_pigment_comes_back_as_stored() {
    let mut it = loaded();
    for (name, want) in TUBE {
        near(
            rgb_of(&mut it, &format!("/{name} pigment")),
            want,
            0.0,
            name,
        );
    }
}

#[test]
fn the_palette_lists_the_thirteen_pigments_in_tube_order() {
    let mut it = loaded();
    it.run_str("Pigments length PigmentNames length")
        .expect("run");
    assert_eq!(it.pop_f64().expect("names"), 13.0, "PigmentNames length");
    assert_eq!(it.pop_f64().expect("dict"), 13.0, "Pigments length");
    for (i, (name, _)) in TUBE.iter().enumerate() {
        it.run_str(&format!(
            "PigmentNames {i} get /{name} eq  Pigments /{name} known and"
        ))
        .expect("run");
        assert_eq!(it.pop().expect("bool").repr(), "true", "{name} at {i}");
    }
}

/// The mixes the issue names, pinned against the model's output. The
/// tolerance is tight on purpose: a change to a pigment's colour or
/// strength moves these, and should be a deliberate edit of this table.
#[test]
fn reference_mixes_are_pinned() {
    let mut it = loaded();
    let cases: [(&str, [f64; 3]); 6] = [
        (
            "[ /phthaloblue 1 /cadmiumyellow 1 ] pigmix",
            [0.1551, 0.3492, 0.0734],
        ),
        (
            "[ /phthaloblue 1 /alizarin 1 ] pigmix",
            [0.1570, 0.0793, 0.3197],
        ),
        (
            "[ /sapgreen 1 /cadmiumyellow 1 ] pigmix",
            [0.2767, 0.5194, 0.0509],
        ),
        (
            "[ /phthaloblue 1 /alizarin 1 /titaniumwhite 6 ] pigmix",
            [0.6372, 0.5056, 0.7738],
        ),
        ("/phthaloblue 0.25 pigtint", [0.2271, 0.4458, 0.7233]),
        (
            "[ /vandykebrown 2 /prussianblue 1 /midnightblack 1 ] pigmix",
            [0.0691, 0.0908, 0.0941],
        ),
    ];
    for (src, want) in cases {
        let got = rgb_of(&mut it, src);
        // Recorded to 4 decimals: this pins the model, not a tolerance on it.
        near(got, want, 0.0006, src);
    }
}

#[test]
fn blue_and_yellow_make_green_where_a_lerp_makes_olive() {
    let mut it = loaded();
    let paint = rgb_of(&mut it, "[ /phthaloblue 1 /cadmiumyellow 1 ] pigmix");
    assert!(
        paint[1] > paint[0] + 0.15 && paint[1] > paint[2] + 0.15,
        "green: {paint:?}"
    );
    // The same parts by RGB lerp: the green channel does not stand out.
    let lerp = rgb_of(
        &mut it,
        "/phthaloblue pigment 3 array astore /cadmiumyellow pigment 3 array astore 0.5 mix3",
    );
    assert!(
        (paint[1] - paint[0].max(paint[2])) > (lerp[1] - lerp[0].max(lerp[2])) + 0.1,
        "paint {paint:?} is greener than the lerp {lerp:?}"
    );
}

#[test]
fn blue_and_crimson_make_violet() {
    let mut it = loaded();
    let c = rgb_of(&mut it, "[ /phthaloblue 1 /alizarin 1 ] pigmix");
    assert!(c[2] > c[1] + 0.1, "blue over green: {c:?}");
    assert!(c[0] > c[1] + 0.05, "red over green: {c:?}");
    assert!(c[2] > c[0], "more blue than red: {c:?}");
    // With white, the lavender of distant mountains: red at least green.
    let lav = rgb_of(
        &mut it,
        "[ /phthaloblue 1 /alizarin 1 /titaniumwhite 6 ] pigmix",
    );
    assert!(lav[2] > lav[0] && lav[0] >= lav[1], "lavender: {lav:?}");
}

#[test]
fn white_tints_instead_of_only_lightening() {
    let mut it = loaded();
    for (name, pure) in TUBE
        .iter()
        .filter(|(n, _)| !matches!(*n, "titaniumwhite" | "midnightblack"))
    {
        let mut last_lum = pure.iter().sum::<f64>();
        for t in [0.05, 0.1, 0.2, 0.4, 0.6] {
            let c = rgb_of(&mut it, &format!("/{name} {t} pigtint"));
            let lum: f64 = c.iter().sum();
            assert!(lum > last_lum, "{name} lightens monotonically at t={t}");
            last_lum = lum;
            // Pale, not just light: a real tint has lost saturation.
            if t >= 0.2 {
                assert!(
                    saturation(c) < saturation(*pure) + 1e-9,
                    "{name} does not gain saturation at t={t}"
                );
            }
        }
    }
}

#[test]
fn dark_mixes_stay_dark_and_a_little_black_goes_a_long_way() {
    let mut it = loaded();
    let dark = rgb_of(
        &mut it,
        "[ /vandykebrown 2 /prussianblue 1 /midnightblack 1 ] pigmix",
    );
    assert!(
        dark.iter().all(|&c| c < 0.15),
        "the darkest foreground: {dark:?}"
    );
    let black = rgb_of(&mut it, "/sapgreen 0.1 pigshade");
    let green = rgb_of(&mut it, "/sapgreen pigment");
    assert!(
        black[1] < green[1] * 0.75,
        "10% black deepens the green: {black:?}"
    );
}

#[test]
fn mixes_are_deterministic_scale_free_and_order_free() {
    let mut it = loaded();
    let a = rgb_of(&mut it, "[ /sapgreen 3 /cadmiumyellow 1 ] pigmix");
    near(
        rgb_of(&mut it, "[ /cadmiumyellow 10 /sapgreen 30 ] pigmix"),
        a,
        1e-9,
        "scale and order",
    );
    near(
        rgb_of(&mut it, "[ /sapgreen 3 /cadmiumyellow 1 ] pigmix"),
        a,
        0.0,
        "repeat",
    );
    // A weight is parts: more yellow moves the mix toward yellow.
    let more = rgb_of(&mut it, "[ /sapgreen 3 /cadmiumyellow 2 ] pigmix");
    assert!(
        more[0] > a[0] && more[2] < a[2] + 0.05,
        "more yellow: {more:?} vs {a:?}"
    );
}

#[test]
fn every_pair_of_pigments_mixes_to_a_valid_colour() {
    let mut it = loaded();
    for (i, (a, _)) in TUBE.iter().enumerate() {
        for (b, _) in TUBE.iter().skip(i) {
            for w in ["1 1", "1 9", "9 1"] {
                let ws: Vec<&str> = w.split(' ').collect();
                let c = rgb_of(
                    &mut it,
                    &format!("[ /{a} {} /{b} {} ] pigmix", ws[0], ws[1]),
                );
                assert!(
                    c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
                    "{a}+{b} {w}: {c:?}"
                );
            }
        }
    }
}

#[test]
fn pigmix_takes_no_random_draws() {
    let mut it = loaded();
    it.run_str("7 srand [ /phthaloblue 1 /alizarin 2 ] pigmix pop pop pop /alizarin 0.3 pigtint pop pop pop rand")
        .expect("run");
    let with = it.pop_f64().expect("rand");
    it.run_str("7 srand rand").expect("run");
    assert_eq!(
        with,
        it.pop_f64().expect("rand"),
        "the generator is untouched"
    );
}

#[test]
fn pigmix_is_cheap() {
    // Load once, outside the timed closure: the budget is for the mixing, not
    // for parsing the library.
    let mut it = loaded();
    within_cpu_budget(
        "20000 three-pigment mixes",
        Duration::from_secs(1),
        3,
        || {
            it.run_str(
            "20000 { [ /phthaloblue 1 /alizarin 1 /titaniumwhite 4 ] pigmix pop pop pop } repeat",
        )
        .expect("run");
        },
    );
}

/// The pigments reach a preset through the key it already has, and give
/// the same pixels as writing the rgb out by hand.
#[test]
fn pigcolors_feed_a_loaded_brush_exactly_like_the_rgb_they_stand_for() {
    let render = |colors: &str| -> Vec<u8> {
        let mut it = loaded();
        it.run_str(&format!(
            "1 1 1 setrgbcolor clippath fill 5 srand newpath 20 100 moveto 180 100 lineto \
             << /Width 40 /Bristles 30 /Colors {colors} >> pkfan"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        it.gfx().pixmap.data().to_vec()
    };
    let by_name = render("[ /sapgreen [ /sapgreen 1 /cadmiumyellow 1 ] ] pigcolors");
    let mut it = loaded();
    let green = rgb_of(&mut it, "/sapgreen pigment");
    let mix = rgb_of(&mut it, "[ /sapgreen 1 /cadmiumyellow 1 ] pigmix");
    let by_hand = render(&format!(
        "[ [ {} {} {} ] [ {} {} {} ] ]",
        green[0], green[1], green[2], mix[0], mix[1], mix[2]
    ));
    assert_eq!(by_name, by_hand, "same seed, same pixels");
    let plain = render("[ [ 0 0 0 ] [ 1 0 0 ] ]");
    assert_ne!(by_name, plain, "and it is not the control's pixels");
}

#[test]
fn pigcolor_feeds_the_highlight_and_shadow_keys() {
    let mut it = loaded();
    it.run_str(
        "newpath 10 10 moveto 100 10 lineto \
         << /HighlightColor /cadmiumyellow pigcolor /ShadowColor /prussianblue pigcolor >> pkoil \
         newpath 10 60 moveto 100 60 lineto \
         << /Size 6 /Highlight 1 /Shadow 1 \
            /HighlightColor [ /cadmiumyellow 2 /sapgreen 1 ] pigcolor >> pkdab",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
}

/// Bare names in the colour keys (#356) paint the same pixels as the
/// `pigcolor`/`pigcolors` forms they stand for.
#[test]
fn bare_names_in_colors_paint_the_same_pixels_as_pigcolors() {
    let render = |colors: &str| -> Vec<u8> {
        let mut it = loaded();
        it.run_str(&format!(
            "1 1 1 setrgbcolor clippath fill 5 srand newpath 20 100 moveto 180 100 lineto \
             << /Width 40 /Bristles 30 /Colors {colors} >> pkfan"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        it.gfx().pixmap.data().to_vec()
    };
    let bare = render("[ /sapgreen /cadmiumyellow ]");
    assert_eq!(bare, render("[ /sapgreen /cadmiumyellow ] pigcolors"));
    assert_ne!(
        bare,
        render("[ [ 0 0 0 ] [ 1 0 0 ] ]"),
        "and not the control"
    );
    // Names and rgb arrays mix in one list.
    assert_eq!(
        render("[ /sapgreen [ 0.2 0.4 1 ] ]"),
        render("[ /sapgreen pigcolor [ 0.2 0.4 1 ] ]")
    );
}

#[test]
fn bare_names_in_the_highlight_and_shadow_keys_match_pigcolor() {
    let render = |keys: &str, preset: &str| -> Vec<u8> {
        let mut it = loaded();
        it.run_str(&format!(
            "1 1 1 setrgbcolor clippath fill 0.4 0.3 0.2 setrgbcolor 5 srand newpath \
             20 60 moveto 180 60 lineto << /Size 8 /Highlight 1 /Shadow 1 {keys} >> {preset}"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        it.gfx().pixmap.data().to_vec()
    };
    for preset in ["pkoil", "pkdab"] {
        let bare = render(
            "/HighlightColor /cadmiumyellow /ShadowColor /prussianblue",
            preset,
        );
        let wrapped = render(
            "/HighlightColor /cadmiumyellow pigcolor /ShadowColor /prussianblue pigcolor",
            preset,
        );
        assert_eq!(bare, wrapped, "{preset}: a bare name is its pigcolor");
        let plain = render("/HighlightColor [ 0 0 0 ] /ShadowColor [ 1 1 1 ]", preset);
        assert_ne!(bare, plain, "{preset}: and not the control");
    }
}

#[test]
fn the_colour_keys_name_the_fix_for_an_unknown_name_or_a_spec() {
    let guard = |opts: &str, preset: &str| -> String {
        let mut it = loaded();
        let e = it
            .run_str(&format!("newpath 0 0 moveto 50 0 lineto {opts} {preset}"))
            .expect_err("must refuse");
        it.error_report(&e)
    };
    let r = guard("<< /Colors [ /nothue ] >>", "pkfan");
    assert!(
        r.contains("pkfan-colors-unknown-pigment-see-PigmentNames"),
        "{r}"
    );
    let r = guard(
        "<< /Colors [ [ /sapgreen 1 /cadmiumyellow 1 ] ] >>",
        "pktrowel",
    );
    assert!(
        r.contains("pktrowel-colors-pigmix-spec-needs-pigcolors"),
        "{r}"
    );
    let r = guard("<< /HighlightColor /nothue >>", "pkoil");
    assert!(
        r.contains("pkoil-highlightcolor-unknown-pigment-see-PigmentNames"),
        "{r}"
    );
    let r = guard("<< /ShadowColor [ /alizarin 1 ] >>", "pkdab");
    assert!(
        r.contains("pkdab-shadowcolor-pigmix-spec-needs-pigcolors"),
        "{r}"
    );
    // A name that is not a pigment but is an artkit palette: still an unknown pigment.
    let r = guard("<< /Colors [ /dusk ] >>", "pkbroad");
    assert!(
        r.contains("pkbroad-colors-unknown-pigment-see-PigmentNames"),
        "{r}"
    );
}

#[test]
fn the_catalog_lists_the_pigment_entries() {
    let caps = capabilities::catalog();
    for name in [
        "pigmix",
        "pigment",
        "pigtint",
        "pigshade",
        "pigcolor",
        "pigcolors",
        "Pigments",
        "PigmentNames",
    ] {
        let cap = caps
            .iter()
            .find(|c| c.source == "lib/paintkit.ps" && c.name == name)
            .unwrap_or_else(|| panic!("{name} in the catalog"));
        assert_eq!(cap.load, "(lib/artkit.ps) run (lib/paintkit.ps) run");
        assert!(
            !cap.description.is_empty() && !cap.example.is_empty(),
            "{name}"
        );
    }
}

fn gs_available() -> bool {
    std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Ghostscript computes the same mixes, to its 32-bit real precision.
/// Runs locally only: CI has no gs.
#[test]
fn ghostscript_agrees_on_the_numbers() {
    if !gs_available() {
        eprintln!("skipping gs pigment check: gs not installed");
        return;
    }
    let specs = [
        "[ /phthaloblue 1 /cadmiumyellow 1 ] pigmix",
        "[ /phthaloblue 1 /alizarin 1 /titaniumwhite 6 ] pigmix",
        "/phthaloblue 0.25 pigtint",
        "/sapgreen 0.1 pigshade",
        "[ [ 0.8 0.2 0.4 ] 3 /yellowochre 1 ] pigmix",
    ];
    let mut src = String::new();
    for p in ["lib/artkit.ps", "lib/paintkit.ps"] {
        src.push_str(&std::fs::read_to_string(p).expect("read lib"));
        src.push('\n');
    }
    for s in specs {
        src.push_str(&format!("{s} 3 array astore == flush\n"));
    }
    src.push_str("quit\n");
    let dir = common::scratch_dir("pscat-pigment-gs");
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join("pigment.ps");
    std::fs::write(&ps, src).unwrap();
    let out = std::process::Command::new("gs")
        .args(["-dNODISPLAY", "-dNOPAUSE", "-dBATCH", "-q"])
        .arg(&ps)
        .output()
        .expect("run gs");
    let _ = std::fs::remove_dir_all(&dir);
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let lines: Vec<&str> = text.lines().filter(|l| l.starts_with('[')).collect();
    assert_eq!(lines.len(), specs.len(), "gs printed {text}");
    let mut it = loaded();
    for (line, spec) in lines.iter().zip(specs) {
        let gs: Vec<f64> = line
            .trim_matches(|c| c == '[' || c == ']')
            .split_whitespace()
            .map(|v| v.parse().expect("number"))
            .collect();
        let ours = rgb_of(&mut it, spec);
        near([gs[0], gs[1], gs[2]], ours, 1e-5, spec);
    }
}

/// The specimen page runs under Ghostscript too (it uses no alpha).
#[test]
fn ghostscript_accepts_the_pigment_demo() {
    if !gs_available() {
        eprintln!("skipping gs compatibility check: gs not installed");
        return;
    }
    let out = std::process::Command::new("gs")
        .args([
            "-dNOSAFER",
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=png16m",
            "-g780x980",
            "-r72",
            "-o/dev/null",
            "examples/paintkit_pigment_demo.ps",
        ])
        .output()
        .expect("run gs");
    assert!(
        out.status.success() && !String::from_utf8_lossy(&out.stdout).contains("Error"),
        "gs rejected examples/paintkit_pigment_demo.ps: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}
