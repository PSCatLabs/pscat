//! `pscat --bands N` (issue #736): render one PNG as N horizontal bands on
//! N threads, then stitch the bands together.
//!
//! Each band is a separate, complete run of the program in its own
//! [`Interp`] on its own thread. Nothing PostScript-visible is shared: the
//! name interner is thread-local, and the process-wide state (font catalog,
//! resource roots, font packs) is behind `OnceLock`/`RwLock`. Threads are
//! cheaper than spawning N copies of pscat and need no temporary files.
//!
//! Before the program runs, userdict holds `/BandIndex` (0 is the top band),
//! `/BandCount` and `/BandBBox [llx lly urx ury]`, the band's area in default
//! user space. A program that knows about bands can skip work outside its
//! box. One that doesn't still renders correctly: every band paints the
//! whole page, pscat keeps only the band's own device rows, and image
//! rasterization (`image`, `colorimage`, `imagemask`) skips the other rows.
//! The rows are whole device pixels and every band's pixels inside them are
//! the ones a single run paints, so the stitched PNG is byte-for-byte the
//! single-run PNG, anti-aliased edges across the seams included.
//!
//! Band `i` of `N` on a canvas `H` pixels tall owns rows
//! `floor(i*H/N) .. floor((i+1)*H/N)`.
//!
//! Limits: `--max-steps` and `--max-time` apply to each band on its own (an
//! unaware program takes as many steps in every band as in a single run, so
//! the same limit fits). `--max-memory` reads the process heap, so it caps
//! all bands together; every band's canvas is allocated before any band
//! starts counting.

use std::io::Write;
use std::process::ExitCode;
use std::sync::{Condvar, Mutex};

use pscat::Interp;
use tiny_skia::Pixmap;

use crate::Options;

/// The most bands `--bands` accepts: past the core count of any machine
/// pscat runs on, extra bands only cost memory (a canvas each).
pub const MAX_BANDS: u32 = 64;

/// Each band thread's stack. The main thread gets 8 MiB on macOS and Linux,
/// and a band runs the same interpreter a single render runs there.
const BAND_STACK: usize = 8 << 20;

/// `--bands N`'s value.
pub fn parse_count(n: &str) -> Result<u32, String> {
    n.parse()
        .ok()
        .filter(|b| (1..=MAX_BANDS).contains(b))
        .ok_or_else(|| format!("invalid --bands value: {n} (expected 1..{MAX_BANDS})"))
}

/// The device rows band `index` of `count` owns on a canvas `height` pixels
/// tall: `top..bottom`.
pub fn rows(index: u32, count: u32, height: u32) -> (u32, u32) {
    let at = |i: u32| (u64::from(i) * u64::from(height) / u64::from(count)) as u32;
    (at(index), at(index + 1))
}

/// Refuse every combination `--bands` can't honour, before anything runs.
fn check_options(options: &Options) -> Result<(), String> {
    let others: [(bool, &str); 11] = [
        (options.svg.is_some(), "--svg"),
        (options.pdf.is_some(), "--pdf"),
        (options.lint, "--lint"),
        (options.sweep_seed.is_some(), "--sweep-seed"),
        (options.sweep_param.is_some(), "--sweep"),
        (options.contact_sheet.is_some(), "--contact-sheet"),
        (options.grid.is_some(), "--grid"),
        (options.spool.is_some(), "--spool"),
        (options.interactive, "--interactive"),
        (options.progress, "--progress"),
        // Each band re-runs the whole program, so a per-band profile would
        // report N copies of the same calls (#737). Profile a plain run.
        (options.profile, "--profile"),
    ];
    if let Some((_, flag)) = others.iter().find(|(on, _)| *on) {
        return Err(format!(
            "--bands writes a PNG only; it doesn't combine with {flag}"
        ));
    }
    if !options.allow_write.is_empty() {
        // Every band runs the whole program, so each would write the file.
        return Err("--bands doesn't combine with --allow-write".to_string());
    }
    if options.png.is_none() {
        return Err("--bands needs --png".to_string());
    }
    if options.file.is_none() && options.eval.is_none() {
        return Err("--bands needs a file, - or -e".to_string());
    }
    Ok(())
}

