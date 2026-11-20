//! Mesh shadings for `shfill` (#1325, PLRM 4.9.3): `ShadingType` 4
//! (free-form Gouraud triangles) and 5 (lattice-form Gouraud triangles).
//!
//! `parse` reads and checks the dictionary, and keeps the vertex data as
//! numbers (an array `/DataSource`) or bytes (a string, or a file read to
//! its end). `MeshShading::decode` turns that into triangles when
//! the shading is painted, under the limits. `mesh_layer` paints them into
//! a layer the way `shfill`'s Type 1 path paints its function, so the same
//! paint path, SVG image and PDF hook serve both.
//!
//! **Painting.** A pixel takes the colour of the last triangle that holds
//! its centre (later data paints over earlier). The colour is
//! interpolated in the shading's own components, or as `t` through the
//! `/Function`, never in converted RGB. Coverage is a 4x4 grid of samples
//! per pixel, OR-ed over all triangles, so shared edges have no seams and
//! only the mesh's outer edge is anti-aliased.
//!
//! gs 10.07.1 is the reference for every rule below (png16m renders and
//! `stopped` probes on a real device; `nullpage` skips mesh decoding, so it
//! reports no data errors).

use std::rc::Rc;

use crate::error::PsError;
use crate::object::{Dict, Value};
use crate::shading::CsKind;

/// The most bytes a file `/DataSource` is read for (limitcheck past it).
const MAX_FILE_BYTES: usize = 1 << 28;

/// Coverage samples per pixel side: 4x4, one bit each in a `u16`.
const SUB: usize = 4;

/// Work units (`shfill`'s, ~16 a plain painted pixel) a pixel inside a
/// triangle's box costs, and a triangle itself.
const PIXEL_WORK: u64 = 32;
const TRIANGLE_WORK: u64 = 64;

/// Work units decoding a vertex or patch costs, the bounds scan over it
/// included, and how many are decoded between looks at the limits.
const DECODE_WORK: u64 = 64;
const DECODE_POLL: u64 = 4096;

/// A vertex: its point in shading (user) space and its colour values,
/// `n` components or, with a `/Function`, one `t`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Vertex {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) c: [f32; 4],
}

/// A decoded mesh.
pub(crate) enum Mesh {
    /// Type 4: triangles, in data order.
    Triangles(Vec<[Vertex; 3]>),
    /// Type 5: rows of `per_row` vertices.
    Lattice { verts: Vec<Vertex>, per_row: usize },
}

/// Packed vertex data (a string or file `/DataSource`).
#[derive(Clone)]
struct Packed {
    data: Rc<[u8]>,
    flag_bits: u32,
    coord_bits: u32,
    comp_bits: u32,
    /// `[x y v1 v2 ...]` ranges; `None` when the dictionary has no
    /// `/Decode`, which gs accepts and paints nothing for.
    decode: Option<Vec<(f64, f64)>>,
}

#[derive(Clone)]
enum Source {
    Numbers(Vec<f64>),
    Packed(Packed),
}

/// A parsed mesh shading, its data not yet decoded.
#[derive(Clone)]
pub(crate) struct MeshShading {
    /// 4 to 7.
    pub(crate) ty: u8,
    cs: CsKind,
    /// With a `/Function`: its colour ramp as RGB stops over its Domain
    /// (`build_stops`, as an axial shading's), and that Domain.
    pub(crate) ramp: Option<Ramp>,
    per_row: usize,
    source: Source,
    pub(crate) bbox: Option<(f64, f64, f64, f64)>,
    /// Bytes read from a file `/DataSource` while parsing, which the
    /// operator that parsed it charges, one step a KiB.
    pub(crate) read: u64,
}

/// A mesh's `/Function` as RGB stops over `domain`, positions 0 to 1.
#[derive(Clone)]
pub(crate) struct Ramp {
    pub(crate) stops: Vec<(f32, f32, f32, f32)>,
    pub(crate) domain: (f64, f64),
}

impl MeshShading {
    /// Colour values a vertex carries: one `t` with a Function, else the
    /// colour space's components.
    fn nvals(&self) -> usize {
        if self.ramp.is_some() {
            1
        } else {
            self.cs.ncomp()
        }
    }

