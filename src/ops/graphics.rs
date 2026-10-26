//! Path construction, painting, graphics state, and coordinate-system
//! operators. These are thin operand-stack adapters; the geometry lives
//! in `crate::gfx`.

use tiny_skia::{FillRule, Transform};

use crate::error::PsError;
use crate::interp::Interp;
use crate::object::{Dict, Object, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    // Path construction
    op(dict, "newpath", newpath);
    op(dict, "moveto", moveto);
    op(dict, "lineto", lineto);
    op(dict, "rmoveto", rmoveto);
    op(dict, "rlineto", rlineto);
    op(dict, "curveto", curveto);
    op(dict, "rcurveto", rcurveto);
    op(dict, "arc", arc);
    op(dict, "arcn", arcn);
    op(dict, "closepath", closepath);
    op(dict, "currentpoint", currentpoint);
    // Painting
    op(dict, "fill", fill);
    op(dict, "eofill", eofill);
    op(dict, "stroke", stroke);
    op(dict, "strokepath", strokepath);
    op(dict, "erasepage", erasepage);
    op(dict, "showpage", showpage);
    op(dict, "copypage", copypage);
    op(dict, "nulldevice", nulldevice);
    op(dict, "initgraphics", initgraphics);
    // Graphics state
    op(dict, "gsave", gsave);
    op(dict, "grestore", grestore);
    op(dict, "grestoreall", grestoreall);
    // Level 2 gstate objects (#394)
    op(dict, "gstate", gstate);
    op(dict, "currentgstate", currentgstate);
    op(dict, "setgstate", setgstate);
    op(dict, "setgray", setgray);
    op(dict, "setrgbcolor", setrgbcolor);
    op(dict, "setlinewidth", setlinewidth);
    op(dict, "setflat", setflat);
    op(dict, "currentflat", currentflat);
    op(dict, "setlinecap", setlinecap);
    op(dict, "setlinejoin", setlinejoin);
    op(dict, "setmiterlimit", setmiterlimit);
    op(dict, "currentlinecap", currentlinecap);
    op(dict, "currentlinejoin", currentlinejoin);
    op(dict, "currentmiterlimit", currentmiterlimit);
    op(dict, "setstrokeadjust", setstrokeadjust);
    op(dict, "currentstrokeadjust", currentstrokeadjust);
    op(dict, "setdash", setdash);
    op(dict, "currentdash", currentdash);
    op(dict, "sethsbcolor", sethsbcolor);
    op(dict, "currenthsbcolor", currenthsbcolor);
    op(dict, "setcmykcolor", setcmykcolor);
    op(dict, "currentcmykcolor", currentcmykcolor);
    op(dict, "currentrgbcolor", currentrgbcolor);
    op(dict, "currentgray", currentgray);
    op(dict, "currentlinewidth", currentlinewidth);
    // pscat extensions (issue #47) -- not PLRM operators; see their
    // definitions below and docs/dev/design/WATERCOLOR.md.
    op(dict, "setalpha", setalpha);
    op(dict, "currentalpha", currentalpha);
    op(dict, "setblendmode", setblendmode);
    op(dict, "currentblendmode", currentblendmode);
    // pscat extension (issue #488): a clip identity query, dot-named as
    // non-standard; lib/drykit.ps probes it with `where`.
    op(dict, ".sameclip", sameclip);
    // Rectangle conveniences (Level 2)
    op(dict, "rectfill", rectfill);
    op(dict, "rectstroke", rectstroke);
    op(dict, "rectclip", rectclip);
    // Clipping
    op(dict, "clip", clip);
    op(dict, "eoclip", eoclip);
    op(dict, "clippath", clippath);
    op(dict, "initclip", initclip);
    // Level 3 (#494)
    op(dict, "clipsave", clipsave);
    op(dict, "cliprestore", cliprestore);
    op(dict, "pathbbox", pathbbox);
    op(dict, "pathforall", pathforall);
    op(dict, "flattenpath", flattenpath);
    // Coordinate system (the matrix-operand forms live in ops::matrix)
    op(dict, "translate", translate);
    op(dict, "scale", scale);
    op(dict, "rotate", rotate);
}

