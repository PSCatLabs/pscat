//! Compiled kernels (`kernelimage`, issue #742): the compiled path must give
//! exactly the interpreter's bytes, or exactly its error.
//!
//! Every case renders twice: once under `/strict .kernelmode` (compiled, or
//! an error saying why not) and once under `/interpret .kernelmode` (the
//! reference: the procedure run by the interpreter in a fresh dict per
//! pixel). The pages must match byte for byte, and an error must be the
//! same error. The differential fuzz at the end does the same for random
//! programs.

use pscat::{Interp, Limits, PsError};
use std::time::Duration;

const W: u32 = 70; // more than one 64-lane batch, and a partial one
const H: u32 = 9;

/// Render `prog` (which draws one `kernelimage` filling the page) in
/// `mode`: the page's RGBA bytes, or the error name.
fn render(mode: &str, prog: &str) -> Result<Vec<u8>, String> {
    render_sized(mode, prog, W, H)
}

fn render_sized(mode: &str, prog: &str, w: u32, h: u32) -> Result<Vec<u8>, String> {
    render_counted(mode, prog, w, h).0
}

/// As `render_sized`, plus whether the compiled path finished the image.
fn render_counted(mode: &str, prog: &str, w: u32, h: u32) -> (Result<Vec<u8>, String>, bool) {
    let mut it = Interp::with_page(w, h).expect("page");
    let src = format!(
        "/{mode} .kernelmode\n/W {w} def /H {h} def\n{prog}\n\
         gsave W H scale W H [W 0 0 H neg 0 H] /pixel load 3 kernelimage grestore showpage"
    );
    let r = it.run_str(&src);
    it.run_str("clear .kernelstats").expect("stats");
    let ran_compiled = it.operand_stack()[0].repr() == "1";
    let out = match r {
        Ok(()) => Ok(it.pages()[0].rgba8()),
        Err(e) => Err(match &e {
            PsError::Undefined(s) if s.starts_with("kernelimage: not compiled") => s.clone(),
            other => other.name().to_string(),
        }),
    };
    (out, ran_compiled)
}

/// Assert `prog` compiles and matches the interpreter.
#[track_caller]
fn same(prog: &str) {
    let (k, ran) = render_counted("strict", prog, W, H);
    let r = render("interpret", prog);
    assert!(ran, "the compiled path did not finish the image\n{prog}");
    assert!(
        !matches!(&k, Err(s) if s.starts_with("kernelimage: not compiled")),
        "did not compile: {k:?}\n{prog}"
    );
    assert!(r.is_ok(), "the reference failed: {r:?}\n{prog}");
    assert!(k == r, "compiled and interpreted renders differ\n{prog}");
}

/// Assert `prog` compiles, and both paths raise the same error.
#[track_caller]
fn same_error(prog: &str, err: &str) {
    let k = render("strict", prog);
    let r = render("interpret", prog);
    assert_eq!(r, Err(err.to_string()), "reference\n{prog}");
    assert_eq!(k, r, "compiled\n{prog}");
}

/// `proc 3 kernelcompile` for `prog`'s /pixel.
fn compiles(prog: &str) -> (bool, String) {
    let mut it = Interp::new();
    it.run_str(&format!(
        "/W {W} def /H {H} def {prog} /pixel load 3 kernelcompile"
    ))
    .expect("kernelcompile");
    let st = it.operand_stack();
    (st[0].repr() == "true", st[1].repr())
}

// Map any number to a component that shows its low bits too.
const SHOW: &str = "/show3 { dup 0.013 mul dup floor sub exch dup 1048576 mul dup floor sub exch \
                    1099511627776 mul dup floor sub } bind def";

#[test]
fn arithmetic_and_types() {
    same(&format!(
        "{SHOW} /pixel {{ /j exch def /i exch def
           i j add 3 mul j 7.5 sub div          % int, int, real
           i 2 idiv i 3 mod add                 % idiv, mod
           i neg abs i 5 sub abs add
           j 0.5 add floor j 0.5 sub ceiling add j 0.5 add round add 2.7 truncate add
           add add add
           i 7 mul 360 div sin i cos add i 1 add sqrt add 2 i 0.1 mul exp add i 1 add ln add i 1 add log add
           add i j 1 add atan add i 11 max j min add 1.5 cvi add i cvr add
           show3 }} bind def"
    ));
}