/// What one band hands back. Strings, not `PsError`s: those hold `Rc`s and
/// cannot leave the band's thread.
struct BandOutput {
    pages: Vec<Pixmap>,
    /// The error report (and operand stack under `--pstack-on-error`) if
    /// the run failed.
    error: Option<String>,
}

/// Holds every band at a line until all N reach it. `run_band` passes two:
/// one once every canvas exists, so `--max-memory`'s base (taken when a band
/// sets its limits) counts all of them, and one once every band has its
/// base, so no program's allocations escape the shared budget. A `Barrier` would leave the others waiting forever if one
/// thread failed to spawn; `abort` releases them instead.
struct StartGate {
    state: Mutex<(u32, bool)>,
    ready: Condvar,
    count: u32,
}

impl StartGate {
    fn new(count: u32) -> Self {
        StartGate {
            state: Mutex::new((0, false)),
            ready: Condvar::new(),
            count,
        }
    }

    /// Arrive and wait for the others. False when the render was abandoned
    /// before all of them arrived; an abort after that changes nothing.
    fn arrive(&self) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        state.0 += 1;
        self.ready.notify_all();
        while state.0 < self.count && !state.1 {
            state = match self.ready.wait(state) {
                Ok(s) => s,
                Err(_) => return false,
            };
        }
        state.0 >= self.count
    }

    fn abort(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.1 = true;
        }
        self.ready.notify_all();
    }
}

/// Aborts both gates when dropped; see `run_band`.
struct AbortOnDrop<'a>(&'a [StartGate; 2]);

impl Drop for AbortOnDrop<'_> {
    fn drop(&mut self) {
        self.0.iter().for_each(StartGate::abort);
    }
}

/// Said by a band released from a gate because another band failed.
const ABANDONED: &str = "abandoned";

/// `pscat --bands N`: check the options, run the bands, stitch and write.
pub fn run(options: &Options, count: u32) -> ExitCode {
    if let Err(msg) = check_options(options) {
        eprintln!("pscat: {msg}");
        return ExitCode::FAILURE;
    }
    let source = match (&options.eval, &options.file) {
        (Some(expr), _) => expr.as_bytes().to_vec(),
        (None, Some(path)) => match crate::read_source(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("pscat: {e}");
                return ExitCode::FAILURE;
            }
        },
        (None, None) => unreachable!("check_options requires a source"),
    };
    let scale = options.dpi / 72.0;
    // The canvas size, from a probe rather than a copy of
    // `with_page_scaled`'s formula (as `run_sweep` does). Checked here, on
    // the main thread, so no band can fail to build its canvas.
    let Some(probe) = Interp::with_page_scaled(options.page.0, options.page.1, scale) else {
        eprintln!(
            "pscat: unusable page size {}x{}",
            options.page.0, options.page.1
        );
        return ExitCode::FAILURE;
    };
    let (width, height) = (probe.gfx().pixmap.width(), probe.gfx().pixmap.height());
    drop(probe);
    if count > height {
        eprintln!("pscat: --bands {count} is more bands than the page has pixel rows ({height})");
        return ExitCode::FAILURE;
    }

    let outputs = match render(options, &source, count, height) {
        Ok(o) => o,
        Err(msg) => {
            eprintln!("pscat: {msg}");
            return ExitCode::FAILURE;
        }
    };
    let mut failed = false;
    if let Some((i, report)) = outputs
        .iter()
        .enumerate()
        .find_map(|(i, o)| o.error.as_ref().map(|e| (i, e)))
    {
        eprintln!("pscat: band {}/{count} failed:", i + 1);
        eprintln!("{report}");
        failed = true;
    }
    let pages = match stitch(&outputs, width, height) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("pscat: {msg}");
            return ExitCode::FAILURE;
        }
    };
    let png = options
        .png
        .as_deref()
        .expect("check_options requires --png");
    let refs: Vec<&Pixmap> = pages.iter().collect();
    if !crate::write_pngs(&refs, png, options.halftone) || failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Run the N bands, one thread each, and collect what they produced.