fn pop_xy(it: &mut Interp) -> Result<(f64, f64), PsError> {
    let y = it.pop_f64()?;
    let x = it.pop_f64()?;
    Ok((x, y))
}

fn newpath(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.newpath();
    Ok(())
}

fn moveto(it: &mut Interp) -> Result<(), PsError> {
    let (x, y) = pop_xy(it)?;
    it.gfx.moveto(x, y);
    Ok(())
}

fn lineto(it: &mut Interp) -> Result<(), PsError> {
    let (x, y) = pop_xy(it)?;
    it.gfx.lineto(x, y)
}

fn rmoveto(it: &mut Interp) -> Result<(), PsError> {
    let (dx, dy) = pop_xy(it)?;
    it.gfx.rmoveto(dx, dy)
}

fn rlineto(it: &mut Interp) -> Result<(), PsError> {
    let (dx, dy) = pop_xy(it)?;
    it.gfx.rlineto(dx, dy)
}

fn pop6(it: &mut Interp) -> Result<[f64; 6], PsError> {
    let mut v = [0.0; 6];
    for slot in v.iter_mut().rev() {
        *slot = it.pop_f64()?;
    }
    Ok(v)
}

fn curveto(it: &mut Interp) -> Result<(), PsError> {
    let [x1, y1, x2, y2, x3, y3] = pop6(it)?;
    it.gfx.curveto(x1, y1, x2, y2, x3, y3)
}

fn rcurveto(it: &mut Interp) -> Result<(), PsError> {
    let [dx1, dy1, dx2, dy2, dx3, dy3] = pop6(it)?;
    it.gfx.rcurveto(dx1, dy1, dx2, dy2, dx3, dy3)
}

fn arc_common(it: &mut Interp, ccw: bool) -> Result<(), PsError> {
    let ang2 = it.pop_f64()?;
    let ang1 = it.pop_f64()?;
    let r = it.pop_f64()?;
    let (x, y) = pop_xy(it)?;
    it.gfx.arc(x, y, r, ang1, ang2, ccw)
}

fn arc(it: &mut Interp) -> Result<(), PsError> {
    arc_common(it, true)
}

fn arcn(it: &mut Interp) -> Result<(), PsError> {
    arc_common(it, false)
}

fn closepath(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.closepath();
    Ok(())
}

fn currentpoint(it: &mut Interp) -> Result<(), PsError> {
    let (x, y) = it.gfx.current_user_point()?;
    it.push(Object::real(x));
    it.push(Object::real(y));
    Ok(())
}

/// move line curve close pathforall — enumerate the current path,
/// running the matching proc per element with its coordinates (user
/// space) on the stack. The path is snapshotted when the operator
/// runs; the PLRM leaves mid-enumeration path mutation undefined, and
/// a snapshot keeps the charpath-then-rebuild idiom safe.
fn pathforall(it: &mut Interp) -> Result<(), PsError> {
    let mut procs: [Object; 4] = std::array::from_fn(|_| Object::lit(Value::Null));
    for slot in procs.iter_mut().rev() {
        let obj = it.pop()?;
        match &obj.value {
            Value::Array(_) if obj.executable => *slot = obj,
            _ => return Err(PsError::Typecheck),
        }
    }
    use crate::gfx::PathElement;
    use crate::interp::PathForallEl;
    let mut elems = Vec::new();
    // Per the PLRM: a lineto/curveto that immediately follows closepath
    // with no intervening moveto behaves as though a moveto to the
    // current point (which closepath resets to the subpath's own
    // start) had been inserted first -- Ghostscript honors this
    // (confirmed directly), starting a fresh subpath there rather than
    // silently extending the just-closed one. `PsPath`'s own segment
    // list doesn't carry that implicit moveto, so it's synthesized here
    // rather than in the path model itself -- this is the one consumer
    // that cares about subpath *boundaries*; renderers and exporters
    // walk the same segments unchanged and don't need it (a cross-model
    // review finding, PR #76).
    let mut last_move = None;
    let mut just_closed = false;
    for el in it.gfx.state().path.elements() {
        match el {
            PathElement::Move(p) => {
                last_move = Some(p);
                just_closed = false;
            }
            PathElement::Line(_) | PathElement::Curve(..) if just_closed => {
                if let Some(p) = last_move {
                    let (x, y) = it.gfx.device_to_user(p)?;
                    elems.push(PathForallEl::Move(x, y));
                }
                just_closed = false;
            }
            PathElement::Close => just_closed = true,
            _ => {}
        }
        elems.push(match el {
            PathElement::Move(p) => {
                let (x, y) = it.gfx.device_to_user(p)?;
                PathForallEl::Move(x, y)
            }
            PathElement::Line(p) => {
                let (x, y) = it.gfx.device_to_user(p)?;
                PathForallEl::Line(x, y)
            }
            PathElement::Curve(c1, c2, p) => {
                let (x1, y1) = it.gfx.device_to_user(c1)?;
                let (x2, y2) = it.gfx.device_to_user(c2)?;
                let (x3, y3) = it.gfx.device_to_user(p)?;
                PathForallEl::Curve([x1, y1, x2, y2, x3, y3])
            }
            PathElement::Close => PathForallEl::Close,
        });
    }
    it.begin_pathforall(elems, procs)
}

