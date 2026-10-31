//! Behavioral contracts of the PostScript libraries. No engine changes.
//! Guard errors live alongside their guards in %%SelfTest blocks.
mod common;

use common::CommandDeadline as _;
use pscat::{Interp, PsError};
use std::process::Command;

const FUNCTIONAL: &str = include_str!("../lib/functional.ps");
const ASPECT: &str = include_str!("../lib/aspect.ps");

fn interpreter() -> Interp {
    let mut it = Interp::with_page(100, 100).expect("test page");
    it.capture_stdout();
    it.run_str(FUNCTIONAL).expect("functional library");
    it.run_str(ASPECT).expect("aspect library");
    it
}

fn run(src: &str) -> Interp {
    let mut it = interpreter();
    it.run_str(src)
        .unwrap_or_else(|e| panic!("{src}: {}", it.error_report(&e)));
    assert_eq!(it.dict_stack_len(), 2, "library dictionary leaked");
    it
}

fn eval(src: &str) -> Vec<String> {
    run(src).operand_stack().iter().map(|o| o.repr()).collect()
}

#[test]
fn empty_and_singleton_collections() {
    assert_eq!(
        eval(
            "[] {missing} fp-map [] {missing} fp-filter [] 42 {missing} fp-fold [] {missing} fp-each [7] {1 add} fp-map [9] {false exch pop} fp-filter"
        ),
        ["[]", "[]", "42", "[8]", "[]"]
    );
    assert_eq!(
        eval("[8] {true exch pop} fp-filter [5] 10 {sub} fp-fold"),
        ["[8]", "5"]
    );
}

#[test]
fn nested_calls_and_noncommutative_fold() {
    assert_eq!(
        eval("[[1 2] [] [3]] { {dup mul} fp-map } fp-map [1 2 3] 20 {sub} fp-fold"),
        ["[[1 4] [] [9]]", "14"]
    );
    assert_eq!(
        eval("[[1 2] [3 4]] 0 {0 {add} fp-fold add} fp-fold"),
        ["10"]
    );
}

#[test]
fn compose_order_associativity_and_quotation_boundaries() {
    assert_eq!(
        eval(
            "3 {1 add} {2 mul} fp-compose exec 3 {1 add} {2 mul} fp-compose {5 sub} fp-compose exec 3 {1 add} {2 mul} {5 sub} fp-compose fp-compose exec { {7} } {exec} fp-compose exec"
        ),
        ["8", "3", "3", "7"]
    );
}

#[test]
fn curry_and_dip_preserve_executable_data_without_running_it() {
    assert_eq!(
        eval(
            "/never cvx {} fp-curry exec xcheck (missing) cvx {} fp-curry exec type /add load {} fp-curry exec type {9} {} fp-curry exec type 2 3 /saved {add} fp-dip"
        ),
        [
            "true",
            "stringtype",
            "operatortype",
            "arraytype",
            "5",
            "/saved"
        ]
    );
    assert_eq!(eval("[1 2] 5 {sub} fp-curry fp-map"), ["[-4 -3]"]);
}

#[test]
fn explicit_capture_survives_dictionary_lifetime_but_names_stay_dynamic() {
    assert_eq!(
        eval(
            "/f 1 dict begin /n 10 def n {add} fp-curry end def /n 20 def 3 f 3 {n add} exec 1 dict begin /n 30 def 3 f 3 {n add} exec end"
        ),
        ["13", "23", "13", "33"]
    );
}

#[test]
fn callbacks_see_exact_caller_stack_and_dictionary() {
    assert_eq!(
        eval(
            "/owner currentdict def /total 0 def 77 [1 2 3] {count 2 eq currentdict owner eq and not {bad_scope} if total add /total exch def} fp-each total"
        ),
        ["77", "6"]
    );
    assert_eq!(
        eval(
            "1 dict begin /owner currentdict def [4] {/seen exch def currentdict owner eq} fp-map seen end"
        ),
        ["[true]", "4"]
    );
}

#[test]
fn nested_calls_and_repeated_execution_have_independent_state() {
    assert_eq!(
        eval(
            "/q {[1 2] {dup mul} fp-map} def q q /sum 0 def [1 2] {pop [3 4] {sum add /sum exch def} fp-each} fp-each sum"
        ),
        ["[1 4]", "[1 4]", "14"]
    );
}

