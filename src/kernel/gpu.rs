//! The GPU preview backend (cargo feature `gpu`, CLI `--gpu-preview`):
//! a compiled kernel lowered to WGSL and run through wgpu (Metal on macOS,
//! Vulkan or DX12 elsewhere).
//!
//! **A preview, not a render.** It computes in f32 (consumer GPUs have no
//! fast f64, Apple's none at all), ignores PostScript's error checks (a
//! division by zero gives whatever the GPU gives), and caps every loop at
//! [`LOOP_CAP`] iterations so a runaway kernel cannot hang the display. A
//! lane that reaches a certain error (`Ins::Abort`) paints magenta. Its
//! pixels are close to the CPU path's, never claimed equal; final renders
//! stay on the CPU (design: `docs/dev/design/KERNELS.md`).

use std::fmt::Write as _;
use std::sync::{Mutex, OnceLock};

use super::exec::{RunCtl, Stop};
use super::ir::{BinOp, IBinOp, IUnOp, Ins, Kernel, NoiseKind, Op, TableKind, UnOp};
use crate::error::PsError;

/// Why [`run`] gave no image.
pub(crate) enum GpuStop {
    /// A time or step limit fired between dispatches: the same error the
    /// CPU path raises.
    Limit(PsError),
    /// No device, a rejected shader, a failed readback: the CPU paths run
    /// the image instead.
    Failed(String),
}

impl From<String> for GpuStop {
    fn from(why: String) -> Self {
        GpuStop::Failed(why)
    }
}

/// Loop trips (all loops together) a pixel may run on the GPU.
const LOOP_CAP: u32 = super::GPU_LOOP_CAP;
/// Pixels per dispatch: keeps each submission short of a GPU watchdog,
/// and is the unit at which the limits are looked at.
const CHUNK_PIXELS: usize = super::GPU_CHUNK_PIXELS;
const WG: u32 = 64;

struct Ctx {
    device: wgpu::Device,
    queue: wgpu::Queue,
    name: String,
}

static CTX: OnceLock<Result<Mutex<Ctx>, String>> = OnceLock::new();

fn ctx() -> Result<&'static Mutex<Ctx>, String> {
    CTX.get_or_init(|| {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|e| format!("no GPU adapter: {e}"))?;
        let name = adapter.get_info().name;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("pscat kernels"),
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .map_err(|e| format!("no GPU device: {e}"))?;
        Ok(Mutex::new(Ctx {
            device,
            queue,
            name,
        }))
    })
    .as_ref()
    .map_err(|e| e.clone())
}

/// The adapter's name, for notes and benchmarks.
pub(crate) fn adapter_name() -> Result<String, String> {
    let c = ctx()?;
    let g = c.lock().map_err(|_| "GPU context poisoned".to_string())?;
    Ok(g.name.clone())
}

