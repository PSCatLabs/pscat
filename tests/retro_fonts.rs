//! The retro screen and printer faces (lib/fonts/, #1426): /Phosphor,
//! /Pinpoint-Draft and -NLQ, /Diode7 and /Diode5. They are bitmap Type 3
//! fonts drawn in the caller's colour, so unlike the display library
//! (tests/fontlib.rs) they can be checked pixel by pixel against their own
//! `/Bitmaps` at true size.

use pscat::Interp;

mod common;
use common::CommandDeadline as _;

/// (file, every face it defines)
const FILES: [(&str, &[&str]); 4] = [
    ("lib/fonts/phosphor.ps", &["Phosphor", "Phosphor-Scan"]),
    ("lib/fonts/pinpoint.ps", &["Pinpoint-Draft", "Pinpoint-NLQ"]),
    ("lib/fonts/diode7.ps", &["Diode7"]),
    ("lib/fonts/diode5.ps", &["Diode5"]),
];

fn loaded(path: &str, w: u32, h: u32) -> Interp {
    let src = std::fs::read(path).expect("font file present");
    let mut it = Interp::with_page(w, h).expect("page");
    it.run_source(&src)
        .unwrap_or_else(|e| panic!("{path} failed to load: {}", it.error_report(&e)));
    it
}

fn run(it: &mut Interp, ps: &str) {
    it.run_str(ps)
        .unwrap_or_else(|e| panic!("{ps}: {}", it.error_report(&e)));
}

fn top(it: &Interp) -> String {
    it.operand_stack()
        .last()
        .map(|o| o.repr())
        .unwrap_or_default()
}

/// Pixels whose red channel is above `min` (the faces draw in pure red here
/// on a black ground).
fn lit(it: &Interp, min: u8) -> usize {
    it.gfx()
        .pixmap
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] > min)
        .count()
}

