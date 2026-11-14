//! FlateDecode against Ghostscript (issue #1187). The decoder used to feed
//! the inflater one byte at a time into a 512-byte buffer and lose the next
//! byte whenever one expanded past it, so any stream with long flat runs
//! came out cut short: 1 MiB of spaces decoded to 354,232 bytes.

use std::io::Write as _;
use std::process::Command;

use common::CommandDeadline as _;
use pscat::Interp;

mod common;

fn zlib(data: &[u8]) -> Vec<u8> {
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(data).expect("compress in memory");
    z.finish().expect("compress in memory")
}

/// Text, long flat runs and noise, the mix PDF-to-PS output and page
/// images have.
fn mixed(len: usize) -> Vec<u8> {
    let mut x: u32 = 5;
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        match (x >> 28) % 4 {
            0 => out.extend(std::iter::repeat_n((x >> 8) as u8, 30_000)),
            1 => out.extend_from_slice(b"%!PS-Adobe-3.0 0 0 moveto (Hello) show\n"),
            _ => out.extend((0..700).map(|i| (x >> (i % 24)) as u8 ^ i as u8)),
        }
    }
    out.truncate(len);
    out
}

/// What the PostScript below prints for `data`: its length and an Adler-32
/// style pair of sums, each kept below 65521 so no integer overflows in gs.
fn summary(data: &[u8]) -> String {
    let (mut a, mut b) = (1u64, 0u64);
    for &x in data {
        a = (a + u64::from(x)) % 65521;
        b = (b + a) % 65521;
    }
    format!("{} {a} {b}\n", data.len())
}

/// Read `f` (a file object left on the stack by `open`) to its end in 64 KiB
/// strings and print what `summary` prints.
const SUMMARISE: &str = "/f exch def /n 0 def /a 1 def /b 0 def /buf 65536 string def \
    { f buf readstring exch dup length n add /n exch def \
      { a add 65521 mod dup /a exch def b add 65521 mod /b exch def } forall \
      not { exit } if } loop \
    n 20 string cvs print ( ) print a 20 string cvs print ( ) print b =";

/// Run `src` in pscat; what it printed.
fn pscat_prints(src: &[u8]) -> String {
    let mut it = Interp::new();
    it.capture_stdout();
    it.run_source(src)
        .unwrap_or_else(|e| panic!("pscat: {}", it.error_report(&e)));
    String::from_utf8_lossy(it.captured_stdout()).into_owned()
}

