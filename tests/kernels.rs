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
    // Finished by the compiled path, possibly with some pixels (an integer
    // overflow the interpreter turns into a real, say) left to the
    // interpreter: either way the kernel produced the image's pixels.
    let st = it.operand_stack();
    let ran_compiled = st[0].repr() == "1" || st[1].repr() == "1";
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

/// Assert `prog` compiles and matches the interpreter, some pixels of it
/// possibly finished by the interpreter.
#[track_caller]
fn same_or_partial(prog: &str) {
    let k = render("strict", prog);
    let r = render("interpret", prog);
    assert!(r.is_ok(), "the reference failed: {r:?}");
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
           i 2 div 17 sub round 3 mul add          % PostScript rounds -0.5 up to 0
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
fn integer_hashes_are_exact_past_2_pow_53() {
    // fractal_clouds' per-pixel hash: 31-bit products reach ~2^58, exact
    // in the interpreter's i64 (an f64 would lose the low bits).
    same(&format!(
        "{SHOW} /pixel {{ /j exch def /i exch def
           i j 16 bitshift add 16#2545F491 xor
           dup 13 bitshift xor 16#7fffffff and
           dup -17 bitshift xor
           dup 5 bitshift xor 16#7fffffff and
           16#45D9F3B mul 16#7fffffff and dup -15 bitshift xor
           16#45D9F3B mul 16#7fffffff and dup -16 bitshift xor
           dup 255 and 255 div exch -8 bitshift 255 and 0.5 mul add
           i 16#7fffff mul 16#7fffffff mul j add 1000 mod add     % ~2^60
           show3 }} bind def"
    ));
}