#[test]
fn integer_promotion_matches() {
    // Results that stay integers, and a merge of an integer with a real
    // (the clamp idiom), which carries a per-lane integer tag.
    same(&format!(
        "{SHOW} /pixel {{ /j exch def /i exch def
           i 31 sub 0.37 mul dup 0 lt {{ pop 0 }} if dup 5 gt {{ pop 5 }} if
           dup 2 mul exch pop
           i j mul 2 mod 0 eq {{ 1 }} {{ 1.0 }} ifelse add
           show3 }} bind def"
    ));
}

#[test]
fn integer_only_ops_on_reals_error_like_the_interpreter() {
    same_error(
        "/pixel { /j exch def /i exch def i 0.5 mul 2 idiv dup dup } bind def",
        "typecheck",
    );
    same_error(
        "/pixel { /j exch def /i exch def i 40 sub 30 idiv 1 exch div dup dup } bind def",
        "undefinedresult",
    );
    same_error(
        "/pixel { /j exch def /i exch def i 60 sub sqrt dup dup } bind def",
        "rangecheck",
    );
}

#[test]
fn loops_and_exits() {
    same(&format!(
        "{SHOW} /pixel {{ /j exch def /i exch def
           0 1 1 j 1 add {{ add }} for                       % integer for
           0 0 0.25 1.0 {{ add }} for                        % real for
           10 -3 0 {{ 0.5 mul add }} for                     % negative step
           i 3 {{ 1.5 mul 1 add }} repeat
           i 1 add {{ dup 200 gt {{ exit }} if 3 mul 1 add }} loop
           0 1 i {{ /k exch def k j gt {{ exit }} if }} for
           add add add
           show3 }} bind def"
    ));
}

#[test]
fn nested_loops_with_names_and_exits() {
    same(&format!(
        "{SHOW} /pixel {{ /j exch def /i exch def
           /s 0 def
           0 1 4 {{ /a exch def
             0 1 4 {{ /b exch def
               a b mul i add 7 mod 0 eq {{ exit }} if
               /s s a b add i mul add def
             }} for
             s 1000 gt {{ exit }} if
           }} for
           /zr 0.0 def /zi 0.0 def /n 0 def
           {{ zr zr mul zi zi mul add 4 gt n 40 ge or {{ exit }} if
              zr zr mul zi zi mul sub i 35 sub 30 div add
              zr zi mul 2 mul j 4 sub 5 div add /zi exch def /zr exch def
              /n n 1 add def }} loop
           s n add show3 }} bind def"
    ));
}

#[test]
fn stack_carried_loop_state() {
    // The fractal_clouds style: the running state lives on the stack.
    same(&format!(
        "{SHOW} /pixel {{ /j exch def /i exch def
           0 1 i 0.1 mul j 0.1 mul                % sum amp x y
           0 1 3 {{ pop
             1 index 1 index mul 3 index mul       % sum amp x y v
             5 -1 roll add 4 1 roll                % sum' amp x y
             3 -1 roll 0.5 mul 3 1 roll            % sum' amp' x y
             exch 1.7 mul exch 1.3 mul             % sum' amp' x' y'
           }} for
           add add add show3 }} bind def"
    ));
}

#[test]
fn dictionaries_and_procedures() {
    same(&format!(
        "{SHOW}
         /sq {{ dup mul }} bind def
         /hyp {{ 4 dict begin /b exch def /a exch def a sq b sq add sqrt end }} bind def
         /Gain 1.25 def
         /pixel {{ /j exch def /i exch def
           i j hyp Gain mul
           /Gain 2 def Gain add                     % a local Gain shadows the global
           {{ 3 add }} exec
           show3 }} bind def"
    ));
}

#[test]
fn per_pixel_dict_is_fresh() {
    // A read before the def sees the outside value on every pixel, never
    // the previous pixel's: carried state is impossible by construction.
    same(&format!(
        "{SHOW} /acc 0.5 def
         /pixel {{ /j exch def /i exch def
           /acc acc i add def
           i 2 mod 0 eq {{ /t i def }} if
           acc show3 }} bind def"
    ));
}

