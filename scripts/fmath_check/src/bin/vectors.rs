// Prints (input bits, std/musl result bits) vectors for tests/fmath.rs.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 { self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0 }
    fn unit(&mut self) -> f64 { (self.next() >> 11) as f64 / (1u64 << 53) as f64 }
    fn uni(&mut self, a: f64, b: f64) -> f64 { a + (b - a) * self.unit() }
}
fn main() {
    assert!(cfg!(target_env = "musl"));
    let mut r = Rng(0xDEADBEEFCAFEF00D);
    let mut ex: Vec<f64> = vec![0.0, -0.0, 1.0, -1.0, 0.5, 709.78, 709.79, 710.0, -708.0, -745.0, -745.2, -746.0, 1e-300, -1e-300, 1e-20, f64::INFINITY, f64::NEG_INFINITY, f64::NAN, 88.72283905206835, 0.6931471805599453];
    for _ in 0..40 { ex.push(r.uni(-60.0, 60.0)); }
    for _ in 0..8 { ex.push(r.uni(-745.0, 709.0)); }
    println!("pub const EXP: &[(u64, u64)] = &[");
    for x in ex { println!("    ({:#018x}, {:#018x}),", x.to_bits(), x.exp().to_bits()); }
    println!("];");
    let mut lg: Vec<f64> = vec![1.0, 0.0, -0.0, -1.0, 2.0, 0.5, 10.0, 0.01, 1.0000001, 0.9999999, 5e-324, 2.2250738585072014e-308, 1e-310, 1e308, f64::INFINITY, f64::NAN, f64::MAX, 1.0 + 1e-3, 1.0 - 1e-3, std::f64::consts::E];
    for _ in 0..40 { lg.push(r.uni(0.0, 100.0)); }
    for _ in 0..8 { lg.push(r.uni(-700.0, 700.0).exp()); }
    println!("pub const LN: &[(u64, u64)] = &[");
    for x in lg { println!("    ({:#018x}, {:#018x}),", x.to_bits(), x.ln().to_bits()); }
    println!("];");
    let mut pw: Vec<(f64, f64)> = vec![(2.0, 0.5), (2.0, 10.0), (10.0, -3.0), (-8.0, 3.0), (-8.0, 2.0), (-8.0, 1.0 / 3.0), (0.0, -1.0), (0.0, 0.0), (0.0, 2.5), (-0.0, -3.0), (1.0, f64::NAN), (f64::NAN, 0.0), (1e-310, 0.5), (2.0, 1023.0), (2.0, 1024.0), (2.0, -1074.0), (2.0, -1075.0), (-2.0, -1074.0), (0.5, f64::INFINITY), (2.0, f64::NEG_INFINITY), (1e300, 2.0), (f64::INFINITY, -1.0), (-f64::INFINITY, 3.0), (3.0, 1e-70)];
    for _ in 0..40 { pw.push((r.uni(0.0, 100.0), r.uni(-20.0, 20.0))); }
    for _ in 0..8 { pw.push((r.uni(-700.0, 700.0).exp(), r.uni(-3.0, 3.0))); }
    for _ in 0..8 { pw.push((-r.uni(0.0, 50.0), (r.next() % 41) as f64 - 20.0)); }
    println!("pub const POW: &[(u64, u64, u64)] = &[");
    for (x, y) in pw { println!("    ({:#018x}, {:#018x}, {:#018x}),", x.to_bits(), y.to_bits(), x.powf(y).to_bits()); }
    println!("];");
}
