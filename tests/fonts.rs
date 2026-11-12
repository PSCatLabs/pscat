//! Stage 6 font tests: dict plumbing, metrics, show/charpath rendering,
//! and the re-encoding idiom. Liberation faces are metric-compatible
//! with the Adobe base fonts, so Courier advances land at 600/1000 em
//! (within TTF rounding) — the metric assertions lean on that.

use pscat::{Interp, PsError, Value};

fn run(src: &str) -> Interp {
    let mut it = Interp::with_page(100, 100).expect("test page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run of {src:?} failed: {e}"));
    it
}

fn pop_f64(it: &mut Interp) -> f64 {
    match it.pop().expect("operand").value {
        Value::Integer(i) => i as f64,
        Value::Real(r) => r,
        ref v => panic!(
            "expected number, got {v:?}",
            v = pscat::Object::lit(v.clone())
        ),
    }
}

fn ink_count(it: &Interp) -> usize {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|p| p.red() < 128)
        .count()
}

// --- dict plumbing ---------------------------------------------------------

#[test]
fn findfont_returns_a_font_dict() {
    let mut it = run("/Helvetica findfont dup /FontName get exch /FontType get");
    assert_eq!(it.pop().expect("FontType").repr(), "42");
    assert_eq!(it.pop().expect("FontName").repr(), "/Helvetica");
}

#[test]
fn findfont_twice_returns_the_same_dict() {
    let mut it = run("/Times-Roman findfont /Times-Roman findfont eq");
    assert_eq!(it.pop().expect("eq result").repr(), "true");
}

#[test]
fn unknown_font_substitutes_instead_of_erroring() {
    let mut it = run("/Frutiger-Weird findfont /FontType get");
    assert_eq!(it.pop().expect("FontType").repr(), "42");
}

#[test]
fn scalefont_composes_the_matrix_and_leaves_the_original() {
    let mut it = run(
        "/Helvetica findfont dup 12 scalefont /FontMatrix get 0 get \
         exch /FontMatrix get 0 get",
    );
    let original = pop_f64(&mut it);
    let scaled = pop_f64(&mut it);
    assert!((original - 0.001).abs() < 1e-12, "original {original}");
    assert!((scaled - 0.012).abs() < 1e-12, "scaled {scaled}");
}

#[test]
fn scaled_font_shares_the_encoding_array() {
    // Shallow copy per the PLRM: same Encoding object, not a duplicate.
    let mut it = run("/Helvetica findfont dup 12 scalefont /Encoding get \
         exch /Encoding get eq");
    assert_eq!(it.pop().expect("eq").repr(), "true");
}

#[test]
fn definefont_installs_into_fontdirectory() {
    let mut it = run(
        "/My /Helvetica findfont dup length dict copy definefont pop \
         FontDirectory /My known \
         /My findfont /FontName get",
    );
    assert_eq!(it.pop().expect("FontName").repr(), "/Helvetica");
    assert_eq!(it.pop().expect("known").repr(), "true");
}

#[test]
fn selectfont_sets_the_scaled_font() {
    let mut it = run("/Courier 24 selectfont currentfont /FontMatrix get 3 get");
    let d = pop_f64(&mut it);
    assert!((d - 0.024).abs() < 1e-12, "matrix d {d}");
}

#[test]
fn gsave_grestore_restores_the_font() {
    let mut it = run("/Courier 12 selectfont \
         gsave /Helvetica 24 selectfont grestore \
         currentfont /FontName get");
    assert_eq!(it.pop().expect("FontName").repr(), "/Courier");
}

#[test]
fn font_errors() {
    let mut it = Interp::with_page(100, 100).expect("test page");
    // show with no font set
    assert_eq!(
        it.run_str("0 0 moveto (hi) show"),
        Err(PsError::InvalidFont)
    );
    // show with a font but no current point
    assert_eq!(
        it.run_str("/Helvetica 12 selectfont newpath (hi) show"),
        Err(PsError::NoCurrentPoint)
    );
    // setfont on a non-font dict
    assert_eq!(it.run_str("1 dict setfont"), Err(PsError::InvalidFont));
    // stringwidth needs no current point — pure metrics.
    assert!(it.run_str("newpath (hi) stringwidth").is_ok());
}

