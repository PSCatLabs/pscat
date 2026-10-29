//! The page device, as far as `/PageSize` (#909): `setpagedevice` and
//! `currentpagedevice`.
//!
//! What is modelled, pinned against Ghostscript 10.07.1:
//!
//! - `/PageSize [w h]` sizes the page in points, unless the host fixed the
//!   size ([`PageSizeSource::Explicit`]: `--page`, an MCP tool's
//!   width/height, any `Interp::with_page` caller). Then the request is
//!   ignored with a stderr note, once. The page's origin returns to
//!   (0, 0), as a page device has no bounding-box offset.
//! - Every call that succeeds, even `<< >> setpagedevice` or one whose
//!   `/PageSize` an explicit page ignores, erases the page and does
//!   `initgraphics`, as gs does. Marks not yet shown are
//!   discarded, not emitted (`EndPage` reason 2 returns false), and
//!   `nulldevice` is left.
//! - The device belongs to the graphics state: `grestore`, `restore` and
//!   `setgstate` bring the earlier one back, erased (`Gfx::sync_device`).
//! - A `/PageSize` that is not an array of two positive finite numbers, or
//!   whose page would be over 8000 pixels a side or over `--max-pixels`,
//!   is a `configurationerror`, as gs raises for an unusable `/PageSize`,
//!   and changes nothing. gs takes a three-element array and reads two;
//!   pscat refuses it.
//! - Any other key is ignored, as gs ignores keys it does not know (and the
//!   PLRM's default `/PolicyNotFound` 1 does), but not silently: one
//!   `pscat: warning:` line per key, so a program that relies on
//!   `/Duplex` or `/EndPage` hears that pscat did not do it.
//! - `currentpagedevice` returns a new dictionary holding only
//!   `/PageSize`: the page in points (as asked for, when `setpagedevice` set
//!   it). It does not pretend to hold the other parameters.
//!
//! [`PageSizeSource::Explicit`]: crate::gfx::PageSizeSource::Explicit

use std::rc::Rc;

use crate::error::PsError;
use crate::gfx::{PageDevice, PageSizeSource};
use crate::interp::Interp;
use crate::object::{Dict, Object, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "setpagedevice", setpagedevice);
    op(dict, "currentpagedevice", currentpagedevice);
}

/// The most device pixels a page may have on a side, as for `--page`.
const MAX_SIDE: u32 = 8000;

fn setpagedevice(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    let Value::Dict(d) = &obj.value else {
        it.push(obj);
        return Err(PsError::Typecheck);
    };
    let mut request = None;
    let mut ignored = Vec::new();
    for (key, value) in d.borrow().pairs() {
        match &key.value {
            Value::Name(n) if &**n == "PageSize" => request = Some(value),
            Value::Name(n) => ignored.push(format!("/{}", &**n)),
            _ => ignored.push(key.repr()),
        }
    }
    let size = match request {
        Some(value) => match page_size(&value) {
            Some(size) => Some(size),
            None => {
                it.push(obj);
                return Err(PsError::ConfigurationError);
            }
        },
        None => None,
    };
    let current = it.gfx.page_device();
    let device = match size {
        Some(size) if current.source != PageSizeSource::Explicit => match page_pixels(it, size) {
            Ok(px) => PageDevice {
                size,
                px,
                origin: (0.0, 0.0),
                source: PageSizeSource::PageDevice,
            },
            Err(e) => {
                it.push(obj);
                return Err(e);
            }
        },
        Some(size) => {
            if size != current.size {
                note_once(
                    it,
                    "explicit",
                    &format!(
                        "pscat: warning: setpagedevice: the page is fixed at {}x{} by the caller \
                         (--page, or a tool's width and height); /PageSize [{} {}] ignored",
                        fmt(current.size.0),
                        fmt(current.size.1),
                        fmt(size.0),
                        fmt(size.1)
                    ),
                );
            }
            (*current).clone()
        }
        None => (*current).clone(),
    };
    ignored.sort();
    for key in ignored {
        let line = format!(
            "pscat: warning: setpagedevice: {key} is not supported and was ignored \
             (only /PageSize is)"
        );
        note_once(it, &format!("key {key}"), &line);
    }
    // A new canvas can be a quarter of a gigabyte: refused before it is
    // made when it would pass `--max-memory`, as `array` and `string` are.
    if device.px != current.px {
        let bytes = device.px.0 as usize * device.px.1 as usize * 4;
        if let Err(e) = it.check_alloc(bytes) {
            it.push(obj);
            return Err(e);
        }
    }
    if let Err(e) = it.gfx.install_device(Rc::new(device)) {
        it.push(obj);
        return Err(e);
    }
    it.gfx.leave_null_device();
    it.gfx.init_graphics();
    it.after_device_change();
    Ok(())
}

