//! Color spaces beyond the implicit device three — Stage 8 task 4.
//!
//! `setcolorspace` accepts the device names plus the two array forms
//! found files actually use: `[/Indexed base hival lookup]` with a
//! string lookup table, and `[/Separation name alt tintProc]`. The
//! tint transform is a PostScript procedure; it runs through the
//! machine as a `Frame::PostOp` continuation (`interp.rs`), never by
//! recursing into the interpreter.
//!
//! Documented gaps: Indexed lookup *procedures* (strings are what
//! Photoshop-era EPS emits) and nested Indexed/Separation bases are
//! typecheck errors; CIE-based spaces are undefined.

use std::rc::Rc;

use super::graphics::f32_readback;
use crate::error::PsError;
use crate::gfx::{ColorSpace, IndexedTable};
use crate::interp::{Interp, PostOp};
use crate::object::{Dict, Object, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "setcolorspace", setcolorspace);
    op(dict, "currentcolorspace", currentcolorspace);
    op(dict, "setcolor", setcolor);
    op(dict, "currentcolor", currentcolor);
    op(dict, "makepattern", makepattern);
    op(dict, "setpattern", setpattern);
}

fn device_space(name: &str) -> Option<ColorSpace> {
    match name {
        "DeviceGray" => Some(ColorSpace::Gray),
        "DeviceRGB" => Some(ColorSpace::Rgb),
        "DeviceCMYK" => Some(ColorSpace::Cmyk),
        _ => None,
    }
}

/// A base-space operand for Indexed/Separation: a device-space name,
/// or a one-element array wrapping one.
fn base_ncomp(obj: &Object) -> Result<u32, PsError> {
    let name = match &obj.value {
        Value::Name(n) => n.clone(),
        Value::Array(a) => match a.get(0).as_ref().map(|o| &o.value) {
            Some(Value::Name(n)) => n.clone(),
            _ => return Err(PsError::Typecheck),
        },
        _ => return Err(PsError::Typecheck),
    };
    device_space(&name)
        .map(|cs| cs.ncomp())
        .ok_or(PsError::Typecheck)
}

