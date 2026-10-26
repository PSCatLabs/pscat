//! The one place pscat's transcendental math lives (issue #738).
//!
//! `f64::sin` and friends call the platform's libm: Apple's on macOS, glibc's
//! or musl's on Linux, the compiler's port in the browser. Those round
//! differently in the last bit, and a seeded program (noise, erosion) grows a
//! one-bit difference into visible pixels, so the same seeded piece rendered
//! differently on a Mac and on Linux. Every function here is the same code on
//! every target, so results are bit for bit identical everywhere, and they
//! are musl's results, so Linux musl output (pscat's static Linux builds, and
//! the reference renders made with them) is what it always was:
//!
//! - `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `sinh`, `tanh`,
//!   `log10`, `hypot` and the `f32` ones come from the pure-Rust `libm` crate,
//!   which is musl's (FreeBSD msun's) code. Checked bit for bit against musl
//!   1.2.5 on aarch64 and x86_64 over 4 million random inputs each.
//! - `exp`, `ln` and `powf` are ported from musl 1.2.5 in `musl.rs`, because
//!   the `libm` crate has the older FreeBSD versions of those three, which
//!   differ from musl 1.2's in about 5% of results. The port equals musl on
//!   x86_64 on every input tried, and on aarch64 on all but about 6 in
//!   100,000 `ln` and 1 in 400,000 `powf` (musl's aarch64 build fuses a
//!   multiply-add that its x86_64 build does not; this is the x86_64 form,
//!   the same on every target).
//! - Not provided because `libm` does not match musl: `cosh`, `cbrt`, `log2`,
//!   `exp2`. Add one only with a bit-for-bit check against musl.
//!
//! Rules:
//! - Call these, never `f64::sin` and the like. `clippy.toml` bans the
//!   direct methods (`disallowed-methods`), so `cargo clippy` fails on one,
//!   and `Transform::from_rotate` (use [`rotation`]).
//! - `sqrt` is IEEE-exact on every platform and is not banned; `sqrt` here is
//!   for call sites that want one name, and changes no bits.
//! - `to_radians`, `to_degrees`, `floor` and the like are plain arithmetic or
//!   exact, so they stay as methods. Avoid `powi`: Rust documents its result
//!   as platform-dependent (nothing in `src/` uses it).
//! - See `notes/738-fmath-one-libm.md` for the evidence.

mod musl;
mod tables;

/// Sine of `x` radians.
#[inline]
pub fn sin(x: f64) -> f64 {
    libm::sin(x)
}

/// Cosine of `x` radians.
#[inline]
pub fn cos(x: f64) -> f64 {
    libm::cos(x)
}

/// Tangent of `x` radians.
#[inline]
pub fn tan(x: f64) -> f64 {
    libm::tan(x)
}

/// Arc sine, in radians.
#[inline]
pub fn asin(x: f64) -> f64 {
    libm::asin(x)
}

/// Arc cosine, in radians.
#[inline]
pub fn acos(x: f64) -> f64 {
    libm::acos(x)
}

/// Arc tangent, in radians.
#[inline]
pub fn atan(x: f64) -> f64 {
    libm::atan(x)
}

/// Four-quadrant arc tangent of `y / x`, in radians (`f64::atan2`'s argument
/// order: `y` first).
#[inline]
pub fn atan2(y: f64, x: f64) -> f64 {
    libm::atan2(y, x)
}

/// `e` to the power `x`.
#[inline]
pub fn exp(x: f64) -> f64 {
    musl::exp(x)
}

/// Natural logarithm.
#[inline]
pub fn ln(x: f64) -> f64 {
    musl::log(x)
}

/// Base-10 logarithm.
#[inline]
pub fn log10(x: f64) -> f64 {
    libm::log10(x)
}

/// `x` to the power `y` (`f64::powf`).
#[inline]
pub fn powf(x: f64, y: f64) -> f64 {
    musl::pow(x, y)
}

/// Hyperbolic sine.
#[inline]
pub fn sinh(x: f64) -> f64 {
    libm::sinh(x)
}

/// Hyperbolic tangent.
#[inline]
pub fn tanh(x: f64) -> f64 {
    libm::tanh(x)
}

/// Square root. IEEE-exact on every platform, so this equals `f64::sqrt`
/// bit for bit; it is here only so a call site needs one module.
#[inline]
pub fn sqrt(x: f64) -> f64 {
    x.sqrt()
}

/// `sqrt(x * x + y * y)` without undue overflow.
#[inline]
pub fn hypot(x: f64, y: f64) -> f64 {
    libm::hypot(x, y)
}

/// Single-precision [`hypot`].
#[inline]
pub fn hypotf(x: f32, y: f32) -> f32 {
    libm::hypotf(x, y)
}

/// Single-precision sine and cosine of `x` radians, as `(sin, cos)`.
#[inline]
pub fn sin_cos_f32(x: f32) -> (f32, f32) {
    (libm::sinf(x), libm::cosf(x))
}

/// A rotation by `angle` degrees: `Transform::from_rotate` with this module's
/// sine and cosine (tiny-skia's own calls the platform libm).
#[inline]
pub fn rotation(angle: f32) -> tiny_skia::Transform {
    let (b, a) = sin_cos_f32(angle.to_radians());
    tiny_skia::Transform::from_row(a, b, -b, a, 0.0, 0.0)
}
