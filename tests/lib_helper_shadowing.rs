//! A library procedure reaches its helpers in a way the caller cannot shadow
//! (issue #1225).
//!
//! `pkliner` used to call artkit's `shade` by name, looked up when the brush
//! ran. A caller that kept a colour in a local `/shade` still had its own
//! dictionary on the dictionary stack, so the brush found the caller's array
//! instead of the procedure and failed with a typecheck that named `shade`
//! and nothing of the library. `bind` does not help: it replaces only names
//! that are operators, and a helper is a procedure.
//!
//! The libraries now write `//shade exec`: the name is replaced by the
//! procedure when the procedure is scanned, so the run-time lookup (and the
//! hazard) is gone. Two tests pin it:
//!
//! * a static guard: in each library below, no helper that the library
//!   reaches through `//name exec` is also reached by a bare name after its
//!   definition (a new brush that writes `shade` again fails here, as a new
//!   unbound procedure fails `tests/libbind.rs`);
//! * a behavioural check: with a dictionary on the stack that defines every
//!   such helper name as a procedure that raises a distinctive error, a
//!   scene that calls the libraries' public procedures renders byte for byte
//!   what it renders with no such dictionary.
//!
//! What stays by name, and why, is in `notes/1225-library-helpers-by-name.md`:
//! forward references (a helper defined later in the file cannot be scanned
//! yet), data and state a caller may set (`wkmaxsegs`, `Palettes`, the turtle
//! registers), names two libraries define (`getopt`), and the test seams a
//! test replaces by name (`pktrowel`).

mod common;

use pscat::{Interp, PsError};
use std::collections::{BTreeMap, BTreeSet};

/// Every library whose helpers are reached by `//name exec`, with the
/// libraries it needs loaded first (its `@requires` line).
const LIBS: &[(&str, &[&str])] = &[
    ("lib/artkit.ps", &[]),
    ("lib/paintkit.ps", &["lib/artkit.ps"]),
    ("lib/pagekit.ps", &["lib/artkit.ps"]),
    ("lib/fillkit.ps", &["lib/artkit.ps"]),
    ("lib/lettering.ps", &["lib/artkit.ps"]),
    ("lib/stipplekit.ps", &["lib/artkit.ps"]),
    ("lib/surfacekit.ps", &["lib/artkit.ps"]),
    ("lib/graph.ps", &[]),
    ("lib/drykit.ps", &["lib/artkit.ps", "lib/paintkit.ps"]),
    (
        "lib/drycrayon.ps",
        &["lib/artkit.ps", "lib/paintkit.ps", "lib/drykit.ps"],
    ),
    (
        "lib/drycharcoal.ps",
        &[
            "lib/artkit.ps",
            "lib/paintkit.ps",
            "lib/drykit.ps",
            "lib/hatchkit.ps",
        ],
    ),
    (
        "lib/printkit.ps",
        &["lib/artkit.ps", "lib/hatchkit.ps", "lib/surfacekit.ps"],
    ),
    ("lib/released.ps", &["lib/artkit.ps", "lib/surfacekit.ps"]),
    (
        "lib/apparel.ps",
        &["lib/artkit.ps", "lib/headline.ps", "lib/lettering.ps"],
    ),
];

// ---------------------------------------------------------------------
// A small PostScript scanner: enough to find executable names inside
// procedures, ignoring comments, strings and `/literal` names.
// ---------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Tok {
    text: String,
    /// Byte offset of the token's first character.
    at: usize,
    /// `{` nesting depth the token sits at (a `{` or `}` sits at the outer
    /// side of its own brace).
    depth: usize,
    is_string: bool,
}