#[test]
fn tables_tuples_and_local_arrays() {
    same(&format!(
        "{SHOW}
         /T1 [ 3 1.5 -2 7 0.25 9 4 4.5 ] def
         /T2 [ [0.1 0.2 0.3] [1 2 3] [0.5 -1 2.5] [7 8 9] ] def
         /S (Hello, kernels!) def
         /pixel {{ /j exch def /i exch def
           T1 i 8 mod get
           T2 i 4 mod get aload pop add add add
           T2 j 4 mod get 1 get add
           S i S length mod get add
           [ i j i j add ] dup 2 get exch 0 get add add
           [ 1 2 3 4 ] i 4 mod get add
           /A 5 array def
           0 1 4 {{ /k exch def A k k i mul put }} for
           A j 5 mod get add
           A aload pop add add add add add
           show3 }} bind def"
    ));
}

#[test]
fn table_index_out_of_range_errors() {
    same_error(
        "/T [1 2 3] def /pixel { /j exch def /i exch def T i get dup dup } bind def",
        "rangecheck",
    );
}

#[test]
fn noise_operators() {
    same(&format!(
        "{SHOW} 11 noiseseed
         /pixel {{ /j exch def /i exch def
           i 0.07 mul j 0.11 mul gnoise2
           i 0.05 mul j 0.05 mul 0.3 gnoise3 add
           i 0.1 mul j 0.1 mul vnoise2 add
           i 0.1 mul j 0.1 mul 0.7 vnoise3 add
           i 0.03 mul j 0.03 mul 5 0.5 2.0 fbm2 add
           i 0.03 mul j 0.03 mul 1.5 4 0.5 2.0 fbm3 add
           i 0.03 mul j 0.03 mul 3 0.5 2.0 ridged2 add
           i 0.03 mul j 0.03 mul 0.2 3 0.5 2.0 ridged3 add
           i 0.03 mul j 0.03 mul 3 0.5 2.0 vfbm2 add
           i 0.04 mul j 0.04 mul gnoise2d add add add
           show3 }} bind def"
    ));
}

#[test]
fn not_compilable_falls_back_and_says_why() {
    let prog = "/pixel { /j exch def /i exch def i W div rand pop dup dup } bind def";
    let (ok, why) = compiles(prog);
    assert!(!ok);
    assert!(why.contains("rand"), "{why}");
    assert_eq!(render("auto", prog), render("interpret", prog));
    assert!(render("strict", prog).unwrap_err().contains("rand"));
}

#[test]
fn writes_to_outside_data_are_refused() {
    let prog =
        "/T [1 2 3] def /pixel { /j exch def /i exch def T 0 i put i W div dup dup } bind def";
    let (ok, why) = compiles(prog);
    assert!(!ok, "{why}");
    assert_eq!(render("auto", prog), render("interpret", prog));
}

#[test]
fn wrong_output_count_is_the_interpreters_error() {
    let prog = "/pixel { /j exch def /i exch def i W div dup } bind def";
    assert!(!compiles(prog).0);
    assert_eq!(render("auto", prog), render("interpret", prog));
    assert!(render("interpret", prog).is_err());
}

#[test]
fn compiles_reports_size() {
    let (ok, why) = compiles("/pixel { pop W div dup dup } bind def");
    assert!(ok, "{why}");
    assert!(why.contains("instructions"), "{why}");
}

#[test]
fn a_runaway_kernel_loop_stops_at_the_time_limit() {
    let mut it = Interp::with_page(64, 64).expect("page");
    it.set_limits(Limits::none().with_max_time(Duration::from_millis(300)))
        .expect("limits");
    let src = "/pixel { pop pop 0 { 1 add } loop } bind def \
               64 64 [64 0 0 -64 0 64] /pixel load 1 kernelimage";
    let err = it.run_str(src).unwrap_err();
    assert_eq!(err, PsError::TimeExceeded);
}

#[test]
fn a_kernel_is_bounded_by_the_step_limit() {
    let mut it = Interp::with_page(64, 64).expect("page");
    it.set_limits(Limits::none().with_max_steps(5_000_000))
        .expect("limits");
    let src = "/pixel { pop pop 0 { 1 add } loop } bind def \
               64 64 [64 0 0 -64 0 64] /pixel load 1 kernelimage";
    assert_eq!(it.run_str(src).unwrap_err(), PsError::StepsExceeded);
}