fn setcolorspace(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    match &obj.value {
        Value::Name(n) if &**n == "Pattern" => {
            it.gfx.set_colorspace(ColorSpace::Pattern {
                base: None,
                src: Object::array(vec![Object::name("Pattern")]),
            });
            Ok(())
        }
        Value::Name(n) => {
            let cs = device_space(n).ok_or_else(|| PsError::Undefined(n.to_string()))?;
            set_device_space(it, cs);
            Ok(())
        }
        Value::Array(a) => {
            let family = match a.get(0).as_ref().map(|o| &o.value) {
                Some(Value::Name(n)) => n.clone(),
                _ => return Err(PsError::Typecheck),
            };
            match &*family {
                "DeviceGray" | "DeviceRGB" | "DeviceCMYK" => {
                    set_device_space(it, device_space(&family).expect("matched above"));
                    Ok(())
                }
                "Indexed" => {
                    if a.len() != 4 {
                        return Err(PsError::Rangecheck);
                    }
                    let base = base_ncomp(&a.get(1).expect("len checked"))?;
                    let hival = match a.get(2).as_ref().map(|o| &o.value) {
                        Some(Value::Integer(n)) if (0..=4095).contains(n) => *n as u32,
                        Some(Value::Integer(_)) => return Err(PsError::Rangecheck),
                        _ => return Err(PsError::Typecheck),
                    };
                    let lookup = match a.get(3).as_ref().map(|o| &o.value) {
                        Some(Value::String(s)) => s.to_vec(),
                        // Procedure lookups: documented gap (module doc).
                        _ => return Err(PsError::Typecheck),
                    };
                    if lookup.len() < ((hival + 1) * base) as usize {
                        return Err(PsError::Rangecheck);
                    }
                    let table = Rc::new(IndexedTable {
                        base_ncomp: base,
                        hival,
                        lookup,
                    });
                    // Initial color for Indexed is index 0, per the PLRM.
                    let (r, g, b) = table.rgb_at(0);
                    it.gfx.set_rgb(f64::from(r), f64::from(g), f64::from(b));
                    if let Some(cmyk) = table.cmyk_at(0) {
                        it.gfx.set_native_cmyk(cmyk);
                    }
                    it.gfx.set_spot(0.0);
                    it.gfx
                        .set_colorspace(ColorSpace::Indexed { table, src: obj });
                    Ok(())
                }
                // `[/Pattern]` or `[/Pattern base]` (#1038): the colour is
                // the null pattern until `setcolor` gives one. The base,
                // an uncoloured pattern's component space, is a device
                // space; it reads back as an array, as in gs.
                "Pattern" => {
                    let (base, src) = match a.len() {
                        1 => (None, Object::array(vec![Object::name("Pattern")])),
                        2 => {
                            let b = a.get(1).expect("len checked");
                            let name = match &b.value {
                                Value::Name(n) => n.clone(),
                                Value::Array(inner) if inner.len() == 1 => {
                                    match inner.get(0).map(|o| o.value) {
                                        Some(Value::Name(n)) => n,
                                        _ => return Err(PsError::Typecheck),
                                    }
                                }
                                _ => return Err(PsError::Typecheck),
                            };
                            let cs = device_space(&name)
                                .ok_or_else(|| PsError::Undefined(name.to_string()))?;
                            let src = Object::array(vec![
                                Object::name("Pattern"),
                                Object::array(vec![Object::name(&name)]),
                            ]);
                            (Some(Box::new(cs)), src)
                        }
                        _ => return Err(PsError::Rangecheck),
                    };
                    it.gfx.set_colorspace(ColorSpace::Pattern { base, src });
                    Ok(())
                }
                "Separation" => {
                    if a.len() != 4 {
                        return Err(PsError::Rangecheck);
                    }
                    // a[1] is the colorant name (name or string) — kept
                    // only inside `src` for currentcolorspace.
                    let alt = base_ncomp(&a.get(2).expect("len checked"))?;
                    let proc = a.get(3).expect("len checked");
                    if !proc.executable || !matches!(proc.value, Value::Array(_)) {
                        return Err(PsError::Typecheck);
                    }
                    it.gfx.set_colorspace(ColorSpace::Separation {
                        alt_ncomp: alt,
                        tint_proc: proc.clone(),
                        src: obj.clone(),
                    });
                    // Initial color is tint 1.0 (full colorant), per the
                    // PLRM — run the transform like any setcolor.
                    it.gfx.set_spot(1.0);
                    it.begin_postop(
                        PostOp::SeparationColor { alt_ncomp: alt },
                        vec![Object::real(1.0)],
                        &proc,
                    )
                }
                _ => Err(PsError::Undefined(family.to_string())),
            }
        }
        _ => Err(PsError::Typecheck),
    }
}

/// A device space comes with its initial colour in gs; pscat leaves the
/// painting colour alone (changing it would move renders), but a CMYK
/// space reads back the PLRM's initial `0 0 0 1` (#513).
fn set_device_space(it: &mut Interp, cs: ColorSpace) {
    if matches!(cs, ColorSpace::Cmyk) {
        it.gfx.set_native_cmyk([0.0, 0.0, 0.0, 1.0]);
    }
    it.gfx.set_colorspace(cs);
}

/// `- currentcolor comp1 ... compn` -- the components of the current
/// colour in the current space (#513): one for Gray, three for RGB, four
/// for CMYK, and the index (an integer) or the tint for Indexed and
/// Separation. Rounded to the `f32` they are held as, as gs prints them.
fn currentcolor(it: &mut Interp) -> Result<(), PsError> {
    let out: Vec<Object> = match it.gfx.colorspace() {
        ColorSpace::Gray => vec![Object::real(f32_readback(it.gfx.rgb().0))],
        ColorSpace::Rgb => {
            let (r, g, b) = it.gfx.rgb();
            [r, g, b]
                .into_iter()
                .map(|v| Object::real(f32_readback(v)))
                .collect()
        }
        ColorSpace::Cmyk => it
            .gfx
            .cmyk()
            .into_iter()
            .map(|v| Object::real(f32_readback(v)))
            .collect(),
        ColorSpace::Indexed { .. } => vec![Object::int(it.gfx.spot() as i64)],
        ColorSpace::Separation { .. } => vec![Object::real(f32_readback(f64::from(it.gfx.spot())))],
        // The pattern dictionary, after an uncoloured pattern's components
        // in the base space; `null` for the null pattern (gs).
        ColorSpace::Pattern { base, .. } => match it.gfx.pattern() {
            None => vec![Object::lit(Value::Null)],
            Some(p) => {
                let mut out = Vec::new();
                if p.pattern.uncolored()
                    && let Some(base) = base
                {
                    out = base_components(it, base);
                }
                out.push(p.dict.clone());
                out
            }
        },
    };
    for o in out {
        it.push(o);
    }
    Ok(())
}