/// `- currentpagedevice dict`: a new dictionary with `/PageSize`.
fn currentpagedevice(it: &mut Interp) -> Result<(), PsError> {
    let (w, h) = it.gfx.page_size();
    let mut dict = Dict::new();
    dict.put("PageSize".into(), Object::array(vec![number(w), number(h)]));
    it.push(Object::lit(Value::Dict(Rc::new(std::cell::RefCell::new(
        dict,
    )))));
    Ok(())
}

/// A `/PageSize` value: an array of exactly two positive finite numbers.
fn page_size(value: &Object) -> Option<(f64, f64)> {
    let Value::Array(a) = &value.value else {
        return None;
    };
    if a.len() != 2 {
        return None;
    }
    let side = |i: usize| {
        let v = a.get(i)?.num()?.to_f64();
        (v.is_finite() && v > 0.0).then_some(v)
    };
    Some((side(0)?, side(1)?))
}

/// The canvas a page of `size` points needs at this run's dpi, rounded as
/// `Interp::with_page_scaled` rounds (and the MCP server's pixel check), or
/// `configurationerror` when it is over the page limits.
fn page_pixels(it: &Interp, size: (f64, f64)) -> Result<(u32, u32), PsError> {
    let scale = it.gfx.scale();
    let px = |pt: f64| ((pt as f32 * scale).round() as f64).max(1.0);
    let (w, h) = (px(size.0), px(size.1));
    let too_big = w > f64::from(MAX_SIDE) || h > f64::from(MAX_SIDE);
    let over_cap = it.max_page_pixels.is_some_and(|max| w * h > max as f64);
    if too_big || over_cap {
        if let Some(max) = it.max_page_pixels {
            // The line the MCP server reads back to answer
            // `output_too_large` (`limits::parse_pixel_cap_message`).
            // `as` saturates, and the message multiplies saturating, so a
            // `/PageSize [1e10 1e10]` reports rather than overflows.
            let msg = crate::limits::pixel_cap_message(w as u64, h as u64, max);
            let _ = it.write_stderr(format!("pscat: setpagedevice: {msg}\n").as_bytes(), true);
        } else {
            let _ = it.write_stderr(
                format!(
                    "pscat: setpagedevice: a {w}x{h}-pixel page is over {MAX_SIDE} pixels a side\n"
                )
                .as_bytes(),
                true,
            );
        }
        return Err(PsError::ConfigurationError);
    }
    Ok((w as u32, h as u32))
}

/// Write `line` to stderr the first time `key` is noted on this interpreter.
fn note_once(it: &mut Interp, key: &str, line: &str) {
    if it.page_device_notes.insert(key.to_string()) {
        let _ = it.write_stderr(format!("{line}\n").as_bytes(), true);
    }
}

/// A whole number as an integer, as `currentpagedevice` hands back a size
/// set in whole points; anything else as a real.
fn number(v: f64) -> Object {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        Object::int(v as i64)
    } else {
        Object::real(v)
    }
}

fn fmt(v: f64) -> String {
    number(v).repr()
}