#[test]
fn every_printable_ascii_code_has_a_drawn_glyph() {
    for (path, faces) in FILES {
        for face in faces {
            let mut it = loaded(path, 50, 50);
            // Codes 33..126 must each name a glyph in /Bitmaps with at
            // least one lit pixel; space must exist and be blank.
            run(
                &mut it,
                &format!(
                    "/F /{face} findfont def /B F /Bitmaps get def
                     /bad [ 33 1 126 {{
                       /n F /Encoding get 2 index get def
                       B n known {{
                         false B n get {{ (#) search {{ pop pop pop true or }} {{ pop }} ifelse }} forall
                         {{ pop }} if
                       }} if
                     }} for ] def
                     F /Encoding get 32 get /space eq bad length 0 eq and"
                ),
            );
            assert_eq!(top(&it), "true", "{face}: a code with no drawn glyph");
        }
    }
}

#[test]
fn every_bitmap_fits_its_cell() {
    // Monospaced faces: no row wider than the cell, no glyph taller than
    // the cell. Proportional faces: every row of a glyph the same width
    // (the width is the advance), and no taller than the cell.
    for (path, faces) in FILES {
        let mut it = loaded(path, 10, 10);
        run(
            &mut it,
            &format!(
                "/F /{} findfont def
                 /rows F /FontBBox get dup 3 get exch 1 get sub def
                 /bad [ F /Bitmaps get {{
                   /r exch def /n exch def
                   r length rows gt {{ n }} if
                   F /Proportional get {{
                     r {{ length r 0 get length ne {{ n exit }} if }} forall
                   }} {{
                     r {{ length F /Cell get gt {{ n exit }} if }} forall
                   }} ifelse
                 }} forall ] def
                 bad length 0 eq",
                faces[0]
            ),
        );
        assert_eq!(top(&it), "true", "{path}: a bitmap outside its cell");
    }
}

/// At one font pixel per device pixel, on whole-pixel coordinates, square
/// marks must reproduce the bitmap exactly: no blur, no seams, nothing
/// outside it. This is the raw-grid case (an LED strip, a frame at true size).
#[test]
fn true_size_square_marks_match_the_bitmaps_pixel_for_pixel() {
    // (file, face, scalefont for one unit per pixel, baseline y, a square
    // variant's extra dials, the glyphs to probe)
    let cases = [
        ("lib/fonts/phosphor.ps", "Phosphor", 16, 4, "", "AgQ@0~_"),
        (
            "lib/fonts/pinpoint.ps",
            "Pinpoint-Draft",
            9,
            2,
            "/DotShape /square /DotSize 1",
            "AgQ@0~_",
        ),
        (
            "lib/fonts/diode7.ps",
            "Diode7",
            8,
            1,
            "/DotShape /square /DotSize 1",
            "AgQ@0~_",
        ),
        (
            "lib/fonts/diode5.ps",
            "Diode5",
            5,
            0,
            "/DotShape /square /DotSize 1",
            "AQ@0~W",
        ),
    ];
    for (path, face, size, base, dials, glyphs) in cases {
        for ch in glyphs.chars() {
            let (w, h) = (12u32, 16u32);
            let mut it = loaded(path, w, h);
            run(
                &mut it,
                &format!(
                    "/T /{face} << {dials} >> pf-variant
                     0 setgray clippath fill 1 0 0 setrgbcolor
                     /T findfont {size} scalefont setfont 0 {base} moveto ({ch}) show
                     /{face} findfont /Encoding get {code} get
                     /{face} findfont /Bitmaps get exch get
                     /{face} findfont /Ascent get",
                    code = ch as u32
                ),
            );
            let st = it.operand_stack();
            let ascent: i64 = st[st.len() - 1].repr().parse().expect("Ascent");
            let rows: Vec<String> = st[st.len() - 2]
                .repr()
                .trim_matches(|c| c == '[' || c == ']')
                .split_whitespace()
                .map(|r| r.trim_matches(|c| c == '(' || c == ')').to_string())
                .collect();
            for py in 0..h {
                for px in 0..w {
                    // device row py (top down) is user y = h-1-py .. h-py;
                    // bitmap row r covers y = base+ascent-r-1 .. base+ascent-r
                    let r = i64::from(base) + ascent - 1 - (i64::from(h) - 1 - i64::from(py));
                    let want = usize::try_from(r)
                        .ok()
                        .and_then(|r| rows.get(r))
                        .and_then(|row| row.as_bytes().get(px as usize))
                        == Some(&b'#');
                    let p = it.gfx().pixmap.pixel(px, py).expect("in bounds");
                    let got = match p.red() {
                        255 => true,
                        0 => false,
                        partial => panic!("{face} {ch:?}: blurred pixel {px},{py} = {partial}"),
                    };
                    assert_eq!(got, want, "{face} {ch:?}: pixel {px},{py}");
                }
            }
        }
    }
}

#[test]
fn the_faces_draw_in_the_callers_colour() {
    for (path, faces) in FILES {
        for face in faces {
            let mut it = loaded(path, 400, 60);
            run(
                &mut it,
                &format!(
                    "0 setgray clippath fill 0 0.8 1 setrgbcolor
                     /{face} findfont 30 scalefont setfont 4 20 moveto (Hello 42) show"
                ),
            );
            let px = it.gfx().pixmap.data().as_chunks::<4>().0;
            let inked: Vec<_> = px.iter().filter(|p| p[1] > 0 || p[2] > 0).collect();
            assert!(inked.len() > 200, "{face} inked {} pixels", inked.len());
            assert!(
                inked.iter().all(|p| p[0] == 0),
                "{face} painted a colour of its own"
            );
        }
    }
}

#[test]
fn scan_gaps_and_nlq_change_the_ink_as_documented() {
    let ink = |path: &str, face: &str| {
        let mut it = loaded(path, 600, 80);
        run(
            &mut it,
            &format!(
                "0 setgray clippath fill 1 0 0 setrgbcolor
                 /{face} findfont 36 scalefont setfont 4 20 moveto (READY HELLO world) show"
            ),
        );
        lit(&it, 128)
    };
    let solid = ink("lib/fonts/phosphor.ps", "Phosphor");
    let scan = ink("lib/fonts/phosphor.ps", "Phosphor-Scan");
    assert!(
        scan * 10 < solid * 8,
        "scan lines darken the rows: {scan} vs {solid}"
    );
    let draft = ink("lib/fonts/pinpoint.ps", "Pinpoint-Draft");
    let nlq = ink("lib/fonts/pinpoint.ps", "Pinpoint-NLQ");
    assert!(
        nlq * 10 > draft * 11,
        "NLQ fills between the dots: {nlq} vs {draft}"
    );
}

#[test]
fn pf_variant_makes_a_new_face_and_leaves_the_base_alone() {
    let mut it = loaded("lib/fonts/diode7.ps", 10, 10);
    run(
        &mut it,
        "/Big /Diode7 << /DotSize 1.2 /DotShape /square >> pf-variant
         /Big findfont /DotSize get 1.2 eq
         /Big findfont /DotShape get /square eq and
         /Diode7 findfont /DotSize get 0.8 eq and
         /Diode7 findfont /DotShape get /round eq and",
    );
    assert_eq!(top(&it), "true");
}

#[test]
fn diode5_prints_lowercase_as_capitals() {
    let render = |text: &str| {
        let mut it = loaded("lib/fonts/diode5.ps", 300, 40);
        run(
            &mut it,
            &format!("/Diode5 findfont 25 scalefont setfont 5 8 moveto ({text}) show"),
        );
        it.gfx().pixmap.data().to_vec()
    };
    assert_eq!(render("Hello World"), render("HELLO WORLD"));
}

/// The pixel renderer is one block copied into each file (each file stays
/// self-contained, the lib/fonts/ doctrine): a fix to one copy must reach
/// all of them.
#[test]
fn the_renderer_block_is_the_same_in_every_file() {
    let block = |path: &str| {
        let s = std::fs::read_to_string(path).expect("font file");
        let start = s
            .find("  % --- the pixel renderer")
            .expect("renderer start");
        let end = s[start..]
            .find("Encoding 3 1 roll put } for pop\n")
            .expect("renderer end");
        s[start..start + end].to_string()
    };
    let first = block(FILES[0].0);
    for (path, _) in &FILES[1..] {
        assert!(
            block(path) == first,
            "{path}: renderer differs from phosphor.ps"
        );
    }
}

#[test]
fn ghostscript_accepts_every_face() {
    if !common::gs_available() {
        return;
    }
    let dir = common::scratch_dir("pscat-retro-fonts");
    for (path, faces) in FILES {
        let lib = std::fs::read_to_string(path).expect("font file");
        for face in faces {
            let driver = format!(
                "0 setgray clippath fill 0.3 1 0.45 setrgbcolor\n\
                 /{face} findfont 40 scalefont setfont\n\
                 10 100 moveto (ABC xyz 123!?) show\n\
                 /V /{face} << /DotShape /round /DotSize 0.7 /Interpolate true >> pf-variant\n\
                 /V findfont 40 scalefont setfont 10 40 moveto (Qq@~) show showpage\n"
            );
            let combined = dir.join(format!("{face}.ps"));
            std::fs::write(&combined, format!("{lib}\n{driver}")).expect("write");
            let out = std::process::Command::new("gs")
                .args([
                    "-dNOPAUSE",
                    "-dBATCH",
                    "-q",
                    "-sDEVICE=png16m",
                    "-g400x200",
                    "-r72",
                    "-o/dev/null",
                ])
                .arg(&combined)
                .output_deadline()
                .expect("run gs");
            assert!(
                out.status.success() && out.stdout.is_empty(),
                "gs rejected {face}: {}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}

#[test]
fn the_specimen_sheet_renders() {
    let src = std::fs::read("examples/retro_fonts.ps").expect("specimen present");
    let mut it = Interp::with_page(760, 1400).expect("page");
    it.run_source(&src)
        .unwrap_or_else(|e| panic!("retro_fonts.ps failed: {}", it.error_report(&e)));
    assert_eq!(it.gfx().pages().len(), 1);
}
