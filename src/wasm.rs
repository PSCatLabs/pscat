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
//! - `ps_set_max_steps(steps)` — cap the steps (objects run, loop passes,
//!   and a step per KiB a decode filter produces) the program may run
//!   from now on, as `pscat --max-steps` does; 0 = no cap, the default.
//!   It applies to the program in progress, counting from this call, and
//!   to every program begun after it; 0 = ok, -1 = not a whole number
//!   >= 0. A wasm module has no clock, so a step cap is the only thing
//!   that stops a program inside one `ps_step`, such as one operator
//!   draining a decompression bomb (#1208, #1210). Set once before
//!   `ps_begin`, it caps the whole program; set again before each
//!   `ps_step`, it caps that call's work, which is what bounds a frame in
//!   a browser. Over the cap the error is `limitexceeded (steps)`. `steps`
//!   is an f64 so JS passes a Number;
//! - `ps_steps_used()` — steps run since the cap was last set or the
//!   program began, counted only while a cap is set (0 otherwise). It
//!   trails the true figure by up to one slice. An f64;
//! - `ps_output_ptr`/`ps_output_len` — what the program has printed so
//!   far (`=`, `==`, `stack`, `pstack`, `print`), UTF-8 as written; a
//!   wasm32 module has no stdout, so `ps_begin` captures it (#252);
//! - `ps_warnings_ptr`/`ps_warnings_len` — the one-line notes `findfont`
//!   raised since `ps_begin` for pack faces it replaced with a built-in
//!   one (what the native binary prints on stderr), UTF-8, newline-
//!   joined, once per name (#750);
//! - `ps_begin_vector(src, len, w, h, scale, flags)` — `ps_begin_scaled`
//!   that also records the page(s) as SVG (`flags` bit 0) and/or PDF
//!   (bit 1), with the same recorders as `pscat --svg`/`--pdf`, so the
//!   bytes equal the CLI's for the same program, page and dpi (#1003).
//!   With PDF on, the source's `%%Title:`/`%%For:` become the PDF's
//!   `/Info`, as the CLI does for a file. Bit 2, with bit 0, writes
//!   standard-font text as SVG `<text>` (`pscat --svg-text`, #1119).
//!   -1 also for an unknown flag bit, or bit 2 without bit 0;
//! - `ps_svg_count()` — SVG pages so far (0 when not recording);
//! - `ps_page_count()` — pages the document holds now, whatever is
//!   recorded: those `showpage`/`copypage` emitted plus the live page when
//!   it holds art or is the only one; what `ps_svg_count` returns with
//!   SVG on, and the PDF's page count. 0 when no program has begun (#1011);
//! - `ps_svg(page)` / `ps_pdf()` — finish SVG page `page` (from 0) / the
//!   PDF document into the vector buffer: 0 = ok, -1 = not recorded (or
//!   page out of range), -2 = no program begun. Callable mid-run: the
//!   document so far, like `ps_pixels`;
//! - `ps_vector_ptr`/`ps_vector_len` — that buffer; valid until the next
//!   `ps_svg`, `ps_pdf` or begin, so copy it out first.
//!
//! The vector exports started as PSCatLabs/print-shop's adapter patch
//! (`patches/pscat-vector-wasm.patch` there), which recorded both formats
//! on every run and returned the SVG pages as JSON; here recording is
//! opt-in, so a pixels-only page pays nothing, and pages come back one at
//! a time as raw bytes.

use std::cell::{Cell, RefCell};

use crate::Interp;

thread_local! {
    static INTERP: RefCell<Option<Interp>> = const { RefCell::new(None) };
    static ERROR: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    /// The last `ps_svg`/`ps_pdf` result.
    static VECTOR: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    /// The finished SVG pages, kept between `ps_svg` calls: finishing is
    /// per document, so reading N pages one by one would otherwise finish
    /// all of them N times. Cleared whenever the program can draw again.
    static SVG_PAGES: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
    /// The step cap `ps_set_max_steps` set, for every program begun after
    /// it (the module instance outlives each `ps_begin`). 0 = none.
    static MAX_STEPS: Cell<u64> = const { Cell::new(0) };
}

/// `ps_begin_vector` flag bits.
const RECORD_SVG: u32 = 1;
const RECORD_PDF: u32 = 2;
const SVG_TEXT: u32 = 4;

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

