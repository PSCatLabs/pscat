//! Issue #277: `forall` over a dict must visit entries in a run-to-run
//! stable order. Non-name keys (integers, reals, booleans, arrays, dicts)
//! come back in insertion order; name keys keep their deterministic
//! (interned-id hash) order, listed before the others.

use pscat::Interp;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_pscat");

/// Run `src` in a fresh interpreter and return the operand stack as `==`
/// text, bottom first.
fn eval(src: &str) -> Vec<String> {
    let mut it = Interp::new();
    it.run_str(src)
        .unwrap_or_else(|e| panic!("eval of {src:?} failed: {e}"));
    it.operand_stack().iter().map(|o| o.repr()).collect()
}

/// A scrambled key sequence: with a randomly seeded hasher the odds of
/// seeing this order by chance are 1 in 16!, per map.
const SCRAMBLED: &str = "[7 3 12 0 15 9 1 14 5 11 2 13 6 10 4 8]";

#[test]
fn int_keys_iterate_in_insertion_order() {
    // Each iteration builds a fresh dict (and so a fresh RandomState).
    for _ in 0..20 {
        let got = eval(&format!(
            "/d 20 dict def {SCRAMBLED} {{ d exch 1 put }} forall \
             d {{ pop }} forall count array astore"
        ));
        assert_eq!(got, ["[7 3 12 0 15 9 1 14 5 11 2 13 6 10 4 8]"]);
    }
}

#[test]
fn mixed_exotic_keys_iterate_in_insertion_order() {
    // Reals, booleans, a whole-number real, and the pointer-keyed array
    // and dict identities; the key object handed back is the one stored.
    let got = eval(
        "/a [1 2] def /e 1 dict def /d 20 dict def \
         d 2.5 1 put d true 1 put d a 1 put d 3.0 1 put d e 1 put d -1 1 put d false 1 put \
         d { pop } forall count array astore",
    );
    assert_eq!(got, ["[2.5 true [1 2] 3.0 -dict- -1 false]"]);
}

#[test]
fn redefining_a_key_keeps_its_slot() {
    let got = eval(
        "/d 8 dict def d 5 1 put d 3 1 put d 9 1 put \
         d 3 99 put d 5 7 put \
         d { exch pop } forall count array astore \
         d 3 get d 5 get",
    );
    assert_eq!(got, ["[7 99 1]", "99", "7"]);
}

#[test]
fn undef_then_put_moves_the_key_to_the_end() {
    let got = eval(
        "/d 8 dict def d 5 1 put d 3 1 put d 9 1 put \
         d 5 undef d 5 1 put \
         d { pop } forall count array astore",
    );
    assert_eq!(got, ["[3 9 5]"]);
}

#[test]
fn copy_and_save_restore_preserve_the_order() {
    let got = eval(
        "/d 8 dict def d 5 1 put d 3 1 put d 9 1 put \
         /c 8 dict def d c copy pop \
         [ c { pop } forall ] \
         save /s exch def d 1 1 put d 0 1 put s restore \
         [ d { pop } forall ]",
    );
    assert_eq!(got, ["[5 3 9]", "[5 3 9]"]);
}

#[test]
fn names_come_first_then_exotic_keys_in_insertion_order() {
    let got = eval(
        "/d 8 dict def d 10 1 put d /x 1 put d 2 1 put d /y 1 put d 1 1 put \
         d { pop } forall count array astore",
    );
    // Name order is the interned-id hash order (an implementation detail:
    // accept either), then the integers as inserted.
    assert!(
        got == ["[/x /y 10 2 1]"] || got == ["[/y /x 10 2 1]"],
        "{got:?}"
    );
}

/// The issue's own reproducer, run as separate processes (each gets its
/// own hasher seeds and its own ASLR layout).
#[test]
fn forall_order_is_identical_across_processes() {
    let run = |src: &str| {
        let out = Command::new(BIN).args(["-e", src]).output().expect("spawn");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let issue = "/d 20 dict def 0 1 15 { d exch 1 put } for d { pop } forall count array astore ==";
    let first = run(issue);
    assert_eq!(first.trim(), "[0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15]");
    // A mixed name + int + array-key + dict-key dict, to show name order
    // and the pointer-keyed entries are stable too.
    let mixed = "/a [1] def /e 1 dict def /d 20 dict def \
        d /alpha 1 put d /beta 2 put d 7 1 put d a 1 put d /gamma 3 put \
        d e 1 put d 2.5 1 put d true 1 put d { pop } forall count array astore ==";
    let first_mixed = run(mixed);
    for _ in 0..6 {
        assert_eq!(run(issue), first);
        assert_eq!(run(mixed), first_mixed);
    }
}
