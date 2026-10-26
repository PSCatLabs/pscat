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
//! When the caller knows where the cost is, `--band-cuts` (#869) sets the
//! band edges instead, as fractions of the page height from the top:
//! thin bands where rows are expensive, tall ones where they are cheap, so
//! the bands cost about the same. Equal-height bands can't get under the
//! cost of the hottest one, however they are queued.
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
//! `floor(i*H/K) .. floor((i+1)*H/K)`, or, with `--band-cuts`, the rows
//! between its cuts, each rounded to the nearest device row. [`Plan`]
//! holds the edges, and everything (the band's rows, `/BandBBox`, the
//! stitch) reads them from there.
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
//!
//! Output: only the top band writes stdout and stderr. The others capture
//! their stdout and drop their stderr (`(%stderr)` files, `handleerror`),
//! so an unaware program prints once. `findfont`'s stand-in notes from the
//! other bands are printed at the end, each once, unless band 0 printed
//! them already.

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
    if options.band_rows.is_some() && options.band_cuts.is_some() {
        return Err("--band-rows and --band-cuts each choose the bands; give one".to_string());
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
    /// `findfont`'s stand-in notes, newline-joined.
    font_notes: String,
}

/// How a render is cut up: bands on a canvas `height` rows tall, run by
/// `threads` workers. Band `i` owns device rows `edges[i]..edges[i + 1]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub threads: u32,
    pub height: u32,
    edges: Vec<u32>,
}

impl Plan {
    /// Bands of equal height (rows differ by at most one), as [`rows`] cuts them.
    fn equal(threads: u32, height: u32, count: u32) -> Plan {
        let count = count.clamp(1, height.max(1));
        let mut edges: Vec<u32> = (0..count).map(|i| rows(i, count, height).0).collect();
        edges.push(height);
        Plan::with_edges(threads, height, edges)
    }

    fn with_edges(threads: u32, height: u32, edges: Vec<u32>) -> Plan {
        let count = (edges.len() - 1) as u32;
        Plan {
            threads: threads.min(count).max(1),
            height,
            edges,
        }
    }

    /// How many bands there are (`/BandCount`).
    pub fn count(&self) -> u32 {
        (self.edges.len() - 1) as u32
    }

    /// The device rows band `index` owns: `top..bottom`.
    pub fn rows(&self, index: u32) -> (u32, u32) {
        (self.edges[index as usize], self.edges[index as usize + 1])
    }
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
    };
    Plan::equal(threads, height, count)
}

/// `--band-cuts`' split: a band edge at each cut, a fraction of the page
/// height from the top, rounded to the nearest device row. Fails if a cut
/// rounds to the top or bottom edge, or onto the row of the one before.
pub fn plan_cuts(threads: u32, height: u32, cuts: &[f64]) -> Result<Plan, String> {
    let mut edges = vec![0];
    for &cut in cuts {
        let row = (cut * f64::from(height)).round() as u32;
        let prev = *edges.last().unwrap_or(&0);
        if row == 0 || row >= height || row <= prev {
            return Err(format!(
                "--band-cuts: the cut at {cut} falls on device row {row} of {height}, \
                 leaving an empty band; use fewer or farther-apart cuts"
            ));
        }
        edges.push(row);
    }
    edges.push(height);
    Ok(Plan::with_edges(threads, height, edges))
}