    /// The RGB colour of a vertex's values.
    pub(crate) fn rgb(&self, c: &[f32; 4]) -> (f32, f32, f32) {
        match &self.ramp {
            Some(r) => r.at(c[0]),
            None => {
                let mut v = [0.0f64; 4];
                for (o, &x) in v.iter_mut().zip(c) {
                    *o = f64::from(x);
                }
                let (r, g, b) = self.cs.to_rgb(&v[..self.cs.ncomp()]);
                (r as f32, g as f32, b as f32)
            }
        }
    }
}

impl Ramp {
    /// The ramp's position for `t`, 0 to 1 over the Domain.
    /// `t`'s position over the Domain, not clamped: what the PDF stream
    /// carries, so a reader interpolates before its function clips, as the
    /// raster does (Codex round 1 on #1376).
    pub(crate) fn unclamped(&self, t: f32) -> f64 {
        let (d0, d1) = self.domain;
        if d1 > d0 {
            (f64::from(t) - d0) / (d1 - d0)
        } else {
            0.0
        }
    }

    pub(crate) fn pos(&self, t: f32) -> f32 {
        let (d0, d1) = self.domain;
        if d1 > d0 {
            ((f64::from(t) - d0) / (d1 - d0)).clamp(0.0, 1.0) as f32
        } else {
            0.0
        }
    }

    fn at(&self, t: f32) -> (f32, f32, f32) {
        let pos = self.pos(t);
        let s = &self.stops;
        // The first stop past `pos`; a run of stops at one position (a
        // hard edge) reads as its last, as tiny-skia reads a ramp.
        let i = s.partition_point(|st| st.0 <= pos);
        match (i.checked_sub(1).map(|k| s[k]), s.get(i)) {
            (Some(a), Some(&b)) if b.0 > a.0 => {
                let f = (pos - a.0) / (b.0 - a.0);
                let mix = |x: f32, y: f32| x + (y - x) * f;
                (mix(a.1, b.1), mix(a.2, b.2), mix(a.3, b.3))
            }
            (Some(a), _) => (a.1, a.2, a.3),
            (None, Some(b)) => (b.1, b.2, b.3),
            (None, None) => (0.0, 0.0, 0.0),
        }
    }
}

/// An integral number of bits from `key`, one of `allowed` (rangecheck
/// otherwise; gs takes `8.0`). Missing is `undefined`, as in gs.
fn bits_key(d: &Dict, key: &str, allowed: &[u32]) -> Result<u32, PsError> {
    let v = match d.get(key).map(|o| o.value) {
        None => return Err(PsError::Undefined(key.to_string())),
        Some(Value::Integer(n)) => n as f64,
        Some(Value::Real(r)) => r,
        Some(_) => return Err(PsError::Typecheck),
    };
    if v.fract() == 0.0 && allowed.iter().any(|&a| f64::from(a) == v) {
        Ok(v as u32)
    } else {
        Err(PsError::Rangecheck)
    }
}

/// Bytes read from a file `/DataSource` per poll of the limits.
const READ_CHUNK: usize = 1 << 16;

/// A file's bytes to its end (EOF, or a filter's EOD), at most
/// `MAX_FILE_BYTES`, and the bytes read from it (the data, and any left in
/// a filter chain), which `shfill` charges as steps, one a KiB, as a
/// filter charges what it decodes. A filter chain is then drained layer by
/// layer to each one's own EOD, as an image's is (`image.rs`), so the
/// program resumes right after the encoded data (an ASCII85 layer's `~>` is
/// not left unread under a Flate one).
///
/// Under the limits (Codex round 1 on #1376) each chunk's growth of the
/// buffer is held against the memory limit before it is read, and the
/// limits are looked at, with the bytes so far charged, between chunks.
fn read_file(
    f: &crate::file::FileHandle,
    probe: &crate::limits::LimitProbe,
) -> Result<(Vec<u8>, u64), PsError> {
    let mut out = Vec::new();
    let mut read = 0u64;
    {
        let mut top = f.borrow_mut();
        if !top.is_open() {
            return Ok((out, 0));
        }
        loop {
            probe.check_alloc(out.len().saturating_add(READ_CHUNK))?;
            let got = top.read_into(&mut out, READ_CHUNK)?;
            read += got as u64;
            probe.check(read / 1024)?;
            if got < READ_CHUNK {
                break;
            }
            if out.len() > MAX_FILE_BYTES {
                return Err(PsError::Limitcheck);
            }
        }
    }
    let mut layer = f.clone();
    while layer.borrow().is_filter() {
        while layer.borrow_mut().read_byte()?.is_some() {
            read += 1;
            if read.is_multiple_of(READ_CHUNK as u64) {
                probe.check(read / 1024)?;
            }
        }
        let Some(next) = layer.borrow().filter_source() else {
            break;
        };
        layer = next;
    }
    Ok((out, read))
}