fn flattenpath(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.flatten_path();
    Ok(())
}

fn fill(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.fill(FillRule::Winding);
    Ok(())
}

fn eofill(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.fill(FillRule::EvenOdd);
    Ok(())
}

fn stroke(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.stroke();
    Ok(())
}

/// `strokepath` -- replace the current path with the outline `stroke`
/// would paint. No error on an empty path (gs-pinned).
fn strokepath(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.stroke_path();
    Ok(())
}

fn erasepage(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.erase();
    Ok(())
}

/// showpage snapshots the finished page and resets the graphics state
/// (full initgraphics — pinned against gs: CTM, color, width all
/// reset). The erase is lazy: the image stays on the canvas until the
/// next painting op, so single-page programs keep their picture and
/// the window keeps showing it — resolving the Stage 2 deviation
/// without defeating the point of watching.
fn showpage(it: &mut Interp) -> Result<(), PsError> {
    // Buffered disk files go out with each page (issue #762); a failure is
    // kept on the file and reported by its own `closefile`.
    let _ = it.flush_files();
    it.gfx.showpage();
    it.gfx.init_graphics();
    it.gfx.dirty = true;
    Ok(())
}

/// Level 1 copypage: emit the page, keep canvas and graphics state.
fn copypage(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.copypage();
    Ok(())
}

/// Level 1 nulldevice (#739): install a device that discards all marking.
/// `gsave nulldevice ... grestore` is the idiom; the flag is graphics state.
fn nulldevice(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.nulldevice();
    Ok(())
}

fn initgraphics(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.init_graphics();
    Ok(())
}

fn gsave(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.gsave();
    Ok(())
}

/// Pop to the innermost save's boundary (the state stays available for
/// that save's restore), or to the bottom when no save is live.
fn grestoreall(it: &mut Interp) -> Result<(), PsError> {
    it.do_grestoreall();
    Ok(())
}

/// `- gstate gstate`: a new gstate object holding a copy of the current
/// graphics state (path, CTM, colour, font, clip and the rest). The
/// copy is cheap: the clip mask and its chain are shared `Rc`s, so the
/// cost is the current path's length, not the page's pixel count.
fn gstate(it: &mut Interp) -> Result<(), PsError> {
    let g = it.gfx.state().clone();
    it.push(Object::lit(Value::GState(std::rc::Rc::new(
        std::cell::RefCell::new(g),
    ))));
    Ok(())
}

/// `gstate currentgstate gstate`: overwrite the operand's contents with
/// the current graphics state and push it back (the same object, so
/// `eq` still holds). Journaled, so `restore` undoes it.
fn currentgstate(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    let Value::GState(g) = &obj.value else {
        it.push(obj);
        return Err(PsError::Typecheck);
    };
    let g = g.clone();
    it.journal_gstate(&g);
    *g.borrow_mut() = it.gfx.state().clone();
    it.push(obj);
    Ok(())
}

