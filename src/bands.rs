//! `pscat --bands N` (issues #736, #844): render one PNG on N threads as
//! horizontal bands, then stitch the bands together.
//!
//! Each band is a separate, complete run of the program in its own
//! [`Interp`]. Nothing PostScript-visible is shared: the name interner is
//! thread-local, and the process-wide state (font catalog, resource roots,
//! font packs) is behind `OnceLock`/`RwLock`. Threads are cheaper than
//! spawning copies of pscat and need no temporary files.
//!
//! The bands are work, not threads (#844). The page is cut into
//! [`plan`]'s count of equal-height bands (by default two per thread, none
//! under eight rows; `--band-rows R` picks the height), and the N worker
//! threads take them from a shared queue, top band first, until none are
//! left. A scene whose cost is uneven down the page (a cheap sky over
//! expensive terrain) no longer waits on the one thread that drew the
//! expensive strip, and a slow efficiency core holds up less of the page.
//!
//! Before the program runs, userdict holds `/BandIndex` (0 is the top band),
//! `/BandCount` and `/BandBBox [llx lly urx ury]`, the band's area in default
//! user space: the queued band's, not the thread's. A program that knows
//! about bands can skip work outside its box. One that doesn't still renders
//! correctly: every band paints the whole page, pscat keeps only the band's
//! own device rows, and image rasterization (`image`, `colorimage`,
//! `imagemask`) skips the other rows. But it then pays for the whole program
//! once per band, so `--bands` is for programs that read the names. The rows
//! are whole device pixels and every band's pixels inside them are the ones
//! a single run paints, so the stitched PNG is byte-for-byte the single-run
//! PNG, anti-aliased edges across the seams included.
//!
//! Band `i` of `K` on a canvas `H` pixels tall owns rows
//! `floor(i*H/K) .. floor((i+1)*H/K)`.
//!
//! Output: only the top band (0) writes stdout and stderr, so a program
//! that is unaware of bands prints once, not once per band. The others
//! capture their stdout and drop their stderr (`(%stderr)` files,
//! `handleerror`, #877). What can still reach the user from them: an
//! uncaught error, which `run` reports once, from the first band that hit
//! it, labelled `band i/N failed`; and a font stand-in note a lower band
//! raised that band 0 did not, printed once at the end. A program that
//! catches an error itself and writes about it (to `(%stderr)` or with
//! `handleerror`) in one lower band only is not heard: that is its own
//! output, written by a copy the page doesn't keep.
//!
//! Lint (#847): `--lint` and `--lint-strict` work with `--bands`. The
//! findings are printed once, after the run, as in a plain render, with a
//! `pscat: lint: note:` line saying what was looked at. Band 0 (the top
//! band, whose stderr is the one kept) is linted for the declared page
//! count, `gsave` depth and the operand and dictionary stacks: program
//! behaviour, the same in every band unless the program branches on
//! `/BandIndex` (a leak in another band only goes unseen). The blank-page
//! check is the one lint reads the canvas for, and a band-aware program
//! leaves its own canvas empty outside the band, so it runs on the
//! stitched pages with each page's ink flag the OR across bands, which is
//! what a plain run sees. An error in any band makes the verdict not clean.
//!
//! Limits: `--max-steps` and `--max-time` apply to each band on its own (an
//! unaware program takes as many steps in every band as in a single run, so
//! the same limit fits). The first band to hit any limit stops the queue:
//! bands already running finish, none start, and the rows of those never run
//! stay blank in the partial page, so a runaway program costs about one
//! band's budget per thread, not one per queued band. Other errors don't stop
//! it, so the partial page is the one a plain render leaves. `--max-memory` is one budget for the whole render:
//! every band counts from one heap level, taken before the workers start,
//! that leaves out one fresh interpreter per worker ([`MemoryBase`]).