// --- metrics ---------------------------------------------------------------

#[test]
fn courier_advances_are_600_per_em() {
    let mut it = run("/Courier 12 selectfont (abc) stringwidth");
    let wy = pop_f64(&mut it);
    let wx = pop_f64(&mut it);
    assert_eq!(wy, 0.0);
    // 3 chars × 600/1000 × 12pt = 21.6, within TTF unit rounding.
    assert!((wx - 21.6).abs() < 0.05, "courier width {wx}");
}

#[test]
fn monospace_is_monospaced_and_helvetica_is_not() {
    let mut it = run(
        "/Courier 12 selectfont (i) stringwidth pop (m) stringwidth pop \
         /Helvetica 12 selectfont (i) stringwidth pop (m) stringwidth pop",
    );
    let helv_m = pop_f64(&mut it);
    let helv_i = pop_f64(&mut it);
    let cour_m = pop_f64(&mut it);
    let cour_i = pop_f64(&mut it);
    assert!(
        (cour_i - cour_m).abs() < 1e-9,
        "courier {cour_i} vs {cour_m}"
    );
    assert!(helv_i < helv_m, "helvetica {helv_i} vs {helv_m}");
}

#[test]
fn stringwidth_scales_linearly() {
    let mut it = run("/Times-Roman 10 selectfont (Width) stringwidth pop \
         /Times-Roman 20 selectfont (Width) stringwidth pop");
    let w20 = pop_f64(&mut it);
    let w10 = pop_f64(&mut it);
    assert!((w20 - 2.0 * w10).abs() < 1e-9, "{w10} vs {w20}");
}

#[test]
fn widthshow_adds_per_character_advance() {
    let mut it = run("/Helvetica 12 selectfont \
         (aa) stringwidth pop \
         0 0 moveto 5 0 97 (aa) widthshow currentpoint pop \
         exch sub");
    let extra = pop_f64(&mut it);
    assert!((extra - 10.0).abs() < 1e-6, "widthshow extra {extra}");
}

#[test]
fn ashow_adds_uniform_advance() {
    let mut it = run("/Helvetica 12 selectfont \
         (ab) stringwidth pop \
         0 0 moveto 3 0 (ab) ashow currentpoint pop \
         exch sub");
    let extra = pop_f64(&mut it);
    assert!((extra - 6.0).abs() < 1e-6, "ashow extra {extra}");
}

// --- rendering -------------------------------------------------------------

#[test]
fn show_paints_and_advances_the_current_point() {
    let mut it = run("/Helvetica 40 selectfont 10 20 moveto (Hi) show currentpoint");
    let y = pop_f64(&mut it);
    let x = pop_f64(&mut it);
    assert!(ink_count(&it) > 100, "expected glyph ink");
    assert_eq!(y, 20.0);
    assert!(x > 30.0, "current point advanced to {x}");
    // show must not leave glyph outlines behind in the current path:
    // a fill afterwards paints nothing new.
    let before = ink_count(&it);
    it.run_str("fill").expect("fill");
    assert_eq!(ink_count(&it), before);
}

#[test]
fn show_respects_color_and_rotation() {
    let it = run("1 0 0 setrgbcolor /Times-Bold 30 selectfont \
         50 10 translate 45 rotate 0 0 moveto (Ab) show");
    let red = it
        .gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|p| p.red() > 128 && p.green() < 128)
        .count();
    assert!(red > 50, "expected rotated red text, got {red} px");
}

