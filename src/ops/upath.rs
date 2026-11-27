//! Hit testing and user paths (#1264, PLRM 3rd ed.): `infill`, `ineofill`,
//! `instroke`, `inufill`, `inueofill`, `inustroke`, `uappend`, `upath`,
//! `ufill`, `ueofill`, `ustroke`, `ustrokepath`, `ucache` and `setbbox`.
//!
//! A user path is an array of numbers and the operators `moveto lineto
//! rmoveto rlineto curveto rcurveto arc arcn arct closepath`, led by
//! `[ucache] llx lly urx ury setbbox`. `append_userpath` runs it the way
//! Ghostscript does: each operator is the path operator of that name under
//! the box `setbbox` set, so a point past the box is a `rangecheck`, and a
//! user path that fails leaves the current path as it was. The operators
//! that paint or test one run it on a path of its own (`ufill` is `gsave
//! newpath userpath uappend fill grestore`), swapped in for the current
//! path and swapped back whatever happens.
//!
//! Left out, with `notes/1264-hit-testing-and-user-paths.md`: the forms of
//! the hit tests whose operand is a user path in place of a point (the
//! region forms), and encoded user paths.

use tiny_skia::{FillRule, Transform};

use crate::error::PsError;
use crate::gfx::PathElement;
use crate::interp::Interp;
use crate::name::PsName;
use crate::object::{Dict, Object, PsArray, Value};

use super::graphics::{arc_apply, arcto_apply, fill_common, pop_xy, wait_for_pattern};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "setbbox", setbbox);
    op(dict, "ucache", ucache);
    op(dict, "uappend", uappend);
    op(dict, "upath", upath);
    op(dict, "ufill", ufill);
    op(dict, "ueofill", ueofill);
    op(dict, "ustroke", ustroke);
    op(dict, "ustrokepath", ustrokepath);
    op(dict, "infill", infill);
    op(dict, "ineofill", ineofill);
    op(dict, "instroke", instroke);
    op(dict, "inufill", inufill);
    op(dict, "inueofill", inueofill);
    op(dict, "inustroke", inustroke);
}

/// Elements of a user path between looks at the limits, as for a path of
/// segments (`gfx::FLATTEN_POLL`): each look is 4 of the 1024-element steps.
const POLL: usize = 4096;

/// Bytes one user-path element can add to the path (a segment of up to
/// three points and its tag), held against the heap cap before it is added.
const SEG_BYTES: usize = 64;

/// `llx lly urx ury setbbox`: the box later path points must lie in.
fn setbbox(it: &mut Interp) -> Result<(), PsError> {
    let ury = it.pop_f64()?;
    let urx = it.pop_f64()?;
    let lly = it.pop_f64()?;
    let llx = it.pop_f64()?;
    it.gfx.set_bbox(llx, lly, urx, ury, true)
}

/// `- ucache -`: a hint that the user path may be cached. Nothing here
/// caches, so it is accepted and does nothing, as in gs.
fn ucache(_it: &mut Interp) -> Result<(), PsError> {
    Ok(())
}

/// One of the operators a user path may hold.
#[derive(Clone, Copy, PartialEq)]
enum UOp {
    Ucache,
    Setbbox,
    Moveto,
    Lineto,
    Rmoveto,
    Rlineto,
    Curveto,
    Rcurveto,
    Arc,
    Arcn,
    Arct,
    Closepath,
}

impl UOp {
    fn named(name: &str) -> Option<UOp> {
        Some(match name {
            "ucache" => UOp::Ucache,
            "setbbox" => UOp::Setbbox,
            "moveto" => UOp::Moveto,
            "lineto" => UOp::Lineto,
            "rmoveto" => UOp::Rmoveto,
            "rlineto" => UOp::Rlineto,
            "curveto" => UOp::Curveto,
            "rcurveto" => UOp::Rcurveto,
            "arc" => UOp::Arc,
            "arcn" => UOp::Arcn,
            "arct" => UOp::Arct,
            "closepath" => UOp::Closepath,
            _ => return None,
        })
    }

    /// How many numbers it takes.
    fn operands(self) -> usize {
        match self {
            UOp::Ucache | UOp::Closepath => 0,
            UOp::Moveto | UOp::Lineto | UOp::Rmoveto | UOp::Rlineto => 2,
            UOp::Setbbox => 4,
            UOp::Arc | UOp::Arcn | UOp::Arct => 5,
            UOp::Curveto | UOp::Rcurveto => 6,
        }
    }
}

/// What one element of a user path is.
enum Token {
    Number(f64),
    Op(UOp),
    Other,
}

