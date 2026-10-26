//! pscat: a PostScript interpreter that renders to PNG, SVG and PDF.
//!
//! This is the library behind the `pscat` command-line tool. It runs
//! PostScript programs (Level 1 plus the commonly used Level 2/3 operators)
//! on a raster page and hands back the rendered pages. The PostScript
//! libraries under `lib/` (`lib/artkit.ps`, `lib/graph.ps`, ...) are compiled
//! in, so `(lib/artkit.ps) run` works from any program with no files on disk.
//!
//! # Rendering a program to PNG
//!
//! ```
//! use pscat::Interp;
//!
//! // A 200x100-point page at 72 dpi: 200x100 pixels.
//! let mut interp = Interp::with_page(200, 100).expect("valid page size");
//! interp.run_str(
//!     "0.2 0.4 0.9 setrgbcolor
//!      20 20 160 60 rectfill
//!      showpage",
//! )?;
//! let pages = interp.pages();
//! assert_eq!(pages.len(), 1);
//! let png: Vec<u8> = pages[0].to_png().expect("PNG encoding");
//! assert_eq!(&png[1..4], b"PNG");
//! # Ok::<(), pscat::PsError>(())
//! ```
//!
//! Errors come back as a [`PsError`] named after the PLRM error
//! (`undefined`, `typecheck`, ...); [`Interp::error_report`] formats one as
//! a LaserWriter-style report naming the offending command and line:
//!
//! ```
//! let mut interp = pscat::Interp::new();
//! let err = interp.run_str("1 2 nosuchop").unwrap_err();
//! assert_eq!(err.name(), "undefined");
//! assert!(interp.error_report(&err).contains("nosuchop"));
//! ```
//!
//! SVG and PDF output are opt-in, because they record every drawing
//! operation as it happens: call [`Interp::enable_svg`] or
//! [`Interp::enable_pdf`] before running the program, then read
//! [`Interp::svg_pages`] or [`Interp::pdf_document`].
//!
//! # Running a program you do not trust
//!
//! Nothing limits a program by default. [`Interp::set_limits`] takes a
//! [`Limits`] that stops one with [`PsError::StepsExceeded`] (or `TimeExceeded`, `MemoryExceeded`) after too many
//! steps, too long, or too much memory; a program cannot catch that error
//! with `stopped`. See the [`limits`] module for what each limit can and cannot
//! stop.
//!
//! # Threads
//!
//! An [`Interp`] is neither `Send` nor `Sync`: PostScript objects share
//! storage through `Rc`. Create one interpreter per thread; they are
//! independent and cheap to make.
//!
//! # Features
//!
//! - `window` (default): the live window behind `pscat file.ps`,
//!   `--interactive` and `--spool`, through `winit` and `softbuffer`. It is a
//!   command-line feature with no library API. Library users should turn it
//!   off, which keeps both crates out of the dependency graph:
//!
//!   ```toml
//!   [dependencies]
//!   pscat = { version = "0.6", default-features = false }
//!   ```
//!
//! # Supported API and stability
//!
//! The supported API is what this documentation shows: [`Interp`], [`Page`],
//! [`PsError`] and [`Limits`]. The crate has other public modules that the `pscat` and
//! `pscat-mcp` binaries, the browser build and the test suite use; they are
//! hidden from the documentation and can change in any release. Before 1.0,
//! a minor version (0.6 to 0.7) may break the supported API and a patch
//! version does not.

#![deny(missing_docs)]

// The supported surface: documented, and checked by `missing_docs`.
pub mod error;
pub mod interp;
pub mod limits;
pub mod memtrack;
pub mod page;

// Everything below is public only so the binaries, the wasm build and the
// integration tests can reach it. `#[doc(hidden)]` keeps it out of the docs
// and out of cargo-semver-checks' view of the API (issue #565).
#[doc(hidden)]
pub mod capabilities;
#[doc(hidden)]
pub mod chalk;
pub(crate) mod clock;
#[doc(hidden)]
pub mod contact_sheet;
pub(crate) mod coverage;
#[doc(hidden)]
pub mod encodings;
#[doc(hidden)]
pub mod file;
#[doc(hidden)]
pub mod filenames;
#[doc(hidden)]
pub mod fmath;
#[doc(hidden)]
pub mod font;
#[doc(hidden)]
pub mod fontpacks;
#[doc(hidden)]
pub mod fuzzing;
#[doc(hidden)]
pub mod gfx;
pub(crate) mod guard;
#[doc(hidden)]
pub mod halftone;
pub(crate) mod image;
#[doc(hidden)]
pub mod lexer;
#[doc(hidden)]
pub mod lint;
#[doc(hidden)]
pub mod name;
#[doc(hidden)]
pub mod object;
#[doc(hidden)]
pub mod ops;
#[doc(hidden)]
pub mod paths;
#[doc(hidden)]
pub mod pdf;
#[doc(hidden)]
pub mod profile;
#[doc(hidden)]
pub mod read_roots;
#[doc(hidden)]
pub mod repl;
// Reads library files off disk to run their `%%SelfTest` blocks —
// a developer/CI tool, not something the wasm playground can host.
#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
pub mod selftest;
pub(crate) mod shading;
#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
pub mod spool;
#[doc(hidden)]
pub mod svg;
pub(crate) mod type1;
#[cfg(target_arch = "wasm32")]
#[doc(hidden)]
pub mod wasm;
#[cfg(all(feature = "window", not(target_arch = "wasm32")))]
#[doc(hidden)]
pub mod window;
// Without the `window` feature the CLI still names `window::run_windowed`
// and friends; this stand-in returns an error from each (issue #565).
#[cfg(all(not(feature = "window"), not(target_arch = "wasm32")))]
#[doc(hidden)]
#[path = "window_stub.rs"]
pub mod window;
#[doc(hidden)]
pub mod write_roots;

pub use error::PsError;
pub use interp::Interp;
pub use limits::{Limit, Limits, LimitsError};
pub use memtrack::TrackingAllocator;
#[doc(hidden)]
pub use object::{Object, Value};
pub use page::Page;