use std::io::Write;
use std::process::ExitCode;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};

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
    let others: [(bool, &str); 10] = [
        (options.svg.is_some(), "--svg"),
        (options.pdf.is_some(), "--pdf"),
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

/// What one band hands back: its own rows of every page it produced, and
/// its error report. Strings, not `PsError`s: those hold `Rc`s and cannot
/// leave the band's thread.
struct BandOutput {
    /// Per page, the band's device rows (RGBA, premultiplied, as the
    /// page's own bytes).
    rows: Vec<Vec<u8>>,
    /// The error report (and operand stack under `--pstack-on-error`) if
    /// the run failed.
    error: Option<String>,
    /// The run hit `--max-steps`, `--max-time` or `--max-memory`: the queue
    /// stops, so a runaway program is stopped after about one band's
    /// budget, not one per queued band.
    hit_limit: bool,
    /// The font stand-in notes this band raised, one per line
    /// (`Interp::font_warnings`).
    font_notes: String,
    /// Per page of `rows`, whether anything was painted on it since the
    /// previous page (the `--lint` ink flag). Every band reports it: a
    /// band-aware program skips work outside its band, so the page's real
    /// flag is the OR across bands.
    had_ink: Vec<bool>,
    /// `--lint`'s findings, from band 0 only and without the blank-page
    /// check (`lint::check_band`); `None` for the other bands, and when
    /// `--lint` is off.
    lint: Option<Vec<pscat::lint::LintFinding>>,
}

/// How a render is cut up: `count` bands on a canvas `height` rows tall,
/// run by `threads` workers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub threads: u32,
    pub count: u32,
    pub height: u32,
}

/// The split `--bands threads` uses when `--band-rows` doesn't choose one
/// (`notes/844-bands-work-queue.md` has the measurements): `PER_THREAD`
/// bands for each thread, none shorter than `MIN_ROWS` rows, and one band
/// for one thread (there is nothing to balance).
pub fn plan(threads: u32, height: u32, band_rows: Option<u32>) -> Plan {
    let count = match band_rows {
        Some(r) => height.div_ceil(r.max(1)),
        None if threads <= 1 => 1,
        None => (threads * PER_THREAD).min(height / MIN_ROWS).max(threads),
    }
    .clamp(1, height.max(1));
    Plan {
        threads: threads.min(count).max(1),
        count,
        height,
    }
}

/// Bands per thread in the default split.
const PER_THREAD: u32 = 2;
/// The shortest band the default split makes, in device rows.
const MIN_ROWS: u32 = 8;

/// `--band-rows R`'s value.
pub fn parse_rows(n: &str) -> Result<u32, String> {
    n.parse()
        .ok()
        .filter(|r| *r >= 1)
        .ok_or_else(|| format!("invalid --band-rows value: {n} (expected 1 or more rows)"))
}

/// `pscat --bands N`: check the options, run the bands, stitch and write.
pub fn run(options: &Options, threads: u32) -> ExitCode {
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
    // the main thread, so no band can fail to build its canvas. The heap the
    // probe takes is what one band's fresh interpreter costs: the memory
    // budget leaves that much out for every worker, as a plain render's
    // budget leaves out its own interpreter.
    let before = pscat::memtrack::live_bytes();
    let Some(probe) = Interp::with_page_scaled(options.page.0, options.page.1, scale) else {
        eprintln!(
            "pscat: unusable page size {}x{}",
            options.page.0, options.page.1
        );
        return ExitCode::FAILURE;
    };
    let per_band = pscat::memtrack::live_bytes().saturating_sub(before);
    let (width, height) = (probe.gfx().pixmap.width(), probe.gfx().pixmap.height());
    drop(probe);
    if options.band_rows.is_none() && threads > height {
        eprintln!("pscat: --bands {threads} is more bands than the page has pixel rows ({height})");
        return ExitCode::FAILURE;
    }
    let plan = plan(threads, height, options.band_rows);

    let outputs = match render(options, &source, plan, width, per_band) {
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
        .find_map(|(i, o)| o.as_ref()?.error.as_ref().map(|e| (i, e)))
    {
        eprintln!("pscat: band {}/{} failed:", i + 1, plan.count);
        eprintln!("{report}");
        failed = true;
    }
    print_font_notes(&outputs);
    let pages = match stitch(&outputs, width, height, options.transparent) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("pscat: {msg}");
            return ExitCode::FAILURE;
        }
    };
    if options.lint {
        // After the error report, so a `run-error` line's "see the report
        // above" is true. The other bands' stderr is dropped, so lint says
        // once, here, what it looked at.
        let findings = banded_lint(&outputs, &pages);
        let note = format!(
            "--bands: the page-count, gsave and stack checks ran on band 1 of {} (the top \
             band) only, as every band runs the same program; the blank-page check ran on the stitched page",
            plan.count
        );
        let found = crate::print_lint(&findings, !failed, options.lint_strict, Some(&note));
        failed |= found && options.lint_strict;
    }
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

