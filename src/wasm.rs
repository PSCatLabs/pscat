//! The wasm32 export surface (Stage 16) — pscat in a browser tab.
//!
//! A hand-rolled C ABI over byte buffers, no wasm-bindgen: the caller
//! (web/pscat.js) copies the program into linear memory, drives the
//! interpreter with the same `begin_source`/`step_n` budget the winit
//! window uses, and blits the live RGBA pixmap into a `<canvas>` via
//! putImageData. One interpreter per module instance (wasm is
//! single-threaded; the state is a thread_local).
//!
//! Contract:
//! - `ps_alloc`/`ps_dealloc` — scratch buffers for passing sources in;
//! - `ps_begin(src, len, w, h)` — fresh page, program queued; 0 = ok,
//!   -1 = unusable page size;
//! - `ps_step(n)` — run up to n objects: 1 = more work, 0 = done,
//!   -1 = a PostScript error (report via `ps_error_*`; the machine is
//!   unwound but page and operand stack survive, REPL-style),
//!   -2 = no program begun;
//! - `ps_pixels`/`ps_pixels_len`/`ps_width`/`ps_height` — the live
//!   canvas, premultiplied RGBA (opaque page, so plain RGBA);
//! - `ps_begin_scaled(src, len, w, h, scale)` — `ps_begin` at `scale`
//!   device pixels per point (dpi / 72); -1 also for a scale that is
//!   not finite and positive;
//! - `ps_run(src, len, w, h)` — begin + drive to completion;
//! - `ps_add_font(name, name_len, data, data_len)` — register a font
//!   file's bytes under `name` (a font pack's file stem) for `findfont`;
//!   0 = ok, -1 = name not UTF-8, -2 = not a font (report via
//!   `ps_error_*`). Fonts are per module instance and outlive `ps_begin`;
//! - `ps_output_ptr`/`ps_output_len` — what the program has printed so
//!   far (`=`, `==`, `stack`, `pstack`, `print`), UTF-8 as written; a
//!   wasm32 module has no stdout, so `ps_begin` captures it (#252);
//! - `ps_warnings_ptr`/`ps_warnings_len` — the one-line notes `findfont`
//!   raised since `ps_begin` for pack faces it replaced with a built-in
//!   one (what the native binary prints on stderr), UTF-8, newline-
//!   joined, once per name (#750).

use std::cell::RefCell;

use crate::Interp;

thread_local! {
    static INTERP: RefCell<Option<Interp>> = const { RefCell::new(None) };
    static ERROR: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len.max(1));
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// # Safety
/// `ptr` must come from `ps_alloc(len)` and not be freed twice.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ps_dealloc(ptr: *mut u8, len: usize) {
    unsafe {
        drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
    }
}

/// # Safety
/// `src..src+len` must be readable (a `ps_alloc` buffer the caller
/// filled).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ps_begin(src: *const u8, len: usize, w: u32, h: u32) -> i32 {
    let source = unsafe { std::slice::from_raw_parts(src, len) }.to_vec();
    let Some(it) = Interp::with_page(w, h) else {
        return -1;
    };
    install(it, &source);
    0
}

/// # Safety
/// Same contract as [`ps_begin`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ps_begin_scaled(
    src: *const u8,
    len: usize,
    w: u32,
    h: u32,
    scale: f32,
) -> i32 {
    if !(scale.is_finite() && scale > 0.0) {
        return -1;
    }
    let source = unsafe { std::slice::from_raw_parts(src, len) }.to_vec();
    let Some(it) = Interp::with_page_scaled(w, h, scale) else {
        return -1;
    };
    install(it, &source);
    0
}

fn install(mut it: Interp, source: &[u8]) {
    it.capture_stdout();
    it.begin_source(source);
    INTERP.with(|s| *s.borrow_mut() = Some(it));
    ERROR.with(|e| e.borrow_mut().clear());
}

/// # Safety
/// `name..name+name_len` and `data..data+data_len` must be readable
/// (`ps_alloc` buffers the caller filled).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ps_add_font(
    name: *const u8,
    name_len: usize,
    data: *const u8,
    data_len: usize,
) -> i32 {
    let name = unsafe { std::slice::from_raw_parts(name, name_len) };
    let Ok(name) = std::str::from_utf8(name) else {
        return -1;
    };
    let data = unsafe { std::slice::from_raw_parts(data, data_len) }.to_vec();
    match crate::font::register_font_bytes(name, data) {
        Ok(_) => 0,
        Err(msg) => {
            ERROR.with(|e| *e.borrow_mut() = msg.into_bytes());
            -2
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_step(budget: i32) -> i32 {
    INTERP.with(|s| {
        let mut slot = s.borrow_mut();
        let Some(it) = slot.as_mut() else {
            return -2;
        };
        match it.step_n(budget.max(0) as usize) {
            Ok(true) => 1,
            Ok(false) => 0,
            Err(e) => {
                let report = it.error_report(&e);
                ERROR.with(|er| *er.borrow_mut() = report.into_bytes());
                -1
            }
        }
    })
}

/// # Safety
/// Same contract as [`ps_begin`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ps_run(src: *const u8, len: usize, w: u32, h: u32) -> i32 {
    let rc = unsafe { ps_begin(src, len, w, h) };
    if rc != 0 {
        return rc;
    }
    loop {
        match ps_step(1 << 16) {
            1 => {}
            status => return status,
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_pixels() -> *const u8 {
    INTERP.with(|s| {
        s.borrow()
            .as_ref()
            .map_or(std::ptr::null(), |it| it.gfx().pixmap.data().as_ptr())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_pixels_len() -> usize {
    INTERP.with(|s| {
        s.borrow()
            .as_ref()
            .map_or(0, |it| it.gfx().pixmap.data().len())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_width() -> u32 {
    INTERP.with(|s| s.borrow().as_ref().map_or(0, |it| it.gfx().pixmap.width()))
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_height() -> u32 {
    INTERP.with(|s| s.borrow().as_ref().map_or(0, |it| it.gfx().pixmap.height()))
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_error_ptr() -> *const u8 {
    ERROR.with(|e| e.borrow().as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_error_len() -> usize {
    ERROR.with(|e| e.borrow().len())
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_output_ptr() -> *const u8 {
    INTERP.with(|s| {
        s.borrow()
            .as_ref()
            .map_or(std::ptr::null(), |it| it.captured_stdout().as_ptr())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_warnings_ptr() -> *const u8 {
    INTERP.with(|s| {
        s.borrow()
            .as_ref()
            .map_or(std::ptr::null(), |it| it.font_warnings().as_ptr())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_warnings_len() -> usize {
    INTERP.with(|s| s.borrow().as_ref().map_or(0, |it| it.font_warnings().len()))
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_output_len() -> usize {
    INTERP.with(|s| {
        s.borrow()
            .as_ref()
            .map_or(0, |it| it.captured_stdout().len())
    })
}
