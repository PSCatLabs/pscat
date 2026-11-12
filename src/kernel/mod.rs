//! Compiled per-pixel kernels: `kernelimage` (issue #742).
//!
//! ```text
//! width height matrix proc ncomp kernelimage -
//! proc ncomp kernelcompile bool string
//! name .kernelmode -        /auto, /interpret or /strict
//! w h proc n kernelarray array          (#1102)
//! w h proc n kernelarrays a1 .. an      (#1102)
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
//!
//! `kernelarray` runs the same kind of procedure over a `w` by `h` grid and
//! returns its `n` values per cell as numbers, each the integer or real the
//! interpreter would leave (cell `(i, j)` at `(j * w + i) * n`);
//! `kernelarrays` returns them as `n` arrays of `w * h`. Their reference is
//! [`CELL_DRIVER`], which runs the procedure cell by cell in raster order,
//! each in a fresh dict.

pub(crate) mod check;
pub(crate) mod code;
pub(crate) mod compile;
pub(crate) mod exec;
#[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
pub(crate) mod gpu;
pub(crate) mod ir;
pub(crate) mod opt;
pub(crate) mod pure;
pub(crate) mod regalloc;
pub(crate) mod vmath;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::PsError;
use crate::image::{ImageColor, ImageCtx, ImageSource};
use crate::interp::Interp;
use crate::object::{Dict, Object, PsArray, PsString, Value};
use compile::Out;
use exec::{LANES, Machine, RunCtl, Stop};
use ir::Kernel;

/// One value a `kernelarray` cell leaves: what the interpreter would leave
/// on the stack. (`Object` holds `Rc`s, so the workers make these.)
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum Num {
    Int(i64),
    #[default]
    Null,
    Real(f64),
}

impl Num {
    /// A real as `kernelarray` keeps it: a NaN as the one quiet NaN
    /// ([`canon_nan`]).
    pub(crate) fn real(r: f64) -> Num {
        Num::Real(canon_nan(r))
    }

    fn object(self) -> Object {
        match self {
            Num::Int(i) => Object::int(i),
            Num::Real(r) => Object::real(r),
            Num::Null => Object::lit(Value::Null),
        }
    }
}

/// `r`, or for any NaN the quiet NaN `0x7ff8000000000000`. A NaN's sign
/// and payload are not the same from one path to another: x86 arithmetic
/// passes on one operand's NaN, and which one depends on the order the
/// compiler put the operands in; Apple silicon returns its default NaN.
/// Nothing else can tell NaNs apart (comparisons with one retire the lane,
/// `cvi` of one is an error, `kernelimage` paints any as 0), so
/// `kernelarray` stores every NaN as this one, compiled and interpreted
/// (Codex review 3).
pub(crate) fn canon_nan(r: f64) -> f64 {
    if r.is_nan() { f64::NAN } else { r }
}

