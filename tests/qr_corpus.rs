//! QR boundary and decode corpus (#973). Every version 1-40 at every ECC
//! level is encoded at its exact byte capacity and one byte past it, and
//! compared with the pinned independent encoder's codewords and finished
//! grids (`tests/data/qr_corpus_vectors.json`, from
//! `scripts/qr_reference_vectors.py`); URL and UTF-8 payloads likewise. A
//! representative subset is rendered through pscat's PNG, SVG and PDF
//! exports and read back by two independent decoders; the whole 160-symbol
//! sweep is `#[ignore]`d (command in docs/dev/design/QR_CODE_TESTING.md).
//! Physical size, the print recipe, the quiet zone and module edges are
//! checked pixel by pixel at several scales.

mod common;
mod qr_support;
use common::CommandDeadline as _;
use pscat::{Interp, Value};
use qr_support::{DECODERS, Grey, Read, level_code, raster_pdf, raster_png, raster_svg};
use sha2::{Digest, Sha256};
use std::process::Command;

const LIB: &str = include_str!("../lib/qr.ps");
const CORPUS: &str = include_str!("data/qr_corpus_vectors.json");
const GITHUB_URL: &str = "https://github.com/PSCatLabs/pscat";

fn corpus() -> serde_json::Value {
    serde_json::from_str(CORPUS).expect("checked-in corpus JSON")
}

fn interpreter() -> Interp {
    let mut it = Interp::with_page(8, 8).expect("small test page");
    it.run_str(LIB)
        .unwrap_or_else(|e| panic!("load qr.ps: {}", it.error_report(&e)));
    assert!(
        it.operand_stack().is_empty(),
        "library load leaked operands"
    );
    it
}

fn ordinal(level: &str) -> u32 {
    match level {
        "L" => 0,
        "M" => 1,
        "Q" => 2,
        "H" => 3,
        other => panic!("level {other}"),
    }
}

/// The corpus payload formula, as in the generator: every byte value, not
/// periodic at 256 bytes.
fn corpus_bytes(length: usize, version: u32, level: &str) -> Vec<u8> {
    (0..length as u32)
        .map(|i| ((i * 149 + i / 251 + version * 7 + ordinal(level) * 41) % 256) as u8)
        .collect()
}

fn ps_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2 + 2);
    out.push('<');
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out.push('>');
    out
}

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn hex_bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex"))
        .collect()
}

/// Run `src`, which must leave exactly the listed number of values.
fn run(it: &mut Interp, src: &str, values: usize, label: &str) {
    it.run_str(src)
        .unwrap_or_else(|e| panic!("{label}: {}", it.error_report(&e)));
    assert_eq!(
        it.operand_stack().len(),
        values,
        "{label}: stack {:?}",
        it.operand_stack()
            .iter()
            .map(|o| o.repr())
            .collect::<Vec<_>>()
    );
}

fn pop_bytes(it: &mut Interp) -> Vec<u8> {
    let top = it.operand_stack().last().expect("value");
    let bytes = match &top.value {
        Value::String(s) => s.borrow_bytes().to_vec(),
        _ => panic!("expected a string, got {}", top.repr()),
    };
    it.run_str("pop").expect("pop");
    bytes
}

fn pop_repr(it: &mut Interp) -> String {
    let r = it.operand_stack().last().expect("value").repr();
    it.run_str("pop").expect("pop");
    r
}

/// A public symbol from `encode`, read back as the caller sees it: its
/// version, mask, size and one 0/1 byte per module of `/Modules`, row-major
/// from the top-left, the layout of the fixtures' `final_sha256`.
struct Symbol {
    version: u64,
    mask: u64,
    size: usize,
    modules: Vec<u8>,
}

fn encode(it: &mut Interp, encode: &str, label: &str) -> Symbol {
    run(
        it,
        &format!(
            "{encode} /qsym exch def \
             /qs qsym /Size get def /qo qs qs mul string def \
             0 1 qs 1 sub {{ /qy exch def qsym /Modules get qy get \
               0 1 qs 1 sub {{ /qx exch def dup qx get {{ qo qy qs mul qx add 1 put }} if }} for pop \
             }} for \
             qsym /Version get qsym /Mask get qs qo"
        ),
        4,
        label,
    );
    let modules = pop_bytes(it);
    let size = pop_repr(it).parse().expect("size");
    let mask = pop_repr(it).parse().expect("mask");
    let version = pop_repr(it).parse().expect("version");
    Symbol {
        version,
        mask,
        size,
        modules,
    }
}

/// `payload level` -> (selected version, complete interleaved codewords,
/// padded data), through the internal planner and interleaver: the stage
/// the oracle's `codewords_sha256` pins, so a codeword mismatch is told
/// apart from a placement, mask or metadata one.
fn codewords(it: &mut Interp, payload: &str, level: &str, label: &str) -> (u64, Vec<u8>, Vec<u8>) {
    run(
        it,
        &format!(
            "{payload} /{level} qr--byte-plan dup /Version get exch dup /Data get \
             dup 3 -1 roll dup /Version get exch /Level get qr--interleave"
        ),
        3,
        label,
    );
    let stream = pop_bytes(it);
    let data = pop_bytes(it);
    let version = pop_repr(it).parse().expect("version");
    (version, stream, data)
}

