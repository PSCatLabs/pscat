//! gallery/crayon_drawing.ps (issue #240): a child's crayon drawing built
//! from drycrayon's motions. The look is judged by eye; this pins that the
//! piece builds in pscat and Ghostscript and that its big parts landed
//! where the composition puts them: the sky band along the top, the grass
//! band along the bottom, the air between them left as paper, and the name
//! in the corner.

use pscat::Interp;

const PIECE: &str = "gallery/crayon_drawing.ps";
const W: u32 = 792;
const H: u32 = 612;

/// Fraction of the pixels in the point-space box (x0,y0)-(x1,y1) that
/// satisfy `pred`. y is PostScript's: 0 at the bottom.
fn fraction(px: &[[u8; 3]], x0: u32, y0: u32, x1: u32, y1: u32, pred: fn([u8; 3]) -> bool) -> f64 {
    let mut hit = 0u32;
    let mut all = 0u32;
    for y in (H - y1)..(H - y0) {
        for x in x0..x1 {
            all += 1;
            if pred(px[(y * W + x) as usize]) {
                hit += 1;
            }
        }
    }
    f64::from(hit) / f64::from(all)
}

fn is_blue(p: [u8; 3]) -> bool {
    i32::from(p[2]) > i32::from(p[0]) + 60 && p[2] > p[1]
}

fn is_green(p: [u8; 3]) -> bool {
    i32::from(p[1]) > i32::from(p[0]) + 25 && i32::from(p[1]) > i32::from(p[2]) + 25
}

fn is_ink(p: [u8; 3]) -> bool {
    u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]) < 3 * 200
}

fn is_paper(p: [u8; 3]) -> bool {
    p.iter().all(|&c| c > 235)
}

#[test]
fn the_drawing_builds_in_pscat_with_its_parts_where_the_composition_puts_them() {
    let src = std::fs::read_to_string(PIECE).expect("read the piece");
    // showpage would snapshot and clear the page we want to read back.
    let body = src.replace("\nshowpage", "\n");
    let mut it = Interp::with_page(W, H).expect("page");
    it.run_str(&body)
        .unwrap_or_else(|e| panic!("{PIECE}: {}", it.error_report(&e)));
    let px: Vec<[u8; 3]> = it
        .gfx()
        .pixmap
        .pixels()
        .iter()
        .map(|p| [p.red(), p.green(), p.blue()])
        .collect();

    let sky = fraction(&px, 300, 500, 780, 600, is_blue);
    assert!(sky > 0.5, "the sky band is mostly blue scribble, got {sky}");
    let grass = fraction(&px, 400, 55, 780, 145, is_green);
    assert!(grass < 0.95, "grass has white seams, got {grass}");
    let grass_green = fraction(&px, 0, 55, 792, 145, |p| is_green(p) || p[1] > 180);
    assert!(
        grass_green > 0.5,
        "the ground band is green, got {grass_green}"
    );
    // the air: between the sky's lower edge and the roof, away from the
    // birds and the smoke, is bare paper
    let air = fraction(&px, 400, 300, 590, 390, is_paper);
    assert!(air > 0.95, "the air is paper, got {air}");
    // the name, in the strip under the grass
    let name = fraction(&px, 276, 8, 560, 56, |p| !is_paper(p));
    assert!(
        (0.02..0.5).contains(&name),
        "the name is a few strokes of ink, got {name}"
    );
    // and the outlines went on: black crayon somewhere on the house
    let outline = fraction(&px, 172, 152, 350, 300, is_ink);
    assert!(outline > 0.02, "no outline on the house, got {outline}");
}

#[test]
fn the_drawing_runs_in_ghostscript() {
    let have_gs = std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !have_gs {
        eprintln!("no gs here, skipping");
        return;
    }
    // The piece loads its libraries with relative `run`s from the repo
    // root (cargo's test cwd), which gs's default -dSAFER refuses.
    let out = std::process::Command::new("gs")
        .args([
            "-dNOSAFER",
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=nullpage",
        ])
        .arg(format!("-dDEVICEWIDTHPOINTS={W}"))
        .arg(format!("-dDEVICEHEIGHTPOINTS={H}"))
        .arg(PIECE)
        .output()
        .expect("run gs");
    let text =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "gs rejected the piece: {text}");
    assert!(!text.contains("Error"), "gs reported an error: {text}");
}