fn render(
    options: &Options,
    source: &[u8],
    count: u32,
    height: u32,
) -> Result<Vec<BandOutput>, String> {
    let gates = [StartGate::new(count), StartGate::new(count)];
    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(count as usize);
        for index in 0..count {
            let gates = &gates;
            let spawned = std::thread::Builder::new()
                .name(format!("band-{index}"))
                .stack_size(BAND_STACK)
                .spawn_scoped(scope, move || {
                    run_band(options, source, index, count, height, gates)
                });
            match spawned {
                Ok(h) => handles.push(h),
                Err(e) => {
                    gates.iter().for_each(StartGate::abort);
                    return Err(format!("cannot start band thread: {e}"));
                }
            }
        }
        // Join every band before looking at any result: a handle left
        // unjoined after a panic would make `scope` itself panic.
        let joined: Vec<_> = handles.into_iter().map(|h| h.join()).collect();
        // Report the band that failed, not one it left abandoned.
        let failure = |(i, r): (usize, &std::thread::Result<Result<BandOutput, String>>)| match r {
            Ok(Ok(_)) => None,
            Ok(Err(msg)) if msg == ABANDONED => None,
            Ok(Err(msg)) => Some(format!("band {}/{count}: {msg}", i + 1)),
            Err(_) => Some(format!("band {}/{count} crashed", i + 1)),
        };
        if let Some(msg) = joined.iter().enumerate().find_map(failure) {
            return Err(msg);
        }
        joined
            .into_iter()
            .map(|r| match r {
                Ok(Ok(out)) => Ok(out),
                _ => Err("a band was abandoned".to_string()),
            })
            .collect()
    })
}

/// One band: a full run of the program with the band names defined, its
/// pages handed back as pixmaps.
fn run_band(
    options: &Options,
    source: &[u8],
    index: u32,
    count: u32,
    height: u32,
    gates: &[StartGate; 2],
) -> Result<BandOutput, String> {
    // However this band ends (an error, a panic), release the others
    // still waiting at a gate it never reached.
    let _release = AbortOnDrop(gates);
    let scale = options.dpi / 72.0;
    let interp = Interp::with_page_scaled(options.page.0, options.page.1, scale);
    // Arrive whether or not the canvas was built, so no band is left waiting.
    if !gates[0].arrive() {
        return Err(ABANDONED.to_string());
    }
    let mut interp = interp.ok_or("unusable page size")?;
    // The process exits right after the stitch: leave the dictionaries to
    // it, as the single-run path does (`main`).
    interp.leave_dicts_to_process_exit();
    if options.transparent {
        interp.gfx_mut().set_transparent_background();
    }
    let (top, bottom) = rows(index, count, height);
    interp.gfx_mut().set_band_rows(top, bottom);
    // Only the top band's output reaches stdout; the others' is kept in a
    // bounded buffer and dropped, so an unaware program prints once.
    if index > 0 {
        interp.capture_stdout();
    }
    let width_pt = interp.gfx().pixmap.width() as f32 / scale;
    let user_y = |row: u32| (height - row) as f32 / scale;
    let preamble = format!(
        "/BandIndex {index} def /BandCount {count} def /BandBBox [0 {} {} {}] def",
        user_y(bottom),
        width_pt,
        user_y(top)
    );
    interp
        .run_source(preamble.as_bytes())
        .map_err(|e| format!("internal error defining the band names: {e}"))?;
    // Limits after the band names, so they bound the program alone, as in a
    // plain render; after the first gate, so the heap base counts every
    // band's canvas.
    interp
        .set_limits(options.limits)
        .map_err(|e| format!("limits: {e}"))?;
    // `--read-root`/`--confine-reads` (#763) bind every band, as they bind a
    // plain render.
    if let Some(dirs) = &options.read_roots {
        interp
            .set_read_roots(dirs)
            .map_err(|e| format!("--read-root: {e}"))?;
    }
    // No band runs the program until every band has its base: a band that
    // started early would put its allocations under a later band's base,
    // out of every check.
    if !gates[1].arrive() {
        return Err(ABANDONED.to_string());
    }
    let error = match interp.run_source(source) {
        Ok(()) => None,
        Err(e) => {
            let mut report = interp.error_report(&e);
            if options.pstack_on_error {
                report.push('\n');
                report.push_str(&crate::pstack_text(&interp));
            }
            Some(report)
        }
    };
    let _ = std::io::stdout().flush();
    // The pages `--png` writes, chosen as `finish_headless` chooses them.
    let mut pages: Vec<Pixmap> = interp.gfx().pages().to_vec();
    if pages.is_empty() || interp.gfx().has_trailing_art() {
        // Moved out rather than cloned: the run is over, and a band holds
        // a whole canvas.
        let empty = Pixmap::new(1, 1).ok_or("cannot allocate a pixmap")?;
        pages.push(std::mem::replace(&mut interp.gfx_mut().pixmap, empty));
    }
    Ok(BandOutput { pages, error })
}