#[test]
fn integer_overflow_promotes_like_the_interpreter() {
    // Past i64 the interpreter's product is a real: the lanes that overflow
    // run in the interpreter, the rest stay compiled.
    same_or_partial(&format!(
        "{SHOW} /pixel {{ /j exch def /i exch def
           i 1 add 16#7fffffffffff mul 16#7fffff mul 1e-30 mul
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
    match r.below(21) {
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
        // Integer hashing, the procedural-art staple: products past 2^53
        // (exact in the interpreter's i64), masks, shifts and xors.
        17 => format!("{} cvi 16#45D9F3B mul 16#7fffffff and", e!()),
        18 => format!(
            "{} cvi dup 13 bitshift xor dup -17 bitshift xor 1000003 mul 255 and 255 div",
            e!()
        ),
        19 => format!(
            "{} cvi 16#2545F491 mul {} cvi 1 or {}",
            e!(),
            e!(),
            ["idiv", "mod"][r.below(2) as usize]
        ),
        _ => format!("{} {} {}", e!(), e!(), ["min", "max"][r.below(2) as usize]),
    }
}

fn program(r: &mut Rng) -> String {
    program_with(r, false)
}

/// A random program; `seeded`, one that seeds its random stream from the
/// pixel (issue #1063) and draws from it among the expressions' leaves.
fn program_with(r: &mut Rng, seeded: bool) -> String {
    let mut body = String::from("/j exch def /i exch def\n");
    let mut vars: Vec<String> = Vec::new();
    if seeded {
        body += &match r.below(4) {
            0 => format!("{} cvi srand\n", expr(r, 2, &vars)),
            1 => format!(
                "{} 0 gt {{ i j hash srand }} {{ j 7 mul i add srand }} ifelse\n",
                expr(r, 2, &vars)
            ),
            _ => "i j hash srand\n".into(),
        };
        vars.push("rand 1000 mod".into());
        vars.push("rand 16#7fffffff div".into());
    }
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
        if !ran_compiled && reference.is_ok() && std::env::var_os("KFUZZ_DEBUG").is_some() {
            eprintln!("FELLBACK case {n}:\n{prog}");
        }
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

// --- threads and bands (CLI) ----------------------------------------------

mod common;

const BIN: &str = env!("CARGO_BIN_EXE_pscat");

/// Render `src` with the CLI: the PNG bytes.
fn cli_png(src: &str, args: &[&str], threads: Option<&str>) -> Vec<u8> {
    let dir = common::scratch_dir("kernels");
    let ps = dir.join("k.ps");
    let png = dir.join("k.png");
    std::fs::write(&ps, src).expect("write");
    let mut cmd = std::process::Command::new(BIN);
    cmd.args(["--page", "200x130", "--png"])
        .arg(&png)
        .args(args)
        .arg(&ps);
    match threads {
        Some(t) => cmd.env("PSCAT_KERNEL_THREADS", t),
        None => cmd.env_remove("PSCAT_KERNEL_THREADS"),
    };
    let out = common::wait_output(
        cmd.stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn"),
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::read(&png).expect("png")
}

const SCENE: &str = "%!PS
/pixel { /j exch def /i exch def
  /cr i 100 sub 60 div 0.5 sub def /ci j 65 sub 60 div def
  /zr 0.0 def /zi 0.0 def /n 0 def
  { zr zr mul zi zi mul add 4 gt n 60 ge or { exit } if
    zr zr mul zi zi mul sub cr add zr zi mul 2 mul ci add /zi exch def /zr exch def
    /n n 1 add def } loop
  n 60 div dup dup mul i 200 div } bind def
gsave 15 10 translate 5 rotate 180 110 scale
200 130 [200 0 0 -130 0 130] /pixel load 3 kernelimage grestore showpage
";

#[test]
fn bytes_do_not_depend_on_threads_or_bands() {
    let one = cli_png(SCENE, &[], Some("1"));
    assert_eq!(cli_png(SCENE, &[], Some("7")), one, "7 threads");
    assert_eq!(cli_png(SCENE, &[], None), one, "all cores");
    for n in ["2", "3", "8"] {
        assert_eq!(cli_png(SCENE, &["--bands", n], None), one, "--bands {n}");
    }
    // And the interpreter's own render of the same program.
    let interp = format!("/interpret .kernelmode\n{SCENE}");
    assert_eq!(cli_png(&interp, &[], None), one, "interpreted");
}

// --- regressions from the Codex adversarial review ---------------------------

/// Run `src` on a fresh 4x4 page in `mode`: Ok(page bytes) or the error.
fn run_mode(mode: &str, src: &str, limits: Option<Limits>) -> Result<Vec<u8>, String> {
    let mut it = Interp::with_page(4, 4).expect("page");
    if let Some(l) = limits {
        it.set_limits(l).expect("limits");
    }
    // The image's unit square fills the 4x4 page, so every sample shows.
    match it.run_str(&format!("/{mode} .kernelmode 4 4 scale\n{src}\nshowpage")) {
        Ok(()) => Ok(it.pages()[0].rgba8()),
        Err(e) => Err(e.name().to_string()),
    }
}

#[track_caller]
fn auto_matches_interpreter(src: &str) {
    assert_eq!(
        run_mode("auto", src, None),
        run_mode("interpret", src, None),
        "{src}"
    );
}

#[test]
fn a_name_alias_cycle_is_refused_not_a_crash() {
    let mut it = Interp::new();
    it.run_str("/a /a cvx def { pop pop a 0 } 1 kernelcompile")
        .expect("kernelcompile");
    let st = it.operand_stack();
    assert_eq!(st[0].repr(), "false");
    assert!(st[1].repr().contains("cycle"), "{}", st[1].repr());
}

#[test]
fn huge_dimensions_are_a_limitcheck_not_a_panic() {
    let err = run_mode(
        "auto",
        "4611686018427387904 1 [1 0 0 1 0 0] {pop pop 0 0 0 0} 4 kernelimage",
        None,
    );
    assert_eq!(err, Err("limitcheck".into()));
}

#[test]
fn exponential_inlining_stops_at_the_time_limit() {
    let mut src = String::from("/p0 {} def\n");
    for n in 1..=30 {
        src += &format!("/p{n} {{ p{} p{} }} def\n", n - 1, n - 1);
    }
    src += "1 1 [1 0 0 1 0 0] { pop pop p30 0 } 1 kernelimage";
    let limits = Limits::none().with_max_time(Duration::from_millis(200));
    let (r, cpu) = common::cpu_time_of(|| run_mode("auto", &src, Some(limits)));
    assert!(r.is_err(), "{r:?}");
    assert!(
        cpu < common::scale_budget(Duration::from_secs(5)),
        "took {cpu:?}"
    );
}

#[test]
fn worker_storage_respects_the_memory_limit() {
    // Sixteen local arrays a pixel: the compiled run's storage (on every
    // worker) would pass a small memory cap, so the interpreter does the
    // image instead, and the result is the interpreter's.
    let body =
        "4 4 [4 0 0 -4 0 4] { pop pop 16 { 4096 array pop } repeat 0.5 } 1 kernelimage showpage";
    let run = |mode: &str| {
        let dir = common::scratch_dir("kmem");
        let ps = dir.join("m.ps");
        let png = dir.join("m.png");
        std::fs::write(&ps, format!("/{mode} .kernelmode {body}")).expect("write");
        let out = common::wait_output(
            std::process::Command::new(BIN)
                .args(["--page", "4x4", "--max-memory", "6", "--png"])
                .arg(&png)
                .arg(&ps)
                .env("PSCAT_KERNEL_THREADS", "4")
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("spawn"),
        );
        (out.status.success(), std::fs::read(&png).ok())
    };
    let (ok, png) = run("auto");
    assert!(ok, "the compiled path must fall back, not fail");
    assert_eq!((ok, png), run("interpret"));
}

#[test]
fn side_effects_in_uncompiled_branches_keep_the_interpreters_result() {
    auto_matches_interpreter(
        "/X 0 def 2 1 [2 0 0 -1 0 1] \
         { pop /i exch def i 0 eq { /X 1 store } if i 1 eq X 1 eq and { 1 0 div } { 0 } ifelse } \
         1 kernelimage",
    );
}

#[test]
fn integers_beside_mixed_values_stay_exact() {
    // At i = 0, 2^53 + 1 (an exact integer) against a value that is the
    // integer 2^53 on this lane and a real elsewhere: eq is false and the
    // difference is 1 in the interpreter. Rounding 2^53 + 1 to an f64 would
    // make them equal.
    let src = "4 4 [4 0 0 -4 0 4] { pop /i exch def \
           /a i 1 add 53 bitshift 1 add def \
           /b i 0 eq { 9007199254740992 } { 0.0 } ifelse def \
           a b eq { 0.9 } { 0.1 } ifelse a b sub 0 gt { 0.5 } { 0.0 } ifelse add 0.5 mul } \
         1 kernelimage";
    let interp = run_mode("interpret", src, None);
    assert!(interp.is_ok(), "{interp:?}");
    assert_eq!(run_mode("auto", src, None), interp);
    // And the compiled path really ran (retiring the inexact lane).
    let mut it = Interp::with_page(4, 4).expect("page");
    it.run_str(&format!("{src} .kernelstats")).expect("run");
    let st = it.operand_stack();
    assert!(
        st[st.len() - 3].repr() == "1" || st[st.len() - 2].repr() == "1",
        "not compiled"
    );
}

#[test]
fn a_discarded_table_row_is_still_range_checked() {
    let src = "/T [[1 2]] def 1 1 [1 0 0 -1 0 1] { /j exch def /i exch def T i 1 add get pop 0 } 1 kernelimage";
    assert_eq!(run_mode("strict", src, None), Err("rangecheck".into()));
    auto_matches_interpreter(src);
}

#[test]
fn bind_and_count_inside_a_kernel_are_left_to_the_interpreter() {
    auto_matches_interpreter(
        "1 1 [1 0 0 -1 0 1] { pop pop /p { 0 0 add } bind def /add { pop pop 1 } def \
           p 1 eq { 0 } { 1 0 div } ifelse } 1 kernelimage",
    );
    auto_matches_interpreter(
        "1 1 [1 0 0 -1 0 1] { pop pop count 0 eq { 0 } { 1 0 div } ifelse } 1 kernelimage",
    );
}

// --- regressions from the second Codex review ----------------------------------

#[test]
fn copy_cannot_grow_the_compile_time_stack_without_bound() {
    // Doubling with `n copy` sixteen times: the compiler refuses it (the
    // interpreter then runs it and meets the real limits) instead of
    // building a huge symbolic stack.
    let mut body = String::from("[ 0 ");
    let mut n = 1;
    for _ in 0..16 {
        body += &format!("{n} copy ");
        n *= 2;
    }
    body += "] pop 0";
    let mut it = Interp::new();
    it.run_str(&format!("{{ pop pop {body} }} 1 kernelcompile"))
        .expect("kernelcompile");
    let st = it.operand_stack();
    assert_eq!(st[0].repr(), "false", "{}", st[1].repr());
}

#[test]
fn a_merged_array_keeps_its_aliases_or_is_not_compiled() {
    auto_matches_interpreter(
        "2 1 [2 0 0 -1 0 1] { pop /i exch def /A [0] def /B A def \
           i 1 eq { /A [1] def } if A 0 1 put B 0 get } 1 kernelimage",
    );
}

#[test]
fn table_rows_compare_by_identity_at_run_time() {
    auto_matches_interpreter(
        "/T [[1]] def 1 1 [1 0 0 -1 0 1] { pop /i exch def \
           T i get T i 0 add get eq { 1 } { 0 } ifelse } 1 kernelimage",
    );
}

#[test]
fn operations_on_null_or_string_rows_error_like_the_interpreter() {
    let null_len =
        "/T [null [1]] def 2 1 [2 0 0 -1 0 1] { pop T exch get length 10 div } 1 kernelimage";
    assert_eq!(
        run_mode("auto", null_len, None),
        run_mode("interpret", null_len, None)
    );
    assert!(run_mode("interpret", null_len, None).is_err());
    let str_aload =
        "/T [(a) (b)] def 2 1 [2 0 0 -1 0 1] { pop T exch get aload pop 255 div } 1 kernelimage";
    assert_eq!(
        run_mode("auto", str_aload, None),
        run_mode("interpret", str_aload, None)
    );
    assert!(run_mode("interpret", str_aload, None).is_err());
}

#[test]
fn compiling_charges_steps_every_time() {
    // Twenty probes of a procedure that inlines 2^7 copies of a leaf: each
    // compile is charged, so a small step budget runs out.
    let mut src = String::from("/p0 { 1 pop } def\n");
    for n in 1..=7 {
        src += &format!("/p{n} {{ p{} p{} }} def\n", n - 1, n - 1);
    }
    src += "20 { { pop pop p7 0 } 1 kernelcompile pop pop } repeat";
    let mut it = Interp::new();
    it.set_limits(Limits::none().with_max_steps(1000))
        .expect("limits");
    assert_eq!(it.run_str(&src).unwrap_err(), PsError::StepsExceeded);
}

#[test]
fn the_interpreter_fallback_checks_its_row_buffer_against_the_memory_limit() {
    // A 100-megasample row: the interpreter driver's row string must be
    // refused by --max-memory, not allocated.
    let dir = common::scratch_dir("kfallmem");
    let ps = dir.join("f.ps");
    std::fs::write(
        &ps,
        "/interpret .kernelmode 100000000 1 [100000000 0 0 -1 0 1] { pop pop 0 } 1 kernelimage showpage",
    )
    .expect("write");
    let out = common::wait_output(
        std::process::Command::new(BIN)
            .args(["--page", "4x4", "--max-memory", "50", "--png"])
            .arg(dir.join("f.png"))
            .arg(&ps)
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn"),
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(err.contains("limitexceeded (memory)"), "{err}");
}

// --- large kernels (issue #1064) -------------------------------------------

/// `prog` compiles under `/strict` and matches the interpreter, on a small
/// image (a large kernel's interpreter reference is slow per pixel).
#[track_caller]
fn same_small(prog: &str) {
    let (k, ran) = render_counted("strict", prog, 5, 3);
    let (r, _) = render_counted("interpret", prog, 5, 3);
    assert!(
        !matches!(&k, Err(s) if s.starts_with("kernelimage: not compiled")),
        "did not compile: {k:?}"
    );
    assert!(ran, "the compiled path did not finish the image");
    assert!(r.is_ok(), "the reference failed: {r:?}");
    assert!(k == r, "compiled and interpreted renders differ");
}

#[test]
fn the_size_cap_counts_each_instruction_once() {
    // About 120k instructions, all at the top level of one branch. The
    // count once added a branch's whole code again at its join (and a loop
    // body's at its back edge), so this passed the 200k cap.
    let prog = "/a { x 1.0000001 mul 0.37 add /x exch def } def
                /b { a a a a a a a a a a } def /c { b b b b b b b b b b } def
                /d { c c c c c c c c c c } def /e { d d d d d d d d d d } def
                /pixel { /j exch def /i exch def /x i 0.1 mul def
                  j 1 ge { e e e e e e } if
                  3 { x 1.0001 mul /x exch def } repeat
                  x 0.001 mul dup floor sub j 0.25 mul i 0.2 mul } def";
    let (ok, why) = compiles(prog);
    assert!(ok, "{why}");
    let n: usize = why
        .trim_matches(|c: char| !c.is_ascii_digit())
        .parse()
        .expect("a size");
    assert!((110_000..200_000).contains(&n), "{why}");
    same_small(prog);
}

/// `prog` matches the interpreter with procedure bodies shared as
/// subroutines (`/all`: every named procedure; `/large`: the default rule
/// once a kernel is too large), and inlined.
#[track_caller]
fn same_shared(prog: &str) {
    for mode in ["all", "large", "never"] {
        same(&format!("/{mode} .kernelshare {prog}"));
    }
}

/// `kernelcompile`'s instruction count for `prog` under share `mode`.
fn size_with(mode: &str, prog: &str) -> usize {
    let (ok, why) = compiles(&format!("/{mode} .kernelshare {prog}"));
    assert!(ok, "{why}");
    why.trim_matches('(')
        .split(' ')
        .next()
        .and_then(|n| n.parse().ok())
        .expect("a size")
}

// A procedure that reads its argument, globals of the pixel and the stack
// below it, and leaves a result and some definitions.
const SHADE: &str = "/shade { /sx exch def
      sx 1.3 mul sin sx 0.7 mul cos add k mul
      sx 3 gt { /hi sx def } { /lo sx def } ifelse   % defined on some paths only
      0 1 9 { 0.5 mul sx abs add sqrt add } for
      dup 0 lt { neg } if 0.1 add } def";

#[test]
fn a_procedure_shared_across_many_contexts() {
    same_shared(&format!(
        "{SHOW} {SHADE}
         /pixel {{ /j exch def /i exch def /k 0.25 def
           i shade j shade add                         % two register contexts
           3 shade add 3 shade add                     % a constant context, twice
           /k i 0.01 mul def i j add shade add          % k now a register
           j 4 gt {{ i shade }} {{ j 2 mul shade }} ifelse add    % in branches
           0 1 3 {{ shade add }} for                    % in a loop, the counter
           i 10 mod {{ dup 2.5 gt {{ exit }} if 0.3 add dup shade pop }} loop add
           i 5 add shade pop hi add                     % hi is defined here on every lane
           show3 }} def"
    ));
}

#[test]
fn sharing_makes_a_kernel_with_repeated_calls_smaller() {
    let prog = format!(
        "{SHADE} /pixel {{ /j exch def /i exch def /k 0.5 def
           i shade j shade i j add shade i j sub shade i j mul shade
           i 2 mul shade j 3 mul shade i j 2 mul add shade
           add add add add add add add 0.01 mul dup dup }} def"
    );
    let (inl, shared) = (size_with("never", &prog), size_with("large", &prog));
    assert!(shared * 2 < inl, "inlined {inl}, shared {shared}");
    same_shared(&format!("{SHOW} {}", prog.replace("dup dup }", "show3 }")));
}

#[test]
fn a_shared_procedure_with_its_own_exit_and_one_that_exits_the_callers_loop() {
    // `seek` exits its own loop: shared. `stop5` exits the loop it is
    // called from: it must stay inline.
    same_shared(&format!(
        "{SHOW} /seek {{ /sv exch def 0 {{ 1 add dup sv gt {{ exit }} if }} loop }} def
         /stop5 {{ dup 5 gt {{ exit }} if }} def
         /pixel {{ /j exch def /i exch def
           i seek j seek add i j add seek add
           0 {{ 1 add stop5 }} loop add
           0 {{ 2 add j 1 add {{ 0.5 add }} repeat stop5 }} loop add
           show3 }} def"
    ));
}

#[test]
fn shared_procedures_on_the_stack_and_in_nested_calls() {
    // `rot3` works on the caller's stack below its own; `outer` calls
    // `inner` several times; `pair` leaves two values; `nothing` leaves
    // only definitions.
    same_shared(&format!(
        "{SHOW} /rot3 {{ 3 1 roll 2 index add }} def
         /inner {{ dup mul 0.5 add }} def
         /outer {{ inner exch inner add 1 index inner mul }} def
         /pair {{ dup 2 mul exch 3 add }} def
         /nothing {{ /n1 i 2 mul def /n2 j def }} def
         /pixel {{ /j exch def /i exch def
           i j 7 rot3 add add
           i j outer j i outer add 0.001 mul add
           i pair mul j pair mul add 0.01 mul add
           nothing nothing n1 n2 add 0.01 mul add
           j 3 gt {{ i 0.5 j outer }} {{ j 0.25 i outer }} ifelse pop add
           show3 }} def"
    ));
}

#[test]
fn shared_procedures_raise_the_interpreters_errors() {
    let p = "/half { dup 0 eq { pop 1 } if 10 exch div } def
             /root { 7 exch sub sqrt } def
             /pixel { /j exch def /i exch def i 3 sub half j half add i root add
                      j root add 0.01 mul dup dup } def";
    for mode in ["all", "large"] {
        same_error(&format!("/{mode} .kernelshare {p}"), "rangecheck");
    }
}

/// A program whose procedures are called from many contexts: with
/// constants and registers, inside branches and loops, nested in each
/// other, reading the caller's names and stack and defining names of
/// their own, some with loops and `exit`s of their own and some exiting
/// the caller's loop.
fn shared_program(r: &mut Rng) -> String {
    let mut defs = String::new();
    let mut procs: Vec<String> = Vec::new();
    for k in 0..3 {
        let name = format!("p{k}");
        // Each takes one number and leaves one; `g` is the caller's.
        let mut vars = vec!["a".to_string(), "g".to_string()];
        let mut body = String::from("/a exch def ");
        if let Some(prev) = procs.last()
            && r.below(2) == 0
        {
            body += &format!("/a a {prev} def ");
        }
        match r.below(5) {
            0 => {
                body += &format!(
                    "a 0 gt {{ /q{k} {} def }} {{ /q{k} {} def }} ifelse ",
                    expr(r, 2, &vars),
                    expr(r, 2, &vars)
                );
                vars.push(format!("q{k}"));
            }
            1 => {
                // Defined on some paths only.
                body += &format!("a 1 gt {{ /h{k} {} def }} if ", expr(r, 2, &vars));
            }
            2 => {
                body += &format!(
                    "/a a abs 1 add {{ dup 20 gt {{ exit }} if {} abs 1 add add }} loop def ",
                    expr(r, 1, &vars)
                );
            }
            3 => body += "/g g 0.5 mul a add def ",
            _ => {}
        }
        body += &expr(r, 3, &vars);
        defs += &format!("/{name} {{ {body} }} def\n");
        procs.push(name);
    }
    defs += "/ex2 { dup 2 gt { exit } if } def\n";
    let mut main = String::from("/j exch def /i exch def /g i j add 0.1 mul def 0\n");
    let vars = vec!["i".to_string(), "j".to_string(), "g".to_string()];
    for _ in 0..6 {
        let p = &procs[r.below(procs.len() as u64) as usize];
        let call = match r.below(7) {
            0 => format!("{} {p}", expr(r, 2, &vars)),
            1 => format!("{} {p}", r.below(9) as i64 - 4),
            2 => format!(
                "{} 0 gt {{ {} {p} }} {{ {} {p} }} ifelse",
                expr(r, 1, &vars),
                expr(r, 1, &vars),
                expr(r, 1, &vars)
            ),
            3 => format!("0 0 1 {} {{ {p} add }} for", r.below(20)),
            4 => format!("0 {{ 1 add dup {p} pop ex2 }} loop"),
            5 => format!("/g {} def g {p} g add", expr(r, 1, &vars)),
            _ => format!("i {p} j {p} 2 index pop add"),
        };
        main += &format!("{call} add\n");
    }
    format!(
        "{SHOW} 5 noiseseed /Tbl [ 3 1.5 -2 7 0.25 9 4 4.5 ] def\n{defs}/pixel {{ {main} show3 }} bind def"
    )
}

#[test]
fn differential_fuzz_shared() {
    let cases = 150;
    let mut r = Rng(0x5eed_1064);
    let (mut ran, mut errors, mut shared) = (0, 0, 0);
    for n in 0..cases {
        let prog = shared_program(&mut r);
        // The sharing must really happen: count the kernels with any.
        let (_, why) = compiles(&format!("/all .kernelshare {prog}"));
        shared += usize::from(why.contains("shared subroutines"));
        let reference = render_sized("interpret", &prog, 33, 5);
        errors += usize::from(reference.is_err());
        for mode in ["all", "large"] {
            let p = format!("/{mode} .kernelshare {prog}");
            let (k, ran_compiled) = render_counted("strict", &p, 33, 5);
            ran += usize::from(ran_compiled);
            if let Err(s) = &k
                && s.starts_with("kernelimage: not compiled")
            {
                // Outside the subset either way (a type change across a
                // loop, say): inlined, it must not compile either.
                let (inl, _) =
                    render_counted("strict", &format!("/never .kernelshare {prog}"), 33, 5);
                assert!(
                    matches!(&inl, Err(t) if t.starts_with("kernelimage: not compiled")),
                    "case {n} /{mode}: compiles only inlined: {s}\n{prog}"
                );
                continue;
            }
            assert!(
                k == reference,
                "case {n} /{mode}: compiled {:?} vs interpreted {:?}\n{prog}",
                k.as_ref().err(),
                reference.as_ref().err()
            );
        }
    }
    assert!(errors < cases / 2, "{errors} of {cases} cases errored");
    assert!(
        ran >= cases,
        "only {ran} of {} images came from the compiled path",
        2 * cases
    );
    assert!(
        shared >= cases / 2,
        "only {shared} of {cases} kernels shared code"
    );
}

#[test]
fn unused_results_of_operations_that_can_fail_still_fail() {
    // Dead-code elimination drops unused pure results only: a division by
    // zero or a negative sqrt whose value is popped is still the error.
    same_error(
        "/pixel { /j exch def /i exch def 1 i 3 sub div pop i j add 0.01 mul dup dup } def",
        "undefinedresult",
    );
    same_error(
        "/pixel { /j exch def /i exch def i 50 sub sqrt pop i 0.01 mul dup dup } def",
        "rangecheck",
    );
    // And unused pure results go without changing the picture.
    same(
        "/pixel { /j exch def /i exch def i j mul pop i 3 add /u exch def
           j 2 gt { /w i def } { /w j def } ifelse
           i 0.013 mul dup j 0.1 mul } def",
    );
}

// --- per-pixel seeded rand/srand (issue #1063) -------------------------------

/// fractal_clouds' `PixSeed` shape: a seed from the pixel's position.
const HASH: &str = "/hash { 16#45D9F3B mul exch 16#2545F491 mul xor 16#7fffffff and } bind def";

/// What a render of `prog` leaves: the page (or error), the random stream
/// after the `kernelimage` (three `rand`s and an `rrand`), and whether a
/// seed override fired.
#[derive(Debug, PartialEq)]
struct RandRun {
    page: Result<Vec<u8>, String>,
    after: Vec<String>,
    fired: bool,
}

/// Render `prog` in `mode` under `seed` (as `--sweep-seed`): what it
/// leaves, and the `.kernelstats` triple.
fn rand_run(mode: &str, prog: &str, seed: Option<i64>) -> (RandRun, [i64; 3]) {
    rand_run_sized(mode, prog, seed, W, H)
}

fn rand_run_sized(
    mode: &str,
    prog: &str,
    seed: Option<i64>,
    w: u32,
    h: u32,
) -> (RandRun, [i64; 3]) {
    let mut it = Interp::with_page(w, h).expect("page");
    it.set_seed_override(seed);
    let src = format!(
        "/{mode} .kernelmode\n/W {w} def /H {h} def\n{HASH}\n{prog}\n\
         gsave W H scale W H [W 0 0 H neg 0 H] /pixel load 3 kernelimage grestore showpage"
    );
    let page = match it.run_str(&src) {
        Ok(()) => Ok(it.pages()[0].rgba8()),
        Err(e) => Err(match &e {
            PsError::Undefined(s) if s.starts_with("kernelimage: not compiled") => s.clone(),
            other => other.name().to_string(),
        }),
    };
    let fired = it.seed_override_fired();
    it.run_str("clear rand rand rand rrand")
        .expect("rand after");
    let after = it.operand_stack().iter().map(|o| o.repr()).collect();
    it.run_str("clear .kernelstats").expect("stats");
    let st = it.operand_stack();
    let n = |k: usize| st[k].repr().parse::<i64>().expect("stat");
    (RandRun { page, after, fired }, [n(0), n(1), n(2)])
}

/// `prog` compiles, the compiled path finishes the image, and the image,
/// the random stream after it and the override's `fired` all match the
/// interpreter's.
#[track_caller]
fn same_rand(prog: &str, seed: Option<i64>) {
    let (k, stats) = rand_run("strict", prog, seed);
    let (r, _) = rand_run("interpret", prog, seed);
    assert!(r.page.is_ok(), "the reference failed: {:?}\n{prog}", r.page);
    assert_eq!(
        stats[0],
        1,
        "not finished by the compiled path ({stats:?}): {:?}\n{prog}",
        k.page.as_ref().err()
    );
    assert!(
        k == r,
        "compiled {:?} {:?} {} vs interpreted {:?} {}\n{prog}",
        k.page.as_ref().err(),
        k.after,
        k.fired,
        r.after,
        r.fired
    );
}

/// As `same_rand`, but the image may go to the interpreter (`/auto`);
/// returns the compiled run's `.kernelstats`.
#[track_caller]
fn same_rand_auto(prog: &str, seed: Option<i64>) -> [i64; 3] {
    let (k, stats) = rand_run("auto", prog, seed);
    let (r, _) = rand_run("interpret", prog, seed);
    assert!(
        k == r,
        "auto {:?} {:?} {} vs interpreted {:?} {:?} {}\n{prog}",
        k.page.as_ref().err(),
        k.after,
        k.fired,
        r.page.as_ref().err(),
        r.after,
        r.fired
    );
    stats
}

const SEEDED: &str = "/pixel { /j exch def /i exch def i j hash srand
   rand 255 and 255 div rand 16#7fffffff div rand 1000 mod 1000 div } bind def";

#[test]
fn a_pixel_seeded_stream_compiles_and_matches() {
    for seed in [None, Some(42), Some(-7)] {
        same_rand(SEEDED, seed);
        // Drawn in a loop, in a dict scope, and through rrand.
        same_rand(
            "/pixel { 8 dict begin /j exch def /i exch def i j hash srand
               0 1 1 4 { pop rand 7 mod add } for 30 div
               rrand 255 and 255 div
               0 { rand dup 16#40000000 lt { pop exit } if pop 1 add } loop 10 div end } bind def",
            seed,
        );
        // Seeded inside a loop, with no srand before it.
        same_rand(
            "/pixel { /j exch def /i exch def 0 0 1 2 { i add j hash srand rand 3 mod add } for 6 div
               rand 255 and 255 div 0.5 } bind def",
            seed,
        );
    }
    // Reseeded from an rrand of its own stream (the save/restore idiom; a
    // restore under an override retires to the interpreter, below).
    same_rand(
        "/pixel { /j exch def /i exch def i j hash srand rrand rand pop srand
           rand 255 and 255 div 0 0 } bind def",
        None,
    );
}