#[test]
fn glyphs_respect_the_clip() {
    // Clip to a box left of the text start: nothing may paint outside.
    let unclipped = ink_count(&run("/Helvetica 40 selectfont 10 20 moveto (HHH) show"));
    let clipped = ink_count(&run(
        "newpath 10 10 moveto 30 10 lineto 30 60 lineto 10 60 lineto closepath clip newpath \
         /Helvetica 40 selectfont 10 20 moveto (HHH) show",
    ));
    assert!(clipped > 0, "some ink survives inside the clip");
    assert!(
        clipped < unclipped / 2,
        "clip must cut most of the text: {clipped} vs {unclipped}"
    );
}

#[test]
fn charpath_builds_geometry() {
    let mut it = run("/Helvetica 40 selectfont 10 20 moveto (Hi) true charpath pathbbox");
    let uy = pop_f64(&mut it);
    let ux = pop_f64(&mut it);
    let ly = pop_f64(&mut it);
    let lx = pop_f64(&mut it);
    assert!((9.0..20.0).contains(&lx), "bbox lx {lx}");
    assert!((19.0..25.0).contains(&ly), "bbox ly {ly}");
    assert!(ux > lx + 20.0, "bbox spans the text: {lx}..{ux}");
    assert!(uy > ly + 20.0, "bbox spans cap height: {ly}..{uy}");
    // Nothing painted yet — charpath only builds the path.
    assert_eq!(ink_count(&it), 0);
    it.run_str("fill").expect("fill charpath outlines");
    assert!(ink_count(&it) > 100, "filled charpath outlines");
}

// --- encodings -------------------------------------------------------------

#[test]
fn systemdict_carries_the_standard_encodings() {
    let mut it = run("StandardEncoding 65 get ISOLatin1Encoding 65 get \
         StandardEncoding 233 get ISOLatin1Encoding 233 get");
    assert_eq!(it.pop().expect("iso 233").repr(), "/eacute");
    assert_eq!(it.pop().expect("std 233").repr(), "/Oslash");
    assert_eq!(it.pop().expect("iso 65").repr(), "/A");
    assert_eq!(it.pop().expect("std 65").repr(), "/A");
}

#[test]
fn reencoding_changes_which_glyph_a_byte_selects() {
    // The PLRM re-encoding idiom: copy the font dict, swap Encoding,
    // definefont. Byte 233 is Oslash in Standard, eacute in ISOLatin1 —
    // different widths in Helvetica, so stringwidth observes the swap.
    let mut it = run("/Helvetica findfont 12 scalefont setfont
         <e9> stringwidth pop
         /Helvetica findfont dup length dict copy
         dup /Encoding ISOLatin1Encoding put
         /Helvetica-Latin1 exch definefont 12 scalefont setfont
         <e9> stringwidth pop");
    let latin1 = pop_f64(&mut it);
    let standard = pop_f64(&mut it);
    assert!(
        (latin1 - standard).abs() > 0.5,
        "eacute vs Oslash widths: {latin1} vs {standard}"
    );
}

#[test]
fn encoding_is_read_live_from_the_font_dict() {
    // Mutating the Encoding array after setfont must affect the next
    // show — the engine reads it live, it isn't cached at setfont.
    let mut it = run("/Helvetica findfont dup length dict copy
         dup /Encoding StandardEncoding dup length array copy put
         /Live exch definefont 12 scalefont setfont
         (x) stringwidth pop
         currentfont /Encoding get 120 /m put
         (x) stringwidth pop");
    let after = pop_f64(&mut it);
    let before = pop_f64(&mut it);
    assert!(
        after > before + 0.5,
        "x remapped to m must widen: {before} -> {after}"
    );
}

// --- definefont copies keep their face (issue #1022) ------------------------

/// The Level 1 re-encoding idiom as programs write it: copy every entry
/// but `FID`, rename, swap in ISOLatin1Encoding, `definefont`.
const REENCODE: &str = "/reenc { % newname basename -> -
       findfont dup length dict begin
         { 1 index /FID ne { def } { pop pop } ifelse } forall
         /Encoding ISOLatin1Encoding def
         /FontName 1 index def
       currentdict end definefont pop
     } def ";

fn render(page: (u32, u32), src: &str) -> Vec<u8> {
    let mut it = Interp::with_page(page.0, page.1).expect("test page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run of {src:?} failed: {e}"));
    it.gfx().pixmap.data().to_vec()
}