/// `src` must stop with the named error, opening no dictionary frame. The
/// operand stack is cleared to a mark afterwards (operand rollback after a
/// named failure is not promised).
fn refuses(it: &mut Interp, src: &str, name: &str, label: &str) {
    run(
        it,
        &format!(
            "/qdepth countdictstack def mark {{ {src} }} stopped \
             $error /command get /{name} eq and countdictstack qdepth eq and \
             /qok exch def cleartomark qok"
        ),
        1,
        label,
    );
    assert_eq!(pop_repr(it), "true", "{label}: expected {name}");
}

/// Every version of one level, at exactly its byte capacity (forced mask)
/// and one byte past it (automatic mask): the oracle's codewords and whole
/// finished grid, the next version up (or overflow at 40), and a fixed
/// version that cannot hold the extra byte. All in one interpreter, so
/// 160 repeated calls must leave no operand, no dictionary and no state
/// that changes a later result.
fn boundary_level(level: &str) {
    let data = corpus();
    let mut it = interpreter();
    run(&mut it, "countdictstack", 1, "depth");
    let depth = pop_repr(&mut it);
    let mut first: Option<(String, Vec<u8>)> = None;
    let cases: Vec<_> = data["boundaries"]
        .as_array()
        .expect("boundaries")
        .iter()
        .filter(|f| f["level"] == level)
        .collect();
    assert_eq!(cases.len(), 40, "{level}: every version");
    for f in cases {
        let version = f["version"].as_u64().expect("version") as u32;
        let maximum = f["maximum"].as_u64().expect("maximum") as usize;
        let mask = f["forced_mask"].as_u64().expect("mask");
        let tag = format!("v{version}-{level} ({maximum} bytes)");
        let full = corpus_bytes(maximum, version, level);
        assert_eq!(
            sha(&full),
            f["payload_sha256"],
            "{tag}: payload formula drifted from the generator"
        );
        let full = ps_hex(&full);
        let over = ps_hex(&corpus_bytes(maximum + 1, version, level));

        let (planned, stream, _) = codewords(&mut it, &full, level, &tag);
        assert_eq!(planned, u64::from(version), "{tag}: planned version");
        assert_eq!(
            sha(&stream),
            f["max"]["codewords_sha256"],
            "{tag}: codewords differ from the oracle"
        );
        let encode_full = format!("{full} << /Level /{level} /Mask {mask} >> qrencode");
        let sym = encode(&mut it, &encode_full, &tag);
        assert_eq!(
            (sym.version, sym.mask, sym.size),
            (u64::from(version), mask, version as usize * 4 + 17),
            "{tag}: version, mask, size"
        );
        assert_eq!(
            sha(&sym.modules),
            f["max"]["final_sha256"],
            "{tag}: mask {mask} grid differs from the oracle (codewords match)"
        );
        if first.is_none() {
            first = Some((encode_full, sym.modules));
        }

        let tag = format!("v{version}-{level} + 1 byte");
        let up = &f["over"];
        match up["version"].as_u64() {
            None => refuses(
                &mut it,
                &format!("{over} << /Level /{level} >> qrencode"),
                "qr-data-too-long-for-level",
                &tag,
            ),
            Some(next) => {
                let (planned, stream, _) = codewords(&mut it, &over, level, &tag);
                assert_eq!(planned, next, "{tag}: planned version");
                assert_eq!(
                    sha(&stream),
                    up["codewords_sha256"],
                    "{tag}: codewords differ from the oracle"
                );
                let sym = encode(
                    &mut it,
                    &format!("{over} << /Level /{level} >> qrencode"),
                    &tag,
                );
                assert_eq!(sym.version, next, "{tag}: automatic version");
                if sym.mask != up["mask"].as_u64().expect("mask")
                    || sha(&sym.modules) != up["final_sha256"]
                {
                    run(
                        &mut it,
                        &format!(
                            "{over} /{level} qr--byte-plan dup /Data get exch dup /Version get \
                             exch /Level get 3 copy qr--interleave 2 index qr--matrix \
                             exch qr--mask-penalties 3 1 roll pop pop"
                        ),
                        1,
                        &tag,
                    );
                    panic!(
                        "{tag}: automatic symbol differs: mask {} vs oracle {}; \
                         penalties {} vs oracle {}",
                        sym.mask,
                        up["mask"],
                        pop_repr(&mut it),
                        up["penalties"]
                    );
                }
            }
        }
        refuses(
            &mut it,
            &format!("{over} << /Level /{level} /Version {version} >> qrencode"),
            "qr-data-too-long-for-version",
            &format!("{tag} at fixed v{version}"),
        );
        run(&mut it, "countdictstack", 1, &tag);
        assert_eq!(pop_repr(&mut it), depth, "{tag}: dictionary stack");
    }
    // Repeated call: the first symbol again, after 159 other calls.
    let (src, modules) = first.expect("first case");
    let again = encode(&mut it, &src, "repeat");
    assert!(again.modules == modules, "{level}: a repeated call differs");
    assert!(it.operand_stack().is_empty(), "{level}: operands left");
}