fn scan(src: &str) -> Vec<Tok> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut depth = 0usize;
    while i < b.len() {
        let c = b[i];
        match c {
            b' ' | b'\t' | b'\r' | b'\n' | 0x0c | 0 => i += 1,
            b'%' => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'(' => {
                let start = i;
                let mut nest = 1;
                i += 1;
                while i < b.len() && nest > 0 {
                    match b[i] {
                        b'\\' => i += 1,
                        b'(' => nest += 1,
                        b')' => nest -= 1,
                        _ => {}
                    }
                    i += 1;
                }
                out.push(Tok {
                    text: src[start..i.min(b.len())].to_string(),
                    at: start,
                    depth,
                    is_string: true,
                });
            }
            b'<' if b.get(i + 1) == Some(&b'<') => {
                out.push(tok("<<", i, depth));
                i += 2;
            }
            b'>' if b.get(i + 1) == Some(&b'>') => {
                out.push(tok(">>", i, depth));
                i += 2;
            }
            b'<' => {
                let start = i;
                while i < b.len() && b[i] != b'>' {
                    i += 1;
                }
                i += 1;
                out.push(Tok {
                    text: src[start..i.min(b.len())].to_string(),
                    at: start,
                    depth,
                    is_string: true,
                });
            }
            b'{' => {
                out.push(tok("{", i, depth));
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                out.push(tok("}", i, depth));
                i += 1;
            }
            b'[' | b']' => {
                out.push(tok(&src[i..i + 1], i, depth));
                i += 1;
            }
            _ => {
                let start = i;
                i += 1;
                if c == b'/' && b.get(i) == Some(&b'/') {
                    i += 1;
                }
                while i < b.len()
                    && !matches!(
                        b[i],
                        b' ' | b'\t'
                            | b'\r'
                            | b'\n'
                            | 0x0c
                            | 0
                            | b'('
                            | b')'
                            | b'{'
                            | b'}'
                            | b'['
                            | b']'
                            | b'<'
                            | b'>'
                            | b'/'
                            | b'%'
                    )
                {
                    i += 1;
                }
                out.push(tok(&src[start..i], start, depth));
            }
        }
    }
    out
}

fn tok(text: &str, at: usize, depth: usize) -> Tok {
    Tok {
        text: text.to_string(),
        at,
        depth,
        is_string: false,
    }
}

/// An executable name: not a string, not `/literal`, not `//immediate`, not a
/// delimiter.
fn exec_name(t: &Tok) -> Option<&str> {
    if t.is_string
        || t.text.starts_with('/')
        || matches!(t.text.as_str(), "{" | "}" | "[" | "]" | "<<" | ">>")
    {
        None
    } else {
        Some(&t.text)
    }
}

/// The names the source reaches by `//name exec`.
fn fixed_names(src: &str) -> BTreeSet<String> {
    let toks = scan(src);
    let mut out = BTreeSet::new();
    for w in toks.windows(2) {
        if let Some(n) = w[0].text.strip_prefix("//")
            && !w[0].is_string
            && w[1].text == "exec"
            && !w[1].is_string
        {
            out.insert(n.to_string());
        }
    }
    out
}

/// Byte offset just past the first top-level `/name { ... } [bind] def`.
fn top_level_def_end(toks: &[Tok], name: &str) -> Option<usize> {
    let lit = format!("/{name}");
    for (i, t) in toks.iter().enumerate() {
        if t.depth == 0 && !t.is_string && t.text == lit && toks.get(i + 1)?.text == "{" {
            let mut j = i + 2;
            let mut nest = 1;
            while j < toks.len() && nest > 0 {
                match toks[j].text.as_str() {
                    "{" if !toks[j].is_string => nest += 1,
                    "}" if !toks[j].is_string => nest -= 1,
                    _ => {}
                }
                j += 1;
            }
            if toks.get(j).map(|t| t.text.as_str()) == Some("bind") {
                j += 1;
            }
            if let Some(d) = toks.get(j)
                && d.text == "def"
            {
                return Some(d.at + 3);
            }
        }
    }
    None
}

/// Bare uses (inside a procedure, after the definition when this file has
/// one) of a name the file also reaches through `//name exec`.
fn bare_uses(src: &str) -> Vec<(usize, String)> {
    let toks = scan(src);
    let fixed = fixed_names(src);
    let ends: BTreeMap<&str, Option<usize>> = fixed
        .iter()
        .map(|n| (n.as_str(), top_level_def_end(&toks, n)))
        .collect();
    let mut bad = Vec::new();
    for t in &toks {
        if t.depth == 0 {
            continue;
        }
        let Some(n) = exec_name(t) else { continue };
        let Some(end) = ends.get(n) else { continue };
        // A name defined in this file is only reachable by `//` once its
        // definition has been scanned: a use before (or inside) it is a
        // forward reference or a recursion and stays by name.
        if end.is_none_or(|e| t.at > e) {
            let line = src[..t.at].matches('\n').count() + 1;
            bad.push((line, n.to_string()));
        }
    }
    bad
}

#[test]
fn no_helper_reached_by_immediate_name_is_also_reached_by_a_bare_name() {
    let mut report = Vec::new();
    for (lib, _) in LIBS {
        let src = std::fs::read_to_string(lib).unwrap_or_else(|e| panic!("{lib}: {e}"));
        assert!(
            !fixed_names(&src).is_empty(),
            "{lib} reaches no helper through `//name exec`: is it in this test by mistake?"
        );
        for (line, name) in bare_uses(&src) {
            report.push(format!("{lib}:{line}: `{name}` by name"));
        }
    }
    assert!(
        report.is_empty(),
        "a caller's local variable of the same name would shadow these (write \
         `//name exec`, issue #1225):\n{}",
        report.join("\n")
    );
}