/// Page `k` of the result is band `i`'s page `k` in band `i`'s rows.
fn stitch(outputs: &[BandOutput], width: u32, height: u32) -> Result<Vec<Pixmap>, String> {
    let count = outputs.len() as u32;
    let pages = outputs.first().map_or(0, |o| o.pages.len());
    if let Some((i, o)) = outputs
        .iter()
        .enumerate()
        .find(|(_, o)| o.pages.len() != pages)
    {
        return Err(format!(
            "band {}/{count} produced {} page(s) but band 1 produced {pages}; \
             a banded program must emit the same pages in every band",
            i + 1,
            o.pages.len()
        ));
    }
    let row_bytes = width as usize * 4;
    (0..pages)
        .map(|k| {
            let mut page = Pixmap::new(width, height).ok_or("cannot allocate the page")?;
            let data = page.data_mut();
            for (i, out) in outputs.iter().enumerate() {
                let (top, bottom) = rows(i as u32, count, height);
                let span = top as usize * row_bytes..bottom as usize * row_bytes;
                let band = out.pages[k].data();
                if band.len() != data.len() {
                    return Err("a band's page has the wrong size".to_string());
                }
                data[span.clone()].copy_from_slice(&band[span]);
            }
            Ok(page)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_tile_the_canvas_without_gaps_or_overlap() {
        for height in [1u32, 7, 100, 792, 1101] {
            for count in 1..=height.min(MAX_BANDS) {
                let mut next = 0;
                for i in 0..count {
                    let (top, bottom) = rows(i, count, height);
                    assert_eq!(top, next, "h={height} n={count} i={i}");
                    assert!(bottom > top, "empty band h={height} n={count} i={i}");
                    next = bottom;
                }
                assert_eq!(next, height);
            }
        }
    }

    #[test]
    fn a_gate_releases_everyone_or_aborts_the_waiters() {
        let gate = StartGate::new(3);
        let all = std::thread::scope(|s| {
            let hs: Vec<_> = (0..3).map(|_| s.spawn(|| gate.arrive())).collect();
            hs.into_iter()
                .map(|h| h.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(all, [true; 3]);
        gate.abort(); // after everyone arrived: changes nothing
        assert!(gate.state.lock().unwrap().0 >= 3);

        let gate = StartGate::new(3);
        let waited = std::thread::scope(|s| {
            let h = s.spawn(|| gate.arrive());
            while gate.state.lock().unwrap().0 == 0 {
                std::thread::yield_now();
            }
            gate.abort();
            h.join().unwrap()
        });
        assert!(
            !waited,
            "a waiter is released, told the render was abandoned"
        );
    }

    #[test]
    fn count_is_bounded() {
        assert_eq!(parse_count("4"), Ok(4));
        assert!(parse_count("0").is_err());
        assert!(parse_count("65").is_err());
        assert!(parse_count("two").is_err());
    }
}
