#![no_main]
//! `pscat::syntax::check_syntax` on arbitrary bytes: it must return `Ok` or
//! `Err`, never panic (a panic would take down a host that validates
//! untrusted programs). Seeds are the lexer's. Run: `cargo +nightly fuzz run check_syntax`.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Either outcome is fine; the target is the absence of a panic.
    let _ = pscat::syntax::check_syntax(data);
});