/// Run `k` over rows `j0..j1` of a `w` by `h` grid: `ncomp` bytes per
/// sample, rows outside the range left 0.
pub(crate) fn run(
    k: &Kernel,
    w: usize,
    h: usize,
    ncomp: usize,
    (j0, j1): (usize, usize),
    ctl: &RunCtl,
) -> Result<Vec<u8>, GpuStop> {
    let timing = std::env::var_os("PSCAT_KERNEL_TIMING").is_some();
    let t = std::time::Instant::now();
    let (src, data) = lower(k, ncomp)?;
    let c = ctx()?;
    if timing {
        eprintln!(
            "pscat: gpu lower + device: {:.2} ms",
            t.elapsed().as_secs_f64() * 1e3
        );
    }
    let t = std::time::Instant::now();
    let g = c.lock().map_err(|_| "GPU context poisoned".to_string())?;
    let dev = &g.device;
    let scope = dev.push_error_scope(wgpu::ErrorFilter::Validation);
    let module = dev.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("kernel"),
        source: wgpu::ShaderSource::Wgsl(src.into()),
    });
    let pipeline = dev.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("kernel"),
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    use wgpu::util::DeviceExt;
    let tables = dev.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("tables"),
        contents: bytemuck_f32(&data),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let rows_per = (CHUNK_PIXELS / w.max(1)).max(1);
    let out_size = (rows_per * w * 4) as u64;
    let out = dev.create_buffer(&wgpu::BufferDescriptor {
        label: Some("out"),
        size: out_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let staging = dev.create_buffer(&wgpu::BufferDescriptor {
        label: Some("staging"),
        size: out_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let params = dev.create_buffer(&wgpu::BufferDescriptor {
        label: Some("params"),
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind = dev.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("kernel"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: params.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: tables.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: out.as_entire_binding(),
            },
        ],
    });
    if let Some(e) = pollster::block_on(scope.pop()) {
        return Err(GpuStop::Failed(format!("shader rejected: {e}")));
    }
    if timing {
        eprintln!(
            "pscat: gpu shader + pipeline: {:.2} ms",
            t.elapsed().as_secs_f64() * 1e3
        );
    }
    let t = std::time::Instant::now();
    let mut bytes = vec![0u8; w * h * ncomp];
    let mut j = j0;
    while j < j1 {
        let n = rows_per.min(j1 - j);
        let px = (n * w) as u32;
        // A dispatch cannot be interrupted, so the limits are looked at
        // before each one (#1065): a dispatch is at most CHUNK_PIXELS
        // pixels of at most LOOP_CAP loop trips each. Its steps are the
        // kernel's straight-line size per pixel; loop trips are capped,
        // not counted.
        match ctl.poll(u64::from(px).saturating_mul(k.size() as u64)) {
            Ok(()) => {}
            Err(Stop::Limit(e)) => return Err(GpuStop::Limit(e)),
            Err(Stop::Cancelled) => return Err(GpuStop::Failed("stopped".into())),
        }
        let p: [u32; 4] = [w as u32, j as u32, px, 0];
        g.queue.write_buffer(&params, 0, bytemuck_u32(&p));
        let mut enc = dev.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(px.div_ceil(WG), 1, 1);
        }
        let used = u64::from(px) * 4;
        enc.copy_buffer_to_buffer(&out, 0, &staging, 0, used);
        g.queue.submit(Some(enc.finish()));
        let slice = staging.slice(..used);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        dev.poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| format!("GPU poll: {e}"))?;
        rx.recv()
            .map_err(|e| format!("GPU map: {e}"))?
            .map_err(|e| format!("GPU map: {e}"))?;
        {
            let view = slice
                .get_mapped_range()
                .map_err(|e| format!("GPU map: {e:?}"))?;
            for (p, word) in view.chunks_exact(4).enumerate() {
                let dst = (j * w + p) * ncomp;
                bytes[dst..dst + ncomp].copy_from_slice(&word[..ncomp]);
            }
        }
        staging.unmap();
        j += n;
    }
    if timing {
        eprintln!(
            "pscat: gpu dispatch + readback: {:.2} ms",
            t.elapsed().as_secs_f64() * 1e3
        );
    }
    Ok(bytes)
}

fn bytemuck_f32(v: &[f32]) -> &[u8] {
    // SAFETY: f32 has no padding and any byte pattern is a valid u8.
    unsafe { std::slice::from_raw_parts(v.as_ptr().cast::<u8>(), std::mem::size_of_val(v)) }
}

fn bytemuck_u32(v: &[u32]) -> &[u8] {
    // SAFETY: as `bytemuck_f32`.
    unsafe { std::slice::from_raw_parts(v.as_ptr().cast::<u8>(), std::mem::size_of_val(v)) }
}