/// Most values one `kernelarray` may return: `array`'s own limit.
const MAX_CELLS: usize = 16_777_216;

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
    op(dict, "kernelarray", kernelarray);
    op(dict, "kernelarrays", kernelarrays);
    op(dict, "kernelcompile", kernelcompile);
    op(dict, ".kernelshare", kernelshare);
    op(dict, ".kernelcaps", kernelcaps);
    op(dict, ".kernelmode", kernelmode);
    op(dict, ".kernelmath", kernelmath);
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
        ".kfpaint" => (".kfpaint", kfpaint),
        ".kanext" => (".kanext", kanext),
        ".kaput" => (".kaput", kaput),
        ".kadone" => (".kadone", kadone),
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
        let compiled = compile_noted(it, "kernelimage", &a.proc, Out::image(a.ncomp));
        lap("compile", t0);
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
                let fast = it.kernel_fast_math;
                let g = Grid {
                    w: a.w,
                    h: a.h,
                    ncomp: a.ncomp,
                    rows,
                    fast,
                };
                let r = run(it, &k, g, |m, l, out| m.pixel(l, out));
                lap("cpu run", t1);
                match r {
                    Ok(ran) if ran.pixels.is_empty() => {
                        // The random state after the call: what the last
                        // pixel in raster order leaves (#1063).
                        match final_rand_state(it, &k, a.w, a.h, &ran.seeds, rows, fast) {
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
                        // With `srand` (#1063), a retired pixel's
                        // interpreted `rand` may read the state its compiled
                        // predecessor left, which the interpreter never saw.
                        // An uncompiled branch may have side effects (`store`,
                        // `put`, `srand`) that pixels compiled from the
                        // snapshot did not see: unless nothing the procedure
                        // can reach has any (#1093), only the whole image in
                        // the interpreter is exact. `soft` is read off the
                        // finished kernel (#1092), and the scan is of the whole
                        // procedure, so no verdict outlives a redone pass.
                        let whole = if k.rand.is_some() {
                            Some("a retired pixel's rand may read state a compiled one left".into())
                        } else if !soft.is_empty() && rows != (0, a.h) {
                            // Under `--bands` the rows outside this band are
                            // not computed, so a soft pixel there, which can
                            // still change the operand stack or raise an
                            // error, would not run at all.
                            Some("a branch that did not compile, under --bands".into())
                        } else if !soft.is_empty() {
                            side_effect(it, &a.proc).map(|why| {
                                format!(
                                    "a pixel that takes a branch that did not compile \
                                     cannot run alone: {why}"
                                )
                            })
                        } else {
                            None
                        };
                        retire_note(it, "kernelimage", &k, &ran, whole.as_deref(), timing);
                        if whole.is_some() {
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
            Err(compile::Refusal::Not(..)) => {}
        }
    }
    it.kernel_stats.2 += 1;
    fallback(it, a)
}

/// Compile `proc` for operator `name`, saying what the user should know:
/// branches left to the interpreter, or why it did not compile (an error
/// under `/strict`). The steps it took are charged.
fn compile_noted(
    it: &mut Interp,
    name: &str,
    proc: &PsArray,
    out: Out,
) -> Result<compile::Compiled, compile::Refusal> {
    let strict = it.kernel_mode == KernelMode::Strict;
    let compiled = compile::compile(it, proc, out, strict, it.kernel_share);
    match &compiled {
        Ok(c) => {
            it.charge_steps(c.work);
            for why in &c.soft {
                if it.kernel_notes.insert(why.clone()) {
                    let note = format!(
                        "pscat: {name}: a branch runs in the interpreter when taken ({why})\n"
                    );
                    let _ = it.write_stderr(note.as_bytes(), true);
                }
            }
            if std::env::var_os("PSCAT_KERNEL_TIMING").is_some() {
                eprintln!(
                    "pscat: kernel size: {}, {} registers",
                    size_note(&c.kernel),
                    c.kernel.nregs
                );
            }
        }
        Err(compile::Refusal::Not(why, work)) => {
            it.charge_steps(*work);
            if strict {
                return Err(compile::Refusal::Limit(PsError::Undefined(format!(
                    "{name}: not compiled: {why}"
                ))));
            }
            if it.kernel_notes.insert(why.clone()) {
                let note = format!("pscat: {name}: running in the interpreter ({why})\n");
                let _ = it.write_stderr(note.as_bytes(), true);
            }
        }
        Err(compile::Refusal::Limit(_)) => {}
    }
    compiled
}

/// `kernelarray`'s and `kernelarrays`' operands.
struct Cells {
    w: usize,
    h: usize,
    proc: PsArray,
    n: usize,
    /// `kernelarrays`: `n` arrays of `w * h`, not one of `w * h * n`.
    split: bool,
}

impl Cells {
    fn name(&self) -> &'static str {
        if self.split {
            "kernelarrays"
        } else {
            "kernelarray"
        }
    }
}

fn pop_cells(it: &mut Interp, split: bool) -> Result<Cells, PsError> {
    let n = it.pop_int()?;
    if !(1..=64).contains(&n) {
        return Err(PsError::Rangecheck);
    }
    let p = it.pop()?;
    let proc = match &p.value {
        Value::Array(a) if p.executable => a.clone(),
        _ => return Err(PsError::Typecheck),
    };
    let h = it.pop_int()?;
    let w = it.pop_int()?;
    if w < 0 || h < 0 {
        return Err(PsError::Rangecheck);
    }
    // Every size computed from these must fit (untrusted input).
    let cells = (w as u64)
        .checked_mul(h as u64)
        .and_then(|x| x.checked_mul(n as u64));
    if cells.is_none_or(|c| c > MAX_CELLS as u64) {
        return Err(PsError::Limitcheck);
    }
    Ok(Cells {
        w: w as usize,
        h: h as usize,
        proc,
        n: n as usize,
        split,
    })
}

/// `w h proc n kernelarray array` (#1102): `i j proc v1 .. vn` for every
/// cell of a `w` by `h` grid, in a fresh dict each, as `kernelimage` calls
/// its procedure; the values, exactly as the interpreter leaves them, in a
/// new array, cell `(i, j)`'s at `(j * w + i) * n`.
fn kernelarray(it: &mut Interp) -> Result<(), PsError> {
    let a = pop_cells(it, false)?;
    cells(it, a)
}

/// `w h proc n kernelarrays a1 .. an` (#1102): as `kernelarray`, the
/// values as `n` arrays, `ak` holding each cell's `vk` at `j * w + i`.
fn kernelarrays(it: &mut Interp) -> Result<(), PsError> {
    let a = pop_cells(it, true)?;
    cells(it, a)
}