#[test]
fn the_state_after_the_call_is_the_last_seeding_pixels() {
    for seed in [None, Some(5)] {
        // Only some pixels seed: the last one in raster order is mid-row,
        // and the last rows do not seed at all.
        same_rand(
            "/pixel { /j exch def /i exch def i 40 lt j 5 lt and
               { i j hash srand rand 255 and 255 div } { 0 } ifelse 0 0 } bind def",
            seed,
        );
        // No pixel seeds (the branch is never taken): the state is
        // untouched, and an override does not fire.
        same_rand(
            "/pixel { /j exch def /i exch def i 1000 gt { 9 srand } if 0 0 0 } bind def",
            seed,
        );
        // The last pixel seeds, then draws: the state is its last draw.
        same_rand(
            "/pixel { /j exch def /i exch def i j hash srand rand rand pop 255 and 255 div 0 0 } bind def",
            seed,
        );
    }
}

#[test]
fn rand_without_a_prior_srand_matches_through_the_interpreter() {
    for seed in [None, Some(3)] {
        // Never seeded: refused, so the interpreter does the image.
        let stats = same_rand_auto("/pixel { pop pop rand 255 and 255 div 0 0 } bind def", seed);
        assert_eq!(stats, [0, 0, 1], "never seeded");
        // Seeded on some lanes only: the others retire, and the whole image
        // goes to the interpreter (a retired pixel's rand reads the state
        // its compiled neighbour left).
        let stats = same_rand_auto(
            "/pixel { /j exch def /i exch def i 3 mod 0 eq { i j hash srand } if
               rand 255 and 255 div 0 0 } bind def",
            seed,
        );
        assert_eq!(stats, [0, 0, 1], "seeded on some paths");
        // A branch that draws before seeding, taken by a few pixels.
        same_rand_auto(
            "/pixel { /j exch def /i exch def i 60 gt { rand pop } if i j hash srand
               rand 255 and 255 div 0 0 } bind def",
            seed,
        );
        // Under /strict, a never-seeded rand is not compiled.
        let (k, _) = rand_run("strict", "/pixel { pop pop rand 0 0 } bind def", seed);
        assert!(
            matches!(&k.page, Err(s) if s.contains("rand before srand")),
            "{:?}",
            k.page
        );
    }
}