/// `gstate setgstate -`: replace the current graphics state with a copy
/// of the operand's. The gsave stack is untouched (PLRM), so a later
/// `grestore` still pops to whatever was saved before. The clip comes
/// back exactly: the same mask and chain `Rc`s that were in force when
/// the gstate was taken, nested and `eoclip` links included.
fn setgstate(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    let Value::GState(g) = &obj.value else {
        it.push(obj);
        return Err(PsError::Typecheck);
    };
    *it.gfx.state_mut() = g.borrow().clone();
    Ok(())
}

/// `gstate .sameclip bool` (pscat extension, #488; not a PLRM operator
/// and absent from Ghostscript): true when the clip in force is the very
/// clip the gstate holds -- both none, or the same clip chain. Identity,
/// not geometry: `gsave`/`grestore`, `setgstate` and `save`/`restore`
/// keep it; any `clip`, `eoclip`, `rectclip` or `initclip` after a clip
/// breaks it, even one with an identical path. `lib/drykit.ps` uses it
/// to see a `grestore` it has no shadow for (a bound wrapper's).
fn sameclip(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    let Value::GState(g) = &obj.value else {
        it.push(obj);
        return Err(PsError::Typecheck);
    };
    let same = it.gfx.state().same_clip(&g.borrow());
    it.push(Object::bool(same));
    Ok(())
}

/// Pop the newest `gsave`. At a `save` boundary the state the `save`
/// took comes back without a pop; at the bottom of the stack, the job's
/// initial state (Ghostscript's behaviour, #505).
fn grestore(it: &mut Interp) -> Result<(), PsError> {
    it.do_grestore();
    Ok(())
}

fn setgray(it: &mut Interp) -> Result<(), PsError> {
    let g = it.pop_f64()?;
    it.gfx.set_rgb(g, g, g);
    // The color operators select their color space implicitly, per the
    // PLRM — the image dict form reads it for its component count.
    it.gfx.set_colorspace(crate::gfx::ColorSpace::Gray);
    Ok(())
}

fn setrgbcolor(it: &mut Interp) -> Result<(), PsError> {
    let b = it.pop_f64()?;
    let g = it.pop_f64()?;
    let r = it.pop_f64()?;
    it.gfx.set_rgb(r, g, b);
    it.gfx.set_colorspace(crate::gfx::ColorSpace::Rgb);
    Ok(())
}

/// Flatness is a curve-approximation hint; we store it (clamped to
/// the PLRM's 0.2..100) and let tiny-skia flatten as it pleases.
fn setflat(it: &mut Interp) -> Result<(), PsError> {
    let f = it.pop_f64()?;
    it.gfx.state_mut().flatness = f.clamp(0.2, 100.0);
    Ok(())
}

fn currentflat(it: &mut Interp) -> Result<(), PsError> {
    let f = it.gfx.state().flatness;
    it.push(Object::real(f));
    Ok(())
}

fn setlinewidth(it: &mut Interp) -> Result<(), PsError> {
    let w = it.pop_f64()?;
    it.gfx.set_line_width(w);
    Ok(())
}

/// `num setalpha -` -- fill/stroke opacity, 0 (invisible) to 1
/// (opaque). **A pscat extension, not a PLRM operator**: real
/// Ghostscript exposes no PostScript-callable alpha operator at all
/// (tested in issue #46's spike, `docs/dev/design/WATERCOLOR.md`), so a program
/// that calls this will not render the same way under plain `gs
/// file.ps`. `lib/paintkit.ps`'s watercolor section probes for it with
/// `systemdict /setalpha known` and falls back to flattening each wash
/// against white when it's absent.
///
/// Clamped rather than range-checked, matching how the color operators
/// here already treat out-of-range components. Snapshotted by
/// `gsave`/`grestore` and reset by `initgraphics`, like every other
/// paint attribute. Reaches `fill`, `stroke`, shown text, and `shfill`;
/// **not** `image`/`imagemask`, which blit their own samples straight
/// into the pixmap (documented gap -- see `GraphicsState::alpha`).
fn setalpha(it: &mut Interp) -> Result<(), PsError> {
    let a = it.pop_f64()?;
    it.gfx.state_mut().alpha = a.clamp(0.0, 1.0) as f32;
    Ok(())
}

