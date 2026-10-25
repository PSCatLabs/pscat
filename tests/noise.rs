//! Native noise operators (issue #734): determinism, bit-identity with
//! the PostScript reference (`lib/noiseref.ps`), bit-identity of artkit's
//! `noise2` with its old procedure, rand independence, and errors.

use pscat::{Interp, PsError};

fn interp_with(libs: &[&str]) -> Interp {
    let mut it = Interp::new();
    for lib in libs {
        let src = std::fs::read(lib).unwrap_or_else(|e| panic!("{lib}: {e}"));
        it.run_source(&src)
            .unwrap_or_else(|e| panic!("{lib} failed to load: {}", it.error_report(&e)));
    }
    it
}

fn run(it: &mut Interp, src: &str) -> Vec<String> {
    it.run_str(src)
        .unwrap_or_else(|e| panic!("eval of {src:?} failed: {}", it.error_report(&e)));
    let out = it.operand_stack().iter().map(|o| o.repr()).collect();
    it.run_str("clear").unwrap();
    out
}

fn eval(src: &str) -> Vec<String> {
    run(&mut Interp::new(), src)
}

/// Sample points: negatives, near-lattice values on both sides,
/// integers, large magnitudes past the 256-unit wrap, and plain reals.
fn points() -> Vec<[f64; 3]> {
    let mut pts = Vec::new();
    let specials = [
        0.0,
        1.0,
        -1.0,
        0.5,
        -0.5,
        3.999_999_999_999,
        4.000_000_000_001,
        -7.000_000_000_001,
        255.75,
        256.25,
        -256.125,
        1.0e6 + 0.3,
        -98_765.432_1,
    ];
    for (i, &a) in specials.iter().enumerate() {
        let b = specials[(i * 5 + 3) % specials.len()];
        let c = specials[(i * 7 + 1) % specials.len()];
        pts.push([a, b, c]);
    }
    // A deterministic scatter (a simple LCG, not the interpreter's).
    let mut s: u64 = 0x2545_f491_4f6c_dd1d;
    let mut next = || {
        s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        ((s >> 11) as f64 / (1u64 << 53) as f64) * 40.0 - 20.0
    };
    for _ in 0..300 {
        pts.push([next(), next(), next()]);
    }
    pts
}

fn ps_points() -> String {
    let mut s = String::from("/pts [");
    for p in points() {
        s.push_str(&format!(" [{:?} {:?} {:?}]", p[0], p[1], p[2]));
    }
    s.push_str(" ] def\n");
    s
}

/// `a b chk`: count a mismatch unless `a eq b` and the types agree.
const CHECK: &str = "/bad 0 def /nchk 0 def
    /chk { 2 copy eq 3 1 roll type exch type eq and not { /bad bad 1 add def } if
           /nchk nchk 1 add def } def
    /achk { /ab exch def /aa exch def aa length ab length ne { /bad bad 1 add def } if
            0 1 aa length 1 sub { /ai exch def aa ai get ab ai get chk } for } def\n";

#[test]
fn native_operators_match_the_postscript_reference_bit_for_bit() {
    let mut it = interp_with(&["lib/noiseref.ps"]);
    run(&mut it, &(ps_points() + CHECK));
    for seed in [0i64, 7, -3, 123_456_789] {
        let got = run(
            &mut it,
            &format!(
                "{seed} noiseseed {seed} nrseed /bad 0 def /nchk 0 def
                 pts {{ aload pop /z exch def /y exch def /x exch def
                   x y gnoise2 x y nrgnoise2 chk
                   x y vnoise2 x y nrvnoise2 chk
                   x y z gnoise3 x y z nrgnoise3 chk
                   x y z vnoise3 x y z nrvnoise3 chk
                   [x y gnoise2d] [x y nrgnoise2d] achk
                 }} forall
                 [ [0 0.5 2] [1 0.5 2] [4 0.5 2] [6 0.45 2.03] [3 1 3] [2 -0.7 1.5] ] {{
                   aload pop /lac exch def /gain exch def /oct exch def
                   pts 0 40 getinterval {{ aload pop /z exch def /y exch def /x exch def
                     x y oct gain lac fbm2 x y oct gain lac nrfbm2 chk
                     x y z oct gain lac fbm3 x y z oct gain lac nrfbm3 chk
                     x y oct gain lac vfbm2 x y oct gain lac nrvfbm2 chk
                     x y z oct gain lac vfbm3 x y z oct gain lac nrvfbm3 chk
                     x y oct gain lac ridged2 x y oct gain lac nrridged2 chk
                     x y z oct gain lac ridged3 x y z oct gain lac nrridged3 chk
                   }} forall
                 }} forall
                 bad nchk"
            ),
        );
        assert_eq!(
            got[0], "0",
            "seed {seed}: {} of {} samples differ",
            got[0], got[1]
        );
        assert!(
            got[1].parse::<u32>().unwrap() > 2000,
            "too few samples: {}",
            got[1]
        );
    }
}