/// `--lint`'s findings for a banded render (#847), in the order a plain
/// render gives them: the blank-page check on the stitched `pages`, each
/// page's ink flag the OR of every band's (a band-aware program skips work
/// outside its band, so band 0's canvas, or its flag, can be empty when the
/// page is not), then band 0's own checks (`lint::check_band`).
fn banded_lint(outputs: &[Option<BandOutput>], pages: &[Pixmap]) -> Vec<pscat::lint::LintFinding> {
    let ink = |k: usize| {
        outputs
            .iter()
            .flatten()
            .any(|o| o.had_ink.get(k).copied().unwrap_or(false))
    };
    let flagged: Vec<(&Pixmap, bool)> = pages
        .iter()
        .enumerate()
        .map(|(k, page)| (page, ink(k)))
        .collect();
    let mut findings = pscat::lint::blank_page_findings(&flagged);
    if let Some(top) = outputs.first().and_then(Option::as_ref)
        && let Some(own) = &top.lint
    {
        findings.extend(own.iter().map(|f| pscat::lint::LintFinding {
            check: f.check,
            message: f.message.clone(),
        }));
    }
    findings
}

/// The stand-in notes the bands other than 0 found, each once, leaving out
/// the ones band 0 printed as it ran. Their stderr is dropped, and a
/// band-aware program can reach a font in some bands only.
fn print_font_notes(outputs: &[Option<BandOutput>]) {
    let mut seen = std::collections::HashSet::new();
    let top = outputs.first().and_then(Option::as_ref);
    if let Some(top) = top {
        seen.extend(top.font_notes.lines());
    }
    for line in outputs
        .iter()
        .skip(1)
        .flatten()
        .flat_map(|o| o.font_notes.lines())
    {
        if seen.insert(line) {
            eprintln!("{line}");
        }
    }
}

/// What the workers share: the next band to take, and a flag that stops
/// them taking more after an internal failure.
struct Queue {
    next: AtomicU32,
    stop: AtomicBool,
    count: u32,
}

impl Queue {
    /// The next band nobody has taken, if any are left.
    fn take(&self) -> Option<u32> {
        if self.stop.load(Relaxed) {
            return None;
        }
        let i = self.next.fetch_add(1, Relaxed);
        (i < self.count).then_some(i)
    }

    /// True once every band has been handed out: a worker that has
    /// finished its band now will take no other.
    fn drained(&self) -> bool {
        self.stop.load(Relaxed) || self.next.load(Relaxed) >= self.count
    }
}

/// The process heap level that `--max-memory` is counted from, shared by
/// every band: what was live before the workers started, plus one fresh
/// interpreter per worker.
#[derive(Clone, Copy)]
struct MemoryBase(usize);

/// Run the plan's bands on its worker threads and collect what each band
/// produced, in band order: `None` for a band never run because another
/// hit a limit.
fn render(
    options: &Options,
    source: &[u8],
    plan: Plan,
    width: u32,
    per_band: usize,
) -> Result<Vec<Option<BandOutput>>, String> {
    let queue = Queue {
        next: AtomicU32::new(0),
        stop: AtomicBool::new(false),
        count: plan.count,
    };
    let base = MemoryBase(
        pscat::memtrack::live_bytes()
            .saturating_add(per_band.saturating_mul(plan.threads as usize)),
    );
    let slots: Vec<Mutex<Option<BandOutput>>> = (0..plan.count).map(|_| Mutex::new(None)).collect();
    let failure: Mutex<Option<String>> = Mutex::new(None);
    let fail = |msg: String| {
        queue.stop.store(true, Relaxed);
        if let Ok(mut f) = failure.lock() {
            f.get_or_insert(msg);
        }
    };
    std::thread::scope(|scope| {
        for worker in 0..plan.threads {
            let (queue, slots, fail) = (&queue, &slots, &fail);
            let spawned = std::thread::Builder::new()
                .name(format!("band-worker-{worker}"))
                .spawn_scoped(scope, move || {
                    while let Some(index) = queue.take() {
                        let band = Band { index, plan, width };
                        // Each band gets a fresh thread of its own. The
                        // name interner is per thread, and `forall` over
                        // name keys follows the interned ids, so a band
                        // must start from the ids a plain run starts from,
                        // not the ones an earlier band left behind. A
                        // panic is that band's failure: the worker stops,
                        // and the others stop taking bands.
                        let result = std::thread::scope(|band_scope| {
                            std::thread::Builder::new()
                                .name(format!("band-{index}"))
                                .stack_size(BAND_STACK)
                                .spawn_scoped(band_scope, || {
                                    // Count everything this band's thread
                                    // frees, its thread-locals included.
                                    pscat::memtrack::fold_at_thread_exit();
                                    run_band(options, source, band, base, queue)
                                })
                                .map(|h| h.join())
                        });
                        let result = match result {
                            Ok(joined) => joined,
                            Err(e) => {
                                fail(format!("cannot start a band thread: {e}"));
                                break;
                            }
                        };
                        match result {
                            Ok(Ok(out)) => {
                                if out.hit_limit {
                                    queue.stop.store(true, Relaxed);
                                }
                                if let Ok(mut slot) = slots[index as usize].lock() {
                                    *slot = Some(out);
                                }
                            }
                            Ok(Err(msg)) => {
                                fail(format!("band {}/{}: {msg}", index + 1, plan.count));
                                break;
                            }
                            Err(_) => {
                                fail(format!("band {}/{} crashed", index + 1, plan.count));
                                break;
                            }
                        }
                    }
                });
            if let Err(e) = spawned {
                // The workers already running finish the queue on their
                // own; a render with no worker at all has failed.
                if worker == 0 {
                    fail(format!("cannot start a band thread: {e}"));
                }
                break;
            }
        }
    });
    if let Some(msg) = failure.into_inner().ok().flatten() {
        return Err(msg);
    }
    let outputs: Vec<Option<BandOutput>> = slots
        .into_iter()
        .map(|slot| slot.into_inner().ok().flatten())
        .collect();
    // Only a limit stops the queue early; without one, every band ran.
    if !outputs.iter().flatten().any(|o| o.hit_limit)
        && let Some(i) = outputs.iter().position(Option::is_none)
    {
        return Err(format!("band {}/{} produced nothing", i + 1, plan.count));
    }
    Ok(outputs)
}