/// WGSL for `k`, and the one f32 buffer holding the noise permutation
/// (first 256 entries) and every table (values, then tags when mixed).
fn lower(k: &Kernel, ncomp: usize) -> Result<(String, Vec<f32>), String> {
    let mut data: Vec<f32> = k.perm.iter().map(|&p| f32::from(p)).collect();
    let mut toff = Vec::new();
    for t in &k.tables {
        let off = data.len();
        data.extend((0..t.data.len()).map(|i| t.data.at(i) as f32));
        let tag = match &t.kind {
            TableKind::Mixed(tags) => {
                let o = data.len();
                data.extend(tags.iter().map(|&v| v as f32));
                Some(o)
            }
            _ => None,
        };
        toff.push((off, tag));
    }
    let mut s = String::new();
    s.push_str(PRELUDE);
    s.push_str(
        "@compute @workgroup_size(64)\nfn main(@builtin(global_invocation_id) gid: vec3<u32>) {\n",
    );
    s.push_str("  if (gid.x >= P.npx) { return; }\n");
    s.push_str("  let oi = gid.x;\n  _ = T[0];\n");
    let _ = writeln!(s, "  var r{}: f32 = f32(gid.x % P.w);", k.in_i);
    let _ = writeln!(s, "  var r{}: f32 = f32(P.row0 + gid.x / P.w);", k.in_j);
    for r in 0..k.nregs as u32 {
        if r != k.in_i && r != k.in_j {
            let _ = writeln!(s, "  var r{r}: f32 = 0.0;");
        }
    }
    for (a, n) in k.larrays.iter().enumerate() {
        let n = (*n).max(1);
        let _ = writeln!(
            s,
            "  var la{a}: array<f32, {n}>;\n  var lt{a}: array<f32, {n}>;"
        );
    }
    let mut lp = 0usize;
    let mut body = String::new();
    block(&k.body, k, &toff, &mut body, 1, &mut lp)?;
    let _ = lp;
    s.push_str("  var fuel: u32 = 0u;\n");
    s.push_str(&body);
    let comp = |c: usize| match k.outputs.get(c) {
        Some(o) => format!("q({})", op(*o)),
        None => "0u".into(),
    };
    let _ = writeln!(
        s,
        "  OUT[oi] = {} | ({} << 8u) | ({} << 16u) | ({} << 24u);\n}}",
        comp(0),
        if ncomp > 1 { comp(1) } else { "0u".into() },
        if ncomp > 2 { comp(2) } else { "0u".into() },
        if ncomp > 3 { comp(3) } else { "0u".into() },
    );
    Ok((s, data))
}

fn op(o: Op) -> String {
    match o {
        Op::R(r) => format!("r{r}"),
        Op::Imm(v) => lit(v),
    }
}

fn lit(v: f64) -> String {
    let f = v as f32;
    if f.is_nan() {
        "(0.0)".into()
    } else if f.is_infinite() {
        if f > 0.0 {
            "(3.4e38)".into()
        } else {
            "(-3.4e38)".into()
        }
    } else {
        format!("({f:e})")
    }
}

