//! The live window's stand-in for a build without the `window` feature
//! (issue #565): same names and signatures as `window.rs`, so `main.rs`
//! compiles unchanged, but every entry point returns an error telling the
//! user how to get a window or render headless instead. A library user
//! with `default-features = false` gets no winit or softbuffer at all.

use crate::Interp;
use crate::spool::Watcher;

/// What the windowed modes would have been run with. Kept identical to
/// `window::WindowOptions` so callers need no `cfg`.
pub struct WindowOptions {
    /// The window title.
    pub title: String,
    /// Interpreter steps per frame.
    pub steps_per_frame: usize,
    /// Screen the canvas like a mono laser printer (`--halftone`).
    pub halftone: bool,
}

const NO_WINDOW: &str = "this pscat was built without the `window` feature, so it has no live \
     window; render headless with --png/--svg/--pdf (or --headless), or reinstall with the \
     default features (`cargo install pscat`)";

/// Always `Err`: there is no window in this build.
pub fn run_windowed(_interp: Interp, _options: WindowOptions) -> Result<(), String> {
    Err(NO_WINDOW.to_string())
}

/// Always `Err`: `--interactive` needs the window.
pub fn run_interactive(
    _interp: Interp,
    _options: WindowOptions,
    _prelude: Option<Vec<u8>>,
) -> Result<(), String> {
    Err(NO_WINDOW.to_string())
}

/// Always `Err`: `--spool` shows its jobs in the window.
pub fn run_spool(
    _watcher: Watcher,
    _page: (u32, u32),
    _scale: f32,
    _options: WindowOptions,
) -> Result<(), String> {
    Err(NO_WINDOW.to_string())
}