#[test]
fn seed_override_and_tagged_seeds_match() {
    // An rrand-style tagged seed (2^40 + n, past 2^31): plain, it seeds n;
    // under an override it is a restore, which retires to the interpreter.
    let tagged = "/pixel { /j exch def /i exch def 16#10000000000 i j hash add srand
       rand 255 and 255 div 0 0 } bind def";
    same_rand(tagged, None);
    let stats = same_rand_auto(tagged, Some(11));
    assert_eq!(stats, [0, 0, 1], "a restore under an override");
    // Past 2^53, and negative: masked, as srand does.
    for seed in [None, Some(11)] {
        same_rand(
            "/pixel { /j exch def /i exch def 16#40000000 dup mul i j hash add srand rand 255 and 255 div
               i j hash neg srand rand 255 and 255 div 0 } bind def",
            seed,
        );
    }
    // A real seed is the interpreter's typecheck, overridden or not.
    for seed in [None, Some(1)] {
        let prog = "/pixel { pop pop 1.5 srand 0 0 0 } bind def";
        let (k, _) = rand_run("strict", prog, seed);
        let (r, _) = rand_run("interpret", prog, seed);
        assert_eq!(r.page, Err("typecheck".into()));
        assert_eq!(k, r);
    }
}

const RAND_SCENE: &str = "%!PS
/hash { 16#45D9F3B mul exch 16#2545F491 mul xor 16#7fffffff and } bind def
/pixel { /j exch def /i exch def i j hash srand
  rand 255 and 255 div rand 255 and 255 div j 40 lt { rand 255 and 255 div } { 0.5 } ifelse } bind def
gsave 20 70 translate 120 50 scale
80 50 [80 0 0 -50 0 50] /pixel load 3 kernelimage grestore
% The random state after the call, as full-height stripes every band paints.
0 10 190 { 0 moveto 10 0 rlineto 0 130 rlineto -10 0 rlineto closepath
  rand 255 and 255 div rand 255 and 255 div rand 255 and 255 div setrgbcolor fill } for
showpage
";

#[test]
fn the_state_after_the_call_does_not_depend_on_threads_or_bands() {
    // The image covers part of the page, so some bands have none of its
    // rows; each band must still end with the last pixel's state.
    let interp = format!("/interpret .kernelmode\n{RAND_SCENE}");
    let reference = cli_png(&interp, &[], None);
    assert_eq!(cli_png(RAND_SCENE, &[], Some("1")), reference, "1 thread");
    assert_eq!(cli_png(RAND_SCENE, &[], None), reference, "all cores");
    for n in ["2", "3", "8"] {
        assert_eq!(
            cli_png(RAND_SCENE, &["--bands", n], None),
            reference,
            "--bands {n}"
        );
        assert_eq!(
            cli_png(&interp, &["--bands", n], None),
            reference,
            "interpreted, --bands {n}"
        );
    }
}

#[test]
fn differential_fuzz_seeded() {
    // Random programs that seed per pixel and draw: the image, the stream
    // after the call and the override's `fired` match the interpreter's.
    let cases = if cfg!(debug_assertions) { 80 } else { 300 };
    let mut r = Rng(0x5eed_1063);
    let (mut ran, mut errors) = (0, 0);
    for n in 0..cases {
        let prog = program_with(&mut r, true);
        let seed = (n % 3 == 0).then_some(n as i64);
        let (reference, _) = rand_run_sized("interpret", &prog, seed, 33, 5);
        let (k, stats) = rand_run_sized("strict", &prog, seed, 33, 5);
        if let Err(s) = &k.page
            && s.starts_with("kernelimage: not compiled")
        {
            panic!("case {n} did not compile: {s}\n{prog}");
        }
        ran += usize::from(stats[0] == 1);
        errors += usize::from(reference.page.is_err());
        assert!(
            k == reference,
            "case {n}: compiled {:?} {:?} {} vs interpreted {:?} {:?} {}\n{prog}",
            k.page.as_ref().err(),
            k.after,
            k.fired,
            reference.page.as_ref().err(),
            reference.after,
            reference.fired
        );
    }
    // A retired pixel sends a seeded image wholly to the interpreter, so
    // not every case runs compiled; most must (over 90% when written).
    assert!(
        ran >= (cases - errors) * 3 / 4,
        "only {ran} of {cases} images came from the compiled path ({errors} errored)"
    );
    assert!(errors < cases / 2, "{errors} of {cases} cases errored");
}

#[test]
fn a_shared_procedure_drawing_random_numbers() {
    // `draw` is shared from a seeded state, and called again where the
    // state is seeded on some lanes only: there `rand` must retire the
    // lanes that never seeded (and the image goes to the interpreter), so
    // the seeded call's code may not serve it.
    let draw = "/draw { rand 255 and 255 div rand 1000 mod 1000 div add } def";
    for mode in ["all", "large"] {
        same_rand(
            &format!(
                "/{mode} .kernelshare {draw} /pixel {{ /j exch def /i exch def i j hash srand
                   draw draw j 2 gt {{ draw }} {{ 0.5 }} ifelse 0.25 mul }} bind def"
            ),
            None,
        );
        same_rand_auto(
            &format!(
                "/{mode} .kernelshare {draw} /pixel {{ /j exch def /i exch def
                   j 2 gt {{ i j hash srand draw }} {{ i 3 gt {{ i srand }} if draw }} ifelse
                   0.5 mul dup dup }} bind def"
            ),
            None,
        );
    }
}