fn currentalpha(it: &mut Interp) -> Result<(), PsError> {
    let a = it.gfx.state().alpha;
    it.push(Object::real(a as f64));
    Ok(())
}

/// `name setblendmode -` -- `/Normal` (source-over, the default) or
/// `/Multiply`. Another pscat extension (issue #47), with the same
/// Ghostscript caveat as `setalpha`.
///
/// An unrecognized name is a `rangecheck`, deliberately: silently
/// falling back to /Normal would let a typo diverge from what the
/// program asked for with no signal at all, the same reasoning
/// `lib/paintkit.ps`'s `pkribbon` uses to reject an unknown cap style
/// rather than defaulting it.
fn setblendmode(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    let Value::Name(n) = &obj.value else {
        return Err(PsError::Typecheck);
    };
    let mode = crate::gfx::BlendMode::from_ps_name(n).ok_or(PsError::Rangecheck)?;
    it.gfx.state_mut().blend = mode;
    Ok(())
}

fn currentblendmode(it: &mut Interp) -> Result<(), PsError> {
    let name = it.gfx.state().blend.as_ps_name();
    it.push(Object::name(name));
    Ok(())
}

fn setlinecap(it: &mut Interp) -> Result<(), PsError> {
    let n = it.pop_int()?;
    it.gfx.set_line_cap(n)
}

fn setlinejoin(it: &mut Interp) -> Result<(), PsError> {
    let n = it.pop_int()?;
    it.gfx.set_line_join(n)
}

fn setmiterlimit(it: &mut Interp) -> Result<(), PsError> {
    let limit = it.pop_f64()?;
    it.gfx.set_miter_limit(limit)
}

fn currentlinecap(it: &mut Interp) -> Result<(), PsError> {
    it.push(Object::int(it.gfx.line_cap()));
    Ok(())
}

fn currentlinejoin(it: &mut Interp) -> Result<(), PsError> {
    it.push(Object::int(it.gfx.line_join()));
    Ok(())
}

/// The miter limit is held as `f32`, so it reads back as the shortest
/// decimal of that `f32` (`3.3 setmiterlimit currentmiterlimit` is `3.3`).
fn currentmiterlimit(it: &mut Interp) -> Result<(), PsError> {
    it.push(Object::real(f32_readback(it.gfx.miter_limit())));
    Ok(())
}

/// `bool setstrokeadjust -` (#513). The flag is stored and read back, and
/// saved by `gsave`; it does not change how pscat strokes.
fn setstrokeadjust(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    let Value::Boolean(b) = obj.value else {
        return Err(PsError::Typecheck);
    };
    it.gfx.state_mut().stroke_adjust = b;
    Ok(())
}

fn currentstrokeadjust(it: &mut Interp) -> Result<(), PsError> {
    let b = it.gfx.state().stroke_adjust;
    it.push(Object::bool(b));
    Ok(())
}

fn setdash(it: &mut Interp) -> Result<(), PsError> {
    let phase = it.pop_f64()?;
    let obj = it.pop()?;
    let Value::Array(a) = &obj.value else {
        return Err(PsError::Typecheck);
    };
    let mut pattern = Vec::with_capacity(a.len());
    for i in 0..a.len() {
        match a.get(i).as_ref().map(|o| &o.value) {
            Some(Value::Integer(n)) => pattern.push(*n as f64),
            Some(Value::Real(r)) => pattern.push(*r),
            _ => return Err(PsError::Typecheck),
        }
    }
    it.gfx.set_dash(pattern, phase)
}

fn currentdash(it: &mut Interp) -> Result<(), PsError> {
    let (pattern, phase) = it.gfx.current_dash();
    it.push(Object::array(
        pattern.into_iter().map(Object::real).collect(),
    ));
    it.push(Object::real(phase));
    Ok(())
}

fn sethsbcolor(it: &mut Interp) -> Result<(), PsError> {
    let b = it.pop_f64()?;
    let s = it.pop_f64()?;
    let h = it.pop_f64()?;
    it.gfx.set_hsb(h, s, b);
    Ok(())
}

fn currenthsbcolor(it: &mut Interp) -> Result<(), PsError> {
    let (h, s, b) = it.gfx.hsb();
    it.push(Object::real(f32_readback(h)));
    it.push(Object::real(f32_readback(s)));
    it.push(Object::real(f32_readback(b)));
    Ok(())
}

