//! Issue #881: operator objects are the same object when they are the
//! same *operator*, which this interpreter decides by the name the
//! operator is registered under in systemdict, never by its function's
//! address. Rust guarantees neither that one function has one address
//! nor that two functions have two (identical-code folding), and several
//! operators here share one function body on purpose
//! (`readonly`/`noaccess`/`executeonly`, `rcheck`/`wcheck`).
//!
//! Every expected value below was taken from Ghostscript 10.07.1
//! (`gs -dNODISPLAY -q`); the program is quoted above each case. They run
//! under both `cargo test` and `cargo test --profile ci` (the two build
//! with different optimization settings, which is where an
//! address-based comparison goes wrong).

use pscat::Interp;

/// Run `src` in a fresh interpreter and return the operand stack as `==`
/// text, bottom first.
fn eval(src: &str) -> Vec<String> {
    let mut it = Interp::new();
    it.run_str(src)
        .unwrap_or_else(|e| panic!("eval of {src:?} failed: {e}"));
    it.operand_stack().iter().map(|o| o.repr()).collect()
}

/// One program, one expected stack.
fn check(src: &str, want: &[&str]) {
    assert_eq!(eval(src), want, "{src}");
}

#[test]
fn the_same_operator_is_eq_to_itself() {
    // gs: /add load /add load eq  -> true
    check("/add load /add load eq", &["true"]);
    // gs: /add load /add load ne  -> false
    check("/add load /add load ne", &["false"]);
    // A name bound to an operator is that operator (gs: true).
    check("/myadd /add load def /myadd load /add load eq", &["true"]);
    // The executable attribute never matters (gs: true).
    check("/add load cvx /add load cvlit eq", &["true"]);
    // The same object fetched out of an array (gs: true).
    check("[/add load /sub load] dup 0 get exch 0 get eq", &["true"]);
}

#[test]
fn different_operators_are_not_eq() {
    // gs: /add load /sub load eq  -> false
    check("/add load /sub load eq", &["false"]);
    // gs: /add load /sub load ne  -> true
    check("/add load /sub load ne", &["true"]);
}

#[test]
fn operators_sharing_one_function_are_still_different_operators() {
    // These share a function body here, so an address comparison called
    // them equal. gs: false, false, false; and `ne` -> true.
    check("/readonly load /noaccess load eq", &["false"]);
    check("/executeonly load /noaccess load eq", &["false"]);
    check("/rcheck load /wcheck load eq", &["false"]);
    check("/readonly load /noaccess load ne", &["true"]);
    // ...but each is still equal to itself (gs: false for `ne`).
    check("/readonly load /readonly load ne", &["false"]);
}

#[test]
fn an_operator_is_not_eq_to_a_non_operator() {
    // gs: all false for `eq`, all true for `ne`.
    check("/add load /add eq", &["false"]); // a name
    check("/add load (add) eq", &["false"]); // a string
    check("/add load 1 eq", &["false"]);
    check("/add load null eq", &["false"]);
    check("/add load {add} eq", &["false"]);
    check("/add load 1 ne", &["true"]);
    check("1 /add load ne", &["true"]);
}

#[test]
fn operators_are_dictionary_keys_by_operator() {
    // gs: << /add load 1 >> dup /add load 2 put length  -> 1 (one key)
    check("<< /add load 1 >> dup /add load 2 put length", &["1"]);
    check(
        "<< /add load 1 >> dup /add load 9 put /add load get",
        &["9"],
    );
    // gs: << /add load 1 >> dup /sub load 2 put length  -> 2
    check("<< /add load 1 >> dup /sub load 2 put length", &["2"]);
    // gs: known -> true, then false for a different operator.
    check("<< /add load 1 >> /add load known", &["true"]);
    check("<< /add load 1 >> /sub load known", &["false"]);
    // Operators sharing a function are different keys (gs: 2, 2, false).
    check("<< /readonly load 1 /noaccess load 2 >> length", &["2"]);
    check("<< /rcheck load 1 /wcheck load 2 >> length", &["2"]);
    check("<< /readonly load 1 >> /noaccess load known", &["false"]);
}
