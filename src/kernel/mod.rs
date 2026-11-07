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

pub(crate) mod compile;
pub(crate) mod exec;
#[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
pub(crate) mod gpu;
pub(crate) mod ir;

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

/// Run compiled kernels on the GPU, in f32, as a preview (`--gpu-preview`;
/// cargo feature `gpu`). Process-wide. Fails in a build without the
/// feature. Procedures that do not compile, and any GPU failure, still use
/// the CPU paths.
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
    op(dict, ".kernelmode", kernelmode);
    op(dict, ".kfstore", kfstore);
    op(dict, ".kfrow", kfrow);
    op(dict, ".kernelstats", kernelstats);
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
        let compiled = compile::compile(it, &a.proc, a.ncomp);
        lap("compile", t0);
        if let Ok((k, _)) = &compiled
            && timing
        {
            eprintln!(
                "pscat: kernel size: {} instructions, {} registers",
                k.size(),
                k.nregs
            );
        }
        let t1 = crate::clock::Clock::mono_ns();
        match compiled {
            Ok((k, noise)) => {
                if noise {
                    crate::ops::noise::note_sampled(it);
                }
                let rows = rows_needed(it, a.w, a.h, a.matrix);
                #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
                if GPU_PREVIEW.load(Ordering::Relaxed) {
                    let r = gpu::run(&k, a.w, a.h, a.ncomp, rows);
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
                        Err(why) => {
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
                    Ok(bytes) => {
                        it.kernel_stats.0 += 1;
                        return paint(it, &a, bytes);
                    }
                    Err(Stop::Limit(e)) => return Err(e),
                    // Some pixel would raise an error: the interpreter
                    // raises it, with the real stack (design: "Error rule").
                    Err(Stop::Fallback | Stop::Cancelled) => {}
                }
            }
            Err(compile::NotCompilable(why)) => {
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
    it.kernel_stats.1 += 1;
    fallback(it, a)
}

/// `.kernelstats compiled interpreted`: how many `kernelimage` calls each
/// path finished (a test hook: a differential test must know the
/// compiled path really ran).
fn kernelstats(it: &mut Interp) -> Result<(), PsError> {
    let (c, f) = it.kernel_stats;
    it.push(Object::int(c));
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
) -> Result<Vec<u8>, Stop> {
    let row_bytes = w * ncomp;
    let total = row_bytes
        .checked_mul(h)
        .ok_or(Stop::Limit(PsError::Limitcheck))?;
    it.check_alloc(total).map_err(Stop::Limit)?;
    let mut out = vec![0u8; total];
    let (steps_left, deadline) = it.remaining_budget();
    let ctl = RunCtl::new(deadline, steps_left);
    let threads = thread_count(it).min(j1.saturating_sub(j0)).max(1);
    let rows: Vec<(usize, &mut [u8])> = out
        .chunks_mut(row_bytes)
        .enumerate()
        .skip(j0)
        .take(j1.saturating_sub(j0))
        .collect();
    let queue = Mutex::new(rows.into_iter());
    let first_err: Mutex<Option<Stop>> = Mutex::new(None);
    let work = |ctl: &RunCtl| {
        let mut m = Machine::new(k, ctl);
        loop {
            let next = match queue.lock() {
                Ok(mut q) => q.next(),
                Err(_) => None,
            };
            let Some((j, row)) = next else { break };
            let mut i0 = 0;
            let mut r = Ok(());
            while i0 < w {
                let n = (w - i0).min(LANES);
                r = m.run_batch(
                    j as i64,
                    i0 as i64,
                    n,
                    &mut row[i0 * ncomp..(i0 + n) * ncomp],
                );
                if r.is_err() {
                    break;
                }
                i0 += n;
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
    it.charge_steps(ctl.steps.load(std::sync::atomic::Ordering::Relaxed));
    if let Ok(mut fe) = first_err.lock()
        && let Some(e) = fe.take()
    {
        return Err(e);
    }
    Ok(out)
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
    let (ok, why) = match compile::compile(it, proc, ncomp as usize) {
        Ok((k, _)) => (true, format!("{} instructions", k.size())),
        Err(compile::NotCompilable(why)) => (false, why),
    };
    it.push(Object::bool(ok));
    it.push(Object::lit(Value::String(PsString::from_bytes(
        why.into_bytes(),
    ))));
    Ok(())
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

/// Replace `KS` with the state dict and every other executable name with
/// its systemdict operator (a program's own definitions cannot change it).
fn substitute(obj: &Object, ks: &Object, sys: &Dict) -> Object {
    match &obj.value {
        Value::Name(n) if obj.executable => {
            if &**n == "KS" {
                return ks.clone();
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

/// `i mark c1 .. cn KS .kfstore -`: one pixel's components into the row.
fn kfstore(it: &mut Interp) -> Result<(), PsError> {
    let ks = state(it)?;
    let ks = ks.borrow();
    let n = int_of(&ks, "N")? as usize;
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
    let i = it.pop_int()? as usize;
    let Some(Value::String(row)) = ks.get("Row").map(|o| o.value) else {
        return Err(PsError::Typecheck);
    };
    let mut row = row.borrow_bytes_mut();
    for (c, v) in comps.iter().enumerate() {
        if let Some(b) = row.get_mut(i * n + c) {
            *b = quantize(*v);
        }
    }
    Ok(())
}

/// `KS .kfrow row`: the finished row; moves to the next.
fn kfrow(it: &mut Interp) -> Result<(), PsError> {
    let ks = state(it)?;
    let row = {
        let mut ks = ks.borrow_mut();
        let j = int_of(&ks, "J")?;
        ks.put("J".into(), Object::int(j + 1));
        ks.get("Row").ok_or(PsError::Typecheck)?
    };
    it.push(row);
    Ok(())
}