/// `ShadingType` 4 and 5 (#1325), as gs 10.07.1 checks them:
/// - `/DataSource` is required (rangecheck), an array of numbers, a string
///   or a file; another type is a typecheck, a non-number in the array too.
/// - A string or file needs `/BitsPerCoordinate` (1, 2, 4, 8, 12, 16, 24 or
///   32), `/BitsPerComponent` (1, 2, 4, 8, 12 or 16) and, but for Type 5,
///   `/BitsPerFlag` (2, 4 or 8): missing is `undefined`, another value
///   rangecheck. `/Decode` must have `4 + 2n` numbers (n values a vertex:
///   one with a `/Function`), rangecheck otherwise; without it gs paints
///   nothing. An array's numbers are used as they are.
/// - Type 5 needs `/VerticesPerRow` (`undefined`), an integer of at least 2
///   (rangecheck; `2.0` is taken).
/// - `/Function` is optional, takes one input and gives the colour space's
///   components (rangecheck otherwise).
///
/// The data is copied (an array's numbers, a string's or a file's bytes)
/// under the memory limit, checked before each copy is taken.
pub(crate) fn parse(
    d: &Dict,
    ty: u8,
    cs: CsKind,
    probe: &crate::limits::LimitProbe,
) -> Result<MeshShading, PsError> {
    let ramp = match d.get("Function") {
        None => None,
        Some(func_obj) => {
            let function = crate::shading::parse_shading_function(&func_obj, cs, 1)?;
            let range = crate::shading::parse_function_range(&func_obj, function.ncomp())?;
            let domain = function.domain1();
            let stops = crate::shading::build_stops(cs, &function, range.as_deref(), domain)?;
            Some(Ramp { stops, domain })
        }
    };
    let per_row = if ty == 5 {
        let v = match d.get("VerticesPerRow").map(|o| o.value) {
            None => return Err(PsError::Undefined("VerticesPerRow".into())),
            Some(Value::Integer(n)) => n as f64,
            Some(Value::Real(r)) => r,
            Some(_) => return Err(PsError::Typecheck),
        };
        if v.fract() != 0.0 || !(2.0..=f64::from(u32::MAX)).contains(&v) {
            return Err(PsError::Rangecheck);
        }
        v as usize
    } else {
        0
    };
    let nvals = if ramp.is_some() { 1 } else { cs.ncomp() };
    let src = d.get("DataSource").ok_or(PsError::Rangecheck)?;
    let bytes = match &src.value {
        Value::Array(a) => {
            probe.check_alloc(a.len().saturating_mul(std::mem::size_of::<f64>()))?;
            let nums = (0..a.len())
                .map(|i| match a.get(i).map(|o| o.value) {
                    Some(Value::Integer(n)) => Ok(n as f64),
                    Some(Value::Real(r)) if r.is_finite() => Ok(r),
                    Some(Value::Real(_)) => Err(PsError::Rangecheck),
                    _ => Err(PsError::Typecheck),
                })
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(MeshShading {
                ty,
                cs,
                ramp,
                per_row,
                source: Source::Numbers(nums),
                bbox: crate::shading::parse_bbox(d)?,
                read: 0,
            });
        }
        Value::String(s) => {
            let b = s.borrow_bytes();
            probe.check_alloc(b.len())?;
            (Rc::from(&*b), 0)
        }
        Value::File(f) => {
            let (v, read) = read_file(f, probe)?;
            // `Rc::from` copies the buffer.
            probe.check_alloc(v.len())?;
            (Rc::from(v), read)
        }
        _ => return Err(PsError::Typecheck),
    };
    let (bytes, read) = bytes;
    let flag_bits = if ty == 5 {
        0
    } else {
        bits_key(d, "BitsPerFlag", &[2, 4, 8])?
    };
    let coord_bits = bits_key(d, "BitsPerCoordinate", &[1, 2, 4, 8, 12, 16, 24, 32])?;
    let comp_bits = bits_key(d, "BitsPerComponent", &[1, 2, 4, 8, 12, 16])?;
    let decode = match d.get("Decode") {
        None => None,
        Some(_) => {
            let flat = crate::shading::get_farray(d, "Decode")?;
            if flat.len() != 4 + 2 * nvals {
                return Err(PsError::Rangecheck);
            }
            Some(flat.chunks(2).map(|c| (c[0], c[1])).collect())
        }
    };
    Ok(MeshShading {
        ty,
        cs,
        ramp,
        per_row,
        source: Source::Packed(Packed {
            data: bytes,
            flag_bits,
            coord_bits,
            comp_bits,
            decode,
        }),
        bbox: crate::shading::parse_bbox(d)?,
        read,
    })
}