fn cells(it: &mut Interp, a: Cells) -> Result<(), PsError> {
    let total = a.w * a.h * a.n;
    // The result, made up front as the interpreter's path makes it.
    it.check_alloc(total.saturating_mul(std::mem::size_of::<Object>()))?;
    if total == 0 {
        return push_cells(it, &a, Vec::new());
    }
    let timing = std::env::var_os("PSCAT_KERNEL_TIMING").is_some();
    let name = a.name();
    if it.kernel_mode != KernelMode::Interpret {
        let t0 = crate::clock::Clock::mono_ns();
        let compiled = compile_noted(it, name, &a.proc, Out::array(a.n));
        if timing {
            eprintln!(
                "pscat: kernel compile: {:.2} ms",
                (crate::clock::Clock::mono_ns() - t0) as f64 / 1e6
            );
        }
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
                // Every row, under `--bands` too: the array is the result.
                let rows = (0, a.h);
                let t1 = crate::clock::Clock::mono_ns();
                // Always the exact path: `.kernelmath` is `kernelimage`'s.
                let g = Grid {
                    w: a.w,
                    h: a.h,
                    ncomp: a.n,
                    rows,
                    fast: false,
                };
                let r = run(it, &k, g, |m, l, out| m.cell(l, out));
                if timing {
                    eprintln!(
                        "pscat: kernel cpu run: {:.2} ms",
                        (crate::clock::Clock::mono_ns() - t1) as f64 / 1e6
                    );
                }
                match r {
                    Ok(ran) if ran.pixels.is_empty() => {
                        // As `kernelimage`: the random state the last cell
                        // in raster order leaves (#1063).
                        match final_rand_state(it, &k, a.w, a.h, &ran.seeds, rows, false) {
                            Ok(Some(found)) => {
                                if let Some(s) = found {
                                    it.rand_state = s;
                                    if it.seed_override.is_some() {
                                        it.seed_override_fired = true;
                                    }
                                }
                                it.kernel_stats.0 += 1;
                                return push_cells(it, &a, ran.bytes);
                            }
                            Ok(None) | Err(Stop::Cancelled) => {
                                it.kernel_stats.2 += 1;
                                return cells_fallback(it, a, None);
                            }
                            Err(Stop::Limit(e)) => return Err(e),
                        }
                    }
                    Ok(ran) => {
                        // Cells that would raise an error, or took a branch
                        // that did not compile: the interpreter does those,
                        // in raster order, unless only the whole grid there
                        // is exact (as for `kernelimage`).
                        let whole = if k.rand.is_some() {
                            Some("a retired cell's rand may read state a compiled one left".into())
                        } else if !soft.is_empty() {
                            side_effect(it, &a.proc).map(|why| {
                                format!(
                                    "a cell that takes a branch that did not compile \
                                     cannot run alone: {why}"
                                )
                            })
                        } else {
                            None
                        };
                        retire_note(it, name, &k, &ran, whole.as_deref(), timing);
                        if whole.is_some() {
                            it.kernel_stats.2 += 1;
                            return cells_fallback(it, a, None);
                        }
                        it.kernel_stats.1 += 1;
                        return cells_fallback(it, a, Some((ran.bytes, ran.pixels)));
                    }
                    Err(Stop::Limit(e)) => return Err(e),
                    Err(Stop::Cancelled) => {}
                }
            }
            Err(compile::Refusal::Limit(e)) => return Err(e),
            Err(compile::Refusal::Not(..)) => {}
        }
    }
    it.kernel_stats.2 += 1;
    cells_fallback(it, a, None)
}