/// An element as the decoder reads it: a number, or an operator (an
/// executable name, as in a procedure, or an operator object, as in a bound
/// one: a literal name is neither, as in gs).
fn token(o: &Object) -> Token {
    match &o.value {
        Value::Integer(i) => Token::Number(*i as f64),
        Value::Real(r) => Token::Number(*r),
        Value::Name(n) if o.executable => UOp::named(n).map_or(Token::Other, Token::Op),
        Value::Operator(op) if o.executable => UOp::named(op.name).map_or(Token::Other, Token::Op),
        _ => Token::Other,
    }
}

/// No current point inside a user path is a `rangecheck`, not
/// `nocurrentpoint` (gs).
fn in_userpath(e: PsError) -> PsError {
    if e == PsError::NoCurrentPoint {
        PsError::Rangecheck
    } else {
        e
    }
}

/// Append the user path `up` to the current path, as `uappend` does.
///
/// Runs under the limits: the heap it can add is asked of the cap first,
/// the clock and the step count are looked at every [`POLL`] elements, and
/// an arc is charged by the segments it makes, as `arc` is. Any failure,
/// a limit included, takes back all it added, so the path is as it was.
/// The box `setbbox` sets belongs to the user path alone: the path's own
/// box is back when it is done.
pub(crate) fn append_userpath(it: &mut Interp, up: &PsArray) -> Result<(), PsError> {
    it.check_alloc(up.len().saturating_mul(SEG_BYTES))?;
    let mark = it.gfx.path_mark();
    it.gfx.path_detach();
    let done = run_userpath(it, up);
    it.gfx.path_finish(mark, done.is_ok());
    done
}

fn run_userpath(it: &mut Interp, up: &PsArray) -> Result<(), PsError> {
    // `Head`: before `setbbox` (only `ucache` may come before it), `Body`:
    // after.
    let mut body = false;
    let mut cached = false;
    let mut nums = [0f64; 6];
    let mut have = 0;
    for i in 0..up.len() {
        match up.peek(i, token).unwrap_or(Token::Other) {
            Token::Number(v) => {
                *nums.get_mut(have).ok_or(PsError::Typecheck)? = v;
                have += 1;
            }
            Token::Other => return Err(PsError::Typecheck),
            Token::Op(op) => {
                if have != op.operands() {
                    return Err(PsError::Typecheck);
                }
                let n = &nums[..have];
                have = 0;
                match op {
                    UOp::Ucache if !body && !cached => cached = true,
                    UOp::Setbbox if !body => {
                        it.gfx.set_bbox(n[0], n[1], n[2], n[3], false)?;
                        body = true;
                    }
                    UOp::Ucache | UOp::Setbbox => return Err(PsError::Typecheck),
                    _ if !body => return Err(PsError::Typecheck),
                    UOp::Moveto => it.gfx.moveto_checked(n[0], n[1])?,
                    UOp::Lineto => it.gfx.lineto(n[0], n[1]).map_err(in_userpath)?,
                    UOp::Rmoveto => it.gfx.rmoveto(n[0], n[1]).map_err(in_userpath)?,
                    UOp::Rlineto => it.gfx.rlineto(n[0], n[1]).map_err(in_userpath)?,
                    UOp::Curveto => it
                        .gfx
                        .curveto(n[0], n[1], n[2], n[3], n[4], n[5])
                        .map_err(in_userpath)?,
                    UOp::Rcurveto => it
                        .gfx
                        .rcurveto(n[0], n[1], n[2], n[3], n[4], n[5])
                        .map_err(in_userpath)?,
                    UOp::Arc => arc_apply(it, n[0], n[1], n[2], n[3], n[4], true)?,
                    UOp::Arcn => arc_apply(it, n[0], n[1], n[2], n[3], n[4], false)?,
                    UOp::Arct => {
                        arcto_apply(it, n[0], n[1], n[2], n[3], n[4]).map_err(in_userpath)?;
                    }
                    UOp::Closepath => it.gfx.closepath(),
                }
            }
        }
        if i % POLL == POLL - 1 {
            it.poll_limits(4)?;
        }
    }
    if !body || have != 0 {
        return Err(PsError::Typecheck);
    }
    Ok(())
}

/// The array a user-path operand must be.
fn userpath_operand(o: Object) -> Result<PsArray, PsError> {
    match o.value {
        Value::Array(a) => Ok(a),
        _ => Err(PsError::Typecheck),
    }
}

/// The matrix at the top of the stack, if one is there (six numbers, as
/// gs reads it: any other array is a user path), popped.
fn pop_matrix(it: &mut Interp) -> Result<Option<Transform>, PsError> {
    let top = it.operand_stack().last().ok_or(PsError::StackUnderflow)?;
    let is_matrix = match &top.value {
        Value::Array(a) if a.len() == 6 => (0..6).all(|i| {
            a.peek(i, |o| matches!(o.value, Value::Integer(_) | Value::Real(_))) == Some(true)
        }),
        _ => false,
    };
    if !is_matrix {
        return Ok(None);
    }
    let obj = it.pop()?;
    super::matrix::read_matrix(&obj).map(Some)
}