#[test]
fn a_forall_copy_measures_like_its_source_face() {
    // Ghostscript: Times-Roman (Hamburg) at 100 pt is 377.69, Helvetica
    // 411.28. The copy used to measure as Helvetica.
    let mut it = run(&format!(
        "{REENCODE}
         /Times-Roman-ISO /Times-Roman reenc
         /Copy /Times-Roman findfont definefont pop
         /Bookman-ISO /Bookman-Light reenc
         /Times-Roman findfont 100 scalefont setfont (Hamburg) stringwidth pop
         /Times-Roman-ISO findfont 100 scalefont setfont (Hamburg) stringwidth pop
         /Copy findfont 100 scalefont setfont (Hamburg) stringwidth pop
         /Helvetica findfont 100 scalefont setfont (Hamburg) stringwidth pop
         /Bookman-Light findfont 100 scalefont setfont (Hamburg) stringwidth pop
         /Bookman-ISO findfont 100 scalefont setfont (Hamburg) stringwidth pop"
    ));
    let bookman_iso = pop_f64(&mut it);
    let bookman = pop_f64(&mut it);
    let helvetica = pop_f64(&mut it);
    let copy = pop_f64(&mut it);
    let iso = pop_f64(&mut it);
    let times = pop_f64(&mut it);
    assert!((times - 377.69).abs() < 0.05, "Times {times}");
    assert!((helvetica - 411.28).abs() < 0.05, "Helvetica {helvetica}");
    assert_eq!(iso, times, "re-encoded copy");
    assert_eq!(copy, times, "plain definefont of the original");
    assert_eq!(bookman_iso, bookman, "an embedded base-35 face too");
    assert!(
        (bookman - helvetica).abs() > 1.0,
        "Bookman is not Helvetica"
    );
}

#[test]
fn a_forall_copy_renders_like_its_source_face() {
    let draw = |font: &str| {
        render(
            (300, 80),
            &format!(
                "{REENCODE} /Times-Roman-ISO /Times-Roman reenc
                 /{font} findfont 48 scalefont setfont 10 20 moveto (Hamburg) show"
            ),
        )
    };
    let original = draw("Times-Roman");
    assert_eq!(draw("Times-Roman-ISO"), original, "copy draws Times");
    assert_ne!(draw("Helvetica"), original, "the probe tells faces apart");
}

#[test]
fn an_isolatin1_reencoded_copy_draws_egrave() {
    // `(Fr\350re)` through ISOLatin1 must be exactly Fr, è, re in Times:
    // the same pixels as drawing è by name with glyphshow (#1023), and
    // not the unaccented word.
    let page = (300, 80);
    let reencoded = render(
        page,
        &format!(
            "{REENCODE} /Times-Roman-ISO /Times-Roman reenc
             /Times-Roman-ISO findfont 48 scalefont setfont 10 20 moveto (Fr\\350re) show"
        ),
    );
    let by_name = render(
        page,
        "/Times-Roman findfont 48 scalefont setfont 10 20 moveto
         (Fr) show /egrave glyphshow (re) show",
    );
    let plain = render(
        page,
        "/Times-Roman findfont 48 scalefont setfont 10 20 moveto (Frere) show",
    );
    assert_eq!(reencoded, by_name, "re-encoded byte 232 is Times' egrave");
    assert_ne!(reencoded, plain, "the accent is drawn");

    // Ghostscript: 210.988 at 100 pt.
    let mut it = run(&format!(
        "{REENCODE} /Times-Roman-ISO /Times-Roman reenc
         /Times-Roman-ISO findfont 100 scalefont setfont (Fr\\350re) stringwidth pop"
    ));
    let w = pop_f64(&mut it);
    assert!((w - 210.988).abs() < 0.05, "Fr\\350re width {w}");
}

