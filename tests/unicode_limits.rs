//! `charpath` under `--max-memory` (#1425, Codex round 1): the current path
//! grows a glyph at a time, and the cap is held before each glyph is added,
//! not once the whole string is done. The memory limit counts the whole
//! process's heap, so this lives in a test binary of its own with one test.

use pscat::{Interp, Limits};

#[global_allocator]
static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;

const MIB: usize = 1 << 20;

/// Fill a string of `n` copies of `bytes` by doubling, then `charpath` it
/// under a 16 MiB cap. Returns the error and how much heap the attempt left
/// behind (the current path holds what was appended before the stop).
fn charpath_growth(font: &str, hex: &str, unit: usize, n: usize) -> (String, usize) {
    let mut it = Interp::new();
    it.quiet_stderr();
    it.set_limits(Limits::none().with_max_memory(16 * MIB))
        .expect("the allocator is installed");
    let before = pscat::memtrack::live_bytes();
    let src = format!(
        "{font} /s {len} string def s 0 <{hex}> putinterval /n {unit} def \
         {{ n {len} ge {{ exit }} if s 0 n getinterval s n n getinterval copy pop \
         /n n 2 mul def }} loop 0 0 moveto s false charpath",
        len = unit * n,
    );
    let err = it.run_str(&src).expect_err("the cap stops the charpath");
    let grew = pscat::memtrack::live_bytes().saturating_sub(before);
    (err.to_string(), grew)
}

#[test]
fn charpath_stops_at_the_memory_cap_not_after_the_string() {
    assert!(pscat::memtrack::installed());
    // 65,536 emoji (about 100 MB of layer outlines) and 262,144 letter O's
    // (about 250 MB) both stop near the 16 MiB cap.
    for (font, hex, unit, n) in [
        (
            "/Helvetica findfont 12 scalefont dup /PSCatUnicode true put setfont",
            "F09F9880",
            4,
            65_536,
        ),
        ("/Helvetica findfont 12 scalefont setfont", "4F", 1, 262_144),
    ] {
        let (err, grew) = charpath_growth(font, hex, unit, n);
        assert!(err.contains("limitexceeded"), "{err}");
        assert!(
            grew < 40 * MIB,
            "{hex}: the path grew {} MiB past a 16 MiB cap",
            grew / MIB
        );
    }
}