/// Reads vertex data in order. Running out inside a vertex is a
/// rangecheck, as in gs.
struct Reader<'a> {
    src: &'a Source,
    /// The next number, or the next bit.
    pos: usize,
    nvals: usize,
}

impl Reader<'_> {
    fn at_end(&self) -> bool {
        match self.src {
            Source::Numbers(n) => self.pos >= n.len(),
            Source::Packed(p) => self.pos >= p.data.len() * 8,
        }
    }

    fn bits(&mut self, p: &Packed, n: u32) -> Result<u64, PsError> {
        let end = self.pos + n as usize;
        if end > p.data.len() * 8 {
            return Err(PsError::Rangecheck);
        }
        let mut v = 0u64;
        for k in self.pos..end {
            v = v << 1 | u64::from(p.data[k / 8] >> (7 - k % 8) & 1);
        }
        self.pos = end;
        Ok(v)
    }

    fn number(&mut self, n: &[f64]) -> Result<f64, PsError> {
        let v = n.get(self.pos).copied().ok_or(PsError::Rangecheck)?;
        self.pos += 1;
        Ok(v)
    }

    /// A flag: in an array, a non-negative integer (gs: 0.5 is a
    /// rangecheck).
    fn flag(&mut self) -> Result<u64, PsError> {
        match self.src {
            Source::Numbers(n) => {
                let v = self.number(n)?;
                if v.fract() != 0.0 || v < 0.0 {
                    return Err(PsError::Rangecheck);
                }
                Ok(v as u64)
            }
            Source::Packed(p) => self.bits(p, p.flag_bits),
        }
    }

    /// The value at Decode pair `k` (0 x, 1 y, then the colour values).
    fn decoded(&mut self, k: usize, coord: bool) -> Result<f64, PsError> {
        match self.src {
            Source::Numbers(n) => self.number(n),
            Source::Packed(p) => {
                let bits = if coord { p.coord_bits } else { p.comp_bits };
                let raw = self.bits(p, bits)? as f64;
                let (lo, hi) = p.decode.as_ref().map_or((0.0, 1.0), |d| d[k]);
                Ok(lo + raw * (hi - lo) / ((1u64 << bits) - 1) as f64)
            }
        }
    }

    fn point(&mut self) -> Result<(f64, f64), PsError> {
        Ok((self.decoded(0, true)?, self.decoded(1, true)?))
    }

    fn colour(&mut self) -> Result<[f32; 4], PsError> {
        let mut c = [0.0f32; 4];
        for (k, v) in c.iter_mut().enumerate().take(self.nvals) {
            *v = self.decoded(2 + k, false)? as f32;
        }
        Ok(c)
    }

    fn vertex(&mut self) -> Result<Vertex, PsError> {
        let (x, y) = self.point()?;
        let c = self.colour()?;
        Ok(Vertex { x, y, c })
    }

    /// Packed vertices each start on a byte.
    fn align(&mut self) {
        if matches!(self.src, Source::Packed(_)) {
            self.pos = self.pos.div_ceil(8) * 8;
        }
    }
}

impl MeshShading {
    /// The data's triangles. A data error is a rangecheck: a vertex,
    /// triangle or row cut short; a Type 4 flag over 2; a Type 5 lattice
    /// with no vertices. Memory is held against the
    /// limit before it is taken.
    ///
    /// Returns the mesh and the work decoding it took, which the caller
    /// charges on every path, painting or not (Codex round 1 on #1376: an
    /// off-page mesh cost nothing). The work covers the later bounds scan,
    /// which is as long as the decoding, and the limits are looked at as it
    /// goes.
    pub(crate) fn decode(&self, probe: &crate::limits::LimitProbe) -> Result<(Mesh, u64), PsError> {
        let units = std::cell::Cell::new(0u64);
        let mesh = self.decode_counted(probe, &units)?;
        Ok((mesh, units.get().saturating_mul(DECODE_WORK)))
    }