#[test]
fn input_slots_are_snapshotted_but_nested_objects_alias() {
    assert_eq!(
        eval(
            "/a [1 2 3] def a {a 1 99 put} fp-map a /child [5] def /src [child] def /dst src {} fp-map def dst src eq dst 0 get child eq dst 0 get 0 8 put child"
        ),
        ["[1 2 3]", "[1 99 3]", "false", "true", "[8]"]
    );
    assert_eq!(
        eval("/a [1 2] def a {a 1 99 put pop true} fp-filter a"),
        ["[1 2]", "[1 99]"]
    );
}

#[test]
fn curry_and_compose_retain_composite_and_procedure_references() {
    assert_eq!(
        eval(
            "/a [1] def /q a {} fp-curry def a 0 9 put q /p {1 add} def /c /p load {2 mul} fp-compose def /p load 0 3 put 4 c"
        ),
        ["[9]", "14"]
    );
}

#[test]
fn callback_errors_propagate_without_private_dictionary_leaks() {
    for src in [
        "[1] {pop missing_callback} fp-map",
        "[1] {pop missing_callback} fp-filter",
        "[1] 0 {pop pop missing_callback} fp-fold",
        "[1] {pop missing_callback} fp-each",
        "[1] {pop [2] {pop missing_callback} fp-each} fp-each",
    ] {
        let mut it = interpreter();
        assert!(
            matches!(it.run_str(src), Err(PsError::Undefined(_))),
            "{src}"
        );
        assert_eq!(it.dict_stack_len(), 2, "{src}");
    }
    assert_eq!(
        eval(
            "{[1] {pop missing_callback} fp-map} stopped $error /command get countdictstack [2] {1 add} fp-map"
        ),
        ["true", "missing_callback", "2", "[3]"]
    );
}

#[test]
fn callbacks_can_catch_their_own_errors_or_stop_outward() {
    assert_eq!(eval("[1] {pop {missing} stopped} fp-map"), ["[true]"]);
    assert_eq!(
        eval("{[1 2] {pop stop} fp-each 99} stopped countdictstack"),
        ["true", "2"]
    );
}

#[test]
fn arbitrary_fold_identity_is_not_autoexecuted() {
    assert_eq!(
        eval("[] {missing} {pop} fp-fold xcheck [1] /missing cvx {pop} fp-fold xcheck"),
        ["true", "true"]
    );
}

#[test]
fn large_iteration_does_not_recurse_per_element() {
    assert_eq!(
        eval("/a 10000 array def 0 1 9999 {a exch 1 put} for a 0 {add} fp-fold"),
        ["10000"]
    );
}

#[test]
fn nested_advice_order_and_trace_output() {
    let it = run(
        "/q {(T) print} {(B) print} aop-before {(A) print} aop-after (outer) aop-trace def q q",
    );
    assert_eq!(
        it.captured_stdout(),
        b"enter outer\nBTAleave outer\nenter outer\nBTAleave outer\n"
    );
    assert!(it.operand_stack().is_empty());
}

#[test]
fn around_can_proceed_repeat_or_skip() {
    assert_eq!(
        eval(
            "3 {2 mul} {exec 1 add} aop-around exec 3 {2 mul} {dup fp-dip exec} aop-around exec 3 {missing} {pop 4 add} aop-around exec"
        ),
        ["7", "12", "7"]
    );
}

#[test]
fn counters_count_attempts_and_after_advice_skips_errors() {
    let it = run(
        "/c << /Calls 0 >> def /q {missing} c aop-count {(AFTER) print} aop-after (failing) aop-trace def {q} stopped pop {q} stopped pop c /Calls get",
    );
    assert_eq!(it.captured_stdout(), b"enter failing\nenter failing\n");
    assert_eq!(it.operand_stack()[0].repr(), "2");
}

#[test]
fn advice_runs_in_callers_dictionary_and_preserves_target_stack_contract() {
    assert_eq!(
        eval(
            "/q {add} {/seen true def} aop-before def 1 dict begin /owner currentdict def 10 20 q seen currentdict owner eq end"
        ),
        ["30", "true", "true"]
    );
}

#[test]
fn operator_binding_and_previous_captures_limit_interception() {
    assert_eq!(
        eval(
            "/bound {add} bind def /dynamic {add} def /old /dynamic load def /c << /Calls 0 >> def /add {add} bind c aop-count def 2 3 bound 2 3 dynamic c /Calls get /dynamic {sub} def 9 4 old 9 4 dynamic c /Calls get"
        ),
        ["5", "5", "1", "13", "5", "2"]
    );
}