fn block(
    b: &[Ins],
    k: &Kernel,
    toff: &[(usize, Option<usize>)],
    s: &mut String,
    ind: usize,
    lp: &mut usize,
) -> Result<(), String> {
    let pad = "  ".repeat(ind + 1);
    for ins in b {
        match ins {
            Ins::Mov(d, a) => {
                let _ = writeln!(s, "{pad}r{d} = {};", op(*a));
            }
            Ins::Bin(o, d, a, b) => {
                let (a, b) = (op(*a), op(*b));
                let e = match o {
                    BinOp::Add => format!("{a} + {b}"),
                    BinOp::Sub => format!("{a} - {b}"),
                    BinOp::Mul | BinOp::TagAnd => format!("{a} * {b}"),
                    BinOp::Div => format!("{a} / {b}"),
                    BinOp::Min => format!("min({a}, {b})"),
                    BinOp::Max => format!("max({a}, {b})"),
                    BinOp::Lt => format!("b2f(({a}) < ({b}))"),
                    BinOp::Le => format!("b2f(({a}) <= ({b}))"),
                    BinOp::Gt => format!("b2f(({a}) > ({b}))"),
                    BinOp::Ge => format!("b2f(({a}) >= ({b}))"),
                    BinOp::Eq => format!("b2f(({a}) == ({b}))"),
                    BinOp::Ne => format!("b2f(({a}) != ({b}))"),
                    BinOp::BAnd => format!("b2f({a} != 0.0 && {b} != 0.0)"),
                    BinOp::BOr => format!("b2f({a} != 0.0 || {b} != 0.0)"),
                    BinOp::BXor => format!("b2f(({a} != 0.0) != ({b} != 0.0))"),
                    BinOp::Atan => format!("ps_atan({a}, {b})"),
                    BinOp::Pow => format!("ps_pow({a}, {b})"),
                };
                let _ = writeln!(s, "{pad}r{d} = {e};");
            }
            Ins::Un(o, d, a) => {
                let a = op(*a);
                let e = match o {
                    UnOp::Neg => format!("-{a}"),
                    UnOp::Abs => format!("abs({a})"),
                    UnOp::Floor => format!("floor({a})"),
                    UnOp::Ceil => format!("ceil({a})"),
                    UnOp::Round => format!("floor({a} + 0.5)"),
                    UnOp::Trunc => format!("trunc({a})"),
                    UnOp::Sqrt => format!("sqrt(max({a}, 0.0))"),
                    UnOp::Sin => format!("sin(radians({a}))"),
                    UnOp::Cos => format!("cos(radians({a}))"),
                    UnOp::Ln => format!("log({a})"),
                    UnOp::Log => format!("(log({a}) * 0.4342944819)"),
                    UnOp::BNot => format!("b2f(({a}) == 0.0)"),
                };
                let _ = writeln!(s, "{pad}r{d} = {e};");
            }
            // Integers are f32 values on the GPU (a preview: big hashes lose
            // their low bits).
            Ins::MovI(d, v) => {
                let _ = writeln!(s, "{pad}r{d} = {};", lit(*v as f64));
            }
            Ins::I2F(d, a) | Ins::I2FExact(d, a) => {
                let _ = writeln!(s, "{pad}r{d} = {};", op(*a));
            }
            Ins::F2I(d, a) => {
                let _ = writeln!(s, "{pad}r{d} = trunc({});", op(*a));
            }
            Ins::IBin(o, d, a, b) => {
                let (a, b) = (op(*a), op(*b));
                let e = match o {
                    IBinOp::Add => format!("{a} + {b}"),
                    IBinOp::Sub => format!("{a} - {b}"),
                    IBinOp::Mul => format!("{a} * {b}"),
                    IBinOp::Min => format!("min({a}, {b})"),
                    IBinOp::Max => format!("max({a}, {b})"),
                    IBinOp::Idiv => format!("f32(i32({a}) / nz(i32({b})))"),
                    IBinOp::Mod => format!("f32(i32({a}) % nz(i32({b})))"),
                    IBinOp::And => format!("f32(i32({a}) & i32({b}))"),
                    IBinOp::Or => format!("f32(i32({a}) | i32({b}))"),
                    IBinOp::Xor => format!("f32(i32({a}) ^ i32({b}))"),
                    IBinOp::Shift => format!("shift({a}, {b})"),
                    IBinOp::Lt => format!("b2f(({a}) < ({b}))"),
                    IBinOp::Le => format!("b2f(({a}) <= ({b}))"),
                    IBinOp::Gt => format!("b2f(({a}) > ({b}))"),
                    IBinOp::Ge => format!("b2f(({a}) >= ({b}))"),
                    IBinOp::Eq => format!("b2f(({a}) == ({b}))"),
                    IBinOp::Ne => format!("b2f(({a}) != ({b}))"),
                };
                let _ = writeln!(s, "{pad}r{d} = {e};");
            }
            Ins::IUn(o, d, a) => {
                let a = op(*a);
                let e = match o {
                    IUnOp::Neg => format!("-{a}"),
                    IUnOp::Abs => format!("abs({a})"),
                    IUnOp::Not => format!("f32(~i32({a}))"),
                };
                let _ = writeln!(s, "{pad}r{d} = {e};");
            }
            Ins::ForFin(d, c, inc, lim) => {
                let (c, inc, lim) = (op(*c), op(*inc), op(*lim));
                let _ = writeln!(
                    s,
                    "{pad}r{d} = b2f(select(({c}) < ({lim}), ({c}) > ({lim}), ({inc}) >= 0.0));"
                );
            }
            Ins::CheckInt(..) | Ins::NeedInt(_) | Ins::Assert(_) | Ins::RowCheck(..) => {}
            Ins::Gather {
                d,
                dtag,
                table,
                row,
                col,
                ..
            } => {
                let t = &k.tables[*table as usize];
                let (off, tag) = toff[*table as usize];
                let ix = format!(
                    "u32(clamp(i32({}), 0, {})) * {}u + u32(clamp(i32({}), 0, {}))",
                    op(*row),
                    t.rows.max(1) - 1,
                    t.cols,
                    op(*col),
                    t.cols.max(1) - 1
                );
                let _ = writeln!(s, "{pad}r{d} = T[{off}u + {ix}];");
                if let (Some(dt), Some(tagoff)) = (dtag, tag) {
                    let _ = writeln!(s, "{pad}r{dt} = T[{tagoff}u + {ix}];");
                }
            }
            Ins::LNew(a) => {
                let n = k.larrays[*a as usize].max(1);
                let _ = writeln!(
                    s,
                    "{pad}for (var z = 0u; z < {n}u; z++) {{ la{a}[z] = 0.0; lt{a}[z] = 1.0; }}"
                );
            }
            Ins::LGet {
                d, dtag, arr, idx, ..
            } => {
                let n = k.larrays[*arr as usize].max(1);
                let ix = format!("u32(clamp(i32({}), 0, {}))", op(*idx), n - 1);
                let _ = writeln!(
                    s,
                    "{pad}r{d} = la{arr}[{ix}];\n{pad}r{dtag} = lt{arr}[{ix}];"
                );
            }
            Ins::LPut {
                arr, idx, v, vtag, ..
            } => {
                let n = k.larrays[*arr as usize].max(1);
                let ix = format!("u32(clamp(i32({}), 0, {}))", op(*idx), n - 1);
                let _ = writeln!(
                    s,
                    "{pad}la{arr}[{ix}] = {};\n{pad}lt{arr}[{ix}] = {};",
                    op(*v),
                    op(*vtag)
                );
            }
            Ins::Noise { kind, d, a, .. } => {
                let a: Vec<String> = a.iter().map(|o| op(*o)).collect();
                let call = match kind {
                    NoiseKind::G2 => format!("g2({}, {})", a[0], a[1]),
                    NoiseKind::V2 => format!("v2({}, {})", a[0], a[1]),
                    NoiseKind::G3 => format!("g3({}, {}, {})", a[0], a[1], a[2]),
                    NoiseKind::V3 => format!("v3({}, {}, {})", a[0], a[1], a[2]),
                    NoiseKind::Fbm2 => {
                        format!("oct2({}, {}, {}, {}, {}, 0u)", a[0], a[1], a[2], a[3], a[4])
                    }
                    NoiseKind::VFbm2 => {
                        format!("oct2({}, {}, {}, {}, {}, 1u)", a[0], a[1], a[2], a[3], a[4])
                    }
                    NoiseKind::Ridged2 => {
                        format!("oct2({}, {}, {}, {}, {}, 2u)", a[0], a[1], a[2], a[3], a[4])
                    }
                    NoiseKind::Fbm3 => format!(
                        "oct3({}, {}, {}, {}, {}, {}, 0u)",
                        a[0], a[1], a[2], a[3], a[4], a[5]
                    ),
                    NoiseKind::VFbm3 => format!(
                        "oct3({}, {}, {}, {}, {}, {}, 1u)",
                        a[0], a[1], a[2], a[3], a[4], a[5]
                    ),
                    NoiseKind::Ridged3 => format!(
                        "oct3({}, {}, {}, {}, {}, {}, 2u)",
                        a[0], a[1], a[2], a[3], a[4], a[5]
                    ),
                    NoiseKind::G2D => {
                        let _ = writeln!(
                            s,
                            "{pad}{{ let v = g2d({}, {}); r{} = v.x; r{} = v.y; r{} = v.z; }}",
                            a[0], a[1], d[0], d[1], d[2]
                        );
                        continue;
                    }
                };
                let _ = writeln!(s, "{pad}r{} = {call};", d[0]);
            }
            Ins::If(c, t, e) => {
                let _ = writeln!(s, "{pad}if (r{c} != 0.0) {{");
                block(t, k, toff, s, ind + 1, lp)?;
                if e.is_empty() {
                    let _ = writeln!(s, "{pad}}}");
                } else {
                    let _ = writeln!(s, "{pad}}} else {{");
                    block(e, k, toff, s, ind + 1, lp)?;
                    let _ = writeln!(s, "{pad}}}");
                }
            }
            Ins::Loop(b) => {
                // One budget for every loop trip of the pixel, nested loops
                // included, so a runaway kernel cannot hang the GPU.
                *lp += 1;
                let _ = writeln!(
                    s,
                    "{pad}loop {{\n{pad}  fuel += 1u;\n{pad}  if (fuel > {LOOP_CAP}u) {{ break; }}"
                );
                block(b, k, toff, s, ind + 1, lp)?;
                let _ = writeln!(s, "{pad}}}");
            }
            Ins::Break => {
                let _ = writeln!(s, "{pad}break;");
            }
            Ins::Abort => {
                let _ = writeln!(s, "{pad}OUT[oi] = 0x00ff00ffu; return;");
            }
        }
    }
    Ok(())
}