    fn decode_counted(
        &self,
        probe: &crate::limits::LimitProbe,
        units: &std::cell::Cell<u64>,
    ) -> Result<Mesh, PsError> {
        // One unit a vertex (or patch) read, the limits looked at every
        // `DECODE_POLL` of them.
        let tick = || {
            let n = units.get() + 1;
            units.set(n);
            if n.is_multiple_of(DECODE_POLL) {
                probe.check(n.saturating_mul(DECODE_WORK) / 1024)
            } else {
                Ok(())
            }
        };
        let nvals = self.nvals();
        let mut r = Reader {
            src: &self.source,
            pos: 0,
            nvals,
        };
        // The most vertices the data can hold.
        let flags = usize::from(self.ty != 5);
        let most = match &self.source {
            Source::Numbers(n) => n.len() / (flags + 2 + nvals),
            Source::Packed(p) if p.decode.is_none() => return Ok(Mesh::Triangles(Vec::new())),
            Source::Packed(p) => {
                let bits =
                    p.flag_bits as usize + 2 * p.coord_bits as usize + nvals * p.comp_bits as usize;
                p.data.len() / bits.div_ceil(8)
            }
        };
        match self.ty {
            4 => {
                probe.check_alloc(most.saturating_mul(std::mem::size_of::<[Vertex; 3]>()))?;
                let mut tris: Vec<[Vertex; 3]> = Vec::new();
                while !r.at_end() {
                    tick()?;
                    let flag = r.flag()?;
                    let v = r.vertex()?;
                    r.align();
                    if flag > 2 {
                        return Err(PsError::Rangecheck);
                    }
                    let tri = match (flag, tris.last()) {
                        (1, Some(&[_, b, c])) => [b, c, v],
                        (2, Some(&[a, _, c])) => [a, c, v],
                        // A new triangle; gs starts one on a first vertex
                        // flagged 1 or 2 too. The next two flags are not
                        // used, but must be 0 to 2.
                        _ => {
                            let mut more = [v; 2];
                            for m in &mut more {
                                if r.flag()? > 2 {
                                    return Err(PsError::Rangecheck);
                                }
                                *m = r.vertex()?;
                                r.align();
                            }
                            [v, more[0], more[1]]
                        }
                    };
                    tris.push(tri);
                }
                Ok(Mesh::Triangles(tris))
            }
            _ => {
                probe.check_alloc(most.saturating_mul(std::mem::size_of::<Vertex>()))?;
                let mut verts = Vec::new();
                while !r.at_end() {
                    tick()?;
                    verts.push(r.vertex()?);
                    r.align();
                }
                if verts.is_empty() || verts.len() % self.per_row != 0 {
                    return Err(PsError::Rangecheck);
                }
                Ok(Mesh::Lattice {
                    verts,
                    per_row: self.per_row,
                })
            }
        }
    }
}

/// `[a b c d tx ty]` applied to a point.
fn apply(t: &[f64; 6], (x, y): (f64, f64)) -> (f64, f64) {
    (t[0] * x + t[2] * y + t[4], t[1] * x + t[3] * y + t[5])
}