fn currentcolorspace(it: &mut Interp) -> Result<(), PsError> {
    let obj = match it.gfx.colorspace() {
        ColorSpace::Gray => Object::array(vec![Object::name("DeviceGray")]),
        ColorSpace::Rgb => Object::array(vec![Object::name("DeviceRGB")]),
        ColorSpace::Cmyk => Object::array(vec![Object::name("DeviceCMYK")]),
        ColorSpace::Indexed { src, .. }
        | ColorSpace::Separation { src, .. }
        | ColorSpace::Pattern { src, .. } => src.clone(),
    };
    it.push(obj);
    Ok(())
}

/// Component count of the current space's *setcolor operands* (for
/// Indexed that's one index; for Separation one tint).
fn setcolor(it: &mut Interp) -> Result<(), PsError> {
    match it.gfx.colorspace().clone() {
        ColorSpace::Gray => {
            let g = it.pop_f64()?;
            it.gfx.set_rgb(g, g, g);
            Ok(())
        }
        ColorSpace::Rgb => {
            let b = it.pop_f64()?;
            let g = it.pop_f64()?;
            let r = it.pop_f64()?;
            it.gfx.set_rgb(r, g, b);
            Ok(())
        }
        ColorSpace::Cmyk => {
            let k = it.pop_f64()?;
            let y = it.pop_f64()?;
            let m = it.pop_f64()?;
            let c = it.pop_f64()?;
            it.gfx.set_cmyk(c, m, y, k);
            Ok(())
        }
        ColorSpace::Indexed { table, .. } => {
            // Out-of-range indices clamp, pinned against gs (the PLRM
            // says rangecheck; gs renders — so do we).
            let idx = it.pop_int()?.clamp(0, i64::from(table.hival));
            let (r, g, b) = table.rgb_at(idx as usize);
            it.gfx.set_rgb(f64::from(r), f64::from(g), f64::from(b));
            if let Some(cmyk) = table.cmyk_at(idx as usize) {
                it.gfx.set_native_cmyk(cmyk);
            }
            it.gfx.set_spot(idx as f64);
            Ok(())
        }
        ColorSpace::Separation {
            alt_ncomp,
            tint_proc,
            ..
        } => {
            let tint = it.pop_f64()?.clamp(0.0, 1.0);
            it.gfx.set_spot(tint);
            it.begin_postop(
                PostOp::SeparationColor { alt_ncomp },
                vec![Object::real(tint)],
                &tint_proc,
            )
        }
        ColorSpace::Pattern { base, .. } => set_pattern_color(it, base.as_deref()),
    }
}

/// The current colour's components in a device `base` space, as
/// `currentcolor` gives them.
fn base_components(it: &Interp, base: &ColorSpace) -> Vec<Object> {
    let real = |v: f64| Object::real(f32_readback(v));
    match base {
        ColorSpace::Rgb => {
            let (r, g, b) = it.gfx.rgb();
            vec![real(r), real(g), real(b)]
        }
        ColorSpace::Cmyk => it.gfx.cmyk().into_iter().map(real).collect(),
        _ => vec![real(it.gfx.rgb().0)],
    }
}

/// The pattern dictionary on top of the stack and its tile (#1038):
/// `typecheck` for a non-dictionary, `undefined` for a dictionary
/// `makepattern` did not return (gs).
fn top_pattern(it: &Interp) -> Result<(Object, crate::pattern::PatternRef), PsError> {
    let top = it.ostack.last().ok_or(PsError::StackUnderflow)?;
    let Value::Dict(d) = &top.value else {
        return Err(PsError::Typecheck);
    };
    let tile = it
        .pattern_lazy(d)
        .ok_or_else(|| PsError::Undefined("Implementation".to_string()))?;
    Ok((top.clone(), tile))
}