const PRELUDE: &str = r#"
struct Params { w: u32, row0: u32, npx: u32, pad: u32 };
@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var<storage, read> T: array<f32>;
@group(0) @binding(2) var<storage, read_write> OUT: array<u32>;

fn b2f(b: bool) -> f32 { return select(0.0, 1.0, b); }
fn nz(x: i32) -> i32 { return select(x, 1, x == 0); }
fn q(v: f32) -> u32 {
  let c = clamp(select(v, 0.0, v != v), 0.0, 1.0);
  return u32(floor(c * 255.0 + 0.5));
}
fn shift(a: f32, b: f32) -> f32 {
  let x = i32(a); let s = i32(b);
  if (s >= 32) { return 0.0; }
  if (s <= -32) { return f32(x >> 31u); }
  if (s >= 0) { return f32(x << u32(s)); }
  return f32(x >> u32(-s));
}
fn ps_atan(n: f32, d: f32) -> f32 {
  var a = degrees(atan2(n, d));
  if (a < 0.0) { a = a + 360.0; }
  return a;
}
fn ps_pow(a: f32, b: f32) -> f32 {
  if (a >= 0.0) { return pow(a, b); }
  let m = pow(-a, b);
  if (fract(b * 0.5) != 0.0) { return -m; }
  return m;
}