#[test]
fn a_procedure_that_defines_its_scratch_names_is_shared_once() {
    // `/t exch def` does not read what it replaces: the first call (t not
    // yet defined) and the later ones (t from the call before) are the
    // same context, so one subroutine serves all four.
    let prog = "/sq { /t exch def t t mul t add sqrt } def
                /pixel { /j exch def /i exch def i sq j sq add i j add sq j 2 mul sq add add
                         t add 0.01 mul dup dup } def";
    let (ok, why) = compiles(&format!("/all .kernelshare {prog}"));
    assert!(ok, "{why}");
    assert!(why.contains(", 1 shared subroutines"), "{why}");
    same_shared(prog);
}

#[test]
fn a_name_defined_on_one_path_and_read_on_another_is_part_of_the_context() {
    // `pick` defines t in one branch and reads the caller's t in the
    // other: the caller's t (a constant at the first call, a register at
    // the second) is an input, so the first call's code cannot serve the
    // second.
    same_shared(&format!(
        "{SHOW} /pick {{ /c exch def c 0 gt {{ /t 2 def t }} {{ t 1 add }} ifelse }} def
         /pixel {{ /j exch def /i exch def
           /t 5 def i 3 sub pick
           /t i 0.5 mul def j 3 sub pick add
           /t 7 def i j sub pick add show3 }} def"
    ));
}

