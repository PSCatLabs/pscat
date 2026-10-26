//! `writereals` / `readreals` (issue #735): packed little-endian binary
//! save and load of a numeric array. Writing needs `--allow-write` roots
//! (issue #740); reading follows the normal read policy.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use pscat::{Interp, Object, PsError, Value};

fn setup(tag: &str) -> (PathBuf, Interp) {
    let root = common::scratch_dir(tag);
    let mut it = Interp::with_page(50, 50).expect("page");
    it.set_write_roots(&[&root]).expect("root exists");
    (root, it)
}

fn ps(p: &Path) -> String {
    let s = p.to_str().unwrap();
    assert!(!s.contains(['(', ')']), "{s}");
    format!("({})", s.replace('\\', "\\\\"))
}

fn reals(it: &Interp) -> Vec<f64> {
    let Value::Array(a) = &it.operand_stack().last().expect("an array").value else {
        panic!("not an array");
    };
    (0..a.len())
        .map(|i| match a.get(i).unwrap().value {
            Value::Real(r) => r,
            _ => panic!("not a real"),
        })
        .collect()
}

/// Values that are easy to get wrong: signed zero, a subnormal, the
/// extremes, a non-terminating binary fraction, infinities and NaNs with
/// payloads, then a pseudo-random run long enough to cross several
/// 8192-element chunks.
fn awkward() -> Vec<f64> {
    let mut v = vec![
        0.0,
        -0.0,
        0.1,
        -0.1,
        f64::MIN_POSITIVE,
        f64::MAX,
        f64::MIN,
        f64::from_bits(1),
        f64::from_bits(0x000f_ffff_ffff_ffff),
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::from_bits(0x7ff8_0000_0000_0001),
        1.0 / 3.0,
        std::f64::consts::PI,
    ];
    let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
    for _ in 0..20_000 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        v.push(f64::from_bits(x));
    }
    v
}

fn array_of(vals: &[f64]) -> Object {
    Object::array(vals.iter().map(|&r| Object::real(r)).collect())
}

#[test]
fn f64_round_trip_is_bit_exact() {
    let (root, mut it) = setup("rr-f64");
    let vals = awkward();
    let f = root.join("v.bin");
    it.push(array_of(&vals));
    it.run_str(&format!("{} writereals", ps(&f))).unwrap();
    // The bytes on disk: packed little-endian binary64, nothing else.
    let want: Vec<u8> = vals.iter().flat_map(|v| v.to_le_bytes()).collect();
    assert_eq!(std::fs::read(&f).unwrap(), want);
    it.run_str(&format!("{} {} readreals", ps(&f), vals.len()))
        .unwrap();
    let got = reals(&it);
    assert_eq!(got.len(), vals.len());
    for (i, (g, w)) in got.iter().zip(&vals).enumerate() {
        assert_eq!(g.to_bits(), w.to_bits(), "element {i}");
    }
}

#[test]
fn f32_round_trip() {
    let (root, mut it) = setup("rr-f32");
    let f = root.join("v.bin");
    it.run_str(&format!(
        "[1 -2.5 0.1 3.4e38 1e39 -1e39 0 1e-50] {} /f32 writereals",
        ps(&f)
    ))
    .unwrap();
    let want: Vec<u8> = [
        1.0f32,
        -2.5,
        0.1,
        3.4e38,
        f32::INFINITY,
        f32::NEG_INFINITY,
        0.0,
        0.0,
    ]
    .iter()
    .flat_map(|v| v.to_le_bytes())
    .collect();
    assert_eq!(std::fs::read(&f).unwrap(), want);
    it.run_str(&format!("{} 8 /f32 readreals", ps(&f))).unwrap();
    let got = reals(&it);
    assert_eq!(got[0], 1.0);
    assert_eq!(got[2], f64::from(0.1f32));
    assert_eq!(got[4], f64::INFINITY);
    assert_eq!(got[5], f64::NEG_INFINITY);
}

#[test]
fn integers_are_written_as_reals() {
    let (root, mut it) = setup("rr-int");
    let f = root.join("i.bin");
    it.run_str(&format!(
        "[1 -7 0 9007199254740993 2.5] {} writereals",
        ps(&f)
    ))
    .unwrap();
    let want: Vec<u8> = [1.0f64, -7.0, 0.0, 9007199254740993i64 as f64, 2.5]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    assert_eq!(std::fs::read(&f).unwrap(), want);
    it.run_str(&format!("{} 5 readreals", ps(&f))).unwrap();
    assert_eq!(reals(&it)[3], 9007199254740992.0, "rounds as cvr does");
}

#[test]
fn empty_and_prefix_reads() {
    let (root, mut it) = setup("rr-empty");
    let f = root.join("e.bin");
    it.run_str(&format!("[] {} writereals", ps(&f))).unwrap();
    assert_eq!(std::fs::read(&f).unwrap(), b"");
    it.run_str(&format!("{} 0 readreals length", ps(&f)))
        .unwrap();
    assert_eq!(it.pop_int().unwrap(), 0);
    // A longer file: the first n values; a trailing partial value is ignored.
    it.run_str(&format!("[1 2 3] {} writereals", ps(&f)))
        .unwrap();
    let mut bytes = std::fs::read(&f).unwrap();
    bytes.extend_from_slice(&[1, 2, 3]);
    std::fs::write(&f, bytes).unwrap();
    it.run_str(&format!("{} 2 readreals", ps(&f))).unwrap();
    assert_eq!(reals(&it), [1.0, 2.0]);
    it.run_str(&format!("{} 3 readreals", ps(&f))).unwrap();
    assert_eq!(reals(&it), [1.0, 2.0, 3.0]);
}