#[test]
fn library_helpers_survive_name_shadowing_and_reloading() {
    let mut it = run("/partial 10 {add} fp-curry def /fp-curry {broken} def [1 2] {1 add} fp-map");
    it.run_str(FUNCTIONAL).expect("reload");
    it.run_str("2 partial").expect("old partial");
    assert_eq!(
        it.operand_stack()
            .iter()
            .map(|o| o.repr())
            .collect::<Vec<_>>(),
        ["[2 3]", "12"]
    );
}

#[test]
fn observation_advice_preserves_graphics_state_and_pixels() {
    let drawing =
        "gsave 0.2 0.5 0.7 setrgbcolor 40 40 translate 15 rotate 0 0 20 30 rectfill grestore";
    let plain = run(&format!("/q {{{drawing}}} def q"));
    let observed = run(&format!(
        "/c << /Calls 0 >> def /q {{{drawing}}} c aop-count (draw) aop-trace def q"
    ));
    assert_eq!(plain.gfx().pixmap.data(), observed.gfx().pixmap.data());
    assert_eq!(
        eval("/q {0.3 setgray} {} aop-before def q currentgray"),
        ["0.3"]
    );
    // Advice is allowed to change graphics; there is no implicit save/restore.
    assert_eq!(
        eval("{0.4 setgray missing} {} aop-after {exec} aop-around stopped pop currentgray"),
        ["0.4"]
    );
}

#[test]
fn ghostscript_matches_the_semantics_corpus_including_packed_quotations() {
    if Command::new("gs")
        .arg("--version")
        .output_deadline()
        .is_err()
    {
        eprintln!("gs unavailable: semantic cross-check skipped");
        return;
    }
    let corpus = include_str!("data/functional_semantics.ps");
    let native = run(corpus);
    let scratch = common::scratch_dir("functional-gs");
    let file = scratch.join("semantics.ps");
    std::fs::write(&file, format!("{FUNCTIONAL}\n{ASPECT}\n{corpus}")).expect("write gs program");
    let out = Command::new("gs")
        .args(["-q", "-dNODISPLAY", "-dBATCH", "-dNOPAUSE"])
        .arg(&file)
        .output_deadline()
        .expect("gs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert_eq!(native.captured_stdout(), out.stdout);
}

#[test]
fn library_load_and_invocation_do_not_replace_standard_operators_or_leak_scratch() {
    let mut it = Interp::new();
    it.run_str("/nativeadd /add load def /nativeexec /exec load def")
        .expect("snapshot");
    it.run_str(FUNCTIONAL).expect("functional");
    it.run_str(ASPECT).expect("aspect");
    it.run_str("/nativeadd load /add load eq /nativeexec load /exec load eq /size 0 def /size userdict length def [1 2 3] {1 add} fp-map pop userdict length size eq").expect("identity");
    assert_eq!(
        it.operand_stack()
            .iter()
            .map(|o| o.repr())
            .collect::<Vec<_>>(),
        ["true", "true", "true"]
    );
}

#[test]
fn trace_treats_executable_string_labels_as_data() {
    let it = run("{} (never_execute_this) cvx aop-trace exec");
    assert_eq!(
        it.captured_stdout(),
        b"enter never_execute_this\nleave never_execute_this\n"
    );
}

#[test]
fn poster_variations_match_the_imperative_control_and_committed_renders() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut variations = Vec::new();
    for (style, name) in ["ultraviolet", "sunprint", "blue-hour"].iter().enumerate() {
        let mut pair = Vec::new();
        for driver in ["poster", "imperative"] {
            let mut it = Interp::with_page(720, 960).expect("poster page");
            it.capture_stdout();
            it.run_str(&format!(
                "/Style {style} def (examples/functional/{driver}.ps) run"
            ))
            .unwrap_or_else(|e| panic!("{driver}/{style}: {}", it.error_report(&e)));
            assert_eq!(it.dict_stack_len(), 2);
            assert!(it.operand_stack().is_empty());
            assert_eq!(
                it.captured_stdout(),
                b"enter garden\nleave garden\nflowers=13\nradius-checksum=622\n"
            );
            pair.push(it.gfx().pixmap.data().to_vec());
        }
        assert_eq!(
            pair[0], pair[1],
            "{name}: functional/imperative pixels differ"
        );
        let golden = tiny_skia::Pixmap::load_png(
            root.join(format!("docs/functional-postscript/assets/{name}.png")),
        )
        .expect("committed poster render");
        assert_eq!(
            pair[0],
            golden.data(),
            "{name}: re-render and inspect an intentional change"
        );
        variations.push(pair.remove(0));
    }
    assert_ne!(variations[0], variations[1]);
    assert_ne!(variations[1], variations[2]);
    assert_ne!(variations[0], variations[2]);
}