/// One band of a plan.
#[derive(Clone, Copy)]
struct Band {
    index: u32,
    plan: Plan,
    width: u32,
}

/// One band: a full run of the program with the band names defined, its
/// own rows of every page handed back.
fn run_band(
    options: &Options,
    source: &[u8],
    band: Band,
    base: MemoryBase,
    queue: &Queue,
) -> Result<BandOutput, String> {
    let Band { index, plan, width } = band;
    let scale = options.dpi / 72.0;
    let mut interp = Interp::with_page_scaled(options.page.0, options.page.1, scale)
        .ok_or("unusable page size")?;
    if options.transparent {
        interp.gfx_mut().set_transparent_background();
    }
    let height = plan.height;
    let (top, bottom) = rows(index, plan.count, height);
    interp.gfx_mut().set_band_rows(top, bottom);
    // Only the top band's output reaches stdout and stderr; the others'
    // stdout is kept in a bounded buffer and dropped, and their stderr is
    // dropped, so an unaware program prints once.
    if index > 0 {
        interp.capture_stdout();
        interp.quiet_stderr();
    }
    let width_pt = width as f32 / scale;
    let user_y = |row: u32| (height - row) as f32 / scale;
    let preamble = format!(
        "/BandIndex {index} def /BandCount {} def /BandBBox [0 {} {} {}] def",
        plan.count,
        user_y(bottom),
        width_pt,
        user_y(top)
    );
    // Detached, so the band names take none of the name ids a plain run
    // gives the program's own names (`forall` order follows the ids; #868).
    pscat::name::detached(|| interp.run_source(preamble.as_bytes()))
        .map_err(|e| format!("internal error defining the band names: {e}"))?;
    // Limits after the band names, so they bound the program alone, as in a
    // plain render. Memory is counted from the base all bands share, so the
    // bands running at once stay within one budget.
    interp
        .set_limits(options.limits)
        .map_err(|e| format!("limits: {e}"))?;
    interp.set_memory_base(base.0);
    // `--read-root`/`--confine-reads` (#763) bind every band, as they bind a
    // plain render.
    if let Some(dirs) = &options.read_roots {
        interp
            .set_read_roots(dirs)
            .map_err(|e| format!("--read-root: {e}"))?;
    }
    let mut hit_limit = false;
    let error = match interp.run_source(source) {
        Ok(()) => None,
        Err(e) => {
            hit_limit = e.limit().is_some();
            let mut report = interp.error_report(&e);
            if options.pstack_on_error {
                report.push('\n');
                report.push_str(&crate::pstack_text(&interp));
            }
            Some(report)
        }
    };
    let _ = std::io::stdout().flush();
    // The pages `--png` writes, chosen as `finish_headless` chooses them;
    // only this band's rows of each are kept.
    let span = top as usize * width as usize * 4..bottom as usize * width as usize * 4;
    let take = |page: &Pixmap| -> Result<Vec<u8>, String> {
        page.data()
            .get(span.clone())
            .map(<[u8]>::to_vec)
            .ok_or_else(|| "a band's page has the wrong size".to_string())
    };
    let mut rows = interp
        .gfx()
        .pages()
        .iter()
        .map(take)
        .collect::<Result<Vec<_>, _>>()?;
    if rows.is_empty() || interp.gfx().has_trailing_art() {
        rows.push(take(&interp.gfx().pixmap)?);
    }
    let had_ink = pscat::lint::pages_with_ink_flags(&interp)
        .iter()
        .map(|(_, ink)| *ink)
        .collect();
    // Lint reads the band's interpreter, which can't leave this thread, so
    // band 0 (the top band, the only one whose stderr is kept) lints here.
    let lint = (options.lint && index == 0)
        .then(|| pscat::lint::check_band(&interp, &pscat::lint::scan_declared_pages(source)));
    // The last band a worker runs leaves its dictionaries to the process
    // exit, as a plain render does (`main`); earlier ones free theirs, so
    // a long queue doesn't pile up dead interpreters.
    if queue.drained() {
        interp.leave_dicts_to_process_exit();
    }
    Ok(BandOutput {
        rows,
        error,
        hit_limit,
        font_notes: interp.font_warnings().to_string(),
        had_ink,
        lint,
    })
}