#[test]
fn writing_truncates_an_existing_file() {
    let (root, mut it) = setup("rr-trunc");
    let f = root.join("t.bin");
    std::fs::write(&f, vec![7u8; 1000]).unwrap();
    it.run_str(&format!("[1] {} writereals", ps(&f))).unwrap();
    assert_eq!(std::fs::read(&f).unwrap().len(), 8);
}

#[test]
fn errors() {
    let (root, mut it) = setup("rr-err");
    let f = root.join("x.bin");
    let p = ps(&f);
    // A non-numeric element: typecheck, and the file is never created.
    for src in [
        format!("[1 (a) 3] {p} writereals"),
        format!("[1 /n 3] {p} writereals"),
        format!("[1 [2] 3] {p} writereals"),
        format!("[1 null] {p} /f32 writereals"),
        format!("3 {p} writereals"),
        "[1] 5 writereals".to_string(),
        format!("{p} (3) readreals"),
        "5 5 5 readreals".to_string(),
    ] {
        assert_eq!(it.run_str(&src), Err(PsError::Typecheck), "{src}");
    }
    assert!(!f.exists());
    // A bad element does not truncate an existing file either.
    std::fs::write(&f, b"keep").unwrap();
    assert_eq!(
        it.run_str(&format!("[1 (a)] {p} writereals")),
        Err(PsError::Typecheck)
    );
    assert_eq!(std::fs::read(&f).unwrap(), b"keep");
    // Unknown format names, negative counts.
    assert_eq!(
        it.run_str(&format!("[1] {p} /bogus writereals")),
        Err(PsError::Rangecheck)
    );
    assert_eq!(std::fs::read(&f).unwrap(), b"keep");
    assert_eq!(
        it.run_str(&format!("{p} 1 /f16 readreals")),
        Err(PsError::Rangecheck)
    );
    assert_eq!(
        it.run_str(&format!("{p} -1 readreals")),
        Err(PsError::Rangecheck)
    );
    assert_eq!(
        it.run_str(&format!("{p} 100000000 readreals")),
        Err(PsError::Limitcheck)
    );
    // A short file: rangecheck (end-of-file is not ioerror).
    it.run_str(&format!("[1 2 3] {p} writereals")).unwrap();
    assert_eq!(
        it.run_str(&format!("{p} 4 readreals")),
        Err(PsError::Rangecheck)
    );
    assert_eq!(
        it.run_str(&format!("{p} 4 /f32 readreals length pop")),
        Ok(()),
        "24 bytes are six binary32 values"
    );
    assert_eq!(
        it.run_str(&format!("{p} 7 /f32 readreals")),
        Err(PsError::Rangecheck)
    );
    // Missing files and paths outside the roots.
    assert_eq!(
        it.run_str(&format!("{} 1 readreals", ps(&root.join("none.bin")))),
        Err(PsError::UndefinedFilename)
    );
    let out = common::scratch_dir("rr-err-outside").join("o.bin");
    assert_eq!(
        it.run_str(&format!("[1] {} writereals", ps(&out))),
        Err(PsError::InvalidFileAccess)
    );
    assert_eq!(
        it.run_str("[1] (../o.bin) writereals"),
        Err(PsError::InvalidFileAccess)
    );
    assert_eq!(
        it.run_str("[1] (%stdout) writereals"),
        Err(PsError::UndefinedFilename)
    );
    assert!(!out.exists());
}

#[test]
fn no_write_roots_by_default() {
    let root = common::scratch_dir("rr-default");
    let mut it = Interp::with_page(50, 50).unwrap();
    let f = ps(&root.join("x.bin"));
    assert_eq!(
        it.run_str(&format!("[1 2] {f} writereals")),
        Err(PsError::InvalidFileAccess)
    );
    assert!(!root.join("x.bin").exists());
    // Reading needs no root.
    std::fs::write(root.join("r.bin"), 2.5f64.to_le_bytes()).unwrap();
    it.run_str(&format!("{} 1 readreals", ps(&root.join("r.bin"))))
        .unwrap();
    assert_eq!(reals(&it), [2.5]);
}

#[test]
fn step_limit_stops_a_huge_write_and_read() {
    let (root, _) = setup("rr-steps");
    let f = ps(&root.join("big.bin"));
    // 200k reals is 1.5 MiB: well over 100 steps (one per KiB).
    let mut it = Interp::with_page(50, 50).unwrap();
    it.set_write_roots(&[&root]).unwrap();
    it.run_str(&format!(
        "/a 200000 array def 0 1 199999 {{ a exch 1.5 put }} for a {f} writereals"
    ))
    .unwrap();
    it.set_limits(pscat::Limits::none().with_max_steps(100))
        .unwrap();
    assert_eq!(
        it.run_str(&format!("{f} 200000 readreals pop")),
        Err(PsError::StepsExceeded)
    );
    it.set_limits(pscat::Limits::none().with_max_steps(100))
        .unwrap();
    assert_eq!(
        it.run_str(&format!("a {f} writereals")),
        Err(PsError::StepsExceeded)
    );
}

#[test]
fn cli_round_trip() {
    let root = common::scratch_dir("rr-cli");
    let f = root.join("c.bin");
    let prog = format!(
        "[0.1 2 -0.0] {p} writereals {p} 3 readreals {{ == }} forall",
        p = ps(&f)
    );
    let pscat = env!("CARGO_BIN_EXE_pscat");
    let refused = Command::new(pscat)
        .args(["--headless", "-e", &prog])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let ok = Command::new(pscat)
        .arg("--allow-write")
        .arg(&root)
        .args(["--headless", "-e", &prog])
        .output()
        .unwrap();
    assert!(ok.status.success(), "{ok:?}");
    assert_eq!(String::from_utf8_lossy(&ok.stdout), "0.1\n2.0\n-0.0\n");
}