/// `userpath uappend`.
fn uappend(it: &mut Interp) -> Result<(), PsError> {
    let up = userpath_operand(it.pop()?)?;
    append_userpath(it, &up)
}

/// `bool upath -> userpath`: the current path as a user path in the
/// current user space, led by `ucache` if `bool` is true and by its
/// `pathbbox` as `setbbox`. A lineto or curveto after a closepath comes
/// back with the moveto it starts at, as `pathforall` shows it. The empty
/// path is `0 0 0 0 setbbox`.
fn upath(it: &mut Interp) -> Result<(), PsError> {
    let flag = match it.pop()?.value {
        Value::Boolean(b) => b,
        _ => return Err(PsError::Typecheck),
    };
    let segs = it.gfx.state().path.seg_count();
    // At most a moveto and a lineto's three elements per segment, a curve's
    // seven, and the header: held against the heap cap before the array is
    // made.
    it.check_alloc(
        segs.saturating_mul(10)
            .saturating_add(8)
            .saturating_mul(std::mem::size_of::<Object>()),
    )?;
    let (lx, ly, ux, uy) = match it.gfx.path_bbox() {
        Ok(b) => b,
        Err(PsError::NoCurrentPoint) => (0.0, 0.0, 0.0, 0.0),
        Err(e) => return Err(e),
    };
    let name = |s: &str| Object::exec(Value::Name(PsName::from(s)));
    let (n_move, n_line, n_curve, n_close) = (
        name("moveto"),
        name("lineto"),
        name("curveto"),
        name("closepath"),
    );
    let mut out: Vec<Object> = Vec::with_capacity(segs.saturating_add(8));
    if flag {
        out.push(name("ucache"));
    }
    out.extend([lx, ly, ux, uy].map(Object::real));
    out.push(name("setbbox"));
    // `pathforall`'s rule: after a closepath, a lineto or curveto starts at
    // the start of the subpath that was closed.
    let (mut last_move, mut just_closed) = (None, false);
    for i in 0..segs {
        let Some(el) = it.gfx.state().path.element_at(i) else {
            break;
        };
        if matches!(el, PathElement::Line(_) | PathElement::Curve(..))
            && just_closed
            && let Some(p) = last_move
        {
            let (x, y) = it.gfx.device_to_user(p)?;
            out.extend([Object::real(x), Object::real(y), n_move.clone()]);
        }
        match el {
            PathElement::Move(p) => {
                last_move = Some(p);
                just_closed = false;
                let (x, y) = it.gfx.device_to_user(p)?;
                out.extend([Object::real(x), Object::real(y), n_move.clone()]);
            }
            PathElement::Line(p) => {
                just_closed = false;
                let (x, y) = it.gfx.device_to_user(p)?;
                out.extend([Object::real(x), Object::real(y), n_line.clone()]);
            }
            PathElement::Curve(a, b, p) => {
                just_closed = false;
                for q in [a, b, p] {
                    let (x, y) = it.gfx.device_to_user(q)?;
                    out.extend([Object::real(x), Object::real(y)]);
                }
                out.push(n_curve.clone());
            }
            PathElement::Close => {
                just_closed = true;
                out.push(n_close.clone());
            }
        }
        if i % POLL == POLL - 1 {
            it.poll_limits(4)?;
        }
    }
    it.push(Object::exec(Value::Array(PsArray::from_vec(out))));
    Ok(())
}

/// Run `body` on the user path `up` as the current path, with the current
/// path and CTM put back after (`ufill` is `gsave newpath userpath uappend
/// fill grestore`). With `keep_path`, a `body` that succeeds leaves what it
/// made as the current path (`ustrokepath`).
fn on_userpath<T>(
    it: &mut Interp,
    up: &PsArray,
    keep_path: bool,
    body: impl FnOnce(&mut Interp) -> Result<T, PsError>,
) -> Result<T, PsError> {
    let saved_path = std::mem::take(&mut it.gfx.state_mut().path);
    let saved_ctm = it.gfx.ctm();
    let result = append_userpath(it, up).and_then(|()| body(it));
    it.gfx.set_ctm(saved_ctm);
    if !(keep_path && result.is_ok()) {
        it.gfx.state_mut().path = saved_path;
    }
    result
}

fn ufill(it: &mut Interp) -> Result<(), PsError> {
    if wait_for_pattern(it, "ufill", ufill)? {
        return Ok(());
    }
    ufill_common(it, FillRule::Winding)
}

