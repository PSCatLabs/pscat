//! Compiled per-pixel kernels: `kernelimage` (issue #742).
//!
//! ```text
//! width height matrix proc ncomp kernelimage -
//! proc ncomp kernelcompile bool string
//! name .kernelmode -        /auto, /interpret or /strict
//! ```
//!
//! `proc` is called as `i j proc c1 ... cn` for every sample of a `width`
//! by `height` grid (`i` the column, `j` the row in image data order), in
//! a fresh local dictionary per pixel, and its components (0..1) are
//! painted like an 8-bit `colorimage` with `matrix`.
//!
//! The procedure is compiled ([`compile`]) and run on all cores by the
//! lane-batched executor ([`exec`]) when it is in the compiled subset. The
//! result is byte for byte what the interpreter makes of it. Otherwise
//! (or when any pixel would raise a PostScript error) the same procedure
//! runs through the interpreter, pixel by pixel, in a fresh dict each
//! ([`fallback`]): the reference the compiled path is tested against.
//! The design and its measurements: `docs/dev/design/KERNELS.md`.

pub(crate) mod check;
pub(crate) mod compile;
pub(crate) mod exec;
#[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
pub(crate) mod gpu;
pub(crate) mod ir;
pub(crate) mod opt;
pub(crate) mod pure;
pub(crate) mod regalloc;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::PsError;
use crate::image::{ImageColor, ImageCtx, ImageSource};
use crate::interp::Interp;
use crate::object::{Dict, Object, PsArray, PsString, Value};
use exec::{LANES, Machine, RunCtl, Stop};
use ir::Kernel;

/// How `kernelimage` runs its procedure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KernelMode {
    /// Compile when possible, else interpret (with a one-time note).
    Auto,
    /// Always interpret: the reference path.
    Interpret,
    /// Compile, or fail with an error naming why not.
    Strict,
}

static GPU_PREVIEW: AtomicBool = AtomicBool::new(false);

/// Largest sample buffer a `kernelimage` may make (4 GiB).
const MAX_IMAGE_BYTES: u64 = 1 << 32;

/// Loop trips (all loops together) one pixel may run on the GPU
/// (`--gpu-preview`): the bound on a runaway loop there, which is capped
/// rather than counted as steps.
pub const GPU_LOOP_CAP: u32 = 1 << 20;
/// Pixels per GPU dispatch. A dispatch cannot be interrupted, so the time
/// and step limits are looked at between dispatches.
pub const GPU_CHUNK_PIXELS: usize = 1 << 16;

/// Run compiled kernels on the GPU, in f32, as a preview (`--gpu-preview`;
/// cargo feature `gpu`). Process-wide. Fails in a build without the
/// feature. Procedures that do not compile, and any GPU failure, still use
/// the CPU paths. Under a time or step limit the GPU runs too: the limits
/// are looked at before each dispatch of [`GPU_CHUNK_PIXELS`] pixels.
pub fn set_gpu_preview(on: bool) -> Result<(), String> {
    if on && !cfg!(all(feature = "gpu", not(target_arch = "wasm32"))) {
        return Err("--gpu-preview needs a pscat built with the `gpu` feature \
                    (cargo build --release --features gpu)"
            .into());
    }
    GPU_PREVIEW.store(on, Ordering::Relaxed);
    Ok(())
}

pub fn install(dict: &mut Dict) {
    use crate::ops::op;
    op(dict, "kernelimage", kernelimage);
    op(dict, "kernelcompile", kernelcompile);
    op(dict, ".kernelshare", kernelshare);
    op(dict, ".kernelmode", kernelmode);
    op(dict, ".kernelstats", kernelstats);
    // The drivers' own helpers (`.kfstore` and the rest) are not installed:
    // `substitute` puts them into the driver procedures as operator
    // objects, so no program can call them with a state dict of its own.
}

/// The drivers' private helpers, by the name the driver source uses.
fn helper(name: &str) -> Option<Object> {
    use crate::object::{OpFn, Operator};
    let (name, func): (&'static str, OpFn) = match name {
        ".kfstore" => (".kfstore", kfstore),
        ".kfrow" => (".kfrow", kfrow),
        ".kfnext" => (".kfnext", kfnext),
        // Named as the whole-image driver's operator: an error it raises
        // (a wrong component count) reads the same either way.
        ".kfput" => (".kfstore", kfput),
        ".kfdone" => (".kfdone", kfdone),
        ".kfpaint" => (".kfpaint", kfpaint),
        _ => return None,
    };
    Some(Object::exec(Value::Operator(Operator { name, func })))
}

/// A component (nominally 0..1) as an 8-bit sample: clamped to [0, 1],
/// then `floor(c * 255 + 0.5)`. NaN is 0. The one rule for both paths.
pub(crate) fn quantize(v: f64) -> u8 {
    let c = if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) };
    (c * 255.0 + 0.5).floor() as u8
}

struct Args {
    w: usize,
    h: usize,
    matrix: [f64; 6],
    proc: PsArray,
    ncomp: usize,
}