/// Run the program in `path` in gs (file reads allowed); what it printed.
fn gs_prints(path: &std::path::Path) -> String {
    let out = Command::new("gs")
        .args(["-q", "-dNOSAFER", "-dNODISPLAY", "-dBATCH", "-dNOPAUSE"])
        .arg(path)
        .output_deadline()
        .expect("run gs");
    assert!(
        out.status.success(),
        "gs: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn a_megabyte_of_spaces_decodes_in_full() {
    // The issue's probe.
    let dir = common::scratch_dir("flate-spaces");
    let z = dir.join("sp.z");
    std::fs::write(&z, zlib(&[b' '; 1 << 20])).expect("write scratch file");
    let src = format!(
        "/f ({}) (r) file /FlateDecode filter def /n 0 def /b 65536 string def \
         {{ f b readstring exch length n add /n exch def not {{ exit }} if }} loop n =",
        common::ps_path(&z)
    );
    assert_eq!(pscat_prints(src.as_bytes()), "1048576\n");
    if !common::gs_available() {
        return;
    }
    let ps = dir.join("fl.ps");
    std::fs::write(&ps, &src).expect("write scratch file");
    assert_eq!(gs_prints(&ps), "1048576\n");
}

#[test]
fn a_mixed_stream_decodes_as_ghostscript_decodes_it() {
    let data = mixed(4 << 20);
    let want = summary(&data);
    let dir = common::scratch_dir("flate-mixed");
    let z = dir.join("mixed.z");
    std::fs::write(&z, zlib(&data)).expect("write scratch file");
    let src = format!(
        "({}) (r) file /FlateDecode filter {SUMMARISE}",
        common::ps_path(&z)
    );
    assert_eq!(pscat_prints(src.as_bytes()), want);
    if !common::gs_available() {
        return;
    }
    let ps = dir.join("mixed.ps");
    std::fs::write(&ps, &src).expect("write scratch file");
    assert_eq!(gs_prints(&ps), want);
}

#[test]
fn an_inline_stream_leaves_the_program_right_after_its_end() {
    // The data follows `filter` in the program itself; the scanner must
    // pick up at the byte after the zlib stream, as in gs. Bulk input is
    // the risk here: the inflater sees the program text after the stream.
    let data = mixed(1 << 20);
    // Everything up to `loop` is scanned before the loop runs, so
    // `readstring` reads the stream from the byte after the space that
    // ends `loop`.
    let mut whole = b"currentfile /FlateDecode filter /f exch def /n 0 def /buf 65536 string def \
        { f buf readstring exch length n add /n exch def not { exit } if } loop "
        .to_vec();
    whole.extend_from_slice(&zlib(&data));
    whole.extend_from_slice(b"\nn = (after the stream) =\n");
    let want = format!("{}\nafter the stream\n", data.len());
    assert_eq!(pscat_prints(&whole), want);
    if !common::gs_available() {
        return;
    }
    let dir = common::scratch_dir("flate-inline");
    let ps = dir.join("inline.ps");
    std::fs::write(&ps, &whole).expect("write scratch file");
    assert_eq!(gs_prints(&ps), want);
}

/// A 240 x 160 RGB image with large flat areas, a gradient and a fine
/// checkerboard: the flat rows are what the old decoder cut short.
fn flat_image() -> Vec<u8> {
    let (w, h) = (240usize, 160usize);
    let mut px = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            let rgb = if y < 60 {
                [200, 30, 40]
            } else if y < 100 && x < 120 {
                [20, 60, 180]
            } else if y < 100 {
                [(x * 2) as u8, (y * 2) as u8, 90]
            } else if y < 130 {
                if (x / 3 + y / 3) % 2 == 0 {
                    [0, 0, 0]
                } else {
                    [255, 255, 255]
                }
            } else {
                [240, 220, 120]
            };
            px.extend_from_slice(&rgb);
        }
    }
    px
}

#[test]
fn a_flate_image_with_flat_areas_matches_ghostscript() {
    let dir = common::scratch_dir("flate-image");
    let z = dir.join("img.z");
    std::fs::write(&z, zlib(&flat_image())).expect("write scratch file");
    let body = format!(
        "/f ({}) (r) file /FlateDecode filter def \
         240 160 scale 240 160 8 [240 0 0 -160 0 160] f false 3 colorimage",
        common::ps_path(&z)
    );
    let mut it = Interp::with_page(240, 160).expect("page");
    it.run_str(&body)
        .unwrap_or_else(|e| panic!("pscat: {}", it.error_report(&e)));
    let ours = &it.gfx().pixmap;
    // Every pixel is one sample; compare with the samples themselves first
    // (no gs needed), so a truncated stream (white at the bottom) fails.
    let want = flat_image();
    let off = ours
        .pixels()
        .iter()
        .zip(want.chunks(3))
        .filter(|(p, s)| [p.red(), p.green(), p.blue()] != [s[0], s[1], s[2]])
        .count();
    assert_eq!(off, 0, "pixels that are not their sample");
    if !common::gs_available() {
        return;
    }
    let ps = dir.join("img.ps");
    let png = dir.join("gs.png");
    std::fs::write(&ps, format!("{body} showpage")).expect("write scratch file");
    let out = Command::new("gs")
        .args([
            "-q",
            "-dNOPAUSE",
            "-dBATCH",
            "-dNOSAFER",
            "-sDEVICE=png16m",
            "-g240x160",
            "-r72",
        ])
        .arg(format!("-o{}", png.display()))
        .arg(&ps)
        .output_deadline()
        .expect("run gs");
    assert!(
        out.status.success(),
        "gs: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let gs = tiny_skia::Pixmap::decode_png(&std::fs::read(&png).expect("png")).expect("decode");
    assert_eq!((gs.width(), gs.height()), (ours.width(), ours.height()));
    let differ = ours
        .pixels()
        .iter()
        .zip(gs.pixels())
        .filter(|(a, b)| [a.red(), a.green(), a.blue()] != [b.red(), b.green(), b.blue()])
        .count();
    assert_eq!(differ, 0, "pixels that differ from gs");
}