fn ueofill(it: &mut Interp) -> Result<(), PsError> {
    if wait_for_pattern(it, "ueofill", ueofill)? {
        return Ok(());
    }
    ufill_common(it, FillRule::EvenOdd)
}

fn ufill_common(it: &mut Interp, rule: FillRule) -> Result<(), PsError> {
    let up = userpath_operand(it.pop()?)?;
    on_userpath(it, &up, false, |it| fill_common(it, rule))
}

/// `userpath [matrix] ustroke`: the matrix shapes the pen and is applied
/// after the path is made, as `gsave newpath userpath uappend matrix concat
/// stroke grestore` does.
fn ustroke(it: &mut Interp) -> Result<(), PsError> {
    if wait_for_pattern(it, "ustroke", ustroke)? {
        return Ok(());
    }
    let matrix = pop_matrix(it)?;
    let up = userpath_operand(it.pop()?)?;
    on_userpath(it, &up, false, |it| {
        if let Some(m) = matrix {
            it.gfx.concat_ctm(m);
        }
        let probe = it.limit_probe();
        let work = it.gfx.stroke_limited(&probe)?;
        it.charge_probed_steps(work / 1024);
        Ok(())
    })
}

/// `userpath [matrix] ustrokepath`: the outline of the stroke becomes the
/// current path. An error leaves the current path as it was.
fn ustrokepath(it: &mut Interp) -> Result<(), PsError> {
    let matrix = pop_matrix(it)?;
    let up = userpath_operand(it.pop()?)?;
    on_userpath(it, &up, true, |it| {
        if let Some(m) = matrix {
            it.gfx.concat_ctm(m);
        }
        let probe = it.limit_probe();
        let work = it.gfx.stroke_path_limited(&probe)?;
        it.charge_probed_steps(work / 1024);
        Ok(())
    })
}

/// The answer to a hit test: the boolean, and the work charged.
fn answer(it: &mut Interp, (hit, work): (bool, u64)) {
    it.charge_probed_steps(work / 1024);
    it.push(Object::lit(Value::Boolean(hit)));
}

fn infill(it: &mut Interp) -> Result<(), PsError> {
    hit_fill_current(it, FillRule::Winding)
}

fn ineofill(it: &mut Interp) -> Result<(), PsError> {
    hit_fill_current(it, FillRule::EvenOdd)
}

/// `x y infill -> bool`: whether `fill` would paint the pixel the point is
/// in.
fn hit_fill_current(it: &mut Interp, rule: FillRule) -> Result<(), PsError> {
    let (x, y) = pop_xy(it)?;
    let at = it.gfx.user_to_device(x, y);
    let probe = it.limit_probe();
    let hit = it.gfx.hit_fill(rule, at, &probe)?;
    answer(it, hit);
    Ok(())
}

/// `x y instroke -> bool`.
fn instroke(it: &mut Interp) -> Result<(), PsError> {
    let (x, y) = pop_xy(it)?;
    let at = it.gfx.user_to_device(x, y);
    let probe = it.limit_probe();
    let hit = it.gfx.hit_stroke(at, &probe)?;
    answer(it, hit);
    Ok(())
}

fn inufill(it: &mut Interp) -> Result<(), PsError> {
    hit_fill_userpath(it, FillRule::Winding)
}

fn inueofill(it: &mut Interp) -> Result<(), PsError> {
    hit_fill_userpath(it, FillRule::EvenOdd)
}

/// `x y userpath inufill -> bool`.
fn hit_fill_userpath(it: &mut Interp, rule: FillRule) -> Result<(), PsError> {
    let up = userpath_operand(it.pop()?)?;
    let (x, y) = pop_xy(it)?;
    let at = it.gfx.user_to_device(x, y);
    let hit = on_userpath(it, &up, false, |it| {
        let probe = it.limit_probe();
        it.gfx.hit_fill(rule, at, &probe)
    })?;
    answer(it, hit);
    Ok(())
}

/// `x y userpath [matrix] inustroke -> bool`. The point is in the user
/// space the operator was called in; the matrix shapes the pen only.
fn inustroke(it: &mut Interp) -> Result<(), PsError> {
    let matrix = pop_matrix(it)?;
    let up = userpath_operand(it.pop()?)?;
    let (x, y) = pop_xy(it)?;
    // The pixel is found in the space the point was given in, before the
    // matrix changes it.
    let at = it.gfx.user_to_device(x, y);
    let hit = on_userpath(it, &up, false, |it| {
        if let Some(m) = matrix {
            it.gfx.concat_ctm(m);
        }
        let probe = it.limit_probe();
        it.gfx.hit_stroke(at, &probe)
    })?;
    answer(it, hit);
    Ok(())
}