#[test]
fn the_guard_sees_a_bare_helper_and_ignores_comments_strings_and_literals() {
    // The control: the same scan must flag a bare `shade` after the helper
    // is reached through `//shade exec`, and not flag it in a comment, a
    // string, a literal name or a forward reference.
    let src = "/early { later } bind def\n\
               /later { 1 } bind def\n\
               /ok { //later exec } bind def\n\
               /comment { % later\n 1 } bind def\n\
               /str { (later) pop /later pop } bind def\n\
               /bad { later } bind def\n";
    let hits = bare_uses(src);
    assert_eq!(
        hits,
        vec![(7, "later".to_string())],
        "only the bare use after the definition is a finding: {hits:?}"
    );
}

// ---------------------------------------------------------------------
// Behaviour: a caller dictionary that shadows every fixed helper.
// ---------------------------------------------------------------------

/// Names that a library also reaches by a bare name *before or inside* their
/// own definition (a forward reference to a helper defined later in the
/// file, or a helper calling itself). `//` cannot reach those, so shadowing
/// one would still hit the caller's name: the behavioural check leaves them
/// out. A bare use after the definition is not excused here; the static guard
/// above fails on it, and the shadow check would too.
fn residual_bare_names(src: &str) -> BTreeSet<String> {
    let toks = scan(src);
    let fixed = fixed_names(src);
    let mut out = BTreeSet::new();
    for n in &fixed {
        let Some(end) = top_level_def_end(&toks, n) else {
            continue;
        };
        if toks
            .iter()
            .filter(|t| t.depth >= 1 && exec_name(t) == Some(n.as_str()))
            .any(|t| t.at <= end)
        {
            out.insert(n.clone());
        }
    }
    out
}

/// Every name some library reaches by `//name exec` and reaches in no other
/// way: the names the behavioural check shadows.
fn all_fixed_names() -> BTreeSet<String> {
    let mut all = BTreeSet::new();
    let mut residual = BTreeSet::new();
    for (lib, _) in LIBS {
        let src = std::fs::read_to_string(lib).unwrap_or_else(|e| panic!("{lib}: {e}"));
        all.extend(fixed_names(&src));
        residual.extend(residual_bare_names(&src));
    }
    all.difference(&residual).cloned().collect()
}

/// `begin`s a dictionary that defines each name as a procedure that raises
/// `shadowed-<name>-by-caller` when it is run. Data would be worse as a
/// probe: a call would push it and the failure would surface later.
fn shadow_begin(names: &BTreeSet<String>) -> String {
    let mut s = format!("{} dict begin\n", names.len() + 4000);
    for n in names {
        s.push_str(&format!("/{n} {{ /shadowed-{n}-by-caller load }} def\n"));
    }
    s
}

