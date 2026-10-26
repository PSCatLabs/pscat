//! Compares pscat's fmath (src/fmath/musl.rs and the `libm` crate) bit for bit
//! with `std`'s `f64::exp` and friends. Built on Alpine, `std` links musl's
//! libm, so a zero is "identical to musl"; built on macOS it shows how far the
//! platform libm is. Run it with scripts/fmath_check.sh. Issue #738.
#![allow(clippy::all, unused)]
mod fmath {
    #[path = "tables.rs"]
    pub mod tables;
    #[path = "musl.rs"]
    pub mod musl;
}
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 { self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0 }
    fn unit(&mut self) -> f64 { (self.next() >> 11) as f64 / (1u64 << 53) as f64 }
    fn uni(&mut self, a: f64, b: f64) -> f64 { a + (b - a) * self.unit() }
}
fn same(a: f64, b: f64) -> bool { a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()) }
fn same32(a: f32, b: f32) -> bool { a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()) }
struct Tally { name: &'static str, n: u64, bad: u64, ex: Option<String> }
impl Tally {
    fn new(name: &'static str) -> Self { Tally { name, n: 0, bad: 0, ex: None } }
    fn rec(&mut self, ok: bool, d: impl FnOnce() -> String) { self.n += 1; if !ok { self.bad += 1; if self.ex.is_none() { self.ex = Some(d()); } } }
    fn show(&self) { println!("{:<22} {:>10} inputs {:>9} mismatches  {}", self.name, self.n, self.bad, self.ex.clone().unwrap_or_default()); }
}
fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(2_000_000);
    println!("target arch {} os {} (std = {})", std::env::consts::ARCH, std::env::consts::OS, if cfg!(target_env = "musl") { "musl libm" } else { "platform libm" });
    let mut r = Rng(0x9E3779B97F4A7C15);
    let mut t_exp = Tally::new("port exp vs std");
    let mut t_ln = Tally::new("port ln vs std");
    let mut t_pow = Tally::new("port pow vs std");
    let mut t_expc = Tally::new("libm-crate exp vs std");
    let mut t_lnc = Tally::new("libm-crate ln vs std");
    let mut t_powc = Tally::new("libm-crate pow vs std");
    let mut t_log10 = Tally::new("libm log10 vs std");
    let mut t_sin = Tally::new("libm sin vs std");
    let mut t_cos = Tally::new("libm cos vs std");
    let mut t_tan = Tally::new("libm tan vs std");
    let mut t_atan = Tally::new("libm atan vs std");
    let mut t_atan2 = Tally::new("libm atan2 vs std");
    let mut t_sinf = Tally::new("libm sinf vs std");
    let mut t_cosf = Tally::new("libm cosf vs std");
    let mut t_hypotf = Tally::new("libm hypotf vs std");
    let mut ex = vec![Tally::new("libm asin"),Tally::new("libm acos"),Tally::new("libm sinh"),Tally::new("libm cosh"),Tally::new("libm tanh"),Tally::new("libm hypot"),Tally::new("libm cbrt"),Tally::new("libm log2"),Tally::new("libm exp2")];
    for i in 0..n {
        let k = i % 4;
        let xe = match k { 0 => r.uni(-50.0, 50.0), 1 => r.uni(-745.5, 710.0), 2 => r.uni(-1e-3, 1e-3), _ => f64::from_bits(r.next()) };
        let (a, b) = (fmath::musl::exp(xe), xe.exp());
        t_exp.rec(same(a, b), || format!("exp({xe:e}) port {a:e} std {b:e}"));
        let c = libm::exp(xe); t_expc.rec(same(c, b), || format!("exp({xe:e}) libm {c:e} std {b:e}"));
        let xl = match k { 0 => r.uni(0.0, 10.0), 1 => r.uni(-700.0, 700.0).exp(), 2 => r.uni(0.9, 1.1), _ => f64::from_bits(r.next()) };
        let (a, b) = (fmath::musl::log(xl), xl.ln());
        t_ln.rec(same(a, b), || format!("ln({xl:e}) port {a:e} std {b:e}"));
        let c = libm::log(xl); t_lnc.rec(same(c, b), || format!("ln({xl:e}) libm {c:e} std {b:e}"));
        let l10 = libm::log10(xl); let s10 = xl.log10(); t_log10.rec(same(l10, s10), || format!("log10({xl:e}) libm {l10:e} std {s10:e}"));
        let (px, py) = match k {
            0 => (r.uni(0.0, 100.0), r.uni(-20.0, 20.0)),
            1 => (r.uni(-700.0, 700.0).exp(), r.uni(-3.0, 3.0)),
            2 => (-r.uni(0.0, 50.0), (r.next() % 41) as f64 - 20.0),
            _ => (f64::from_bits(r.next()), f64::from_bits(r.next())),
        };
        let (a, b) = (fmath::musl::pow(px, py), px.powf(py));
        t_pow.rec(same(a, b), || format!("pow({px:e},{py:e}) port {a:e} std {b:e}"));
        let c = libm::pow(px, py); t_powc.rec(same(c, b), || format!("pow({px:e},{py:e}) libm {c:e} std {b:e}"));
        let ys0 = r.uni(-50.0, 50.0);
        let xs = match k { 0 => r.uni(-100.0, 100.0), 1 => r.uni(-1e5, 1e5), 2 => r.uni(-1.6, 1.6), _ => r.uni(-10.0, 10.0) };
        let (a, b) = (libm::sin(xs), xs.sin()); t_sin.rec(same(a, b), || format!("sin({xs:e}) libm {a:e} std {b:e}"));
        let (a, b) = (libm::cos(xs), xs.cos()); t_cos.rec(same(a, b), || format!("cos({xs:e}) libm {a:e} std {b:e}"));
        let (a, b) = (libm::tan(xs), xs.tan()); t_tan.rec(same(a, b), || format!("tan({xs:e}) libm {a:e} std {b:e}"));
        let (a, b) = (libm::atan(xs), xs.atan()); t_atan.rec(same(a, b), || format!("atan({xs:e}) libm {a:e} std {b:e}"));
        let ys = r.uni(-100.0, 100.0);
        let (a, b) = (libm::atan2(ys, xs), ys.atan2(xs)); t_atan2.rec(same(a, b), || format!("atan2({ys:e},{xs:e}) libm {a:e} std {b:e}"));
        let u = r.uni(-1.0, 1.0);
        let (a, b) = (libm::asin(u), u.asin()); ex[0].rec(same(a, b), || format!("asin({u:e}) libm {a:e} std {b:e}"));
        let (a, b) = (libm::acos(u), u.acos()); ex[1].rec(same(a, b), || format!("acos({u:e}) libm {a:e} std {b:e}"));
        let v = r.uni(-20.0, 20.0);
        let (a, b) = (libm::sinh(v), v.sinh()); ex[2].rec(same(a, b), || format!("sinh({v:e}) libm {a:e} std {b:e}"));
        let (a, b) = (libm::cosh(v), v.cosh()); ex[3].rec(same(a, b), || format!("cosh({v:e}) libm {a:e} std {b:e}"));
        let (a, b) = (libm::tanh(v), v.tanh()); ex[4].rec(same(a, b), || format!("tanh({v:e}) libm {a:e} std {b:e}"));
        let (a, b) = (libm::hypot(xs, ys0), xs.hypot(ys0)); ex[5].rec(same(a, b), || format!("hypot({xs:e},{ys0:e}) libm {a:e} std {b:e}"));
        let (a, b) = (libm::cbrt(xs), xs.cbrt()); ex[6].rec(same(a, b), || format!("cbrt({xs:e}) libm {a:e} std {b:e}"));
        let w = xl.abs();
        let (a, b) = (libm::log2(w), w.log2()); ex[7].rec(same(a, b), || format!("log2({w:e}) libm {a:e} std {b:e}"));
        let (a, b) = (libm::exp2(v * 30.0), (v * 30.0).exp2()); ex[8].rec(same(a, b), || format!("exp2({:e}) libm {a:e} std {b:e}", v * 30.0));
        let xf = xs as f32;
        let (a, b) = (libm::sinf(xf), xf.sin()); t_sinf.rec(same32(a, b), || format!("sinf({xf:e}) libm {a:e} std {b:e}"));
        let (a, b) = (libm::cosf(xf), xf.cos()); t_cosf.rec(same32(a, b), || format!("cosf({xf:e}) libm {a:e} std {b:e}"));
        let yf = ys as f32;
        let (a, b) = (libm::hypotf(xf, yf), xf.hypot(yf)); t_hypotf.rec(same32(a, b), || format!("hypotf({xf:e},{yf:e}) libm {a:e} std {b:e}"));
    }
    for t in &ex { t.show(); }
    for t in [&t_exp, &t_ln, &t_pow, &t_expc, &t_lnc, &t_powc, &t_log10, &t_sin, &t_cos, &t_tan, &t_atan, &t_atan2, &t_sinf, &t_cosf, &t_hypotf] { t.show(); }
}