#[test]
fn offline_documentation_assets_and_complete_examples_are_valid() {
    let out = Command::new("python3")
        .args([
            "scripts/check_functional_docs.py",
            "--pscat",
            env!("CARGO_BIN_EXE_pscat"),
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output_deadline()
        .expect("documentation checker");
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn render_study(width: u32, height: u32, source: &str) -> Interp {
    let mut it = Interp::with_page(width, height).expect("study page");
    it.capture_stdout();
    it.run_str(source)
        .unwrap_or_else(|e| panic!("study: {}", it.error_report(&e)));
    assert_eq!(it.dict_stack_len(), 2);
    assert!(it.operand_stack().is_empty());
    it
}

#[test]
fn typography_layout_measures_advances_and_reports_bounds_in_page_space() {
    assert_eq!(
        eval(
            "(examples/functional/type-common.ps) run
         /r (WIDE) 60 80 400 ty-layout def
         r length 4 eq r 0 get 1 get 60 eq
         r 0 {dup 3 get /Helvetica-Bold findfont exch scalefont setfont
                0 get stringwidth pop add} fp-fold 400 sub abs 0.001 lt
         r 0 get ty-bounds dup 0 get 60 ge exch 1 get 80 ge"
        ),
        ["true", "true", "true", "true", "true"]
    );
}

#[test]
fn new_typography_and_aop_renders_match_saved_evidence_and_target_pixels() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for (name, width, height, source, expected) in [
        (
            "type-nocturne",
            960,
            1200,
            "(examples/functional/type-terrain.ps) run",
            "enter type-terrain\nleave type-terrain\nglyphs=15\n",
        ),
        (
            "type-sunprint",
            960,
            1200,
            "/TypeStyle 1 def (examples/functional/type-terrain.ps) run",
            "enter type-terrain\nleave type-terrain\nglyphs=15\n",
        ),
        (
            "press-inspected",
            1280,
            960,
            "(examples/functional/observable-press.ps) run",
            "enter proof\nleave proof\nattempted=7\ncompleted=6\nrejected=1\nfault=obs-font-size-must-be-number\n",
        ),
        (
            "press-clean",
            1280,
            960,
            "/Inspect false def (examples/functional/observable-press.ps) run",
            "enter proof\nleave proof\nattempted=7\ncompleted=6\nrejected=1\nfault=obs-font-size-must-be-number\n",
        ),
        (
            "type-blueprint",
            960,
            1200,
            "/TypeStyle 2 def /TypeFont /AvantGarde-Demi def (examples/functional/type-terrain.ps) run",
            "enter type-terrain\nleave type-terrain\nglyphs=15\n",
        ),
        (
            "type-risograph",
            960,
            1200,
            "/TypeStyle 3 def /TypeFont /Bookman-Demi def (examples/functional/type-terrain.ps) run",
            "enter type-terrain\nleave type-terrain\nglyphs=15\n",
        ),
        (
            "type-neon",
            960,
            1200,
            "/TypeStyle 4 def /TypeFont /Times-BoldItalic def (examples/functional/type-terrain.ps) run",
            "enter type-terrain\nleave type-terrain\nglyphs=15\n",
        ),
        (
            "type-opart",
            960,
            1200,
            "/TypeStyle 5 def /TypeFont /Courier-Bold def (examples/functional/type-terrain.ps) run",
            "enter type-terrain\nleave type-terrain\nglyphs=15\n",
        ),
    ] {
        let it = render_study(width, height, source);
        assert_eq!(it.captured_stdout(), expected.as_bytes());
        let golden = tiny_skia::Pixmap::load_png(
            root.join(format!("docs/functional-postscript/assets/{name}.png")),
        )
        .expect("study golden");
        assert!(
            it.gfx().pixmap.data() == golden.data(),
            "{name}: saved pixels differ"
        );
    }
    // Compare direct/decorated calls at the same coordinates: translating a
    // glyph between proof rows can change f32 anti-alias coverage by one LSB.
    let mut pair = Vec::new();
    for target in ["ty-flat-letter", "obs.mark"] {
        let mut it = render_study(
            1280,
            960,
            "/Inspect false def (examples/functional/observable-press.ps) run",
        );
        it.run_str(&format!(
            "erasepage initgraphics 0.95 0.90 0.78 setrgbcolor [(G) 80 120 190 0 /Helvetica-Bold] {target}"
        ))
        .expect("isolated target");
        pair.push(it.gfx().pixmap.data().to_vec());
    }
    assert!(
        pair[0] == pair[1],
        "observation changed original target pixels"
    );
    let clean = render_study(
        1280,
        960,
        "/Inspect false def /InjectFault false def /ProofText (DREAM) def (examples/functional/observable-press.ps) run",
    );
    assert_eq!(
        clean.captured_stdout(),
        b"enter proof\nleave proof\nattempted=5\ncompleted=5\nrejected=0\nfault=none\n"
    );
}

#[test]
fn committed_live_wasm_payload_matches_the_native_studies() {
    if Command::new("node")
        .arg("--version")
        .output_deadline()
        .is_err()
    {
        eprintln!("node unavailable: live WASM/native study comparison skipped");
        return;
    }
    // The committed payload is a snapshot of the engine; once main moves the
    // engine or JS API, pixel parity with today's native build is not owed.
    let fresh = Command::new("python3")
        .args(["scripts/build_functional_studio.py", "--check"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output_deadline()
        .expect("studio staleness check");
    if !fresh.status.success() {
        eprintln!(
            "studio payload predates the current engine: WASM/native comparison skipped\n{}",
            String::from_utf8_lossy(&fresh.stdout)
        );
        return;
    }
    let scratch = common::scratch_dir("functional-live-wasm");
    let out = Command::new("node")
        .args(["scripts/check_functional_studio.mjs", "--out-dir"])
        .arg(&scratch)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output_deadline()
        .expect("studio WASM check");
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let mut parity_cases = vec![
        (
            "terrain".to_string(),
            960,
            1200,
            "(examples/functional/type-terrain.ps) run".to_string(),
        ),
        (
            "press".to_string(),
            1280,
            960,
            "(examples/functional/observable-press.ps) run".to_string(),
        ),
        (
            "press-valid".to_string(),
            1280,
            960,
            "/Inspect false def /InjectFault false def /ProofText (DREAM) def (examples/functional/observable-press.ps) run".to_string(),
        ),
    ];
    for style in 1..6 {
        parity_cases.push((
            format!("style-{style}"),
            960,
            1200,
            format!("/TypeStyle {style} def (examples/functional/type-terrain.ps) run"),
        ));
    }
    let fonts = [
        "Helvetica-Bold",
        "Helvetica-BoldOblique",
        "Times-Bold",
        "Times-BoldItalic",
        "Courier-Bold",
        "Palatino-Bold",
        "Bookman-Demi",
        "AvantGarde-Demi",
    ];
    for (i, font) in fonts.iter().enumerate().skip(1) {
        parity_cases.push((
            format!("font-{i}"),
            960,
            1200,
            format!(
                "/TypeFont /{font} def /TypeStyle {} def (examples/functional/type-terrain.ps) run",
                i % 6
            ),
        ));
    }
    for (name, width, height, source) in parity_cases {
        let native = render_study(width, height, &source);
        let wasm = std::fs::read(scratch.join(format!("{name}.rgba"))).expect("WASM pixels");
        assert_eq!(wasm.len(), native.gfx().pixmap.data().len());
        let differing = wasm
            .iter()
            .zip(native.gfx().pixmap.data())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(differing, 0, "{name}: WASM/native channel differences");
    }
}

#[test]
fn selected_fonts_are_captured_and_short_runs_fit_their_ink_boxes() {
    let fonts = [
        "Helvetica-Bold",
        "Helvetica-BoldOblique",
        "Times-Bold",
        "Times-BoldItalic",
        "Courier-Bold",
        "Palatino-Bold",
        "Bookman-Demi",
        "AvantGarde-Demi",
    ];
    for font in fonts {
        for word in ["I", "J", "Q", "WIDE", "MWMW"] {
            let source = format!(
                "/TypeFont /{font} def (examples/functional/type-common.ps) run
                 /r ({word}) 93 267 764 ty-layout def
                 r 0 get 5 get /{font} eq
                 r {{ty-bounds aload pop
                    477.02 le exch 857.02 le and
                    exch 200 ge and exch 92.98 ge and}} fp-map
                 true {{and}} fp-fold
                 /before r 0 get ty-bounds def
                 /TypeFont /Courier-Bold def
                 /after r 0 get ty-bounds def
                 [0 1 2 3] {{dup before exch get exch after exch get sub abs 0.001 lt}} fp-map
                 true {{and}} fp-fold"
            );
            assert_eq!(eval(&source), ["true", "true", "true"], "{font}/{word}");
        }
    }
}