fn pop_args(it: &mut Interp) -> Result<Args, PsError> {
    let ncomp = it.pop_int()?;
    if !matches!(ncomp, 1 | 3 | 4) {
        return Err(PsError::Rangecheck);
    }
    let p = it.pop()?;
    let proc = match &p.value {
        Value::Array(a) if p.executable => a.clone(),
        _ => return Err(PsError::Typecheck),
    };
    let matrix = crate::ops::image::read_matrix6(&it.pop()?)?;
    let h = it.pop_int()?;
    let w = it.pop_int()?;
    if w < 0 || h < 0 {
        return Err(PsError::Rangecheck);
    }
    if w > 0 && h > 0 && crate::image::invert6(matrix).is_none() {
        return Err(PsError::UndefinedResult);
    }
    // Every size the paths compute from these must fit (untrusted input).
    let bytes = (w as usize)
        .checked_mul(h as usize)
        .and_then(|x| x.checked_mul(ncomp as usize))
        .filter(|&x| x as u64 <= MAX_IMAGE_BYTES);
    if bytes.is_none() {
        return Err(PsError::Limitcheck);
    }
    Ok(Args {
        w: w as usize,
        h: h as usize,
        matrix,
        proc,
        ncomp: ncomp as usize,
    })
}

fn kernelimage(it: &mut Interp) -> Result<(), PsError> {
    let a = pop_args(it)?;
    if a.w == 0 || a.h == 0 {
        return paint(it, &a, Vec::new());
    }
    let timing = std::env::var_os("PSCAT_KERNEL_TIMING").is_some();
    let t0 = crate::clock::Clock::mono_ns();
    let lap = |what: &str, t: u64| {
        if timing {
            eprintln!(
                "pscat: kernel {what}: {:.2} ms",
                (crate::clock::Clock::mono_ns() - t) as f64 / 1e6
            );
        }
    };
    if it.kernel_mode != KernelMode::Interpret {
        let strict = it.kernel_mode == KernelMode::Strict;
        let compiled = compile::compile(it, &a.proc, a.ncomp, strict, it.kernel_share);
        lap("compile", t0);
        if let Ok(c) = &compiled {
            it.charge_steps(c.work);
            for why in &c.soft {
                if it.kernel_notes.insert(why.clone()) {
                    let note = format!(
                        "pscat: kernelimage: a branch runs in the interpreter when taken ({why})\n"
                    );
                    let _ = it.write_stderr(note.as_bytes(), true);
                }
            }
        }
        if let Ok(c) = &compiled
            && timing
        {
            eprintln!(
                "pscat: kernel size: {}, {} registers",
                size_note(&c.kernel),
                c.kernel.nregs
            );
        }
        let t1 = crate::clock::Clock::mono_ns();
        match compiled {
            Ok(compile::Compiled {
                kernel: k,
                noise,
                soft,
                ..
            }) => {
                if noise {
                    crate::ops::noise::note_sampled(it);
                }
                let rows = rows_needed(it, a.w, a.h, a.matrix);
                // A GPU dispatch cannot be interrupted, so `gpu::run` looks
                // at the time and step limits before each one (#1065): a
                // limit stops the image with the error the CPU path would
                // raise. One dispatch is bounded by its pixel count and the
                // per-pixel loop budget; loop trips are not counted as steps.
                // A kernel with `srand` stays on the CPU (#1063): its state
                // must be exact, and the LCG's products do not fit the
                // shader's i32.
                #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
                if GPU_PREVIEW.load(Ordering::Relaxed)
                    && let Some(n) = a.w.checked_mul(a.h).and_then(|n| n.checked_mul(a.ncomp))
                    && it.check_alloc(n).is_ok()
                    && k.rand.is_none()
                {
                    let (steps_left, deadline) = it.remaining_budget();
                    let ctl = RunCtl::new(deadline, steps_left);
                    let r = gpu::run(&k, a.w, a.h, a.ncomp, rows, &ctl);
                    it.charge_steps(ctl.steps.load(Ordering::Relaxed));
                    lap("gpu run (with any device setup)", t1);
                    match r {
                        Ok(bytes) => {
                            if let Ok(name) = gpu::adapter_name()
                                && it.kernel_notes.insert(format!("gpu {name}"))
                            {
                                let note = format!("pscat: --gpu-preview: f32 on {name}\n");
                                let _ = it.write_stderr(note.as_bytes(), true);
                            }
                            it.kernel_stats.0 += 1;
                            return paint(it, &a, bytes);
                        }
                        Err(gpu::GpuStop::Limit(e)) => return Err(e),
                        Err(gpu::GpuStop::Failed(why)) => {
                            if it.kernel_notes.insert(why.clone()) {
                                let note = format!("pscat: --gpu-preview: {why}; using the CPU\n");
                                let _ = it.write_stderr(note.as_bytes(), true);
                            }
                        }
                    }
                }
                let r = run(it, &k, a.w, a.h, a.ncomp, rows);
                lap("cpu run", t1);
                match r {
                    Ok(ran) if ran.pixels.is_empty() => {
                        // The random state after the call: what the last
                        // pixel in raster order leaves (#1063).
                        match final_rand_state(it, &k, &a, &ran.seeds, rows) {
                            Ok(Some(found)) => {
                                if let Some(s) = found {
                                    it.rand_state = s;
                                    // Every compiled `srand` under an
                                    // override is a forced one (a restore
                                    // retires its lane): some pixel seeded,
                                    // so the override fired.
                                    if it.seed_override.is_some() {
                                        it.seed_override_fired = true;
                                    }
                                }
                                it.kernel_stats.0 += 1;
                                return paint(it, &a, ran.bytes);
                            }
                            // A pixel outside this band's rows retires
                            // before any seeds: the interpreter decides.
                            Ok(None) | Err(Stop::Cancelled) => {
                                it.kernel_stats.2 += 1;
                                return fallback(it, a);
                            }
                            Err(Stop::Limit(e)) => return Err(e),
                        }
                    }
                    Ok(ran) => {
                        // Some pixels would raise an error or took a branch
                        // that did not compile: the interpreter does those
                        // (and raises the error for real, at the same pixel).
                        let why = ran.why.unwrap_or("an error");
                        let note = format!("{} pixels: {why}", ran.pixels.len());
                        if it.kernel_notes.insert(why.to_string()) {
                            let line = format!(
                                "pscat: kernelimage: {note}; those run in the interpreter\n"
                            );
                            let _ = it.write_stderr(line.as_bytes(), true);
                        }
                        // With `srand` (#1063), a retired pixel's
                        // interpreted `rand` may read the state its compiled
                        // predecessor left, which the interpreter never saw:
                        // only the whole image in the interpreter is exact.
                        if k.rand.is_some() {
                            it.kernel_stats.2 += 1;
                            return fallback(it, a);
                        }
                        // An uncompiled branch may have side effects
                        // (`store`, `put`, `srand`) that pixels compiled from
                        // the snapshot did not see. Unless nothing the
                        // procedure can reach has any (#1093), the whole
                        // image runs in the interpreter.
                        if !soft.is_empty()
                            && let Some(why) = side_effect(it, &a.proc)
                        {
                            if it.kernel_notes.insert(format!("impure: {why}")) {
                                let line = format!(
                                    "pscat: kernelimage: the whole image runs in the \
                                     interpreter: a pixel that takes such a branch \
                                     cannot run alone ({why})\n"
                                );
                                let _ = it.write_stderr(line.as_bytes(), true);
                            }
                            it.kernel_stats.2 += 1;
                            return fallback(it, a);
                        }
                        it.kernel_stats.1 += 1;
                        return fallback_pixels(it, a, ran.bytes, ran.pixels);
                    }
                    Err(Stop::Limit(e)) => return Err(e),
                    Err(Stop::Cancelled) => {}
                }
            }
            Err(compile::Refusal::Limit(e)) => return Err(e),
            Err(compile::Refusal::Not(why, work)) => {
                it.charge_steps(work);
                if it.kernel_mode == KernelMode::Strict {
                    return Err(PsError::Undefined(format!(
                        "kernelimage: not compiled: {why}"
                    )));
                }
                if it.kernel_notes.insert(why.clone()) {
                    let note = format!("pscat: kernelimage: running in the interpreter ({why})\n");
                    let _ = it.write_stderr(note.as_bytes(), true);
                }
            }
        }
    }
    it.kernel_stats.2 += 1;
    fallback(it, a)
}