fn setcmykcolor(it: &mut Interp) -> Result<(), PsError> {
    let k = it.pop_f64()?;
    let y = it.pop_f64()?;
    let m = it.pop_f64()?;
    let c = it.pop_f64()?;
    it.gfx.set_cmyk(c, m, y, k);
    it.gfx.set_colorspace(crate::gfx::ColorSpace::Cmyk);
    Ok(())
}

fn currentcmykcolor(it: &mut Interp) -> Result<(), PsError> {
    for v in it.gfx.cmyk() {
        it.push(Object::real(f32_readback(v)));
    }
    Ok(())
}

fn currentrgbcolor(it: &mut Interp) -> Result<(), PsError> {
    // Rounded like `currentgray` (#515), so `0.3 0.6 0.9 setrgbcolor
    // currentrgbcolor` reads back `0.3 0.6 0.9` as gs prints it, not the
    // widened f32s.
    let (r, g, b) = it.gfx.rgb();
    it.push(Object::real(f32_readback(r)));
    it.push(Object::real(f32_readback(g)));
    it.push(Object::real(f32_readback(b)));
    Ok(())
}

fn currentgray(it: &mut Interp) -> Result<(), PsError> {
    let g = it.gfx.gray();
    it.push(Object::real(f32_readback(g)));
    Ok(())
}

/// A colour component as `currentgray`, `currentrgbcolor` (#515) and
/// `currenthsbcolor` report it (#505), and the other readbacks of values held in `f32` (the CMYK
/// components, `currentmiterlimit`: #513).
/// The colour is held in `f32`, and Ghostscript's reals are `f32` too, so
/// the value is rounded to `f32` and then read back as the `f64` nearest
/// that `f32`'s shortest decimal: `0.3 setgray currentgray` gives `0.3`
/// (not the widened `0.30000001192092896`) and the gray of a 50% gray RGB
/// gives `0.5` (not the `0.49999999999999994` that `0.3r + 0.59g + 0.11b`
/// sums to in `f64`), as gs prints. Setting the value again lands on the
/// same `f32`, so a read-then-set round trip still changes nothing.
pub(super) fn f32_readback(x: f64) -> f64 {
    (x as f32).to_string().parse().unwrap_or(x)
}

fn currentlinewidth(it: &mut Interp) -> Result<(), PsError> {
    let w = it.gfx.line_width();
    it.push(Object::real(w));
    Ok(())
}

/// The rect operand forms shared by rectfill/rectstroke/rectclip:
/// `x y width height`, or a flat array of 4n numbers (each quad one
/// rectangle; empty allowed, other lengths typecheck — gs-pinned).
/// The PLRM's encoded-number-string form is not supported; a string
/// typechecks, the same answer gs gives a plain string.
fn pop_rects(it: &mut Interp) -> Result<Vec<[f64; 4]>, PsError> {
    let top = it.pop()?;
    match &top.value {
        Value::Array(a) => {
            if a.len() % 4 != 0 {
                return Err(PsError::Typecheck);
            }
            let mut rects = Vec::with_capacity(a.len() / 4);
            for q in 0..a.len() / 4 {
                let mut r = [0.0; 4];
                for (k, slot) in r.iter_mut().enumerate() {
                    *slot = match a.get(q * 4 + k).as_ref().map(|o| &o.value) {
                        Some(Value::Integer(n)) => *n as f64,
                        Some(Value::Real(x)) => *x,
                        _ => return Err(PsError::Typecheck),
                    };
                }
                rects.push(r);
            }
            Ok(rects)
        }
        Value::Integer(n) => {
            let h = *n as f64;
            pop_rect_under_height(it, h)
        }
        Value::Real(h) => {
            let h = *h;
            pop_rect_under_height(it, h)
        }
        _ => Err(PsError::Typecheck),
    }
}

fn pop_rect_under_height(it: &mut Interp, h: f64) -> Result<Vec<[f64; 4]>, PsError> {
    let w = it.pop_f64()?;
    let (x, y) = pop_xy(it)?;
    Ok(vec![[x, y, w, h]])
}