/// `setcolor` in a pattern space (#1038): `pattern` alone, or an
/// uncoloured pattern's components in the base space under it. Checked
/// before anything is popped. With no base (`[/Pattern]`) an uncoloured
/// pattern takes no components and paints in the colour already set, as
/// gs takes it.
fn set_pattern_color(it: &mut Interp, base: Option<&ColorSpace>) -> Result<(), PsError> {
    let (dict, tile) = top_pattern(it)?;
    let n = match base {
        Some(base) if tile.uncolored() => base.ncomp() as usize,
        _ => 0,
    };
    let len = it.ostack.len();
    if len < n + 1 {
        return Err(PsError::StackUnderflow);
    }
    let mut comps = [0f64; 4];
    for (slot, o) in comps.iter_mut().zip(&it.ostack[len - 1 - n..len - 1]) {
        *slot = o.num().ok_or(PsError::Typecheck)?.to_f64();
    }
    it.ostack.truncate(len - 1 - n);
    match n {
        1 => it.gfx.set_rgb(comps[0], comps[0], comps[0]),
        3 => it.gfx.set_rgb(comps[0], comps[1], comps[2]),
        4 => it.gfx.set_cmyk(comps[0], comps[1], comps[2], comps[3]),
        _ => {}
    }
    it.gfx.set_pattern(crate::gfx::PatternColor {
        pattern: tile,
        dict,
    });
    Ok(())
}

/// `pattern setpattern` / `comp1 ... compn pattern setpattern` (#1038):
/// outside a pattern space, first set `[/Pattern cs]` over the current
/// space `cs` (gs does this for coloured patterns too), then `setcolor`.
/// A current space that is not a device space gives a pattern space
/// with no base.
fn setpattern(it: &mut Interp) -> Result<(), PsError> {
    top_pattern(it)?;
    if !matches!(it.gfx.colorspace(), ColorSpace::Pattern { .. }) {
        let base = match it.gfx.colorspace() {
            cs @ (ColorSpace::Gray | ColorSpace::Rgb | ColorSpace::Cmyk) => {
                Some(Box::new(cs.clone()))
            }
            _ => None,
        };
        currentcolorspace(it)?;
        let current = it.pop()?;
        let src = Object::array(vec![Object::name("Pattern"), current]);
        it.gfx.set_colorspace(ColorSpace::Pattern { base, src });
    }
    setcolor(it)
}

