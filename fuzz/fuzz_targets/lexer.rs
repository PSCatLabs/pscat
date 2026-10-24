#![no_main]
//! The scanner on arbitrary bytes. Run: `cargo +nightly fuzz run lexer`.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| pscat::fuzzing::lexer(data));