/// artkit's `noise2` procedure as it was before #734, taken from the
/// file itself and bound to another name.
fn old_noise2_proc() -> String {
    let src = std::fs::read_to_string("lib/artkit.ps").unwrap();
    let start = src.find("\n/noise2 {").expect("noise2's procedure") + 1;
    let len = src[start..].find("} bind def").expect("its end") + "} bind def".len();
    src[start..start + len].replacen("/noise2 {", "/oldnoise2 {", 1)
}

#[test]
fn artkit_noise2_is_bit_identical_to_its_old_procedure() {
    let mut it = interp_with(&["lib/artkit.ps"]);
    assert_eq!(
        run(&mut it, "/noise2 load 1 get"),
        ["--.permnoise2--"],
        "artkit should run noise2 natively on pscat"
    );
    run(&mut it, &(old_noise2_proc() + &ps_points() + CHECK));
    // Integer operands (exact lattice points: the procedure returns
    // `0` or `0.0` depending on the gradients) and mixed int/real.
    let pairs = [
        "0 0",
        "3 4",
        "-3 4",
        "255 -256",
        "1000 7",
        "5 0.25",
        "-0.75 9",
        "-1 -1",
        "16#7fffffff 2",
    ];
    let ints: String = pairs
        .iter()
        .map(|p| format!("{p} noise2 {p} oldnoise2 chk\n"))
        .collect();
    for seed in [1, 5, 9, 31_337] {
        let got = run(
            &mut it,
            &format!(
                "{seed} srand noiseinit /bad 0 def /nchk 0 def
                 pts {{ aload pop pop /y exch def /x exch def
                   x y noise2 x y oldnoise2 chk
                   x 0.02 mul y 0.02 mul noise2 x 0.02 mul y 0.02 mul oldnoise2 chk
                 }} forall
                 {ints}
                 bad nchk"
            ),
        );
        assert_eq!(
            got[0], "0",
            "seed {seed}: {} of {} samples differ",
            got[0], got[1]
        );
    }
    // The integer-lattice results really do come in both types, so the
    // check above is exercising the typing, not just the values.
    let types = run(
        &mut it,
        "5 srand noiseinit [ 0 1 15 { 0 noise2 type } for ] { /realtype eq } forall",
    );
    assert!(types.iter().any(|t| t == "true") && types.iter().any(|t| t == "false"));
}

#[test]
fn artkit_noise2_reads_perm_on_every_call() {
    // A caller's own table in an inner dict wins, as with the procedure.
    let mut it = interp_with(&["lib/artkit.ps"]);
    run(&mut it, &old_noise2_proc());
    let got = run(
        &mut it,
        "3 srand noiseinit 4 dict begin /Perm [ 255 -1 0 { } for ] def
         1.3 2.7 noise2 1.3 2.7 oldnoise2 eq end
         1.3 2.7 noise2 1.3 2.7 oldnoise2 eq",
    );
    assert_eq!(got, ["true", "true"]);
}

#[test]
fn pinned_values_hold_on_every_platform() {
    // Exact reprs: a change here means the algorithm changed, which
    // breaks every seeded render that uses it.
    assert_eq!(
        eval("1.3 2.7 gnoise2 1.3 2.7 0.4 gnoise3 1.3 2.7 vnoise2 1.3 2.7 0.5 vnoise3"),
        [
            "0.28568276281029065",
            "-0.0627403907604478",
            "0.4303962536374999",
            "-0.26076656578750024"
        ]
    );
    assert_eq!(
        eval("0.3 0.6 5 0.5 2 fbm2 0.3 0.6 5 0.5 2 ridged2 7 noiseseed 1.3 2.7 gnoise2"),
        [
            "-0.42842675311827244",
            "1.1799289968579727",
            "-0.236458318191376"
        ]
    );
    assert_eq!(
        eval("1.3 2.7 gnoise2d"),
        [
            "0.28568276281029065",
            "1.315410716647",
            "0.5754354505289996"
        ]
    );
    // An unseeded interpreter is seed 0.
    assert_eq!(
        eval("1.3 2.7 gnoise2 0 noiseseed 1.3 2.7 gnoise2 eq"),
        ["true"]
    );
}