/// `dict matrix makepattern pattern` (#1038, #1298): PatternType 1
/// (tiling) or 2 (shading). The pattern matrix is `matrix` times the CTM;
/// the dictionary returned is a copy of `dict` with an `/Implementation`
/// entry, as in gs. A tiling pattern's `PaintProc` runs at the pattern's
/// first use, as in gs (`crate::pattern`, #1341).
/// Errors as gs gives them: a missing key is `undefined`, a value of the
/// wrong type `typecheck`, out of range (including an empty or inverted
/// `/BBox` and a zero step) `rangecheck`. gs takes a real PatternType with
/// a whole value (`2.0`).
fn makepattern(it: &mut Interp) -> Result<(), PsError> {
    let len = it.ostack.len();
    if len < 2 {
        return Err(PsError::StackUnderflow);
    }
    let matrix = super::matrix::read_matrix(&it.ostack[len - 1]).map_err(|_| PsError::Typecheck)?;
    let Value::Dict(d) = &it.ostack[len - 2].value else {
        return Err(PsError::Typecheck);
    };
    let d = d.clone();
    let pattern_type = match d.borrow().get("PatternType").map(|o| o.value) {
        None => return Err(PsError::Undefined("PatternType".to_string())),
        Some(Value::Integer(n)) => n as f64,
        Some(Value::Real(r)) => r,
        Some(_) => return Err(PsError::Typecheck),
    };
    if pattern_type == 2.0 {
        return make_shading_pattern(it, &d, matrix);
    }
    if pattern_type != 1.0 {
        return Err(PsError::Rangecheck);
    }
    let (spec, proc) = {
        let dict = d.borrow();
        let get = |key: &str| {
            dict.get(key)
                .ok_or_else(|| PsError::Undefined(key.to_string()))
        };
        let int = |key: &str| match get(key)?.value {
            Value::Integer(n) => Ok(n),
            _ => Err(PsError::Typecheck),
        };
        let num = |o: &Object| o.num().map(|n| n.to_f64()).ok_or(PsError::Typecheck);
        let paint_type = int("PaintType")?;
        let tiling_type = int("TilingType")?;
        if !(1..=2).contains(&paint_type) || !(1..=3).contains(&tiling_type) {
            return Err(PsError::Rangecheck);
        }
        let Value::Array(bbox) = get("BBox")?.value else {
            return Err(PsError::Typecheck);
        };
        if bbox.len() != 4 {
            return Err(PsError::Rangecheck);
        }
        let mut b = [0f64; 4];
        for (i, slot) in b.iter_mut().enumerate() {
            *slot = num(&bbox.get(i).expect("len checked"))?;
        }
        let xstep = num(&get("XStep")?)?;
        let ystep = num(&get("YStep")?)?;
        let proc = get("PaintProc")?;
        if !proc.executable || !matches!(proc.value, Value::Array(_)) {
            return Err(PsError::Typecheck);
        }
        if !(b[0] < b[2] && b[1] < b[3] && b.iter().all(|v| v.is_finite()))
            || xstep == 0.0
            || ystep == 0.0
            || !xstep.is_finite()
            || !ystep.is_finite()
        {
            return Err(PsError::Rangecheck);
        }
        let spec = crate::pattern::TileSpec {
            uncolored: paint_type == 2,
            tiling_type: tiling_type as u8,
            bbox: b,
            xstep,
            ystep,
            matrix: it.gfx.ctm().pre_concat(matrix),
        };
        (spec, proc)
    };
    if matrix.invert().is_none() {
        return Err(PsError::Typecheck);
    }
    let copy = pattern_copy(&d)?;
    // The geometry is checked now; the `PaintProc` runs at first use
    // (#1341, `crate::pattern`).
    let lazy = it.gfx.make_pattern(spec, proc)?;
    it.remember_pattern(&copy, crate::pattern::PatternRef::Tile(lazy));
    it.ostack.truncate(len - 2);
    it.push(Object::lit(Value::Dict(copy)));
    Ok(())
}

/// The dictionary `makepattern` returns: a copy of `d` with an
/// `/Implementation` entry, as in gs.
fn pattern_copy(d: &Rc<std::cell::RefCell<Dict>>) -> Result<Rc<std::cell::RefCell<Dict>>, PsError> {
    let mut copy = Dict::new();
    for (k, v) in d.borrow().pairs() {
        copy.put_obj(&k, v)?;
    }
    copy.put("Implementation".into(), Object::array(Vec::new()));
    Ok(Rc::new(std::cell::RefCell::new(copy)))
}

/// `makepattern` of a PatternType 2 dictionary (#1298): its `/Shading`
/// is read as `shfill` reads one, with the same errors, plus its
/// `/Background`; a missing `/Shading` is `undefined`, one that is not a
/// dictionary `typecheck` (gs 10.07.1). `/ExtGState` is not read (gs takes
/// any value for it, and pscat has no ExtGState). A singular matrix is
/// taken, as gs takes it, and paints nothing. Nothing runs: a shading
/// pattern has no `PaintProc`.
fn make_shading_pattern(
    it: &mut Interp,
    d: &Rc<std::cell::RefCell<Dict>>,
    matrix: tiny_skia::Transform,
) -> Result<(), PsError> {
    let probe = it.limit_probe();
    let (shading, background) = {
        let dict = d.borrow();
        let sh = dict
            .get("Shading")
            .ok_or_else(|| PsError::Undefined("Shading".to_string()))?;
        let Value::Dict(sd) = &sh.value else {
            return Err(PsError::Typecheck);
        };
        let sd = sd.borrow();
        (
            crate::shading::parse_shading_dict(&sd, &probe)?,
            crate::shading::parse_background(&sd)?,
        )
    };
    let copy = pattern_copy(d)?;
    let to_device = it.gfx.ctm().pre_concat(matrix);
    let (sp, work) = it
        .gfx
        .make_shading_pattern(shading, background, to_device, &probe)?;
    it.charge_probed_steps(work / 1024);
    it.remember_pattern(&copy, crate::pattern::PatternRef::Shading(sp));
    let len = it.ostack.len();
    it.ostack.truncate(len - 2);
    it.push(Object::lit(Value::Dict(copy)));
    Ok(())
}