#[test]
fn a_bad_face_entry_falls_back_to_the_substitute() {
    // The face entry is program-writable: anything that does not name a
    // loaded face gets the substitute (Helvetica), never an error.
    for bad in ["999999", "-5", "(Times)", "/Times-Roman", "3.5"] {
        let mut it = run(&format!(
            "/Helvetica findfont 100 scalefont setfont (Hamburg) stringwidth pop
             /Times-Roman findfont dup length dict begin
               {{ 1 index /FID ne {{ def }} {{ pop pop }} ifelse }} forall
               /PSCatFace {bad} def
             currentdict end /Bad exch definefont 100 scalefont setfont
             (Hamburg) stringwidth pop"
        ));
        let bad_w = pop_f64(&mut it);
        let helvetica = pop_f64(&mut it);
        assert_eq!(bad_w, helvetica, "PSCatFace {bad}");
    }
}

// --- glyphshow (issue #1023) -------------------------------------------------

#[test]
fn glyphshow_paints_a_glyph_the_encoding_lacks() {
    // StandardEncoding has no eacute; glyphshow reaches it by name and
    // advances by its width (Ghostscript: 8.88 at 20 pt).
    let mut it = run("/Times-Roman findfont 20 scalefont setfont
         10 10 moveto /eacute glyphshow currentpoint pop");
    let x = pop_f64(&mut it);
    assert!((x - 18.88).abs() < 0.01, "advance {x}");
    assert!(ink_count(&it) > 10, "eacute painted");
}

#[test]
fn glyphshow_errors() {
    let mut it = Interp::with_page(100, 100).expect("test page");
    assert_eq!(
        it.run_str("0 0 moveto /a glyphshow"),
        Err(PsError::InvalidFont),
        "no font"
    );
    assert_eq!(
        it.run_str("/Helvetica 12 selectfont newpath /a glyphshow"),
        Err(PsError::NoCurrentPoint)
    );
    // A name only, as in Ghostscript: a string or an integer is a typecheck.
    assert_eq!(
        it.run_str("0 0 moveto (a) glyphshow"),
        Err(PsError::Typecheck)
    );
    assert_eq!(
        it.run_str("0 0 moveto 97 glyphshow"),
        Err(PsError::Typecheck)
    );
    // An unknown glyph advances by .notdef's width without an error.
    assert!(it.run_str("0 0 moveto /no-such-glyph glyphshow").is_ok());
}

#[test]
fn glyphshow_on_type3_passes_the_name_or_a_code() {
    // BuildGlyph gets the name, even one the Encoding lacks; a
    // BuildChar-only font gets the first Encoding slot holding the name,
    // else 0 (Ghostscript's behaviour).
    let mut it = run("/T3G << /FontType 3 /FontMatrix [0.001 0 0 0.001 0 0]
             /FontBBox [0 0 1000 1000] /Encoding StandardEncoding
             /BuildGlyph { exch pop /seen exch def 600 0 setcharwidth }
             /BuildChar { pop pop 500 0 setcharwidth } >> definefont 10 scalefont setfont
         0 0 moveto /zzz glyphshow seen currentpoint pop
         /T3C << /FontType 3 /FontMatrix [0.001 0 0 0.001 0 0]
             /FontBBox [0 0 1000 1000] /Encoding StandardEncoding
             /BuildChar { exch pop /code exch def 500 0 setcharwidth } >> definefont
           10 scalefont setfont
         0 0 moveto /A glyphshow code /zzz glyphshow code");
    assert_eq!(pop_f64(&mut it), 0.0, "unencoded name -> code 0");
    assert_eq!(pop_f64(&mut it), 65.0, "/A -> 65");
    assert_eq!(pop_f64(&mut it), 6.0, "BuildGlyph width");
    assert_eq!(it.pop().expect("name").repr(), "/zzz");
}