/// Why the pixels a soft branch retires cannot be interpreted one by one
/// after the compiled ones (`pure`), or `None` when they can.
fn side_effect(it: &Interp, proc: &PsArray) -> Option<String> {
    let t = crate::clock::Clock::mono_ns();
    let why = pure::impurity(it, proc, it.operand_stack());
    if std::env::var_os("PSCAT_KERNEL_TIMING").is_some() {
        eprintln!(
            "pscat: kernel side-effect scan: {:.2} ms",
            (crate::clock::Clock::mono_ns() - t) as f64 / 1e6
        );
    }
    why
}

/// `.kernelstats compiled partly interpreted`: how many `kernelimage`
/// calls the compiled path finished, finished with some pixels left to the
/// interpreter, and left wholly to the interpreter (a test hook: a
/// differential test must know the compiled path really ran).
fn kernelstats(it: &mut Interp) -> Result<(), PsError> {
    let (c, p, f) = it.kernel_stats;
    it.push(Object::int(c));
    it.push(Object::int(p));
    it.push(Object::int(f));
    Ok(())
}

/// Paint `bytes` (8-bit, `ncomp` per sample) as `colorimage` would.
fn paint(it: &mut Interp, a: &Args, bytes: Vec<u8>) -> Result<(), PsError> {
    let mut decode = Vec::new();
    for _ in 0..a.ncomp {
        decode.extend([0.0, 1.0]);
    }
    it.begin_image(ImageCtx::new(
        a.w,
        a.h,
        8,
        a.ncomp as u32,
        decode,
        a.matrix,
        None,
        ImageColor::Direct,
        ImageSource::Bytes(bytes),
    ))
}