impl Mesh {
    /// Every triangle of the mesh in device space (`t`), in paint order,
    /// to `f`. Stops at `f`'s first error.
    fn for_each_triangle(
        &self,
        t: &[f64; 6],
        mut f: impl FnMut([(f64, f64); 3], [[f32; 4]; 3]) -> Result<(), PsError>,
    ) -> Result<(), PsError> {
        let dev = |v: &Vertex| (apply(t, (v.x, v.y)), v.c);
        match self {
            Mesh::Triangles(tris) => {
                for tri in tris {
                    let [a, b, c] = tri.each_ref().map(dev);
                    f([a.0, b.0, c.0], [a.1, b.1, c.1])?;
                }
            }
            Mesh::Lattice { verts, per_row } => {
                let rows = verts.len() / per_row;
                for row in 1..rows {
                    for i in 1..*per_row {
                        let at = |r: usize, k: usize| dev(&verts[r * per_row + k]);
                        let (a, b) = (at(row - 1, i - 1), at(row - 1, i));
                        let (c, d) = (at(row, i - 1), at(row, i));
                        f([a.0, b.0, c.0], [a.1, b.1, c.1])?;
                        f([b.0, c.0, d.0], [b.1, c.1, d.1])?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Every colour value the mesh carries, its vertices' (or corners').
    pub(crate) fn colours(&self) -> Box<dyn Iterator<Item = &[f32; 4]> + '_> {
        match self {
            Mesh::Triangles(tris) => Box::new(tris.iter().flatten().map(|v| &v.c)),
            Mesh::Lattice { verts, .. } => Box::new(verts.iter().map(|v| &v.c)),
        }
    }

    /// Every point that bounds the mesh, in shading space.
    fn points(&self) -> Box<dyn Iterator<Item = (f64, f64)> + '_> {
        match self {
            Mesh::Triangles(tris) => Box::new(tris.iter().flatten().map(|v| (v.x, v.y))),
            Mesh::Lattice { verts, .. } => Box::new(verts.iter().map(|v| (v.x, v.y))),
        }
    }

    /// The device-space box of the mesh under `t`, `[x0 y0 x1 y1]`, from its
    /// finite points; `None` when there are none.
    pub(crate) fn device_box(&self, t: &[f64; 6]) -> Option<[f64; 4]> {
        let mut b: Option<[f64; 4]> = None;
        for pt in self.points() {
            let (x, y) = apply(t, pt);
            if !(x.is_finite() && y.is_finite()) {
                continue;
            }
            b = Some(match b {
                None => [x, y, x, y],
                Some(b) => [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)],
            });
        }
        b
    }

    /// The work painting the mesh into a `w` x `h` layer costs: each
    /// triangle, and each pixel of its box inside the layer. This runs
    /// before the limits are looked at.
    pub(crate) fn work(&self, t: &[f64; 6], w: usize, h: usize) -> u64 {
        let mut work = 0u64;
        let mut add = |tris: u64, px: u64| {
            work = work
                .saturating_add(tris.saturating_mul(TRIANGLE_WORK))
                .saturating_add(px.saturating_mul(PIXEL_WORK));
        };
        let _ = self.for_each_triangle(t, |p, _| {
            add(1, TriBox::of(&p, w, h).map_or(0, |b| b.area()));
            Ok(())
        });
        work
    }
}

/// A triangle's pixel box, clipped to the layer.
struct TriBox {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

impl TriBox {
    fn of(p: &[(f64, f64); 3], w: usize, h: usize) -> Option<TriBox> {
        let lo = |k: fn(&(f64, f64)) -> f64| p.iter().map(k).fold(f64::INFINITY, f64::min);
        let hi = |k: fn(&(f64, f64)) -> f64| p.iter().map(k).fold(f64::NEG_INFINITY, f64::max);
        let (x0, x1) = (lo(|q| q.0), hi(|q| q.0));
        let (y0, y1) = (lo(|q| q.1), hi(|q| q.1));
        if !(x0.is_finite() && x1.is_finite() && y0.is_finite() && y1.is_finite()) {
            return None;
        }
        let clampw = |v: f64| v.clamp(0.0, w as f64) as usize;
        let clamph = |v: f64| v.clamp(0.0, h as f64) as usize;
        let b = TriBox {
            x0: clampw(x0.floor()),
            y0: clamph(y0.floor()),
            x1: clampw(x1.ceil()),
            y1: clamph(y1.ceil()),
        };
        (b.x1 > b.x0 && b.y1 > b.y0).then_some(b)
    }

