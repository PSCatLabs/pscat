//! Helpers shared by integration tests that inspect PDF output.
#![allow(dead_code)] // each test crate that mounts this module uses a subset

/// The document as text with every page content stream inflated in
/// place (content streams are Flate-compressed, issue #244), so the
/// assertions below can read operators directly. Image XObjects (dict
/// has `/Subtype /Image`) are left compressed.
pub fn pdf_text(pdf: &[u8]) -> String {
    use flate2::read::ZlibDecoder;
    use std::io::Read as _;

    let find = |hay: &[u8], needle: &[u8], from: usize| {
        hay[from..]
            .windows(needle.len())
            .position(|w| w == needle)
            .map(|i| i + from)
    };
    let mut out: Vec<u8> = Vec::new();
    let mut at = 0;
    while let Some(s) = find(pdf, b"stream\n", at) {
        let body = s + b"stream\n".len();
        // Skip "endstream" matches: a real stream keyword follows ">>\n".
        let dict_start = pdf[..s].windows(2).rposition(|w| w == b"<<").unwrap_or(0);
        let dict = String::from_utf8_lossy(&pdf[dict_start..s]).into_owned();
        if !dict.ends_with(">>\n") || dict.contains("/Subtype /Image") {
            out.extend_from_slice(&pdf[at..body]);
            at = body;
            continue;
        }
        let len: usize = dict
            .split("/Length ")
            .nth(1)
            .and_then(|r| r.split_whitespace().next())
            .and_then(|n| n.parse().ok())
            .expect("stream /Length");
        out.extend_from_slice(&pdf[at..body]);
        let raw = &pdf[body..body + len];
        if dict.contains("/Filter /FlateDecode") {
            let mut inflated = Vec::new();
            ZlibDecoder::new(raw)
                .read_to_end(&mut inflated)
                .expect("content stream inflates");
            out.extend_from_slice(&inflated);
        } else {
            out.extend_from_slice(raw);
        }
        at = body + len;
    }
    out.extend_from_slice(&pdf[at..]);
    String::from_utf8_lossy(&out).into_owned()
}

// ---------------------------------------------------------------------
// CPU-time budgets (issue #329)
//
// A render-budget or "this guard must not walk the path first" test
// measures CPU time of the test's own thread, never wall time. Wall time
// includes every moment the scheduler gave the core to someone else, so
// with several agents running gates on one machine (or a shared CI
// runner) a 1 s render read 2 s and the test went red for no reason.
//
// Thread CPU time, not process CPU time: cargo runs the tests of one
// file in parallel threads of one process, so `RUSAGE_SELF` would count
// the neighbours. The interpreter is single-threaded and the tests build
// and run it on the test thread, so the thread clock is exactly the work
// under test.
// ---------------------------------------------------------------------

use std::time::Duration;

/// CPU time this thread has consumed so far (user + system).
#[cfg(unix)]
pub fn thread_cpu_time() -> Duration {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a valid out-pointer; CLOCK_THREAD_CPUTIME_ID is
    // supported on Linux and macOS (10.12+).
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) };
    assert_eq!(rc, 0, "clock_gettime(CLOCK_THREAD_CPUTIME_ID) failed");
    Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
}

/// Fallback where there is no per-thread CPU clock: wall time. Only the
/// unix runners (Linux, macOS) gate merges.
#[cfg(not(unix))]
pub fn thread_cpu_time() -> Duration {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed()
}

/// Runs `f` once and returns its value with the CPU time it used.
pub fn cpu_time_of<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let t0 = thread_cpu_time();
    let value = f();
    (value, thread_cpu_time().saturating_sub(t0))
}

/// `PSCAT_TEST_BUDGET_SCALE` multiplies every CPU budget in the suite
/// (default 1). Set it far below 1 (for example `0.001`) to confirm a
/// budget test still fails when the budget is wrong: the negative check
/// the timing-test guidance asks for.
pub fn scale_budget(budget: Duration) -> Duration {
    match std::env::var("PSCAT_TEST_BUDGET_SCALE")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
    {
        Some(k) if k.is_finite() && k >= 0.0 => budget.mul_f64(k),
        _ => budget,
    }
}

/// Best-of-`tries` CPU-time budget check. Runs `f` (which must rebuild
/// any state it mutates, and may assert on its own result) up to `tries`
/// times, stops at the first run under the (scaled) budget, and panics
/// unless the best run was under it. A slow outlier costs a retry; a real
/// regression is slow every time. Prints the per-try times so drift shows
/// before it fails. Returns the value of the fastest run.
pub fn within_cpu_budget<T>(
    what: &str,
    budget: Duration,
    tries: usize,
    mut f: impl FnMut() -> T,
) -> T {
    assert!(tries >= 1);
    let budget = scale_budget(budget);
    let mut best: Option<(T, Duration)> = None;
    let mut seen = Vec::new();
    for _ in 0..tries {
        let (value, cpu) = cpu_time_of(&mut f);
        seen.push(format!("{:.3}s", cpu.as_secs_f64()));
        if best.as_ref().is_none_or(|(_, b)| cpu < *b) {
            best = Some((value, cpu));
        }
        if cpu < budget {
            break;
        }
    }
    let (value, cpu) = best.expect("at least one try");
    eprintln!(
        "{what}: cpu {:.3}s of {:.3}s budget (tries: {})",
        cpu.as_secs_f64(),
        budget.as_secs_f64(),
        seen.join(", ")
    );
    assert!(
        cpu < budget,
        "{what}: best CPU time over {} tries was {cpu:?}, over the {budget:?} budget",
        seen.len()
    );
    value
}