/// `--band-cuts F1,F2,...`'s value: fractions of the page height from the
/// top, each strictly between 0 and 1, in increasing order.
pub fn parse_cuts(list: &str) -> Result<Vec<f64>, String> {
    let bad = || {
        format!(
            "invalid --band-cuts value: {list} (expected increasing fractions of the page \
             height between 0 and 1, comma-separated, e.g. 0.4,0.45,0.5)"
        )
    };
    let cuts: Vec<f64> = list
        .split(',')
        .map(|c| c.trim().parse::<f64>().map_err(|_| bad()))
        .collect::<Result<_, _>>()?;
    let in_range = cuts.iter().all(|c| *c > 0.0 && *c < 1.0);
    let increasing = cuts.windows(2).all(|w| w[0] < w[1]);
    if cuts.is_empty() || !in_range || !increasing {
        return Err(bad());
    }
    Ok(cuts)
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
    if options.band_rows.is_none() && options.band_cuts.is_none() && threads > height {
        eprintln!("pscat: --bands {threads} is more bands than the page has pixel rows ({height})");
        return ExitCode::FAILURE;
    }
    let plan = match &options.band_cuts {
        Some(cuts) => match plan_cuts(threads, height, cuts) {
            Ok(p) => p,
            Err(msg) => {
                eprintln!("pscat: {msg}");
                return ExitCode::FAILURE;
            }
        },
        None => plan(threads, height, options.band_rows),
    };

    let outputs = match render(options, &source, &plan, width, per_band) {
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
        eprintln!("pscat: band {}/{} failed:", i + 1, plan.count());
        eprintln!("{report}");
        failed = true;
    }
    print_font_notes(&outputs);
    let pages = match stitch(&outputs, &plan, width, options.transparent) {
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

/// The stand-in notes the bands other than 0 found, each once, leaving out
/// the ones band 0 printed as it ran. Their stderr is dropped, and a
/// band-aware program can reach a font in some bands only.
fn print_font_notes(outputs: &[Option<BandOutput>]) {
    let mut seen = std::collections::HashSet::new();
    let mut ran = outputs.iter().flatten();
    if let Some(top) = ran.next() {
        seen.extend(top.font_notes.lines());
    }
    for line in ran.flat_map(|o| o.font_notes.lines()) {
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
    plan: &Plan,
    width: u32,
    per_band: usize,
) -> Result<Vec<Option<BandOutput>>, String> {
    let queue = Queue {
        next: AtomicU32::new(0),
        stop: AtomicBool::new(false),
        count: plan.count(),
    };
    let base = MemoryBase(
        pscat::memtrack::live_bytes()
            .saturating_add(per_band.saturating_mul(plan.threads as usize)),
    );
    let slots: Vec<Mutex<Option<BandOutput>>> = (0..plan.count()).map(|_| Mutex::new(None)).collect();
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
                                fail(format!("band {}/{}: {msg}", index + 1, plan.count()));
                                break;
                            }
                            Err(_) => {
                                fail(format!("band {}/{} crashed", index + 1, plan.count()));
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
        return Err(format!("band {}/{} produced nothing", i + 1, plan.count()));
    }
    Ok(outputs)
}

/// One band of a plan.
#[derive(Clone, Copy)]
struct Band<'a> {
    index: u32,
    plan: &'a Plan,
    width: u32,
}

/// One band: a full run of the program with the band names defined, its
/// own rows of every page handed back.
fn run_band(
    options: &Options,
    source: &[u8],
    band: Band<'_>,
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
    let (top, bottom) = plan.rows(index);
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
        plan.count(),
        user_y(bottom),
        width_pt,
        user_y(top)
    );
    interp
        .run_source(preamble.as_bytes())
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
    })
}

/// Page `k` of the result is band `i`'s rows of its page `k`, in band
/// `i`'s rows. A band that never ran (the queue stopped at a limit) leaves
/// its rows blank: the page background, white or `transparent`.
fn stitch(
    outputs: &[Option<BandOutput>],
    plan: &Plan,
    width: u32,
    transparent: bool,
) -> Result<Vec<Pixmap>, String> {
    let (count, height) = (plan.count(), plan.height);
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
                let (top, bottom) = plan.rows(i as u32);
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
            (p.threads, p.count())
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
    fn the_equal_plan_uses_the_row_formula() {
        for (threads, height, rows_) in [(4, 792, None), (10, 200, None), (3, 101, Some(7))] {
            let p = plan(threads, height, rows_);
            for i in 0..p.count() {
                assert_eq!(p.rows(i), rows(i, p.count(), height));
            }
        }
    }

    #[test]
    fn band_cuts_round_to_rows_and_tile_the_canvas() {
        let p = plan_cuts(4, 200, &[0.41, 0.455, 0.5]).unwrap();
        assert_eq!(p.count(), 4);
        assert_eq!(p.threads, 4);
        let edges: Vec<_> = (0..4).map(|i| p.rows(i)).collect();
        assert_eq!(edges, [(0, 82), (82, 91), (91, 100), (100, 200)]);
        // More bands than threads: the queue takes them in turn.
        let p = plan_cuts(2, 200, &[0.1, 0.2, 0.3, 0.4]).unwrap();
        assert_eq!((p.threads, p.count()), (2, 5));
        // Fewer bands than threads: no idle workers.
        let p = plan_cuts(8, 200, &[0.5]).unwrap();
        assert_eq!((p.threads, p.count()), (2, 2));
        // A cut that rounds onto an edge or onto the cut before is refused.
        assert!(plan_cuts(4, 200, &[0.001]).is_err());
        assert!(plan_cuts(4, 200, &[0.999]).is_err());
        assert!(plan_cuts(4, 200, &[0.5, 0.501]).is_err());
    }

    #[test]
    fn band_cuts_parse() {
        assert_eq!(parse_cuts("0.4,0.45, 0.5"), Ok(vec![0.4, 0.45, 0.5]));
        for bad in ["", "0", "1", "0.5,0.4", "0.4,0.4", "-0.1", "half", "0.2,,0.3", "NaN"] {
            assert!(parse_cuts(bad).is_err(), "{bad:?}");
        }
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