/// Push the result: one array of the values, or (`kernelarrays`) `n`.
fn push_cells(it: &mut Interp, a: &Cells, vals: Vec<Num>) -> Result<(), PsError> {
    if !a.split {
        it.push(Object::array(vals.into_iter().map(Num::object).collect()));
        return Ok(());
    }
    let cells = a.w * a.h;
    for c in 0..a.n {
        let col: Vec<Object> = (0..cells)
            .map(|x| vals.get(x * a.n + c).copied().unwrap_or_default().object())
            .collect();
        it.push(Object::array(col));
    }
    Ok(())
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

/// The batch's tile, columns by rows (#1101): 8x2 ran fewest instructions
/// on fractal_clouds' scenes (golden_coast: 5% fewer than a row of 16, and
/// 4x4 in between) and on the gallery's. `PSCAT_KERNEL_TILE=WxH` overrides
/// it, for measuring.
fn tile_shape() -> (usize, usize) {
    if let Some((w, h)) = std::env::var("PSCAT_KERNEL_TILE").ok().and_then(|s| {
        let (w, h) = s.split_once('x')?;
        Some((w.parse::<usize>().ok()?, h.parse::<usize>().ok()?))
    }) && w >= 1
        && h >= 1
        && w * h <= LANES
    {
        return (w, h);
    }
    (LANES / 2, 2)
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

/// What [`run`] computes: a `w` by `h` grid of `ncomp` outputs each, the
/// rows `rows` of it, and with `.kernelmath /fast` or not.
struct Grid {
    w: usize,
    h: usize,
    ncomp: usize,
    rows: (usize, usize),
    fast: bool,
}

/// Run a compiled kernel over the grid, `emit` writing a pixel's `ncomp`
/// outputs (bytes for `kernelimage`, [`Num`]s for `kernelarray`). Rows are
/// dealt to the workers one at a time; the result does not depend on which
/// worker computes a row.
fn run<T, F>(it: &mut Interp, k: &Kernel, g: Grid, emit: F) -> Result<Ran<T>, Stop>
where
    T: Copy + Default + Send,
    F: Fn(&Machine, usize, &mut [T]) + Sync,
{
    let Grid {
        w,
        h,
        ncomp,
        rows: (j0, j1),
        fast,
    } = g;
    let row_bytes = w * ncomp;
    let total = row_bytes
        .checked_mul(h)
        .ok_or(Stop::Limit(PsError::Limitcheck))?;
    if it
        .check_alloc(total.saturating_mul(std::mem::size_of::<T>()))
        .is_err()
    {
        return Err(Stop::Cancelled);
    }
    let mut out = vec![T::default(); total];
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
    // A batch is a tile of TW x TH pixels (#1101): nearby pixels mostly
    // take the same branches.
    let (tw, th) = tile_shape();
    let bands: Vec<(usize, &mut [T])> = out[j0 * row_bytes..j1.max(j0) * row_bytes]
        .chunks_mut(row_bytes * th)
        .enumerate()
        .map(|(b, rows)| (j0 + b * th, rows))
        .collect();
    let queue = Mutex::new(bands.into_iter());
    let code = code::Code::new(k, fast);
    let first_err: Mutex<Option<Stop>> = Mutex::new(None);
    let retired: Mutex<Retired> = Mutex::new((Vec::new(), Vec::new()));
    let seeds: Mutex<Vec<(usize, i64)>> = Mutex::new(Vec::new());
    let dispatched = std::sync::atomic::AtomicU64::new(0);
    let work = |ctl: &RunCtl| {
        let mut m = Machine::new(k, &code, ctl);
        let mut mine: Vec<(usize, usize)> = Vec::new();
        let mut my_seeds: Vec<(usize, i64)> = Vec::new();
        let mut at = [(0i64, 0i64); LANES];
        loop {
            let next = match queue.lock() {
                Ok(mut q) => q.next(),
                Err(_) => None,
            };
            let Some((jb, rows)) = next else { break };
            let nrows = rows.len() / row_bytes;
            let mut i0 = 0;
            let mut r = Ok(());
            // The state each row's last seeding pixel left (`Kernel::rand`).
            let mut row_seed = [None; LANES];
            while i0 < w {
                let cols = (w - i0).min(tw);
                let mut n = 0;
                for y in 0..nrows {
                    for x in 0..cols {
                        at[n] = ((i0 + x) as i64, (jb + y) as i64);
                        n += 1;
                    }
                }
                match m.run_lanes(&at[..n]) {
                    Ok(killed) => {
                        for (l, &(i, j)) in at[..n].iter().enumerate() {
                            let (i, j) = (i as usize, j as usize);
                            if killed >> l & 1 != 0 {
                                mine.push((j, i));
                            } else {
                                let o = (j - jb) * row_bytes + i * ncomp;
                                emit(&m, l, &mut rows[o..o + ncomp]);
                            }
                        }
                        if k.rand.is_some() {
                            // Raster order: the tile's rightmost seeding
                            // pixel in each row, later tiles winning.
                            for (y, seed) in row_seed.iter_mut().enumerate().take(nrows) {
                                if let Some(s) = (0..cols)
                                    .rev()
                                    .map(|x| y * cols + x)
                                    .filter(|l| killed >> l & 1 == 0)
                                    .find_map(|l| m.rand_state(l))
                                {
                                    *seed = Some(s);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        r = Err(e);
                        break;
                    }
                }
                i0 += cols;
            }
            for (y, s) in row_seed.iter().enumerate().take(nrows) {
                if let Some(s) = s {
                    my_seeds.push((jb + y, *s));
                }
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
        dispatched.fetch_add(m.dispatched, std::sync::atomic::Ordering::Relaxed);
        if let Ok(mut rt) = retired.lock() {
            rt.0.extend(mine);
            if rt.1.len() < m.retired.len() {
                rt.1.resize(m.retired.len(), 0);
            }
            for (a, b) in rt.1.iter_mut().zip(&m.retired) {
                *a += b;
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
        let d = dispatched.load(std::sync::atomic::Ordering::Relaxed);
        if d > 0 {
            eprintln!(
                "pscat: kernel batches: {tw}x{th} pixels, {d} instructions run, {:.1} of {LANES} lanes active on average, {}, {} math",
                steps as f64 / d as f64,
                exec::Isa::chosen().name(),
                if fast { "fast" } else { "exact" }
            );
        }
    }
    it.charge_steps(steps);
    if let Ok(mut fe) = first_err.lock()
        && let Some(e) = fe.take()
    {
        return Err(e);
    }
    let (mut pixels, counts) = retired.into_inner().unwrap_or_default();
    // Most first; ties in reason order, so the report is deterministic.
    let mut why: Vec<(u64, usize)> = counts
        .into_iter()
        .enumerate()
        .filter(|&(_, n)| n > 0)
        .map(|(c, n)| (n, c))
        .collect();
    why.sort_by_key(|&(n, c)| (std::cmp::Reverse(n), c));
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
    w: usize,
    h: usize,
    seeds: &[(usize, i64)],
    (j0, j1): (usize, usize),
    fast: bool,
) -> Result<Option<Option<i64>>, Stop> {
    match k.rand {
        None => return Ok(Some(None)),
        Some(ir::Op::Imm(v)) if v < 0.0 => return Ok(Some(None)),
        _ => {}
    }
    let (steps_left, deadline) = it.remaining_budget();
    let ctl = RunCtl::new(deadline, steps_left);
    let code = code::Code::new(k, fast);
    let mut m = Machine::new(k, &code, &ctl);
    let mut at = [(0i64, 0i64); LANES];
    let mut scan = || -> Result<Option<Option<i64>>, Stop> {
        for j in (0..h).rev() {
            if (j0..j1).contains(&j) {
                // Computed, with no pixel retired.
                match seeds.binary_search_by_key(&j, |&(r, _)| r) {
                    Ok(x) => return Ok(Some(Some(seeds[x].1))),
                    Err(_) => continue,
                }
            }
            let mut hi = w;
            while hi > 0 {
                let i0 = hi.saturating_sub(LANES);
                let n = hi - i0;
                for (l, p) in at.iter_mut().enumerate().take(n) {
                    *p = ((i0 + l) as i64, j as i64);
                }
                let killed = m.run_lanes(&at[..n])?;
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

/// Pixels (row, column) retired to the interpreter, and how many retired
/// for each reason code (`exec::Why`).
type Retired = (Vec<(usize, usize)>, Vec<u64>);

/// A compiled run's result: the image, and the pixels (row, column) that
/// left the compiled path, for the interpreter to finish.
struct Ran<T> {
    bytes: Vec<T>,
    pixels: Vec<(usize, usize)>,
    /// How many pixels retired for each reason code, most first.
    why: Vec<(u64, usize)>,
    /// Per computed row that has a pixel that seeded (`srand`), the random
    /// state the last such pixel left, by row.
    seeds: Vec<(usize, i64)>,
}

/// Say why pixels of a compiled run retired to the interpreter: one note
/// naming the commonest reason (once per reason), and under
/// `PSCAT_KERNEL_TIMING` every reason with its count.
///
/// `whole` says why, when it is so, the whole image reruns in the
/// interpreter rather than only the retired pixels.
fn retire_note<T>(
    it: &mut Interp,
    name: &str,
    k: &Kernel,
    ran: &Ran<T>,
    whole: Option<&str>,
    timing: bool,
) {
    let n = ran.pixels.len();
    let (what, whole_of) = if name == "kernelimage" {
        ("pixels", "image")
    } else {
        ("cells", "grid")
    };
    let Some(&(top_n, top)) = ran.why.first() else {
        return;
    };
    let top_text = exec::why_text(k, top);
    if it
        .kernel_notes
        .insert(format!("retired {top_text} {}", whole.is_some()))
    {
        let rest = ran.why.len() - 1;
        let mut line = if rest == 0 {
            format!("pscat: {name}: {n} {what} retired: {top_text}")
        } else {
            let s = if rest == 1 { "" } else { "s" };
            format!(
                "pscat: {name}: {n} {what} retired: {top_n} for {top_text}, {} for \
                 {rest} other reason{s} (PSCAT_KERNEL_TIMING=1 lists them)",
                n as u64 - top_n
            )
        };
        match whole {
            Some(why) => line.push_str(&format!(
                "; the whole {whole_of} runs in the interpreter ({why})\n"
            )),
            None => line.push_str("; those run in the interpreter\n"),
        }
        let _ = it.write_stderr(line.as_bytes(), true);
    }
    if timing {
        for &(c, code) in &ran.why {
            eprintln!(
                "pscat: kernel retired: {c} {what}: {}",
                exec::why_text(k, code)
            );
        }
    }
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
    let compiled = compile::compile(it, proc, Out::image(ncomp as usize), false, it.kernel_share);
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
            // The operands under this call stand in for those under the
            // `kernelimage` call (a pixel can reach them).
            let alone = match pure::impurity(it, proc, it.operand_stack()) {
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

/// Whether a new interpreter starts in `.kernelmath /fast` (#1100): yes,
/// unless the environment variable `PSCAT_KERNEL_MATH` is `exact`.
pub(crate) fn default_fast_math() -> bool {
    fast_math_from(std::env::var("PSCAT_KERNEL_MATH").ok().as_deref())
}

/// `PSCAT_KERNEL_MATH`'s value to the starting mode: only `exact` turns
/// the vector math off; anything else, or no variable, leaves it on.
fn fast_math_from(v: Option<&str>) -> bool {
    v != Some("exact")
}

/// `name .kernelmath -` (#1100): `/fast` (the default, see
/// [`default_fast_math`]) runs `sin`, `cos` and `atan` in `kernelimage`'s
/// compiled kernels vectorised (`vmath`), libm's algorithms lane by lane,
/// checked bit for bit against `/exact`; `/exact` runs them one lane at a
/// time through `fmath`. `kernelarray` always runs `/exact`.
fn kernelmath(it: &mut Interp) -> Result<(), PsError> {
    let v = it.pop()?;
    let Value::Name(n) = &v.value else {
        return Err(PsError::Typecheck);
    };
    it.kernel_fast_math = match &**n {
        "exact" => false,
        "fast" => true,
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

/// `inline shared made .kernelcaps -`: the compiler's size caps, in
/// instructions (#1091): an inlining compile's, a sharing compile's, and
/// what subroutines may hold as made. 0 restores a cap's default, and none
/// may exceed it, so a program cannot raise what a compile may hold. A
/// test hook, so the tests reach the caps with small programs.
fn kernelcaps(it: &mut Interp) -> Result<(), PsError> {
    let mut caps = [0usize; 3];
    for k in (0..3).rev() {
        let v = it.pop()?;
        let Value::Integer(n) = v.value else {
            return Err(PsError::Typecheck);
        };
        caps[k] = match n {
            0 => compile::DEFAULT_CAPS[k],
            n if n > 0 => (n as usize).min(compile::DEFAULT_CAPS[k]),
            _ => return Err(PsError::Rangecheck),
        };
    }
    it.kernel_caps = caps;
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
/// same in both. It also runs as many execution-stack frames deep as under
/// `colorimage`'s data procedure and `for`, so the same recursion overflows
/// in both: hence the `{ … loop KS pop } exec` (without the `KS pop` the
/// extra procedure adds no frame, `loop` being its last element; the test
/// `a_soft_pixel_runs_as_deep_in_the_execution_stack_as_the_interpreters`
/// pins the depth). Two things
/// only the whole image can say: a pixel that `exit`s out of the procedure
/// (out of the `loop` here, which `.kfpaint` sees as a pixel `.kfput` never
/// finished; out of the row's `for` in [`DRIVER`]), and one that changed the
/// `i` under its mark ([`DRIVER`] stores the pixel in that column). Then the
/// caller's operands and dictionary stack are put back and `.kfpaint` runs
/// the whole image instead: a pure procedure (`pure`) changed nothing else.
const PIXEL_DRIVER: &str = "{ { { KS .kfnext not { exit } if \
    KS /P get 8 dict begin exec end KS .kfput } loop KS pop } exec \
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
    ks.put("Busy".into(), Object::bool(false));
    ks.put("Fail".into(), Object::bool(false));
    ks.put("Depth".into(), Object::int(it.dict_stack_len() as i64));
    // The caller's operands, to put back for the whole image.
    it.check_alloc(
        it.operand_stack()
            .len()
            .saturating_mul(std::mem::size_of::<Object>()),
    )?;
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
    ks.put("Busy".into(), Object::bool(true));
    // `i mark i j`, as `DRIVER` leaves them for the procedure.
    it.push(i.clone());
    it.push(Object::lit(Value::Mark));
    it.push(i);
    it.push(j);
    it.push(Object::bool(true));
    Ok(())
}

/// The caller's operands and dictionary stack back as `fallback_pixels`
/// found them, and `Fail` set, for `.kfpaint` to run the whole image.
fn give_up(it: &mut Interp, ks: &mut Dict) -> Result<(), PsError> {
    let Some(Value::Array(base)) = ks.get("Base").map(|o| o.value) else {
        return Err(PsError::Typecheck);
    };
    let depth = uint_of(ks, "Depth")?;
    while it.dict_stack_len() > depth {
        it.pop_dict()?;
    }
    it.ostack.clear();
    it.ostack.extend(base.snapshot());
    ks.put("Fail".into(), Object::bool(true));
    ks.put("Busy".into(), Object::bool(false));
    Ok(())
}

/// `i mark c1 .. cn KS .kfput -`: the current pixel's components. It pops
/// what `DRIVER`'s `.kfstore` pops, with the same errors. A pixel that left
/// another column than its own where `.kfstore` reads it cannot be finished
/// alone (see [`PIXEL_DRIVER`]): `give_up`, and no pixel after it runs.
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
    let comps = pop_components(it, n)?;
    let col = usize::try_from(it.pop_int()?).map_err(|_| PsError::Rangecheck)?;
    ks.put("Busy".into(), Object::bool(false));
    if col != i {
        give_up(it, &mut ks)?;
        let end = pix.len() / 2;
        ks.put("K".into(), Object::int(end as i64));
        return Ok(());
    }
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
/// be finished alone (`Fail`, or `Busy`: it left the driver's loop with
/// `exit`), run the whole image in the interpreter.
fn kfpaint(it: &mut Interp) -> Result<(), PsError> {
    let ks = state(it)?;
    let busy = matches!(
        ks.borrow().get("Busy").map(|o| o.value),
        Some(Value::Boolean(true))
    );
    if busy {
        give_up(it, &mut ks.borrow_mut())?;
    }
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

// --- kernelarray's interpreter path (#1102) -----------------------------

/// The reference for `kernelarray`: the procedure per cell, in raster
/// order, in a fresh dict, its values above a mark (`.kanext` pushes
/// `mark i j`; `.kaput` pops back to the mark). The same driver finishes
/// the cells a compiled run retired (`Pix`, in raster order), so a cell
/// sees the same operand stack and execution-stack depth either way. A
/// cell that leaves the driver's loop with `exit` is an `invalidexit`
/// (`.kadone` sees it unfinished); among retired cells it first sends the
/// whole grid back to the interpreter, as one that cannot run alone.
const CELL_DRIVER: &str = "{ { KS .kanext not { exit } if \
    KS /P get 8 dict begin exec end KS .kaput } loop KS .kadone }";

/// A compiled run's values, and the cells (row, column) it left to the
/// interpreter.
type Partial = (Vec<Num>, Vec<(usize, usize)>);

/// Run [`CELL_DRIVER`] over every cell, or (`part`: the compiled values,
/// and the cells (row, column) left to the interpreter) over those.
fn cells_fallback(it: &mut Interp, a: Cells, part: Option<Partial>) -> Result<(), PsError> {
    let mut ks = Dict::new();
    ks.put("P".into(), Object::exec(Value::Array(a.proc.clone())));
    ks.put("N".into(), Object::int(a.n as i64));
    ks.put("W".into(), Object::int(a.w as i64));
    ks.put("H".into(), Object::int(a.h as i64));
    ks.put("Split".into(), Object::bool(a.split));
    ks.put("K".into(), Object::int(0));
    ks.put("Busy".into(), Object::bool(false));
    ks.put("Fail".into(), Object::bool(false));
    ks.put("Depth".into(), Object::int(it.dict_stack_len() as i64));
    let total = a.w * a.h * a.n;
    let vals = match part {
        None => vec![Object::lit(Value::Null); total],
        Some((vals, pixels)) => {
            // The caller's operands, to put back for the whole grid.
            it.check_alloc(
                it.operand_stack()
                    .len()
                    .saturating_mul(std::mem::size_of::<Object>()),
            )?;
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
            vals.into_iter().map(Num::object).collect()
        }
    };
    ks.put("Buf".into(), Object::array(vals));
    let ks = Object::lit(Value::Dict(Rc::new(RefCell::new(ks))));
    let Some((driver, _)) = it.scan_token_from(CELL_DRIVER.as_bytes().to_vec())? else {
        return Err(PsError::Undefined("kernelarray driver".into()));
    };
    let sys = it.systemdict();
    let driver = substitute(&driver, &ks, &sys.borrow());
    let Value::Array(body) = &driver.value else {
        return Err(PsError::Undefined("kernelarray driver".into()));
    };
    it.exec_object(Object::exec(Value::Array(body.clone())))
}

/// `KS .kanext mark i j true` for the next cell to interpret, or `false`.
fn kanext(it: &mut Interp) -> Result<(), PsError> {
    let ks = state(it)?;
    let mut ks = ks.borrow_mut();
    let k = uint_of(&ks, "K")?;
    let (w, h) = (uint_of(&ks, "W")?, uint_of(&ks, "H")?);
    let (i, j) = match ks.get("Pix").map(|o| o.value) {
        Some(Value::Array(pix)) => {
            if k.saturating_mul(2).saturating_add(1) >= pix.len() {
                it.push(Object::bool(false));
                return Ok(());
            }
            match (pix.get(2 * k), pix.get(2 * k + 1)) {
                (Some(i), Some(j)) => (i, j),
                _ => return Err(PsError::Rangecheck),
            }
        }
        _ => {
            if w == 0 || k >= w.saturating_mul(h) {
                it.push(Object::bool(false));
                return Ok(());
            }
            (Object::int((k % w) as i64), Object::int((k / w) as i64))
        }
    };
    ks.put("K".into(), Object::int(k as i64 + 1));
    ks.put("Busy".into(), Object::bool(true));
    it.push(Object::lit(Value::Mark));
    it.push(i);
    it.push(j);
    it.push(Object::bool(true));
    Ok(())
}

/// `mark v1 .. vn KS .kaput -`: the current cell's values, each the
/// integer or real it is (a `typecheck` for anything else, a `rangecheck`
/// for a wrong count).
fn kaput(it: &mut Interp) -> Result<(), PsError> {
    let ks = state(it)?;
    let mut ks = ks.borrow_mut();
    let n = uint_of(&ks, "N")?;
    let w = uint_of(&ks, "W")?;
    let k = uint_of(&ks, "K")?
        .checked_sub(1)
        .ok_or(PsError::Rangecheck)?;
    let mut vals = Vec::with_capacity(n);
    loop {
        let o = it.pop()?;
        match o.value {
            Value::Mark => break,
            Value::Integer(_) => vals.push(o),
            // Every NaN as one (`canon_nan`).
            Value::Real(r) => vals.push(Object::real(canon_nan(r))),
            _ => return Err(PsError::Typecheck),
        }
        if vals.len() > n {
            return Err(PsError::Rangecheck);
        }
    }
    if vals.len() != n {
        return Err(PsError::Rangecheck);
    }
    vals.reverse();
    ks.put("Busy".into(), Object::bool(false));
    let cell = match ks.get("Pix").map(|o| o.value) {
        Some(Value::Array(pix)) => {
            let at = |x: usize| match pix.get(x).map(|o| o.value) {
                Some(Value::Integer(v)) => usize::try_from(v).map_err(|_| PsError::Rangecheck),
                _ => Err(PsError::Typecheck),
            };
            at(2 * k + 1)?
                .checked_mul(w)
                .and_then(|x| x.checked_add(at(2 * k).ok()?))
                .ok_or(PsError::Rangecheck)?
        }
        _ => k,
    };
    let Some(Value::Array(buf)) = ks.get("Buf").map(|o| o.value) else {
        return Err(PsError::Typecheck);
    };
    for (c, v) in vals.into_iter().enumerate() {
        let x = cell
            .checked_mul(n)
            .and_then(|x| x.checked_add(c))
            .ok_or(PsError::Rangecheck)?;
        if x < buf.len() {
            buf.put(x, v)?;
        }
    }
    Ok(())
}

/// `KS .kadone -`: push the result. A cell left unfinished (`Busy`: it
/// left the driver's loop with `exit`) is an `invalidexit`, unless only
/// some cells were being interpreted: then the caller's operands and
/// dictionaries are put back and the whole grid runs in the interpreter.
fn kadone(it: &mut Interp) -> Result<(), PsError> {
    let ks = state(it)?;
    let busy = matches!(
        ks.borrow().get("Busy").map(|o| o.value),
        Some(Value::Boolean(true))
    );
    let part = matches!(
        ks.borrow().get("Pix").map(|o| o.value),
        Some(Value::Array(_))
    );
    let (a, buf) = {
        let ks = ks.borrow();
        let Some(Value::Array(buf)) = ks.get("Buf").map(|o| o.value) else {
            return Err(PsError::Typecheck);
        };
        let Some(Value::Array(p)) = ks.get("P").map(|o| o.value) else {
            return Err(PsError::Typecheck);
        };
        let split = matches!(ks.get("Split").map(|o| o.value), Some(Value::Boolean(true)));
        (
            Cells {
                w: uint_of(&ks, "W")?,
                h: uint_of(&ks, "H")?,
                proc: p,
                n: uint_of(&ks, "N")?,
                split,
            },
            buf,
        )
    };
    let need = a.w.checked_mul(a.h).and_then(|x| x.checked_mul(a.n));
    if !(1..=64).contains(&a.n) || need != Some(buf.len()) {
        return Err(PsError::Rangecheck);
    }
    if busy && part {
        let mut ks = ks.borrow_mut();
        let Some(Value::Array(base)) = ks.get("Base").map(|o| o.value) else {
            return Err(PsError::Typecheck);
        };
        let depth = uint_of(&ks, "Depth")?;
        while it.dict_stack_len() > depth {
            it.pop_dict()?;
        }
        it.ostack.clear();
        it.ostack.extend(base.snapshot());
        ks.put("Busy".into(), Object::bool(false));
        drop(ks);
        it.kernel_stats.1 -= 1;
        it.kernel_stats.2 += 1;
        return cells_fallback(it, a, None);
    }
    if busy {
        return Err(PsError::InvalidExit);
    }
    if !a.split {
        it.push(Object::lit(Value::Array(buf)));
        return Ok(());
    }
    let all = buf.snapshot();
    let cells = a.w * a.h;
    for c in 0..a.n {
        let col: Vec<Object> = (0..cells).map(|x| all[x * a.n + c].clone()).collect();
        it.push(Object::array(col));
    }
    Ok(())
}

#[cfg(test)]
mod math_default_tests {
    #[test]
    fn only_exact_turns_the_vector_math_off() {
        assert!(super::fast_math_from(None));
        assert!(super::fast_math_from(Some("fast")));
        assert!(super::fast_math_from(Some("")));
        assert!(!super::fast_math_from(Some("exact")));
    }
}