/// Page `k` of the result is band `i`'s rows of its page `k`, in band
/// `i`'s rows. A band that never ran (the queue stopped at a limit) leaves
/// its rows blank: the page background, white or `transparent`.
fn stitch(
    outputs: &[Option<BandOutput>],
    width: u32,
    height: u32,
    transparent: bool,
) -> Result<Vec<Pixmap>, String> {
    let count = outputs.len() as u32;
    let ran = || {
        outputs
            .iter()
            .enumerate()
            .filter_map(|(i, o)| Some((i, o.as_ref()?)))
    };
    let pages = ran().next().map_or(0, |(_, o)| o.rows.len());
    if let Some((i, o)) = ran().find(|(_, o)| o.rows.len() != pages) {
        return Err(format!(
            "band {}/{count} produced {} page(s) but the first band produced {pages}; \
             a banded program must emit the same pages in every band",
            i + 1,
            o.rows.len()
        ));
    }
    let row_bytes = width as usize * 4;
    (0..pages)
        .map(|k| {
            let mut page = Pixmap::new(width, height).ok_or("cannot allocate the page")?;
            if !transparent {
                page.fill(tiny_skia::Color::WHITE);
            }
            let data = page.data_mut();
            for (i, out) in ran() {
                let (top, bottom) = rows(i as u32, count, height);
                let span = top as usize * row_bytes..bottom as usize * row_bytes;
                let band = &out.rows[k];
                if band.len() != span.len() {
                    return Err("a band's page has the wrong size".to_string());
                }
                data[span].copy_from_slice(band);
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
    fn the_default_plan_queues_two_bands_a_thread() {
        let p = |threads, height, rows| {
            let p = plan(threads, height, rows);
            (p.threads, p.count)
        };
        assert_eq!(p(1, 200, None), (1, 1), "one thread, nothing to balance");
        assert_eq!(p(10, 200, None), (10, 20));
        assert_eq!(p(4, 792, None), (4, 8));
        // No band shorter than MIN_ROWS, but at least one per thread.
        assert_eq!(p(10, 100, None), (10, 12));
        assert_eq!(p(10, 40, None), (10, 10));
        // --band-rows: ceil(height / rows) bands, never more threads.
        assert_eq!(p(4, 200, Some(1)), (4, 200));
        assert_eq!(p(4, 200, Some(60)), (4, 4));
        assert_eq!(p(4, 200, Some(500)), (1, 1));
        assert_eq!(p(1, 200, Some(10)), (1, 20));
    }

    #[test]
    fn count_is_bounded() {
        assert_eq!(parse_count("4"), Ok(4));
        assert!(parse_count("0").is_err());
        assert!(parse_count("65").is_err());
        assert!(parse_count("two").is_err());
        assert_eq!(parse_rows("16"), Ok(16));
        assert!(parse_rows("0").is_err());
    }
}