#[test]
fn every_version_at_level_l_matches_the_oracle_at_and_past_capacity() {
    boundary_level("L");
}

#[test]
fn every_version_at_level_m_matches_the_oracle_at_and_past_capacity() {
    boundary_level("M");
}

#[test]
fn every_version_at_level_q_matches_the_oracle_at_and_past_capacity() {
    boundary_level("Q");
}

#[test]
fn every_version_at_level_h_matches_the_oracle_at_and_past_capacity() {
    boundary_level("H");
}

#[test]
fn urls_and_utf8_text_match_the_oracle_with_automatic_version_and_mask() {
    let data = corpus();
    let mut it = interpreter();
    for f in data["texts"].as_array().expect("texts") {
        let level = f["level"].as_str().expect("level");
        let payload = ps_hex(&hex_bytes(f["payload"].as_str().expect("payload")));
        let tag = format!("{} at {level}", f["name"]);
        let (version, stream, _) = codewords(&mut it, &payload, level, &tag);
        assert_eq!(Some(version), f["version"].as_u64(), "{tag}: version");
        assert_eq!(sha(&stream), f["codewords_sha256"], "{tag}: codewords");
        let sym = encode(
            &mut it,
            &format!("{payload} << /Level /{level} >> qrencode"),
            &tag,
        );
        assert_eq!(Some(sym.mask), f["mask"].as_u64(), "{tag}: mask");
        assert_eq!(sha(&sym.modules), f["final_sha256"], "{tag}: grid");
    }
}

#[test]
fn oversize_and_invalid_options_raise_named_errors_and_open_no_frame() {
    let mut it = interpreter();
    // Each level's largest payload fits version 40; one byte more does not.
    for (level, max) in [("L", 2953), ("M", 2331), ("Q", 1663), ("H", 1273)] {
        let sym = encode(
            &mut it,
            &format!("{max} string << /Level /{level} >> qrencode"),
            level,
        );
        assert_eq!(sym.version, 40, "{level}: {max} bytes is version 40");
        for over in [max + 1, max + 1000] {
            refuses(
                &mut it,
                &format!("{over} string << /Level /{level} >> qrencode"),
                "qr-data-too-long-for-level",
                &format!("{level}: {over} bytes"),
            );
        }
    }
    for (opts, name) in [
        ("<< /Level /l >>", "qr-level-must-be-L-M-Q-or-H"),
        ("<< /Level /LOW >>", "qr-level-must-be-L-M-Q-or-H"),
        ("<< /Level 1 >>", "qr-level-must-be-L-M-Q-or-H"),
        ("<< /Level null >>", "qr-level-must-be-L-M-Q-or-H"),
        ("<< /Version -1 >>", "qr-version-must-be-in-1-40"),
        ("<< /Version 41 >>", "qr-version-must-be-in-1-40"),
        ("<< /Version 1.5 >>", "qr-version-must-be-integer"),
        ("<< /Version (1) >>", "qr-version-must-be-integer"),
        ("<< /Version true >>", "qr-version-must-be-integer"),
        ("<< /Mask 8 >>", "qr-mask-must-be-in-0-7"),
        ("<< /Mask 2.0 >>", "qr-mask-must-be-integer"),
        ("<< /Mask /0 >>", "qr-mask-must-be-integer"),
        ("[ /Level /M ]", "qr-options-must-be-a-dict"),
        ("null", "qr-options-must-be-a-dict"),
    ] {
        refuses(&mut it, &format!("(A) {opts} qrencode"), name, opts);
    }
    for payload in ["/name", "42", "[ 65 ]", "null"] {
        refuses(
            &mut it,
            &format!("{payload} << >> qrencode"),
            "qr-data-must-be-a-string",
            payload,
        );
    }
    // Options are checked before the payload, the level before the version.
    refuses(
        &mut it,
        "5 << /Level /X /Version 0 >> qrencode",
        "qr-level-must-be-L-M-Q-or-H",
        "first bad argument wins",
    );
    // The 8-bit count of versions 1-9 can never overflow: 255 bytes need
    // version 10 at L, where the count is 16 bits.
    let sym = encode(&mut it, "255 string << /Level /L >> qrencode", "255 bytes");
    assert_eq!(sym.version, 10);
    refuses(
        &mut it,
        "255 string << /Level /L /Version 9 >> qrencode",
        "qr-data-too-long-for-version",
        "255 bytes at v9",
    );
    // Unknown option keys are ignored, as in the other libraries.
    let a = encode(
        &mut it,
        "(A) << /Side 9 /Foo (bar) >> qrencode",
        "unknown keys",
    );
    let b = encode(&mut it, "(A) << >> qrencode", "no keys");
    assert!(a.modules == b.modules);
}

// ---- Rendering ----