var<private> G2: array<f32, 16> = array<f32, 16>(1.0, 0.0, -1.0, 0.0, 0.0, 1.0, 0.0, -1.0,
  0.70710678, 0.70710678, -0.70710678, 0.70710678, 0.70710678, -0.70710678, -0.70710678, -0.70710678);
var<private> G3: array<vec3<f32>, 16> = array<vec3<f32>, 16>(
  vec3(1.0, 1.0, 0.0), vec3(-1.0, 1.0, 0.0), vec3(1.0, -1.0, 0.0), vec3(-1.0, -1.0, 0.0),
  vec3(1.0, 0.0, 1.0), vec3(-1.0, 0.0, 1.0), vec3(1.0, 0.0, -1.0), vec3(-1.0, 0.0, -1.0),
  vec3(0.0, 1.0, 1.0), vec3(0.0, -1.0, 1.0), vec3(0.0, 1.0, -1.0), vec3(0.0, -1.0, -1.0),
  vec3(1.0, 1.0, 0.0), vec3(0.0, -1.0, 1.0), vec3(-1.0, 1.0, 0.0), vec3(0.0, -1.0, -1.0));

fn perm(i: i32) -> i32 { return i32(T[u32(i & 255)]); }
fn hash2(i: i32, j: i32) -> i32 { return perm((perm(i) + j) & 255); }
fn hash3(i: i32, j: i32, k: i32) -> i32 { return perm((hash2(i, j) + k) & 255); }
fn fade(t: f32) -> f32 { let t2 = t * t; let t3 = t2 * t; return ((t3 * 10.0) - ((t3 * t) * 15.0)) + ((t3 * t2) * 6.0); }
fn lerp1(a: f32, b: f32, t: f32) -> f32 { return ((b - a) * t) + a; }
fn grad2(h: i32, x: f32, y: f32) -> f32 { let i = u32((h & 7) * 2); return G2[i] * x + G2[i + 1u] * y; }
fn value(h: i32) -> f32 { return (f32(h) - 127.5) * 0.0078125; }