fn append_rects(it: &mut Interp, rects: &[[f64; 4]]) -> Result<(), PsError> {
    for &[x, y, w, h] in rects {
        it.gfx.moveto(x, y);
        it.gfx.lineto(x + w, y)?;
        it.gfx.lineto(x + w, y + h)?;
        it.gfx.lineto(x, y + h)?;
        it.gfx.closepath();
    }
    Ok(())
}

/// `x y w h rectfill` / `numarray rectfill` — paint inside an implicit
/// gsave: current path and graphics state untouched (gs-pinned).
fn rectfill(it: &mut Interp) -> Result<(), PsError> {
    let rects = pop_rects(it)?;
    it.gfx.gsave();
    it.gfx.newpath();
    let built = append_rects(it, &rects);
    if built.is_ok() {
        it.gfx.fill(FillRule::Winding);
    }
    it.gfx.grestore();
    built
}

/// `x y w h rectstroke` / `numarray rectstroke`, optionally with a
/// matrix on top. gs pin: a 6-element array in top position is always
/// the matrix, never a rect list; any other array is rects.
fn rectstroke(it: &mut Interp) -> Result<(), PsError> {
    let top = it.pop()?;
    let matrix = if matches!(&top.value, Value::Array(a) if a.len() == 6) {
        Some(super::matrix::read_matrix(&top)?)
    } else {
        it.push(top);
        None
    };
    let rects = pop_rects(it)?;
    it.gfx.gsave();
    it.gfx.newpath();
    let built = append_rects(it, &rects);
    if built.is_ok() {
        // The matrix concats *after* the path is built: it shapes the
        // pen, not the rectangles (the PLRM's gsave…concat…stroke form).
        if let Some(m) = matrix {
            it.gfx.concat_ctm(m);
        }
        it.gfx.stroke();
    }
    it.gfx.grestore();
    built
}

/// `x y w h rectclip` / `numarray rectclip` — intersect the clip and
/// leave the current path empty (gs-pinned; an empty rect list clips
/// everything away, also pinned).
fn rectclip(it: &mut Interp) -> Result<(), PsError> {
    let rects = pop_rects(it)?;
    it.gfx.newpath();
    append_rects(it, &rects)?;
    it.gfx.clip(FillRule::Winding)?;
    it.gfx.newpath();
    Ok(())
}

fn clip(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.clip(FillRule::Winding)
}

fn eoclip(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.clip(FillRule::EvenOdd)
}

fn clippath(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.set_path_to_clip();
    Ok(())
}

fn initclip(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.initclip();
    Ok(())
}

/// `- clipsave -` (Level 3, #494): save the clip in force on this `gsave`
/// level's clip stack.
fn clipsave(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.clipsave();
    Ok(())
}

/// `- cliprestore -` (Level 3, #494): put back the clip the newest
/// `clipsave` at this `gsave` level saved; with none, the clip the level
/// started with (no clip at the top level). Semantics pinned against
/// Ghostscript 10.07.1 (`notes/494-clipsave-cliprestore.md`).
fn cliprestore(it: &mut Interp) -> Result<(), PsError> {
    it.do_cliprestore();
    Ok(())
}

fn pathbbox(it: &mut Interp) -> Result<(), PsError> {
    let (lx, ly, ux, uy) = it.gfx.path_bbox()?;
    it.push(Object::real(lx));
    it.push(Object::real(ly));
    it.push(Object::real(ux));
    it.push(Object::real(uy));
    Ok(())
}

fn translate(it: &mut Interp) -> Result<(), PsError> {
    let (tx, ty) = pop_xy(it)?;
    it.gfx
        .concat_ctm(Transform::from_translate(tx as f32, ty as f32));
    Ok(())
}

fn scale(it: &mut Interp) -> Result<(), PsError> {
    let (sx, sy) = pop_xy(it)?;
    it.gfx
        .concat_ctm(Transform::from_scale(sx as f32, sy as f32));
    Ok(())
}

fn rotate(it: &mut Interp) -> Result<(), PsError> {
    let angle = it.pop_f64()?;
    it.gfx.concat_ctm(crate::fmath::rotation(angle as f32));
    Ok(())
}