// --- differential fuzz ----------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// A random expression pushing one number. `vars` are names known to
/// hold numbers here; `depth` bounds the size.
fn expr(r: &mut Rng, depth: u32, vars: &[String]) -> String {
    if depth == 0 || r.below(5) == 0 {
        return match r.below(6) {
            0 => "i".into(),
            1 => "j".into(),
            2 => format!("{}", r.below(19) as i64 - 9),
            3 => format!("{:.3}", r.below(4000) as f64 / 400.0 - 5.0),
            _ if !vars.is_empty() => vars[r.below(vars.len() as u64) as usize].clone(),
            _ => "i".into(),
        };
    }
    let d = depth - 1;
    macro_rules! e {
        () => {
            expr(r, d, vars)
        };
    }
    match r.below(17) {
        0 => format!("{} {} add", e!(), e!()),
        1 => format!("{} {} sub", e!(), e!()),
        2 => format!("{} {} mul", e!(), e!()),
        3 => format!("{} {} div", e!(), e!()),
        4 => format!(
            "{} {} {}",
            e!(),
            ["abs", "neg", "floor", "round", "truncate", "ceiling", "cvr"][r.below(7) as usize],
            ""
        ),
        5 => format!("{} abs sqrt", e!()),
        6 => format!("{} {}", e!(), ["sin", "cos"][r.below(2) as usize]),
        7 => format!("{} {} atan", e!(), e!()),
        8 => format!(
            "{} cvi {} cvi dup 0 eq {{ pop 3 }} if {}",
            e!(),
            e!(),
            ["idiv", "mod"][r.below(2) as usize]
        ),
        9 => format!(
            "{} {} {} {{ {} }} {{ {} }} ifelse",
            e!(),
            e!(),
            ["lt", "le", "gt", "ge", "eq", "ne"][r.below(6) as usize],
            e!(),
            e!()
        ),
        10 => format!("{} dup 0 lt {{ pop 0 }} if", e!()),
        11 => format!("0 1 1 {} {{ {} mul add }} for", r.below(5), e!()),
        12 => format!("{} {} {{ 0.5 mul 1 add }} repeat", e!(), r.below(4)),
        13 => format!(
            "{} abs 1 add {{ dup 50 gt {{ exit }} if 1.7 mul 0.3 add }} loop",
            e!()
        ),
        14 => format!("Tbl {} cvi 8 mod abs get", e!()),
        15 => format!("{} 0.1 mul {} 0.1 mul gnoise2", e!(), e!()),
        _ => format!("{} {} {}", e!(), e!(), ["min", "max"][r.below(2) as usize]),
    }
}

fn program(r: &mut Rng) -> String {
    let mut body = String::from("/j exch def /i exch def\n");
    let mut vars: Vec<String> = Vec::new();
    for k in 0..r.below(4) {
        let e = expr(r, 3, &vars);
        let name = format!("v{k}");
        if r.below(3) == 0 {
            body += &format!(
                "{} 0 gt {{ /{name} {e} def }} {{ /{name} {} def }} ifelse\n",
                expr(r, 2, &vars),
                expr(r, 2, &vars)
            );
        } else {
            body += &format!("/{name} {e} def\n");
        }
        vars.push(name);
    }
    body += &format!("{}\n", expr(r, 4, &vars));
    format!(
        "{SHOW} 5 noiseseed /Tbl [ 3 1.5 -2 7 0.25 9 4 4.5 ] def /pixel {{ {body} show3 }} bind def"
    )
}

#[test]
fn differential_fuzz() {
    let cases = if cfg!(debug_assertions) { 120 } else { 400 };
    let mut r = Rng(0x5eed_0742);
    let (mut compiled, mut errors, mut ran) = (0, 0, 0);
    for n in 0..cases {
        let prog = program(&mut r);
        let reference = render_sized("interpret", &prog, 33, 5);
        let (k, ran_compiled) = render_counted("strict", &prog, 33, 5);
        ran += usize::from(ran_compiled);
        if let Err(s) = &k
            && s.starts_with("kernelimage: not compiled")
        {
            panic!("case {n} did not compile: {s}\n{prog}");
        }
        compiled += 1;
        if reference.is_err() {
            errors += 1;
        }
        assert!(
            k == reference,
            "case {n}: compiled {:?} vs interpreted {:?}\n{prog}",
            k.as_ref().err(),
            reference.as_ref().err()
        );
    }
    assert!(compiled == cases);
    // Most cases must really be decided by the compiled path: a kernel
    // that aborts to the interpreter compares equal trivially.
    assert!(
        ran >= cases - errors,
        "only {ran} of {cases} images came from the compiled path ({errors} errored)"
    );
    // The generator must not be all errors (or all successes).
    assert!(errors < cases / 2, "{errors} of {cases} cases errored");
}