/// A scene is written with plain names; each one that a library reaches
/// itself is turned into `//name exec`, so the scene (scanned before the
/// shadow dictionary is begun) still calls the real procedure.
fn immediate(scene: &str, names: &BTreeSet<String>) -> String {
    scene
        .split_whitespace()
        .map(|w| {
            if names.contains(w) {
                format!("//{w} exec")
            } else {
                w.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn pixels(it: &Interp) -> Vec<u8> {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .flat_map(|p| [p.red(), p.green(), p.blue(), p.alpha()])
        .collect()
}

/// Renders the scene once, with or without the shadow dictionary on the
/// stack while it runs.
fn render(libs: &[&str], scene: &str, shadowed: bool, names: &BTreeSet<String>) -> Vec<u8> {
    let mut it = Interp::with_page(200, 200).expect("page");
    for lib in libs {
        let src = std::fs::read(lib).unwrap_or_else(|e| panic!("{lib}: {e}"));
        it.run_source(&src)
            .unwrap_or_else(|e| panic!("{lib} failed to load: {}", it.error_report(&e)));
    }
    it.run_str("1 setgray clippath fill 0 setgray")
        .expect("white page");
    // The scene is scanned here, outside the shadow dictionary.
    let def = format!("/scene {{ {} }} def", immediate(scene, names));
    it.run_str(&def)
        .unwrap_or_else(|e| panic!("scene did not scan: {}", it.error_report(&e)));
    let run = if shadowed {
        format!("{} scene end", shadow_begin(names))
    } else {
        "scene".to_string()
    };
    it.run_str(&run).unwrap_or_else(|e| {
        panic!(
            "scene failed (shadowed: {shadowed}): {}",
            it.error_report(&e)
        )
    });
    pixels(&it)
}

fn assert_immune(what: &str, libs: &[&str], scene: &str) {
    let names = all_fixed_names();
    let plain = render(libs, scene, false, &names);
    let ink = plain.chunks(4).filter(|p| p[0] < 250).count();
    assert!(ink > 50, "{what}: the scene drew nothing ({ink} px)");
    let shadowed = render(libs, scene, true, &names);
    assert!(
        plain == shadowed,
        "{what}: a caller dictionary that shadows the libraries' helpers changed the render"
    );
}

const PRESETS: &[&str] = &[
    "pkribbon",
    "pknib",
    "pkdry",
    "pkspray",
    "pkoil",
    "pktrowel",
    "pkfan",
    "pkbroad",
    "pkliner",
    "pkfeather",
    "pkdab",
    "pkwash",
    "pkscumble",
    "pkzigzag",
    "pktap",
    "pkpull",
];

#[test]
fn paintkit_brushes_ignore_a_callers_shade_mix3_pkjit_and_the_rest() {
    let mut scene = String::from(
        "0.35 0.20 0.45 setrgbcolor \
         [0.8 0.3 0.2] 0.7 shade setrgbcolor 20 20 20 20 rectfill \
         [0.8 0.3 0.2] [0.1 0.4 0.9] 0.4 mix3 setrgbcolor 40 20 20 20 rectfill ",
    );
    for (i, p) in PRESETS.iter().enumerate() {
        let y = 20 + 10 * (i as i32);
        scene.push_str(&format!(
            "gsave 0.1 0.2 0.3 setrgbcolor newpath 20 {y} moveto 60 {} 120 {} 180 {y} curveto \
             << /Width 8 /ColorJitter 0.2 >> {p} grestore ",
            y + 40,
            y - 20,
        ));
    }
    assert_immune(
        "paintkit presets",
        &["lib/artkit.ps", "lib/paintkit.ps"],
        &scene,
    );
}

#[test]
fn the_issue_repro_renders_with_a_local_shade() {
    // The program from the issue: a local colour named `shade` while pkliner
    // runs. It raised `typecheck; OffendingCommand: shade`.
    let mut it = Interp::with_page(100, 100).expect("page");
    for lib in ["lib/artkit.ps", "lib/paintkit.ps"] {
        let src = std::fs::read(lib).expect("lib");
        it.run_source(&src).expect("load");
    }
    it.run_str(
        "/f { 2 dict begin /shade [0.2 0.3 0.5] def \
         newpath 10 10 moveto 50 0 rlineto 0 50 rlineto 0 0 0 setrgbcolor \
         << /Width 1.2 /Taper 0.35 /Waver 0.35 /Depletion 0.15 >> pkliner end } def f",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let ink = pixels(&it).chunks(4).filter(|p| p[3] > 0).count();
    assert!(ink > 20, "pkliner drew nothing ({ink} px)");
}

#[test]
fn artkit_surface_and_fill_libraries_ignore_a_callers_names() {
    assert_immune(
        "artkit scatter and the surface textures",
        &["lib/artkit.ps", "lib/surfacekit.ps"],
        "0 0 200 200 screct << /Count 80 /Scale [0.4 1.6] /Seed 3 >> grain \
         20 20 160 160 screct << /Count 40 /Rotate [0 360] /Seed 5 >> fiber \
         0 0 200 200 screct << /Count 30 /Seed 2 /Mark { pop pop pop pop } >> scatter",
    );
}

#[test]
fn page_templates_ignore_a_callers_names() {
    assert_immune(
        "pagekit",
        &["lib/artkit.ps", "lib/pagekit.ps"],
        "10 10 180 180 << /Title (Hi) /Body (a quick note) /Signoff (- me) >> pgcard \
         pop",
    );
}

#[test]
fn the_probe_is_a_real_control() {
    // A procedure that reaches `shade` by name must fail under the shadow
    // dictionary, naming the shadow: otherwise the tests above would pass
    // for the wrong reason.
    let names = all_fixed_names();
    assert!(names.contains("shade") && names.contains("mix3") && names.contains("frnd"));
    let mut it = Interp::with_page(50, 50).expect("page");
    let src = std::fs::read("lib/artkit.ps").expect("artkit");
    it.run_source(&src).expect("load");
    it.run_str("/probe { [0.5 0.5 0.5] 0.5 shade } def")
        .expect("define");
    let err = it
        .run_str(&format!("{} probe end", shadow_begin(&names)))
        .expect_err("a bare shade must reach the shadow");
    assert!(
        matches!(err, PsError::Undefined(ref n) if n == "shadowed-shade-by-caller"),
        "expected the shadow's error, got {err}"
    );
}