fn g2(x: f32, y: f32) -> f32 {
  let x0 = floor(x); let y0 = floor(y);
  let xi = i32(x0) & 255; let yi = i32(y0) & 255;
  let xi1 = (xi + 1) & 255; let yi1 = (yi + 1) & 255;
  let xf = x - x0; let yf = y - y0;
  let u = fade(xf); let v = fade(yf);
  let n00 = grad2(hash2(xi, yi), xf, yf);
  let n10 = grad2(hash2(xi1, yi), xf - 1.0, yf);
  let n01 = grad2(hash2(xi, yi1), xf, yf - 1.0);
  let n11 = grad2(hash2(xi1, yi1), xf - 1.0, yf - 1.0);
  return lerp1(lerp1(n00, n10, u), lerp1(n01, n11, u), v);
}
fn v2(x: f32, y: f32) -> f32 {
  let x0 = floor(x); let y0 = floor(y);
  let xi = i32(x0) & 255; let yi = i32(y0) & 255;
  let xi1 = (xi + 1) & 255; let yi1 = (yi + 1) & 255;
  let u = fade(x - x0); let v = fade(y - y0);
  return lerp1(lerp1(value(hash2(xi, yi)), value(hash2(xi1, yi)), u),
               lerp1(value(hash2(xi, yi1)), value(hash2(xi1, yi1)), u), v);
}
fn g2d(x: f32, y: f32) -> vec3<f32> {
  let n = g2(x, y);
  let x0 = floor(x); let y0 = floor(y);
  let xi = i32(x0) & 255; let yi = i32(y0) & 255;
  let xi1 = (xi + 1) & 255; let yi1 = (yi + 1) & 255;
  let xf = x - x0; let yf = y - y0;
  let ha = u32((hash2(xi, yi) & 7) * 2); let hb = u32((hash2(xi1, yi) & 7) * 2);
  let hc = u32((hash2(xi, yi1) & 7) * 2); let hd = u32((hash2(xi1, yi1) & 7) * 2);
  let ga = vec2(G2[ha], G2[ha + 1u]); let gb = vec2(G2[hb], G2[hb + 1u]);
  let gc = vec2(G2[hc], G2[hc + 1u]); let gd = vec2(G2[hd], G2[hd + 1u]);
  let a = ga.x * xf + ga.y * yf;
  let b = gb.x * (xf - 1.0) + gb.y * yf;
  let c = gc.x * xf + gc.y * (yf - 1.0);
  let d = gd.x * (xf - 1.0) + gd.y * (yf - 1.0);
  let u = fade(xf); let v = fade(yf);
  let dfx = ((xf * xf) * 30.0) * ((xf - 1.0) * (xf - 1.0));
  let dfy = ((yf * yf) * 30.0) * ((yf - 1.0) * (yf - 1.0));
  let k1 = b - a; let k2 = c - a; let k3 = ((a - b) - c) + d;
  let dx = (((ga.x + (u * (gb.x - ga.x))) + (v * (gc.x - ga.x))) + ((u * v) * (((ga.x - gb.x) - gc.x) + gd.x))) + (dfx * (k1 + (k3 * v)));
  let dy = (((ga.y + (u * (gb.y - ga.y))) + (v * (gc.y - ga.y))) + ((u * v) * (((ga.y - gb.y) - gc.y) + gd.y))) + (dfy * (k2 + (k3 * u)));
  return vec3(n, dx, dy);
}
fn n3(x: f32, y: f32, z: f32, val: bool) -> f32 {
  let x0 = floor(x); let y0 = floor(y); let z0 = floor(z);
  let xi = i32(x0) & 255; let yi = i32(y0) & 255; let zi = i32(z0) & 255;
  let xi1 = (xi + 1) & 255; let yi1 = (yi + 1) & 255; let zi1 = (zi + 1) & 255;
  let xf = x - x0; let yf = y - y0; let zf = z - z0;
  let x1 = xf - 1.0; let y1 = yf - 1.0; let z1 = zf - 1.0;
  var c: array<f32, 8>;
  let hs = array<i32, 8>(hash3(xi, yi, zi), hash3(xi1, yi, zi), hash3(xi, yi1, zi), hash3(xi1, yi1, zi),
                         hash3(xi, yi, zi1), hash3(xi1, yi, zi1), hash3(xi, yi1, zi1), hash3(xi1, yi1, zi1));
  let px = array<f32, 8>(xf, x1, xf, x1, xf, x1, xf, x1);
  let py = array<f32, 8>(yf, yf, y1, y1, yf, yf, y1, y1);
  let pz = array<f32, 8>(zf, zf, zf, zf, z1, z1, z1, z1);
  for (var i = 0u; i < 8u; i++) {
    if (val) { c[i] = value(hs[i]); } else {
      let g = G3[u32(hs[i] & 15)];
      c[i] = ((g.x * px[i]) + (g.y * py[i])) + (g.z * pz[i]);
    }
  }
  let u = fade(xf); let v = fade(yf); let w = fade(zf);
  let a = lerp1(lerp1(c[0], c[1], u), lerp1(c[2], c[3], u), v);
  let b = lerp1(lerp1(c[4], c[5], u), lerp1(c[6], c[7], u), v);
  return lerp1(a, b, w);
}
fn g3(x: f32, y: f32, z: f32) -> f32 { return n3(x, y, z, false); }
fn v3(x: f32, y: f32, z: f32) -> f32 { return n3(x, y, z, true); }
fn oct2(x: f32, y: f32, oct: f32, gain: f32, lac: f32, kind: u32) -> f32 {
  var sum = 0.0; var amp = 1.0; var freq = 1.0;
  let o = u32(clamp(oct, 0.0, 32.0));
  for (var k = 0u; k < o; k++) {
    let qx = (x * freq) + (f32(k) * 17.3203125);
    let qy = (y * freq) + (f32(k) * 11.4140625);
    var n: f32;
    if (kind == 1u) { n = v2(qx, qy); } else { n = g2(qx, qy); }
    if (kind == 2u) { n = 1.0 - abs(n); n = n * n; }
    sum = sum + amp * n; amp = amp * gain; freq = freq * lac;
  }
  return sum;
}
fn oct3(x: f32, y: f32, z: f32, oct: f32, gain: f32, lac: f32, kind: u32) -> f32 {
  var sum = 0.0; var amp = 1.0; var freq = 1.0;
  let o = u32(clamp(oct, 0.0, 32.0));
  for (var k = 0u; k < o; k++) {
    let qx = (x * freq) + (f32(k) * 17.3203125);
    let qy = (y * freq) + (f32(k) * 11.4140625);
    let qz = (z * freq) + (f32(k) * 7.6015625);
    var n: f32;
    if (kind == 1u) { n = v3(qx, qy, qz); } else { n = g3(qx, qy, qz); }
    if (kind == 2u) { n = 1.0 - abs(n); n = n * n; }
    sum = sum + amp * n; amp = amp * gain; freq = freq * lac;
  }
  return sum;
}
"#;