/// Worker threads for a kernel: `PSCAT_KERNEL_THREADS`, else the cores;
/// one under `--bands` (the bands are already one thread each) and on wasm.
fn thread_count(it: &Interp) -> usize {
    if cfg!(target_arch = "wasm32") || it.gfx.band_rows().is_some() {
        return 1;
    }
    if let Some(n) = std::env::var("PSCAT_KERNEL_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
    {
        return n.clamp(1, 256);
    }
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

/// The sample rows whose footprint reaches this band's device rows (all
/// rows outside `--bands`). A band keeps only its own rows, so the rest
/// need not be computed (#736's image clamp does the same for painting).
fn rows_needed(it: &Interp, w: usize, h: usize, matrix: [f64; 6]) -> (usize, usize) {
    let Some((r0, r1)) = it.gfx.band_rows() else {
        return (0, h);
    };
    let Some(inv) = crate::image::invert6(matrix) else {
        return (0, h);
    };
    let ctm = it.gfx.ctm();
    let dev_y = |x: f64, y: f64| {
        let ux = inv[0] * x + inv[2] * y + inv[4];
        let uy = inv[1] * x + inv[3] * y + inv[5];
        f64::from(ctm.ky) * ux + f64::from(ctm.sy) * uy + f64::from(ctm.ty)
    };
    let (mut lo, mut hi) = (h, 0);
    for j in 0..h {
        let ys = [
            dev_y(0.0, j as f64),
            dev_y(w as f64, j as f64),
            dev_y(0.0, j as f64 + 1.0),
            dev_y(w as f64, j as f64 + 1.0),
        ];
        let ymin = ys.iter().copied().fold(f64::INFINITY, f64::min);
        let ymax = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        // One device row of margin each side, for rounding.
        if ymax >= f64::from(r0) - 1.0 && ymin <= f64::from(r1) + 1.0 {
            lo = lo.min(j);
            hi = hi.max(j + 1);
        }
    }
    if lo >= hi { (0, 0) } else { (lo, hi) }
}

/// Run a compiled kernel over the grid. Rows are dealt to the workers one
/// at a time; the bytes do not depend on which worker computes a row.
fn run(
    it: &mut Interp,
    k: &Kernel,
    w: usize,
    h: usize,
    ncomp: usize,
    (j0, j1): (usize, usize),
) -> Result<Ran, Stop> {
    let row_bytes = w * ncomp;
    let total = row_bytes
        .checked_mul(h)
        .ok_or(Stop::Limit(PsError::Limitcheck))?;
    if it.check_alloc(total).is_err() {
        return Err(Stop::Cancelled);
    }
    let mut out = vec![0u8; total];
    let (steps_left, deadline) = it.remaining_budget();
    let ctl = RunCtl::new(deadline, steps_left);
    let threads = thread_count(it).min(j1.saturating_sub(j0)).max(1);
    // Each worker's register file and local arrays, reserved against the
    // memory limit before any is made.
    let lane_bytes = (LANES * 8) as u64;
    let per = (k.nregs as u64).saturating_mul(lane_bytes).saturating_add(
        k.larrays.iter().fold(0u64, |acc, &n| {
            acc.saturating_add((n as u64).saturating_mul(2 * lane_bytes))
        }),
    );
    let all = per.saturating_mul(threads as u64);
    // Too much for the memory limit: not an error the program would get
    // from the interpreter, so the interpreter does the image instead.
    if it
        .check_alloc(usize::try_from(all).unwrap_or(usize::MAX))
        .is_err()
    {
        return Err(Stop::Cancelled);
    }
    let rows: Vec<(usize, &mut [u8])> = out
        .chunks_mut(row_bytes)
        .enumerate()
        .skip(j0)
        .take(j1.saturating_sub(j0))
        .collect();
    let queue = Mutex::new(rows.into_iter());
    let first_err: Mutex<Option<Stop>> = Mutex::new(None);
    let retired: Mutex<Retired> = Mutex::new((Vec::new(), None));
    let seeds: Mutex<Vec<(usize, i64)>> = Mutex::new(Vec::new());
    let work = |ctl: &RunCtl| {
        let mut m = Machine::new(k, ctl);
        let mut mine: Vec<(usize, usize)> = Vec::new();
        let mut my_seeds: Vec<(usize, i64)> = Vec::new();
        loop {
            let next = match queue.lock() {
                Ok(mut q) => q.next(),
                Err(_) => None,
            };
            let Some((j, row)) = next else { break };
            let mut i0 = 0;
            let mut r = Ok(());
            // The state the row's last seeding pixel left (`Kernel::rand`).
            let mut row_seed = None;
            while i0 < w {
                let n = (w - i0).min(LANES);
                match m.run_batch(
                    j as i64,
                    i0 as i64,
                    n,
                    &mut row[i0 * ncomp..(i0 + n) * ncomp],
                ) {
                    Ok(killed) => {
                        for l in (0..n).filter(|l| killed >> l & 1 != 0) {
                            mine.push((j, i0 + l));
                        }
                        if k.rand.is_some()
                            && let Some(s) = (0..n)
                                .rev()
                                .filter(|l| killed >> l & 1 == 0)
                                .find_map(|l| m.rand_state(l))
                        {
                            row_seed = Some(s);
                        }
                    }
                    Err(e) => {
                        r = Err(e);
                        break;
                    }
                }
                i0 += n;
            }
            if let Some(s) = row_seed {
                my_seeds.push((j, s));
            }
            if let Err(e) = r {
                ctl.stop.store(true, std::sync::atomic::Ordering::Relaxed);
                if let Ok(mut fe) = first_err.lock()
                    && (fe.is_none() || *fe == Some(Stop::Cancelled))
                {
                    *fe = Some(e);
                }
                break;
            }
        }
        if let Ok(mut sd) = seeds.lock() {
            sd.extend(my_seeds);
        }
        if let Ok(mut rt) = retired.lock() {
            rt.0.extend(mine);
            if rt.1.is_none() {
                rt.1 = m.why;
            }
        }
    };
    if threads <= 1 {
        work(&ctl);
    } else {
        std::thread::scope(|s| {
            for _ in 0..threads {
                s.spawn(|| work(&ctl));
            }
        });
    }
    let steps = ctl.steps.load(std::sync::atomic::Ordering::Relaxed);
    if std::env::var_os("PSCAT_KERNEL_TIMING").is_some() {
        eprintln!("pscat: kernel steps: {steps} lane-instructions");
    }
    it.charge_steps(steps);
    if let Ok(mut fe) = first_err.lock()
        && let Some(e) = fe.take()
    {
        return Err(e);
    }
    let (mut pixels, why) = retired.into_inner().unwrap_or_default();
    // Raster order, so the interpreter meets an error at the pixel it
    // would have met it at.
    pixels.sort_unstable();
    let mut seeds = seeds.into_inner().unwrap_or_default();
    seeds.sort_unstable();
    Ok(Ran {
        bytes: out,
        pixels,
        why,
        seeds,
    })
}

/// The random state an interpreted render leaves after the call: the one
/// the last pixel in raster order to call `srand` left (a pixel that never
/// seeds leaves the state as it found it). `Some(None)` when no pixel
/// seeds; `None` when a pixel after the last seeding one would retire (only
/// in rows the run did not compute: outside this `--bands` band).
///
/// `seeds` has the computed rows' answers (`rows`); any other row is run
/// again here, from the last row back, until a pixel that seeded is found.
fn final_rand_state(
    it: &mut Interp,
    k: &Kernel,
    a: &Args,
    seeds: &[(usize, i64)],
    (j0, j1): (usize, usize),
) -> Result<Option<Option<i64>>, Stop> {
    match k.rand {
        None => return Ok(Some(None)),
        Some(ir::Op::Imm(v)) if v < 0.0 => return Ok(Some(None)),
        _ => {}
    }
    let (steps_left, deadline) = it.remaining_budget();
    let ctl = RunCtl::new(deadline, steps_left);
    let mut m = Machine::new(k, &ctl);
    let mut buf = vec![0u8; LANES * a.ncomp];
    let mut scan = || -> Result<Option<Option<i64>>, Stop> {
        for j in (0..a.h).rev() {
            if (j0..j1).contains(&j) {
                // Computed, with no pixel retired.
                match seeds.binary_search_by_key(&j, |&(r, _)| r) {
                    Ok(x) => return Ok(Some(Some(seeds[x].1))),
                    Err(_) => continue,
                }
            }
            let mut hi = a.w;
            while hi > 0 {
                let i0 = hi.saturating_sub(LANES);
                let n = hi - i0;
                let killed = m.run_batch(j as i64, i0 as i64, n, &mut buf[..n * a.ncomp])?;
                for l in (0..n).rev() {
                    if killed >> l & 1 != 0 {
                        return Ok(None);
                    }
                    if let Some(s) = m.rand_state(l) {
                        return Ok(Some(Some(s)));
                    }
                }
                hi = i0;
            }
        }
        Ok(Some(None))
    };
    let r = scan();
    it.charge_steps(ctl.steps.load(std::sync::atomic::Ordering::Relaxed));
    r
}

/// Pixels (row, column) retired to the interpreter, and why the first was.
type Retired = (Vec<(usize, usize)>, Option<&'static str>);

/// A compiled run's result: the image, and the pixels (row, column) that
/// left the compiled path, for the interpreter to finish.
struct Ran {
    bytes: Vec<u8>,
    pixels: Vec<(usize, usize)>,
    why: Option<&'static str>,
    /// Per computed row that has a pixel that seeded (`srand`), the random
    /// state the last such pixel left, by row.
    seeds: Vec<(usize, i64)>,
}

/// `proc ncomp kernelcompile bool string`: would `kernelimage` compile
/// `proc`? The string is the reason when it would not (empty otherwise).
fn kernelcompile(it: &mut Interp) -> Result<(), PsError> {
    let ncomp = it.pop_int()?;
    if !(1..=64).contains(&ncomp) {
        return Err(PsError::Rangecheck);
    }
    let p = it.pop()?;
    let Value::Array(proc) = &p.value else {
        return Err(PsError::Typecheck);
    };
    if !p.executable {
        return Err(PsError::Typecheck);
    }
    let t0 = crate::clock::Clock::mono_ns();
    let compiled = compile::compile(it, proc, ncomp as usize, false, it.kernel_share);
    if std::env::var_os("PSCAT_KERNEL_TIMING").is_some() {
        eprintln!(
            "pscat: kernelcompile: {:.2} ms",
            (crate::clock::Clock::mono_ns() - t0) as f64 / 1e6
        );
    }
    match &compiled {
        Ok(c) => it.charge_steps(c.work),
        Err(compile::Refusal::Not(_, work)) => it.charge_steps(*work),
        Err(compile::Refusal::Limit(_)) => {}
    }
    let (ok, why) = match compiled {
        Ok(c) if c.soft.is_empty() => (true, size_note(&c.kernel)),
        Ok(compile::Compiled {
            kernel: k, soft, ..
        }) => {
            let alone = match pure::impurity(it, proc, &[]) {
                None => "the pixels that take one run alone".to_string(),
                Some(why) => format!("one taken runs the whole image there ({why})"),
            };
            (
                true,
                format!(
                    "{}; branches that run in the interpreter when taken: {}; {alone}",
                    size_note(&k),
                    soft.join("; ")
                ),
            )
        }
        Err(compile::Refusal::Not(why, _)) => (false, why),
        Err(compile::Refusal::Limit(e)) => return Err(e),
    };
    it.push(Object::bool(ok));
    it.push(Object::lit(Value::String(PsString::from_bytes(
        why.into_bytes(),
    ))));
    Ok(())
}

/// "N instructions", and how many of them form shared subroutines.
fn size_note(k: &ir::Kernel) -> String {
    match k.subs.len() {
        0 => format!("{} instructions", k.size()),
        n => format!("{} instructions, {n} shared subroutines", k.size()),
    }
}

fn kernelmode(it: &mut Interp) -> Result<(), PsError> {
    let v = it.pop()?;
    let Value::Name(n) = &v.value else {
        return Err(PsError::Typecheck);
    };
    it.kernel_mode = match &**n {
        "auto" => KernelMode::Auto,
        "interpret" => KernelMode::Interpret,
        "strict" => KernelMode::Strict,
        _ => return Err(PsError::Rangecheck),
    };
    Ok(())
}

/// `name .kernelshare -`: whether the compiler shares procedure bodies
/// as subroutines (#1064): `/auto` (only when inlining makes the kernel
/// too large), `/never`, `/large` (bodies of some size called again) or
/// `/all` (every named procedure: a test hook).
fn kernelshare(it: &mut Interp) -> Result<(), PsError> {
    let v = it.pop()?;
    let Value::Name(n) = &v.value else {
        return Err(PsError::Typecheck);
    };
    it.kernel_share = match &**n {
        "auto" => compile::ShareMode::Auto,
        "never" => compile::ShareMode::Never,
        "large" => compile::ShareMode::Large,
        "all" => compile::ShareMode::All,
        _ => return Err(PsError::Rangecheck),
    };
    Ok(())
}

// --- the interpreter path ------------------------------------------------

/// The driver: a `colorimage` whose data procedure calls `proc` per pixel
/// in a fresh dict, `KS` being the state dict (substituted in as an object,
/// so nothing here is looked up by name in the program's dictionaries).
const DRIVER: &str = "{ KS /W get KS /H get 8 KS /M get \
  { 0 1 KS /W get 1 sub { dup mark exch KS /J get KS /P get 8 dict begin exec end KS .kfstore } for \
    KS .kfrow } \
  false KS /N get colorimage }";

fn fallback(it: &mut Interp, a: Args) -> Result<(), PsError> {
    let mut ks = Dict::new();
    ks.put("W".into(), Object::int(a.w as i64));
    ks.put("H".into(), Object::int(a.h as i64));
    let m: Vec<Object> = a.matrix.iter().map(|v| Object::real(*v)).collect();
    ks.put("M".into(), Object::lit(Value::Array(PsArray::from_vec(m))));
    ks.put("P".into(), Object::exec(Value::Array(a.proc.clone())));
    ks.put("N".into(), Object::int(a.ncomp as i64));
    ks.put("J".into(), Object::int(0));
    it.check_alloc(a.w * a.ncomp)?;
    ks.put(
        "Row".into(),
        Object::lit(Value::String(PsString::from_bytes(vec![0; a.w * a.ncomp]))),
    );
    let ks = Object::lit(Value::Dict(Rc::new(RefCell::new(ks))));
    let Some((driver, _)) = it.scan_token_from(DRIVER.as_bytes().to_vec())? else {
        return Err(PsError::Undefined("kernelimage driver".into()));
    };
    let sys = it.systemdict();
    let driver = substitute(&driver, &ks, &sys.borrow());
    let Value::Array(body) = &driver.value else {
        return Err(PsError::Undefined("kernelimage driver".into()));
    };
    it.exec_object(Object::exec(Value::Array(body.clone())))
}

/// The interpreter for some pixels only: per pixel, as in [`DRIVER`], in a
/// fresh dict, in raster order; then the whole buffer is painted.
///
/// A pixel sees the operand stack as [`DRIVER`] shows it (`i mark i j` on
/// the caller's operands), and `.kfput` pops what `.kfstore` pops, so a
/// pixel that reaches under its own operands (uncompiled code can) does the
/// same in both. Two things only the whole image can say: a pixel that
/// `exit`s out of the procedure (out of the one-trip `repeat` here, out of
/// the row's `for` in [`DRIVER`]), and one that changed the `i` under its
/// mark ([`DRIVER`] stores the pixel in that column). Then `.kfput` puts the
/// caller's operands back and `.kfpaint` runs the whole image instead: a
/// pure procedure (`pure`) changed nothing else.
const PIXEL_DRIVER: &str = "{ { KS .kfnext not { exit } if \
    KS /P get 8 dict begin 1 { exec KS .kfdone } repeat end KS .kfput } loop \
  KS .kfpaint }";

fn fallback_pixels(
    it: &mut Interp,
    a: Args,
    bytes: Vec<u8>,
    pixels: Vec<(usize, usize)>,
) -> Result<(), PsError> {
    let mut ks = Dict::new();
    let m: Vec<Object> = a.matrix.iter().map(|v| Object::real(*v)).collect();
    ks.put("M".into(), Object::lit(Value::Array(PsArray::from_vec(m))));
    ks.put("P".into(), Object::exec(Value::Array(a.proc.clone())));
    ks.put("N".into(), Object::int(a.ncomp as i64));
    ks.put("W".into(), Object::int(a.w as i64));
    ks.put("H".into(), Object::int(a.h as i64));
    ks.put("K".into(), Object::int(0));
    ks.put("Done".into(), Object::bool(false));
    ks.put("Fail".into(), Object::bool(false));
    // The caller's operands, to put back for the whole image.
    let base = it.operand_stack().to_vec();
    ks.put(
        "Base".into(),
        Object::lit(Value::Array(PsArray::from_vec(base))),
    );
    it.check_alloc(
        pixels
            .len()
            .saturating_mul(2 * std::mem::size_of::<Object>()),
    )?;
    let flat: Vec<Object> = pixels
        .iter()
        .flat_map(|&(j, i)| [Object::int(i as i64), Object::int(j as i64)])
        .collect();
    ks.put(
        "Pix".into(),
        Object::lit(Value::Array(PsArray::from_vec(flat))),
    );
    ks.put(
        "Buf".into(),
        Object::lit(Value::String(PsString::from_bytes(bytes))),
    );
    let ks = Object::lit(Value::Dict(Rc::new(RefCell::new(ks))));
    let Some((driver, _)) = it.scan_token_from(PIXEL_DRIVER.as_bytes().to_vec())? else {
        return Err(PsError::Undefined("kernelimage driver".into()));
    };
    let sys = it.systemdict();
    let driver = substitute(&driver, &ks, &sys.borrow());
    let Value::Array(body) = &driver.value else {
        return Err(PsError::Undefined("kernelimage driver".into()));
    };
    it.exec_object(Object::exec(Value::Array(body.clone())))
}

/// Replace `KS` with the state dict and every other executable name with
/// its systemdict operator (a program's own definitions cannot change it).
fn substitute(obj: &Object, ks: &Object, sys: &Dict) -> Object {
    match &obj.value {
        Value::Name(n) if obj.executable => {
            if &**n == "KS" {
                return ks.clone();
            }
            if let Some(h) = helper(n) {
                return h;
            }
            match sys.get_name(n) {
                Some(o) => o,
                None => obj.clone(),
            }
        }
        Value::Array(a) => {
            let items: Vec<Object> = a
                .snapshot()
                .iter()
                .map(|o| substitute(o, ks, sys))
                .collect();
            Object {
                value: Value::Array(PsArray::from_vec(items)),
                executable: obj.executable,
            }
        }
        _ => obj.clone(),
    }
}

fn state(it: &mut Interp) -> Result<Rc<RefCell<Dict>>, PsError> {
    match it.pop()?.value {
        Value::Dict(d) => Ok(d),
        _ => Err(PsError::Typecheck),
    }
}

fn int_of(d: &Dict, k: &str) -> Result<i64, PsError> {
    match d.get(k).map(|o| o.value) {
        Some(Value::Integer(i)) => Ok(i),
        _ => Err(PsError::Typecheck),
    }
}

/// A non-negative integer entry. The state dicts are built here, but a
/// program can reach the drivers' operators (`execstack`), so every entry
/// is checked as untrusted input.
fn uint_of(d: &Dict, k: &str) -> Result<usize, PsError> {
    usize::try_from(int_of(d, k)?).map_err(|_| PsError::Rangecheck)
}

/// `i mark c1 .. cn KS .kfstore -`: one pixel's components into the row.
fn kfstore(it: &mut Interp) -> Result<(), PsError> {
    let ks = state(it)?;
    let ks = ks.borrow();
    let n = uint_of(&ks, "N")?;
    let comps = pop_components(it, n)?;
    let i = usize::try_from(it.pop_int()?).map_err(|_| PsError::Rangecheck)?;
    let Some(Value::String(row)) = ks.get("Row").map(|o| o.value) else {
        return Err(PsError::Typecheck);
    };
    let mut row = row.borrow_bytes_mut();
    for (c, v) in comps.iter().enumerate() {
        if let Some(b) = i
            .checked_mul(n)
            .and_then(|x| x.checked_add(c))
            .and_then(|x| row.get_mut(x))
        {
            *b = quantize(*v);
        }
    }
    Ok(())
}

/// `KS .kfnext i j true` for the next pixel to interpret, or `false`.
fn kfnext(it: &mut Interp) -> Result<(), PsError> {
    let ks = state(it)?;
    let mut ks = ks.borrow_mut();
    let k = uint_of(&ks, "K")?;
    let Some(Value::Array(pix)) = ks.get("Pix").map(|o| o.value) else {
        return Err(PsError::Typecheck);
    };
    if k.saturating_mul(2).saturating_add(1) >= pix.len() {
        it.push(Object::bool(false));
        return Ok(());
    }
    let (Some(i), Some(j)) = (pix.get(2 * k), pix.get(2 * k + 1)) else {
        return Err(PsError::Rangecheck);
    };
    ks.put("K".into(), Object::int(k as i64 + 1));
    ks.put("Done".into(), Object::bool(false));
    // `i mark i j`, as `DRIVER` leaves them for the procedure.
    it.push(i.clone());
    it.push(Object::lit(Value::Mark));
    it.push(i);
    it.push(j);
    it.push(Object::bool(true));
    Ok(())
}

/// `KS .kfdone -`: the current pixel's procedure returned.
fn kfdone(it: &mut Interp) -> Result<(), PsError> {
    let ks = state(it)?;
    ks.borrow_mut().put("Done".into(), Object::bool(true));
    Ok(())
}

/// `i mark c1 .. cn KS .kfput -`: the current pixel's components. It pops
/// what `DRIVER`'s `.kfstore` pops, with the same errors. A pixel that did
/// not return, or left another column than its own where `.kfstore` reads
/// it, cannot be finished alone (see [`PIXEL_DRIVER`]): the caller's
/// operands are put back and `Fail` set, for `.kfpaint` to run the whole
/// image instead.
fn kfput(it: &mut Interp) -> Result<(), PsError> {
    let ks = state(it)?;
    let mut ks = ks.borrow_mut();
    let n = uint_of(&ks, "N")?;
    let k = uint_of(&ks, "K")?
        .checked_sub(1)
        .ok_or(PsError::Rangecheck)?;
    let w = uint_of(&ks, "W")?;
    let Some(Value::Array(pix)) = ks.get("Pix").map(|o| o.value) else {
        return Err(PsError::Typecheck);
    };
    let at = |x: usize| match pix.get(x).map(|o| o.value) {
        Some(Value::Integer(v)) => usize::try_from(v).map_err(|_| PsError::Rangecheck),
        _ => Err(PsError::Typecheck),
    };
    let (i, j) = (at(2 * k)?, at(2 * k + 1)?);
    let Some(Value::Array(base)) = ks.get("Base").map(|o| o.value) else {
        return Err(PsError::Typecheck);
    };
    let done = matches!(ks.get("Done").map(|o| o.value), Some(Value::Boolean(true)));
    let comps = if done {
        let comps = pop_components(it, n)?;
        let col = usize::try_from(it.pop_int()?).map_err(|_| PsError::Rangecheck)?;
        (col == i).then_some(comps)
    } else {
        None
    };
    let Some(comps) = comps else {
        it.ostack.clear();
        it.ostack.extend(base.snapshot());
        ks.put("Fail".into(), Object::bool(true));
        let end = pix.len() / 2;
        ks.put("K".into(), Object::int(end as i64));
        return Ok(());
    };
    let base = j
        .checked_mul(w)
        .and_then(|x| x.checked_add(i))
        .and_then(|x| x.checked_mul(n))
        .ok_or(PsError::Rangecheck)?;
    let Some(Value::String(buf)) = ks.get("Buf").map(|o| o.value) else {
        return Err(PsError::Typecheck);
    };
    let mut buf = buf.borrow_bytes_mut();
    for (c, v) in comps.iter().enumerate() {
        if let Some(b) = base.checked_add(c).and_then(|x| buf.get_mut(x)) {
            *b = quantize(*v);
        }
    }
    Ok(())
}

/// `KS .kfpaint -`: paint the finished buffer, or, when a pixel could not
/// be finished alone (`Fail`), run the whole image in the interpreter.
fn kfpaint(it: &mut Interp) -> Result<(), PsError> {
    let ks = state(it)?;
    let failed = matches!(
        ks.borrow().get("Fail").map(|o| o.value),
        Some(Value::Boolean(true))
    );
    let (a, bytes) = {
        let ks = ks.borrow();
        let Some(Value::String(buf)) = ks.get("Buf").map(|o| o.value) else {
            return Err(PsError::Typecheck);
        };
        let Some(m) = ks.get("M") else {
            return Err(PsError::Typecheck);
        };
        let Some(Value::Array(p)) = ks.get("P").map(|o| o.value) else {
            return Err(PsError::Typecheck);
        };
        (
            Args {
                w: uint_of(&ks, "W")?,
                h: uint_of(&ks, "H")?,
                matrix: crate::ops::image::read_matrix6(&m)?,
                proc: p,
                ncomp: uint_of(&ks, "N")?,
            },
            buf.to_vec(),
        )
    };
    let need = a.w.checked_mul(a.h).and_then(|x| x.checked_mul(a.ncomp));
    if !matches!(a.ncomp, 1 | 3 | 4) || need != Some(bytes.len()) {
        return Err(PsError::Rangecheck);
    }
    if failed {
        it.kernel_stats.1 -= 1;
        it.kernel_stats.2 += 1;
        return fallback(it, a);
    }
    paint(it, &a, bytes)
}

/// The `n` components above the mark (popping them and the mark).
fn pop_components(it: &mut Interp, n: usize) -> Result<Vec<f64>, PsError> {
    let mut comps = Vec::with_capacity(n);
    loop {
        let o = it.pop()?;
        match o.value {
            Value::Mark => break,
            Value::Integer(i) => comps.push(i as f64),
            Value::Real(r) => comps.push(r),
            _ => return Err(PsError::Typecheck),
        }
        if comps.len() > n {
            return Err(PsError::Rangecheck);
        }
    }
    if comps.len() != n {
        return Err(PsError::Rangecheck);
    }
    comps.reverse();
    Ok(comps)
}

/// `KS .kfrow row`: the finished row; moves to the next.
fn kfrow(it: &mut Interp) -> Result<(), PsError> {
    let ks = state(it)?;
    let row = {
        let mut ks = ks.borrow_mut();
        let j = int_of(&ks, "J")?;
        ks.put("J".into(), Object::int(j.saturating_add(1)));
        ks.get("Row").ok_or(PsError::Typecheck)?
    };
    it.push(row);
    Ok(())
}