#[test]
fn a_shared_definition_of_the_value_already_there_still_takes_effect() {
    // Found by Codex: `f` first runs where t is already 0, so its `def`
    // changes nothing there; it must still set t at the second call, where
    // t is 1.
    same_shared(&format!(
        "{SHOW} /f {{ /t 0 def }} def
         /pixel {{ /j exch def /i exch def /t 0 def f /t 1 def f t i 0.01 mul add show3 }} def"
    ));
    // The same with a body large enough for `/large`.
    let pad = "i 1.5 mul sin pop ".repeat(30);
    same_shared(&format!(
        "{SHOW} /f {{ {pad} /t 0 def /u i 0.5 mul def }} def
         /pixel {{ /j exch def /i exch def /t 0 def f /t 1 def f /t 2 def f f t u add 0.01 mul show3 }} def"
    ));
}

#[test]
fn a_shared_conditional_definition_of_the_value_already_there() {
    // As above, with the def under a run-time condition.
    let pad = "i 1.5 mul sin pop ".repeat(30);
    for body in ["", pad.as_str()] {
        same_shared(&format!(
            "{SHOW} /f {{ {body} i 3 gt {{ /t 0 def }} if }} def
             /pixel {{ /j exch def /i exch def /t 0 def f /t 1 def f /t j def f
                       t i 0.01 mul add show3 }} def"
        ));
    }
}