/// Render `program` (after loading lib/qr.ps) with `pscat` on a `w`x`h`
/// point page into `flag` (`--png`, `--svg`, `--pdf`), PNG at `dpi`.
fn render(program: &str, w: u32, h: u32, flag: &str, dpi: f64, tag: &str) -> Vec<u8> {
    let ext = &flag[2..];
    let dir = common::scratch_dir(&format!("qr-corpus-{tag}"));
    let ps = dir.join("symbol.ps");
    let out = dir.join(format!("symbol.{ext}"));
    std::fs::write(&ps, format!("(lib/qr.ps) run\n{program}\n")).expect("write program");
    let mut args = vec!["--page".to_string(), format!("{w}x{h}")];
    if flag == "--png" {
        args.extend(["--dpi".to_string(), format!("{dpi}")]);
    }
    args.extend([
        flag.to_string(),
        out.to_str().expect("utf-8").to_string(),
        ps.to_str().expect("utf-8").to_string(),
    ]);
    let run = Command::new(env!("CARGO_BIN_EXE_pscat"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(&args)
        .output_deadline()
        .unwrap_or_else(|e| panic!("{tag}: run pscat {flag}: {e}"));
    assert!(
        run.status.success(),
        "{tag}: pscat {flag}: {} {}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    std::fs::read(&out).unwrap_or_else(|e| panic!("{tag}: read {flag} export: {e}"))
}

/// A drawn symbol and where it is: the drawing's lower-left corner (x0, y0)
/// in points, its module side and quiet zone, and the page's extent in
/// device pixels (see [`page_px`]): `right` and `top`, the device column and
/// row of the page's right and bottom edges (user y = 0 is the bottom).
struct Placed<'a> {
    modules: &'a [u8],
    n: usize,
    x0: f64,
    y0: f64,
    module: f64,
    quiet: usize,
    right: f64,
    top: f64,
}

/// A page length of `pt` points in device pixels at `scale` px/pt. pscat's
/// PNG rounds the page to whole pixels and puts user y = 0 on the bottom
/// pixel edge; resvg and hayro map the page exactly, so their last pixel
/// row or column may lie partly off the page.
fn page_px(export: &str, pt: f64, scale: f64) -> f64 {
    if export.starts_with("PNG") {
        // As pscat computes it, in f32: 214.5 rounds down to 214 there.
        f64::from((pt as f32 * scale as f32).round())
    } else {
        pt * scale
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Area {
    Outside,
    Light,
    Dark,
}

impl Placed<'_> {
    /// The area of drawing cell (col, row), counted from the drawing's
    /// top-left in modules, quiet zone included.
    fn area(&self, col: i64, row: i64) -> Area {
        let far = (self.n + 2 * self.quiet) as i64;
        if !(0..far).contains(&col) || !(0..far).contains(&row) {
            return Area::Outside;
        }
        let (c, r) = (col - self.quiet as i64, row - self.quiet as i64);
        let n = self.n as i64;
        if (0..n).contains(&c) && (0..n).contains(&r) && self.modules[(r * n + c) as usize] == 1 {
            Area::Dark
        } else {
            Area::Light
        }
    }

    /// Check every device pixel at `scale` pixels per point: a pixel that
    /// lies wholly inside one kind of area (backdrop, light, dark, possibly
    /// spanning several modules of that kind) must have that area's exact
    /// grey. This pins the quiet-zone width, every module edge and the
    /// absence of seams between touching dark modules; only pixels that
    /// straddle an edge between different areas may be grey.
    fn check_pixels(&self, g: &Grey, scale: f64, backdrop: u8, ctx: &str) {
        let far = (self.n + 2 * self.quiet) as f64;
        let eps = 1e-6;
        // Pixel index -> half-open drawing-cell span it covers.
        let span = |p: usize, to_cell: &dyn Fn(f64) -> f64| {
            let (a, b) = (to_cell(p as f64), to_cell((p + 1) as f64));
            let (lo, hi) = if a < b { (a, b) } else { (b, a) };
            ((lo + eps).floor() as i64, (hi - eps).ceil() as i64 - 1)
        };
        // Device pixel edge -> drawing cell coordinate. Device y runs down,
        // user y up; drawing rows count from the drawing's top.
        let col_of = |p: f64| (p / scale - self.x0) / self.module;
        let row_of = |p: f64| far - ((self.top - p) / scale - self.y0) / self.module;
        let mut checked = 0usize;
        // A pixel partly off the page blends with nothing: not checked.
        let rows = (self.top + eps).floor() as usize;
        let cols = (self.right + eps).floor() as usize;
        for py in 0..g.height.min(rows) {
            let (r0, r1) = span(py, &row_of);
            for px in 0..g.width.min(cols) {
                let (c0, c1) = span(px, &col_of);
                let first = self.area(c0, r0);
                let uniform = (r0..=r1).all(|r| (c0..=c1).all(|c| self.area(c, r) == first));
                if !uniform {
                    continue;
                }
                let (want, tol) = match first {
                    Area::Outside => (backdrop, 8),
                    Area::Light => (255, 8),
                    Area::Dark => (0, 8),
                };
                let got = g.at(px, py);
                assert!(
                    got.abs_diff(want) <= tol,
                    "{ctx}: pixel ({px}, {py}) inside {first:?} cells \
                     cols {c0}..={c1} rows {r0}..={r1} is grey {got}, want {want}"
                );
                checked += 1;
            }
        }
        assert!(
            checked > g.width * g.height / 2,
            "{ctx}: too few pixels checked"
        );
    }
}

/// Each decoder must read back exactly `payload`, the level, and whatever
/// else it reports (rqrr: version and mask; rxing: the padded data
/// codewords). Returns the per-decoder outcome for the caller to assert.
fn decode_all(g: &Grey, c: &Corpus) -> Vec<(&'static str, Result<(), String>)> {
    let (payload, version, mask, level, data) =
        (&c.payload, u64::from(c.version), c.mask, c.level, &c.data);
    DECODERS
        .iter()
        .map(|(name, read)| {
            let check = |r: Read| -> Result<(), String> {
                if r.payload != *payload {
                    return Err(format!(
                        "payload {} bytes, expected {}",
                        r.payload.len(),
                        payload.len()
                    ));
                }
                if r.level != level_code(level) {
                    return Err(format!("level code {}", r.level));
                }
                if r.version.is_some_and(|v| v as u64 != version) {
                    return Err(format!("version {:?}", r.version));
                }
                if r.mask.is_some_and(|m| m != mask) {
                    return Err(format!("mask {:?}", r.mask));
                }
                if r.data_codewords.as_ref().is_some_and(|d| d != data) {
                    return Err("data codewords differ from the encoder's".into());
                }
                Ok(())
            };
            (*name, read(g).and_then(check))
        })
        .collect()
}

/// One corpus symbol: the boundary fixture of `version`/`level` at its
/// maximum with its forced mask, its padded data codewords and the symbol
/// `qrencode` made of it (already checked against the oracle grid).
struct Corpus {
    version: u32,
    level: &'static str,
    payload: Vec<u8>,
    mask: u64,
    data: Vec<u8>,
    sym: Symbol,
}

impl Corpus {
    /// Where [`rasters`] drew it: from (`origin`, `origin`) on a square
    /// `page`-point page, four-module quiet zone.
    fn placed(&self, origin: f64, module: f64, export: &str, page: u32, scale: f64) -> Placed<'_> {
        let edge = page_px(export, f64::from(page), scale);
        Placed {
            modules: &self.sym.modules,
            n: self.sym.size,
            x0: origin,
            y0: origin,
            module,
            quiet: 4,
            right: edge,
            top: edge,
        }
    }
}

fn corpus_symbol(
    it: &mut Interp,
    data: &serde_json::Value,
    version: u32,
    level: &'static str,
) -> Corpus {
    let f = data["boundaries"]
        .as_array()
        .expect("boundaries")
        .iter()
        .find(|f| f["version"] == version && f["level"] == level)
        .expect("boundary fixture");
    let maximum = f["maximum"].as_u64().expect("maximum") as usize;
    let mask = f["forced_mask"].as_u64().expect("mask");
    let payload = corpus_bytes(maximum, version, level);
    let tag = format!("v{version}-{level}");
    let (_, _, padded) = codewords(it, &ps_hex(&payload), level, &tag);
    let sym = encode(
        it,
        &format!(
            "{} << /Level /{level} /Mask {mask} >> qrencode",
            ps_hex(&payload)
        ),
        &tag,
    );
    assert_eq!(sha(&sym.modules), f["max"]["final_sha256"], "{tag}: grid");
    Corpus {
        version,
        level,
        payload,
        mask,
        data: padded,
        sym,
    }
}

/// The exports of one corpus symbol at one pitch: PNG at `scale` px/pt,
/// and the SVG and PDF rasterized by resvg and hayro at the same scale.
fn rasters(
    c: &Corpus,
    module: f64,
    origin: f64,
    scale: f64,
    exports: &[&str],
) -> Vec<(String, Grey, u32)> {
    let side = (c.sym.size + 8) as f64 * module;
    let page = (side + 2.0 * origin).ceil() as u32;
    let program = format!(
        "{origin} {origin} moveto {} << /Level /{} /Mask {} >> qrencode << /ModuleSize {module} >> qrdraw",
        ps_hex(&c.payload),
        c.level,
        c.mask
    );
    let tag = format!("v{}-{}-{module}", c.version, c.level);
    exports
        .iter()
        .map(|&export| {
            let g = match export {
                "PNG" => raster_png(&render(&program, page, page, "--png", 72.0 * scale, &tag)),
                "SVG" => raster_svg(
                    &render(&program, page, page, "--svg", 72.0, &tag),
                    scale as f32,
                ),
                "PDF" => raster_pdf(
                    &render(&program, page, page, "--pdf", 72.0, &tag),
                    scale as f32,
                ),
                other => panic!("export {other}"),
            };
            (export.to_string(), g, page)
        })
        .collect()
}

/// Decode-corpus cases in the default suite: every level, versions from the
/// smallest to the largest, the version-information boundary (6/7), the
/// 8/16-bit count boundary (9/10) and unequal block groups, each at a
/// pixel-aligned and a fractional pitch.
const SUBSET: [(u32, &str); 10] = [
    (1, "L"),
    (2, "M"),
    (6, "Q"),
    (7, "H"),
    (9, "L"),
    (10, "M"),
    (14, "Q"),
    (21, "H"),
    (27, "L"),
    (40, "M"),
];

/// (points per module, origin, pixels per point, pixel-aligned). 2 and 3
/// px/module on the pixel grid, where both decoders must read every subset
/// symbol in every export; 3.77 px/module from a fractional origin, where
/// every module edge falls inside a pixel and both decoders have known
/// misses ([`FRACTIONAL_MISSES`]).
const PITCHES: [(f64, f64, f64, bool); 3] = [
    (2.0, 0.0, 1.0, true),
    (3.0, 0.0, 1.0, true),
    (2.9, 3.3, 1.3, false),
];

/// (version, level, export, decoder) that do NOT read the fractional
/// 3.77 px/module raster, as measured with the pinned decoders. The
/// rasters themselves are pixel-exact (checked first), so these are decoder
/// limits, recorded so the table in QR_CODE_TESTING.md cannot drift: a
/// listed miss that starts decoding fails too.
const FRACTIONAL_MISSES: &[(u32, &str, &str, &str)] = &[
    (1, "L", "SVG", "rqrr"),
    (1, "L", "PDF", "rqrr"),
    (6, "Q", "PNG", "rqrr"),
    (7, "H", "SVG", "rqrr"),
    (7, "H", "PDF", "rqrr"),
    (9, "L", "PNG", "rqrr"),
    (9, "L", "SVG", "rqrr"),
    (10, "M", "PDF", "rqrr"),
    (14, "Q", "SVG", "rqrr"),
    (14, "Q", "PDF", "rqrr"),
    (21, "H", "PNG", "rqrr"),
    (21, "H", "PNG", "rxing"),
    (27, "L", "PNG", "rqrr"),
    (27, "L", "PNG", "rxing"),
    (27, "L", "SVG", "rqrr"),
    (27, "L", "SVG", "rxing"),
    (27, "L", "PDF", "rqrr"),
    (27, "L", "PDF", "rxing"),
    (40, "M", "PNG", "rqrr"),
    (40, "M", "PNG", "rxing"),
    (40, "M", "SVG", "rqrr"),
    (40, "M", "SVG", "rxing"),
    (40, "M", "PDF", "rqrr"),
    (40, "M", "PDF", "rxing"),
];

#[test]
fn rendered_subset_decodes_with_both_independent_decoders() {
    // Both decoders are compiled in: a decode that fails here fails the
    // suite, for either decoder, with nothing skipped.
    let data = corpus();
    let mut it = interpreter();
    let mut failures = Vec::new();
    for (version, level) in SUBSET {
        let c = corpus_symbol(&mut it, &data, version, level);
        for (module, origin, scale, aligned) in PITCHES {
            for (export, g, page) in rasters(&c, module, origin, scale, &["PNG", "SVG", "PDF"]) {
                let ctx = format!(
                    "v{version}-{level} mask {} {export} {:.2} px/module",
                    c.mask,
                    module * scale
                );
                c.placed(origin, module, &export, page, scale)
                    .check_pixels(&g, scale, 255, &ctx);
                for (decoder, outcome) in decode_all(&g, &c) {
                    let miss = !aligned
                        && FRACTIONAL_MISSES.contains(&(version, level, export.as_str(), decoder));
                    match (outcome, miss) {
                        (Ok(()), false) | (Err(_), true) => {}
                        (Err(e), false) => failures.push(format!("{ctx}: {decoder}: {e}")),
                        (Ok(()), true) => failures.push(format!(
                            "{ctx}: {decoder} now reads a listed miss; update FRACTIONAL_MISSES \
                             and QR_CODE_TESTING.md"
                        )),
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "decode failures:\n{}",
        failures.join("\n")
    );
}

/// One sweep result: the symbol, export and pitch, and each decoder's
/// outcome (`None` = read back exactly).
struct Outcome {
    level: &'static str,
    version: u32,
    export: String,
    px: f64,
    aligned: bool,
    decoders: Vec<(&'static str, Option<String>)>,
}

/// All 160 boundary symbols, rendered at seven pitches (whole-pixel and
/// fractional, 2 to 5.7 px/module), every raster checked pixel by pixel and
/// decoded by both decoders. Prints every failure and a per-pitch, per-
/// decoder table. Fails if a raster is not pixel-exact, if either decoder
/// misses a symbol at 2 px/module on the pixel grid, or if any pixel-
/// aligned raster is read by neither decoder. About 20 s on 8 threads:
/// `cargo test --profile ci --test qr_corpus -- --ignored --nocapture`.
#[test]
#[ignore = "exhaustive 160-symbol render and decode sweep; see QR_CODE_TESTING.md"]
fn exhaustive_corpus_sweep() {
    let pitches: [(f64, f64, f64, &[&str]); 7] = [
        (2.0, 0.0, 1.0, &["PNG", "SVG", "PDF"]),
        (3.0, 0.0, 1.0, &["PNG", "SVG", "PDF"]),
        (4.0, 0.0, 1.0, &["PNG"]),
        (2.0, 1.3, 1.3, &["PNG"]),
        (2.9, 3.3, 1.3, &["PNG", "SVG", "PDF"]),
        (3.0, 1.7, 1.5, &["PNG"]),
        (3.0, 1.7, 1.9, &["PNG"]),
    ];
    let threads = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(8);
    let jobs: Vec<(u32, &'static str)> = ["L", "M", "Q", "H"]
        .into_iter()
        .flat_map(|l| (1..=40).map(move |v| (v, l)))
        .collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let outcomes = std::sync::Mutex::new(Vec::new());
    let start = std::time::Instant::now();
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                let data = corpus();
                let mut it = interpreter();
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(&(version, level)) = jobs.get(i) else {
                        break;
                    };
                    let c = corpus_symbol(&mut it, &data, version, level);
                    for (module, origin, scale, exports) in pitches {
                        for (export, g, page) in rasters(&c, module, origin, scale, exports) {
                            let px = module * scale;
                            c.placed(origin, module, &export, page, scale).check_pixels(
                                &g,
                                scale,
                                255,
                                &format!("v{version}-{level} {export} {px:.2} px/module"),
                            );
                            let decoders = decode_all(&g, &c)
                                .into_iter()
                                .map(|(d, r)| (d, r.err()))
                                .collect();
                            outcomes.lock().expect("outcomes").push(Outcome {
                                level,
                                version,
                                export,
                                px,
                                aligned: origin == 0.0 && scale == 1.0,
                                decoders,
                            });
                        }
                    }
                }
            });
        }
    });
    let mut outcomes = outcomes.into_inner().expect("outcomes");
    outcomes.sort_by(|a, b| {
        (a.px, &a.export, a.level, a.version)
            .partial_cmp(&(b.px, &b.export, b.level, b.version))
            .expect("ordered")
    });
    let mut required = Vec::new();
    for o in &outcomes {
        let failed: Vec<String> = o
            .decoders
            .iter()
            .filter_map(|(d, e)| e.as_ref().map(|e| format!("{d}: {e}")))
            .collect();
        if failed.is_empty() {
            continue;
        }
        let line = format!(
            "v{}-{} {} {:.2} px/module{}: {}",
            o.version,
            o.level,
            o.export,
            o.px,
            if o.aligned { " (pixel grid)" } else { "" },
            failed.join("; ")
        );
        println!("{line}");
        if o.aligned && (o.px == 2.0 || failed.len() == DECODERS.len()) {
            required.push(line);
        }
    }
    println!("\npx/module export  decoder  read/rasters");
    let mut keys: Vec<(String, String)> = outcomes
        .iter()
        .map(|o| (format!("{:.2}", o.px), o.export.clone()))
        .collect();
    keys.dedup();
    for (px, export) in keys {
        let rows: Vec<&Outcome> = outcomes
            .iter()
            .filter(|o| format!("{:.2}", o.px) == px && o.export == export)
            .collect();
        for (i, (name, _)) in DECODERS.iter().enumerate() {
            let read = rows.iter().filter(|o| o.decoders[i].1.is_none()).count();
            println!("{px:>9} {export:<7} {name:<8} {read}/{}", rows.len());
        }
    }
    println!(
        "{} rasters, {} decodes, in {:.1?} on {threads} threads",
        outcomes.len(),
        outcomes.len() * DECODERS.len(),
        start.elapsed()
    );
    assert!(
        required.is_empty(),
        "required decodes failed:\n{}",
        required.join("\n")
    );
}

// ---- Physical size, print recipe, quiet zone and module edges ----

/// A business-card page (3.5 x 2 in, 252 x 144 pt) with a mid-grey
/// backdrop and a 0.8 in (57.6 pt) GitHub symbol at (180, 72): the PDF
/// MediaBox and SVG size are the page in points, the 300 dpi PNG print
/// recipe gives 1050 x 600 pixels, and in every raster the light square
/// covers exactly the 240 x 240 pixels 0.8 in is at 300 dpi.
#[test]
fn card_exports_keep_physical_size_and_the_300_dpi_print_recipe() {
    let mut it = interpreter();
    let sym = encode(&mut it, &format!("({GITHUB_URL}) << >> qrencode"), "card");
    let program = format!(
        "0.5 setgray clippath fill 180 72 moveto ({GITHUB_URL}) << >> qrencode \
         << /Side 57.6 >> qrdraw"
    );
    let pdf = render(&program, 252, 144, "--pdf", 72.0, "card");
    assert!(
        String::from_utf8_lossy(&pdf).contains("/MediaBox [0 0 252 144]"),
        "PDF MediaBox is not the card in points"
    );
    let svg = render(&program, 252, 144, "--svg", 72.0, "card");
    assert!(
        String::from_utf8_lossy(&svg)
            .contains("width=\"252\" height=\"144\" viewBox=\"0 0 252 144\""),
        "SVG size is not the card in user units (points)"
    );
    let scale = 300.0 / 72.0;
    let png = render(&program, 252, 144, "--png", 300.0, "card");
    let rasters = [
        ("PNG 300 dpi", raster_png(&png)),
        ("SVG via resvg", raster_svg(&svg, scale as f32)),
        ("PDF via hayro", raster_pdf(&pdf, scale as f32)),
    ];
    let module = 57.6 / (sym.size + 8) as f64;
    for (export, g) in &rasters {
        assert_eq!((g.width, g.height), (1050, 600), "{export}: print pixels");
        // The light square: every pixel lighter than the backdrop.
        let light: Vec<(usize, usize)> = (0..g.height)
            .flat_map(|y| (0..g.width).map(move |x| (x, y)))
            .filter(|&(x, y)| g.at(x, y) > 200)
            .collect();
        let xs = light.iter().map(|p| p.0);
        let ys = light.iter().map(|p| p.1);
        let bbox = (
            xs.clone().min().unwrap(),
            xs.max().unwrap(),
            ys.clone().min().unwrap(),
            ys.max().unwrap(),
        );
        // 180 pt = 750 px; 57.6 pt = 240 px; the top edge is 144 - 129.6 pt.
        assert_eq!(bbox, (750, 989, 60, 299), "{export}: 0.8 in light square");
        Placed {
            modules: &sym.modules,
            n: sym.size,
            x0: 180.0,
            y0: 72.0,
            module,
            quiet: 4,
            right: page_px(export, 252.0, scale),
            top: page_px(export, 144.0, scale),
        }
        .check_pixels(g, scale, 128, export);
        let reads = read_both(g, GITHUB_URL.as_bytes(), sym.version, sym.mask);
        assert!(reads.is_empty(), "{export}: {reads:?}");
    }
}

/// Decode with both decoders; return the failures.
fn read_both(g: &Grey, payload: &[u8], version: u64, mask: u64) -> Vec<String> {
    DECODERS
        .iter()
        .filter_map(|(name, read)| match read(g) {
            Ok(r)
                if r.payload == payload
                    && r.version.is_none_or(|v| v as u64 == version)
                    && r.mask.is_none_or(|m| m == mask) =>
            {
                None
            }
            Ok(r) => Some(format!("{name}: read {r:?}")),
            Err(e) => Some(format!("{name}: {e}")),
        })
        .collect()
}

/// The quiet zone is exactly `/QuietZone` modules of light on every side and
/// every module edge lands where it should, at whole and fractional pitches
/// from 1 to about 7 pixels per module, including the 0.3 mm (0.85 pt)
/// print rule of thumb at 300 dpi. Checked pixel by pixel in pscat's PNG and
/// the independent SVG and PDF rasters; from 2 px/module up, both decoders
/// read the symbol too.
#[test]
fn quiet_zone_and_module_edges_land_exactly_at_every_scale() {
    let mut it = interpreter();
    let sym = encode(
        &mut it,
        &format!("({GITHUB_URL}) << /Level /Q >> qrencode"),
        "edges",
    );
    // (module pt, origin pt, px per pt, quiet zone)
    let cases = [
        (1.0, 2.0, 1.0, 4),
        (2.0, 0.0, 1.0, 4),
        (1.0, 3.0, 3.0, 6),
        (1.0, 0.5, 300.0 / 72.0, 4),
        (0.85, 1.1, 300.0 / 72.0, 4),
        (2.9, 3.3, 1.3, 5),
        (1.7, 0.0, 4.0, 4),
    ];
    for (module, origin, scale, quiet) in cases {
        let side = (sym.size + 2 * quiet) as f64 * module;
        let page = (side + 2.0 * origin).ceil() as u32 + 2;
        let program = format!(
            "0.5 setgray clippath fill {origin} {origin} moveto \
             ({GITHUB_URL}) << /Level /Q >> qrencode \
             << /ModuleSize {module} /QuietZone {quiet} >> qrdraw"
        );
        let tag = format!("edges-{module}-{scale:.2}-{quiet}");
        let rasters = [
            (
                "PNG",
                raster_png(&render(&program, page, page, "--png", 72.0 * scale, &tag)),
            ),
            (
                "SVG",
                raster_svg(
                    &render(&program, page, page, "--svg", 72.0, &tag),
                    scale as f32,
                ),
            ),
            (
                "PDF",
                raster_pdf(
                    &render(&program, page, page, "--pdf", 72.0, &tag),
                    scale as f32,
                ),
            ),
        ];
        for (export, g) in &rasters {
            let ctx = format!(
                "{export} {:.2} px/module, quiet zone {quiet}, origin {origin} pt",
                module * scale
            );
            Placed {
                modules: &sym.modules,
                n: sym.size,
                x0: origin,
                y0: origin,
                module,
                quiet,
                right: page_px(export, f64::from(page), scale),
                top: page_px(export, f64::from(page), scale),
            }
            .check_pixels(g, scale, 128, &ctx);
            if module * scale >= 2.0 {
                let reads = read_both(g, GITHUB_URL.as_bytes(), sym.version, sym.mask);
                assert!(reads.is_empty(), "{ctx}: {reads:?}");
            }
        }
    }
}