    fn area(&self) -> u64 {
        ((self.x1 - self.x0) as u64).saturating_mul((self.y1 - self.y0) as u64)
    }
}

/// A mesh painted into a `w` x `h` layer whose top-left is device pixel
/// `(x0, y0)`, `t` mapping shading space to the device (#1325).
/// Premultiplied; transparent outside the mesh and outside `bbox` (the
/// `/BBox` in the layer's own pixels), which is anti-aliased as a fill.
pub(crate) fn mesh_layer(
    shading: &MeshShading,
    mesh: &Mesh,
    t: &[f64; 6],
    (x0, y0, w, h): (f64, f64, usize, usize),
    bbox: Option<&[(f64, f64)]>,
    probe: &crate::limits::LimitProbe,
) -> Result<tiny_skia::Pixmap, PsError> {
    let mut layer = tiny_skia::Pixmap::new(w as u32, h as u32).ok_or(PsError::Limitcheck)?;
    let mut cover = vec![0u16; w * h];
    let mut owned = vec![false; w * h];
    // Shading space to the layer's pixels.
    let t = [t[0], t[1], t[2], t[3], t[4] - x0, t[5] - y0];
    let px = layer.data_mut();
    let mut since_check = 0u64;
    mesh.for_each_triangle(&t, |p, c| {
        since_check += TRIANGLE_WORK;
        if since_check > 1 << 22 {
            since_check = 0;
            probe.check(0)?;
        }
        let painted = raster_triangle(shading, p, c, w, h, px, &mut cover, &mut owned);
        since_check = since_check.saturating_add(painted.saturating_mul(PIXEL_WORK));
        Ok(())
    })?;
    let cut = match bbox {
        Some(q) => {
            let mut m = tiny_skia::Mask::new(w as u32, h as u32).ok_or(PsError::Limitcheck)?;
            let q = crate::gfx::clip_polygon(q, (-1.0, -1.0, w as f64 + 1.0, h as f64 + 1.0));
            if q.len() >= 3 {
                let mut pb = tiny_skia::PathBuilder::new();
                pb.move_to(q[0].0 as f32, q[0].1 as f32);
                for &(x, y) in &q[1..] {
                    pb.line_to(x as f32, y as f32);
                }
                pb.close();
                if let Some(path) = pb.finish() {
                    m.fill_path(
                        &path,
                        tiny_skia::FillRule::Winding,
                        true,
                        tiny_skia::Transform::identity(),
                    );
                }
            }
            Some(m)
        }
        None => None,
    };
    for (k, (&bits, chunk)) in cover.iter().zip(px.as_chunks_mut::<4>().0).enumerate() {
        let mut a = (bits.count_ones() * 255 + 8) / 16;
        if let Some(m) = &cut {
            a = (a * u32::from(m.data()[k]) + 127) / 255;
        }
        for v in &mut chunk[..3] {
            *v = ((u32::from(*v) * a + 127) / 255) as u8;
        }
        chunk[3] = a as u8;
    }
    Ok(layer)
}

/// One triangle into the layer: its coverage samples OR-ed into `cover`,
/// and its colour into every pixel whose centre it holds (`owned` from
/// then on) or, at an edge, into a pixel no triangle's centre holds yet,
/// from the nearest point of the triangle. Returns the pixels it looked at.
#[allow(clippy::too_many_arguments)]
fn raster_triangle(
    shading: &MeshShading,
    mut p: [(f64, f64); 3],
    mut c: [[f32; 4]; 3],
    w: usize,
    h: usize,
    px: &mut [u8],
    cover: &mut [u16],
    owned: &mut [bool],
) -> u64 {
    // Edge k runs from p[k] to p[k + 1]; E_k(x, y) = A x + B y + C is
    // positive on the inside once the triangle is turned the right way.
    let edge = |p: &[(f64, f64); 3], k: usize| {
        let (a, b) = (p[k], p[(k + 1) % 3]);
        let (ea, eb) = (-(b.1 - a.1), b.0 - a.0);
        (ea, eb, -(ea * a.0 + eb * a.1))
    };
    let eval = |e: (f64, f64, f64), x: f64, y: f64| e.0 * x + e.1 * y + e.2;
    let mut area = eval(edge(&p, 0), p[2].0, p[2].1);
    if area < 0.0 {
        p.swap(1, 2);
        c.swap(1, 2);
        area = -area;
    }
    if !(area > 1e-12 && area.is_finite()) {
        return 0;
    }
    let Some(b) = TriBox::of(&p, w, h) else {
        return 0;
    };
    let e = [edge(&p, 0), edge(&p, 1), edge(&p, 2)];
    for y in b.y0..b.y1 {
        let (fy0, fy1) = (y as f64, y as f64 + 1.0);
        for x in b.x0..b.x1 {
            let (fx0, fx1) = (x as f64, x as f64 + 1.0);
            // Each edge's least and greatest value over the pixel square.
            let mut outside = false;
            let mut full = true;
            for &ek in &e {
                let lo = ek.2
                    + ek.0 * if ek.0 < 0.0 { fx1 } else { fx0 }
                    + ek.1 * if ek.1 < 0.0 { fy1 } else { fy0 };
                let hi = ek.2
                    + ek.0 * if ek.0 < 0.0 { fx0 } else { fx1 }
                    + ek.1 * if ek.1 < 0.0 { fy0 } else { fy1 };
                outside |= hi < 0.0;
                full &= lo >= 0.0;
            }
            if outside {
                continue;
            }
            let mask = if full {
                u16::MAX
            } else {
                let mut m = 0u16;
                for sy in 0..SUB {
                    let qy = fy0 + (sy as f64 + 0.5) / SUB as f64;
                    for sx in 0..SUB {
                        let qx = fx0 + (sx as f64 + 0.5) / SUB as f64;
                        if e.iter().all(|&ek| eval(ek, qx, qy) >= 0.0) {
                            m |= 1 << (sy * SUB + sx);
                        }
                    }
                }
                m
            };
            if mask == 0 {
                continue;
            }
            let k = y * w + x;
            cover[k] |= mask;
            let (cx, cy) = (fx0 + 0.5, fy0 + 0.5);
            // Barycentric weights: vertex k's is the edge opposite it.
            let mut l = [
                eval(e[1], cx, cy) / area,
                eval(e[2], cx, cy) / area,
                eval(e[0], cx, cy) / area,
            ];
            let inside = l.iter().all(|&v| v >= 0.0);
            if !inside {
                if owned[k] {
                    continue;
                }
                for v in &mut l {
                    *v = v.max(0.0);
                }
                let s: f64 = l.iter().sum();
                if s > 0.0 {
                    for v in &mut l {
                        *v /= s;
                    }
                }
            }
            owned[k] |= inside;
            let mut val = [0.0f32; 4];
            for (j, o) in val.iter_mut().enumerate() {
                *o = (l[0] * f64::from(c[0][j])
                    + l[1] * f64::from(c[1][j])
                    + l[2] * f64::from(c[2][j])) as f32;
            }
            let (r, g, bl) = shading.rgb(&val);
            let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            px[k * 4..k * 4 + 3].copy_from_slice(&[to8(r), to8(g), to8(bl)]);
        }
    }
    b.area()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mesh(src: &str) -> MeshShading {
        let mut it = crate::Interp::new();
        it.run_str(src).expect("builds the dict");
        let obj = it.pop().expect("the dict");
        let Value::Dict(d) = &obj.value else {
            panic!("a dict");
        };
        let d = d.borrow();
        let ty = match d.get("ShadingType").map(|o| o.value) {
            Some(Value::Integer(n)) => n as u8,
            _ => panic!("a type"),
        };
        parse(&d, ty, CsKind::Rgb, &crate::limits::LimitProbe::NONE).expect("parses")
    }

    /// Packed Type 4 vertices start on a byte (gs: the unaligned packing
    /// paints garbage, the aligned one the triangle).
    #[test]
    fn packed_vertices_are_byte_aligned() {
        // Flag 2 bits, coordinates and components 4: 22 bits a vertex,
        // three bytes each.
        let m = mesh(
            "<< /ShadingType 4 /ColorSpace /DeviceRGB /BitsPerFlag 2 \
             /BitsPerCoordinate 4 /BitsPerComponent 4 \
             /Decode [0 150 0 150 0 1 0 1 0 1] /DataSource <047c003843c01f803c> >>",
        );
        let Mesh::Triangles(t) = m
            .decode(&crate::limits::LimitProbe::NONE)
            .expect("decodes")
            .0
        else {
            panic!("triangles");
        };
        assert_eq!(t.len(), 1);
        assert_eq!((t[0][0].x, t[0][0].y), (10.0, 10.0));
        assert_eq!((t[0][1].x, t[0][1].y), (140.0, 10.0));
        assert_eq!((t[0][2].x, t[0][2].y), (70.0, 140.0));
        assert_eq!(t[0][2].c[..3], [0.0, 0.0, 1.0]);
    }

    #[test]
    fn a_ramp_reads_a_hard_edge_as_its_later_colour() {
        let r = Ramp {
            stops: vec![
                (0.0, 0.0, 0.0, 0.0),
                (0.5, 0.0, 0.0, 0.0),
                (0.5, 1.0, 1.0, 1.0),
                (1.0, 1.0, 1.0, 1.0),
            ],
            domain: (0.0, 2.0),
        };
        assert_eq!(r.at(0.5).0, 0.0);
        assert_eq!(r.at(1.0).0, 1.0);
        assert_eq!(r.at(-3.0).0, 0.0);
        assert_eq!(r.at(9.0).0, 1.0);
    }
}
