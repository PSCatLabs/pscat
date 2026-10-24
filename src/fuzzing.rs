//! Entry points for the fuzz targets in `fuzz/` (issue #604), kept in the
//! library so `tests/fuzz_corpus.rs` can replay the committed seeds and
//! crash regressions on every `cargo test`, with no nightly toolchain.
//!
//! Each function takes arbitrary bytes and must never panic, hang, or use
//! unbounded memory; what a fuzzer reports is a bug in pscat.

use std::time::Duration;

use crate::{Interp, Limits, lexer::Lexer};

/// Tokens read from one input before the lexer target stops.
const MAX_TOKENS: usize = 100_000;

/// Scan `data` as PostScript source, token by token.
pub fn lexer(data: &[u8]) {
    let mut lexer = Lexer::new(data.to_vec());
    for _ in 0..MAX_TOKENS {
        // An error ends the scan, as it ends a program.
        match lexer.next_token() {
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => break,
        }
    }
}

/// Limits for [`interpreter`]: tight, so a fuzzer finds crashes rather than
/// reporting every `{ } loop` as a timeout or an out-of-memory.
pub fn fuzz_limits() -> Limits {
    Limits::none()
        .with_max_steps(200_000)
        .with_max_time(Duration::from_secs(2))
        .with_max_memory(256 << 20)
}

/// Run `data` as a PostScript program on a tiny page, under
/// [`fuzz_limits`]. The memory cap applies only where the binary installs
/// [`TrackingAllocator`](crate::TrackingAllocator) (the fuzz targets do).
pub fn interpreter(data: &[u8]) {
    let Some(mut interp) = Interp::with_page(64, 64) else {
        return;
    };
    let limits = fuzz_limits();
    if interp.set_limits(limits).is_err() {
        // No tracking allocator: keep the step and time caps.
        let _ = interp.set_limits(Limits {
            max_memory: None,
            ..limits
        });
    }
    // Keep `print` and `=` out of the fuzzer's stdout.
    interp.capture_stdout();
    let _ = interp.run_source(data);
}