#[test]
fn a_shared_definition_that_a_join_cannot_tell_from_none() {
    // Each `f` leaves t, on the paths that define it, as the first call
    // found it: the join after cannot tell def from no def there, so at
    // the later calls (t different) the code made at the first is wrong.
    let pad = "i 1.5 mul sin pop ".repeat(30);
    for (f, setup) in [
        // Defined twice on one path, back to the value it had.
        ("i 3 gt { /t 5 def /t 0 def } if", "/t 0 def"),
        // In a scope begun around the call, over the pixel's own t.
        ("i 3 gt { /t 0 def } if t pop", "/t 0 def 4 dict begin"),
        // In a loop.
        ("0 1 i { pop /t 0 def } for", "/t 0 def"),
        // A global's value, at the first call.
        ("i 3 gt { /G 0 def } if", ""),
    ] {
        let ends = if setup.contains("begin") { "end" } else { "" };
        for body in ["", pad.as_str()] {
            same_shared(&format!(
                "{SHOW} /G 0 def /f {{ {body} {f} }} def
                 /pixel {{ /j exch def /i exch def {setup} f /t 1 def /G 1 def f
                           /t j def /G j def f t G add i 0.01 mul add {ends} show3 }} def"
            ));
        }
    }
}

#[test]
fn an_array_built_after_a_merged_one_is_gone_can_still_be_changed() {
    // Merged `[ ]` arrays are frozen (a later put is refused). The frozen
    // set once held bare addresses, so a new array allocated where a frozen
    // one had been was refused too: by chance, so a kernel's compiled size
    // varied from run to run.
    let mut body = String::new();
    for k in 0..40 {
        body += &format!(
            "i {k} gt {{ [1 2 3] }} {{ [4 5 6] }} ifelse /c exch def /c 0 def
             [7 8 9] dup 0 i put 0 get add\n"
        );
    }
    same(&format!(
        "{SHOW} /pixel {{ /j exch def /i exch def 0 {body} 0.001 mul show3 }} def"
    ));
}