#[test]
fn noise_never_touches_the_rand_stream() {
    let plain = eval("11 srand rand rand rand");
    let noisy = eval(
        "11 srand rand 99 noiseseed 1.5 2.5 gnoise2 pop 1 2 3 gnoise3 pop
         0.1 0.2 8 0.5 2 fbm2 pop 0.1 0.2 0.3 6 0.5 2 ridged3 pop rand
         -4 noiseseed 3 3 vnoise2 pop rand",
    );
    assert_eq!(plain, noisy);
    // And rand does not move the noise.
    assert_eq!(
        eval("5 noiseseed 1.3 2.7 gnoise2 100 { rand pop } repeat 77 srand 1.3 2.7 gnoise2 eq"),
        ["true"]
    );
}

#[test]
fn seeds_differ_and_reseeding_repeats() {
    let got =
        eval("1 noiseseed 1.3 2.7 gnoise2 2 noiseseed 1.3 2.7 gnoise2 1 noiseseed 1.3 2.7 gnoise2");
    assert_ne!(got[0], got[1]);
    assert_eq!(got[0], got[2]);
}

#[test]
fn gradient_noise_is_zero_on_the_lattice_and_in_range() {
    let got = eval(
        "/mx 0 def /bad 0 def
         -20 1 20 { /i exch def -20 1 20 { /j exch def
            i j gnoise2 0 ne { /bad bad 1 add def } if
            i j 3 gnoise3 0 ne { /bad bad 1 add def } if } for } for
         0 0.037 40 { /x exch def 0 0.041 40 { /y exch def
            /mx x y gnoise2 abs mx max def } for } for
         bad mx",
    );
    assert_eq!(got[0], "0");
    let mx: f64 = got[1].parse().unwrap();
    assert!(mx > 0.4 && mx <= 0.7072, "gnoise2 max |n| {mx}");
    let got = eval(
        "/m3 0 def /mv 0 def
         0 0.13 12 { /x exch def 0 0.17 12 { /y exch def 0 0.19 6 { /z exch def
            /m3 x y z gnoise3 abs m3 max def /mv x y z vnoise3 abs mv max def } for } for } for
         m3 mv",
    );
    let (m3, mv): (f64, f64) = (got[0].parse().unwrap(), got[1].parse().unwrap());
    assert!(m3 > 0.5 && m3 <= 1.1, "gnoise3 max |n| {m3}");
    assert!(mv > 0.5 && mv <= 0.99609375, "vnoise3 max |v| {mv}");
}

#[test]
fn gnoise2d_derivatives_match_finite_differences() {
    let mut it = Interp::new();
    for (x, y) in [(0.3, 0.7), (-4.21, 1.37), (10.5, -3.25), (100.01, 200.99)] {
        let h = 1e-6;
        let got = run(
            &mut it,
            &format!(
                "{x} {y} gnoise2d {x} {h} add {y} gnoise2 {x} {h} sub {y} gnoise2
                 {x} {y} {h} add gnoise2 {x} {y} {h} sub gnoise2"
            ),
        );
        let v: Vec<f64> = got.iter().map(|s| s.parse().unwrap()).collect();
        let fdx = (v[3] - v[4]) / (2.0 * h);
        let fdy = (v[5] - v[6]) / (2.0 * h);
        assert!(
            (v[1] - fdx).abs() < 1e-5,
            "dx at ({x},{y}): {} vs {fdx}",
            v[1]
        );
        assert!(
            (v[2] - fdy).abs() < 1e-5,
            "dy at ({x},{y}): {} vs {fdy}",
            v[2]
        );
    }
}

#[test]
fn fbm_with_one_octave_is_the_base_noise() {
    assert_eq!(
        eval(
            "1.3 2.7 1 0.5 2 fbm2 1.3 2.7 gnoise2 eq 1.3 2.7 0.4 1 0.5 2 vfbm3 1.3 2.7 0.4 vnoise3 eq 1 2 0 0.5 2 fbm2"
        ),
        ["true", "true", "0.0"]
    );
}

#[test]
fn bad_operands_raise_the_usual_errors() {
    let err = |src: &str| {
        let mut it = Interp::new();
        it.run_str(src).expect_err(src)
    };
    assert!(matches!(err("1.5 noiseseed"), PsError::Typecheck));
    assert!(matches!(err("1 2 33 0.5 2 fbm2"), PsError::Rangecheck));
    assert!(matches!(err("1 2 -1 0.5 2 ridged2"), PsError::Rangecheck));
    assert!(matches!(err("1 2 4.0 0.5 2 fbm2"), PsError::Typecheck));
    assert!(matches!(err("1e300 0 gnoise2"), PsError::Rangecheck));
    assert!(matches!(err("(a) 0 gnoise2"), PsError::Typecheck));
    assert!(matches!(err("1 2 gnoise3"), PsError::StackUnderflow));
    assert!(matches!(
        err("1.5 2.5 [ 1 2 3 ] .permnoise2"),
        PsError::Rangecheck
    ));
    assert!(matches!(err("1.5 2.5 5 .permnoise2"), PsError::Typecheck));
}