/// # Safety
/// Same contract as [`ps_begin`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ps_begin_vector(
    src: *const u8,
    len: usize,
    w: u32,
    h: u32,
    scale: f32,
    flags: u32,
) -> i32 {
    if flags & !(RECORD_SVG | RECORD_PDF | SVG_TEXT) != 0
        || (flags & SVG_TEXT != 0 && flags & RECORD_SVG == 0)
        || !(scale.is_finite() && scale > 0.0)
    {
        return -1;
    }
    let source = unsafe { std::slice::from_raw_parts(src, len) }.to_vec();
    let Some(mut it) = Interp::with_page_scaled(w, h, scale) else {
        return -1;
    };
    // The CLI's order (src/main.rs): recorders on before the program runs,
    // then the DSC title/author for the PDF's /Info.
    if flags & RECORD_SVG != 0 {
        if flags & SVG_TEXT != 0 {
            it.set_svg_text(crate::SvgText::Text);
        }
        it.enable_svg();
    }
    if flags & RECORD_PDF != 0 {
        it.enable_pdf();
        let (title, author) = crate::pdf::scan_document_info(&source);
        it.gfx_mut().set_pdf_info(title, author);
    }
    install(it, &source);
    0
}

fn install(mut it: Interp, source: &[u8]) {
    it.capture_stdout();
    // Before begin_source, which restarts the step count under a cap.
    let max_steps = MAX_STEPS.with(Cell::get);
    if max_steps > 0 {
        arm(&mut it, max_steps);
    }
    it.begin_source(source);
    INTERP.with(|s| *s.borrow_mut() = Some(it));
    ERROR.with(|e| e.borrow_mut().clear());
    VECTOR.with(|v| *v.borrow_mut() = Vec::new());
    SVG_PAGES.with(|p| *p.borrow_mut() = None);
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

/// Cap the steps from now on, for the program in progress and every one
/// begun after; 0 lifts the cap. Counting restarts here, so calling it
/// before each `ps_step` caps that call's work.
#[unsafe(no_mangle)]
pub extern "C" fn ps_set_max_steps(steps: f64) -> i32 {
    if !(steps.is_finite() && steps >= 0.0 && steps.fract() == 0.0) {
        return -1;
    }
    // `as` saturates, so a cap past 2^64 is simply a very large one.
    let max = steps as u64;
    MAX_STEPS.with(|m| m.set(max));
    INTERP.with(|s| {
        if let Some(it) = s.borrow_mut().as_mut() {
            arm(it, max);
        }
    });
    0
}

/// Give `it` a step cap of `max` (0 = none), counting from now.
fn arm(it: &mut Interp, max: u64) {
    let limits = if max > 0 {
        crate::Limits::none().with_max_steps(max)
    } else {
        crate::Limits::none()
    };
    it.set_limits(limits)
        .expect("a step cap needs no allocator");
}

/// Steps the current program has run (0 unless a cap is set).
#[unsafe(no_mangle)]
pub extern "C" fn ps_steps_used() -> f64 {
    INTERP.with(|s| s.borrow().as_ref().map_or(0, Interp::steps_used)) as f64
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_step(budget: i32) -> i32 {
    INTERP.with(|s| {
        let mut slot = s.borrow_mut();
        let Some(it) = slot.as_mut() else {
            return -2;
        };
        SVG_PAGES.with(|p| *p.borrow_mut() = None);
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

/// Run `f` on the finished SVG pages (finishing them once per change);
/// `None` when no program has begun, `Some(None)` when SVG is not recorded.
fn with_svg_pages<R>(f: impl FnOnce(Option<&[String]>) -> R) -> Option<R> {
    INTERP.with(|s| {
        let slot = s.borrow();
        let it = slot.as_ref()?;
        Some(SVG_PAGES.with(|p| {
            let mut cache = p.borrow_mut();
            if cache.is_none() {
                *cache = it.svg_pages();
            }
            f(cache.as_deref())
        }))
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_svg_count() -> u32 {
    with_svg_pages(|pages| pages.map_or(0, |p| p.len() as u32)).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_page_count() -> u32 {
    // The rule svg_pages and pdf_document use: a page the program is still
    // drawing on counts when it holds art, and an empty document has one.
    INTERP.with(|s| {
        s.borrow().as_ref().map_or(0, |it| {
            let gfx = it.gfx();
            let shown = gfx.pages().len();
            (shown + usize::from(shown == 0 || gfx.has_trailing_art())) as u32
        })
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_svg(page: u32) -> i32 {
    let found = with_svg_pages(|pages| {
        pages
            .and_then(|p| p.get(page as usize))
            .map(|svg| svg.as_bytes().to_vec())
    });
    match found {
        None => -2,
        Some(None) => -1,
        Some(Some(bytes)) => {
            VECTOR.with(|v| *v.borrow_mut() = bytes);
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_pdf() -> i32 {
    let doc = INTERP.with(|s| s.borrow().as_ref().map(Interp::pdf_document));
    match doc {
        None => -2,
        Some(None) => -1,
        Some(Some(bytes)) => {
            VECTOR.with(|v| *v.borrow_mut() = bytes);
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_vector_ptr() -> *const u8 {
    VECTOR.with(|v| v.borrow().as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_vector_len() -> usize {
    VECTOR.with(|v| v.borrow().len())
}
