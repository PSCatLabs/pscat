//! Entry points for the fuzz targets in `fuzz/` (issue #604), kept in the
//! library so `tests/fuzz_corpus.rs` can replay the committed seeds and
//! crash regressions on every `cargo test`, with no nightly toolchain.
//!
//! Each function takes arbitrary bytes and must never panic, hang, or use
//! unbounded memory; what a fuzzer reports is a bug in pscat.

use std::time::Duration;

use crate::{Interp, Limits, lexer::Lexer};

/// Tokens read from one input before the lexer target stops.
const MAX_TOKENS: usize = 100_000;

/// Scan `data` as PostScript source, token by token.
pub fn lexer(data: &[u8]) {
    let mut lexer = Lexer::new(data.to_vec());
    for _ in 0..MAX_TOKENS {
        // An error ends the scan, as it ends a program.
        match lexer.next_token() {
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => break,
        }
    }
}

/// Limits for [`interpreter`]: tight, so a fuzzer finds crashes rather than
/// reporting every `{ } loop` as a timeout or an out-of-memory.
pub fn fuzz_limits() -> Limits {
    Limits::none()
        .with_max_steps(200_000)
        .with_max_time(Duration::from_secs(2))
        .with_max_memory(256 << 20)
}

/// For the fuzz targets' `init:`: let a panic that starts inside a
/// third-party crate, while pscat is catching it (`guard::catch`), pass
/// libFuzzer's abort-on-panic hook.
///
/// pscat catches panics from `ttf-parser` and `zune-jpeg` at the places it
/// calls them on untrusted bytes and turns them into a refused font or an
/// `ioerror`; libFuzzer's hook aborts before that unwinding can happen, so
/// without this every run would stop on the same known dependency bug (an
/// offset `debug_assert!`, a `u16` index that overflows only with overflow
/// checks on, a NEON `assert!`). A panic in pscat's own code, or outside a
/// guard, still goes to the previous hook and still aborts.
pub fn tolerate_dependency_panics() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let in_dependency = info
            .location()
            .is_some_and(|l| l.file().replace('\\', "/").contains("/registry/src/"));
        if !(in_dependency && crate::guard::active()) {
            previous(info);
        }
    }));
}

/// A tiny page under [`fuzz_limits`], stdout captured. The memory cap
/// applies only where the binary installs
/// [`TrackingAllocator`](crate::TrackingAllocator) (the fuzz targets and
/// `tests/fuzz_corpus.rs` do); without it the step and time caps stay.
fn limited_interp() -> Option<Interp> {
    let mut interp = Interp::with_page(64, 64)?;
    let limits = fuzz_limits();
    if interp.set_limits(limits).is_err() {
        let _ = interp.set_limits(Limits {
            max_memory: None,
            ..limits
        });
    }
    // Keep `print` and `=` out of the fuzzer's stdout.
    interp.capture_stdout();
    Some(interp)
}

/// Run `data` as a PostScript program on a tiny page, under
/// [`fuzz_limits`].
pub fn interpreter(data: &[u8]) {
    let _ = interpreter_result(data);
}

/// [`interpreter`], returning how the program ended, so a replay test can
/// check that a seed which would run forever (`(/dev/zero) (r) file`) is
/// stopped by a limit and not by luck.
pub fn interpreter_result(data: &[u8]) -> Result<(), crate::PsError> {
    match limited_interp() {
        Some(mut interp) => interp.run_source(data),
        None => Ok(()),
    }
}

/// Run `data` as a PostScript program under [`fuzz_limits`], then show text
/// in every font the program defined. For the `type1_font` target, whose
/// seeds are complete Type 1 font programs: the program does the parsing
/// (`eexec`, `RD`, `definefont`), the epilogue makes the charstring
/// interpreter and the width and outline code run on whatever it built.
pub fn type1_font(data: &[u8]) {
    let Some(mut interp) = limited_interp() else {
        return;
    };
    // The program may have defined its font and then failed on trailing
    // junk; the epilogue runs on whatever exists either way.
    let _ = interp.run_source(data);
    let _ = interp.run_source(
        b"\n\n FontDirectory { exch pop { 12 scalefont setfont 5 20 moveto \
          (AaBb Zz\\000\\377) show (Aa) stringwidth pop pop newpath 5 20 moveto \
          (Ab) false charpath pathbbox pop pop pop pop (a) true charpath } stopped \
          { clear } if } forall",
    );
}

/// One Type 1 charstring and the subroutines it can call, as plain
/// (already decrypted) bytes, straight into the charstring interpreter.
/// Layout: `[n_subrs, n_named, (len, bytes)*n_subrs, (len, bytes)*n_named,
/// code...]`; `seac` fetches named charstrings by hashing the name.
pub fn type1_charstring(data: &[u8]) {
    let mut cur = Cursor::new(data);
    // Up to 32: a call tree needs one subroutine per level to reach the
    // depth limit (30), and a tree that wide is the interesting shape.
    let n_subrs = usize::from(cur.u8() % 33);
    let n_named = usize::from(cur.u8() % 5);
    let take = |cur: &mut Cursor| {
        let len = usize::from(cur.u8() % 96);
        cur.bytes(len).to_vec()
    };
    let subrs: Vec<Vec<u8>> = (0..n_subrs).map(|_| take(&mut cur)).collect();
    let named: Vec<Vec<u8>> = (0..n_named).map(|_| take(&mut cur)).collect();
    let code = cur.rest();
    let by_name = |name: &str| -> Option<Vec<u8>> {
        if named.is_empty() {
            return None;
        }
        let h = name
            .bytes()
            .fold(0usize, |h, b| h.wrapping_mul(31) ^ usize::from(b));
        Some(named[h % named.len()].clone())
    };
    let _ = crate::type1::run(
        code,
        &crate::type1::FontData {
            subrs: &subrs,
            charstring_by_name: &by_name,
        },
    );
}

/// A font file's bytes (TrueType, OpenType/CFF, a collection, a variable
/// font) through the loader `findfont` and `addFont` use, then every glyph
/// path. Registers nothing, so a long run does not grow the process.
pub fn truetype(data: &[u8]) {
    let Some(mut interp) = limited_interp() else {
        return;
    };
    crate::font::fuzz_exercise_face(interp.gfx_mut(), data, "FuzzFace");
}

/// Output bytes read from a filter chain before the target stops: a
/// Flate or LZW bomb is endless to a fuzzer's patience, not to ours.
const MAX_FILTER_OUTPUT: usize = 1 << 20;

/// The decode filters on arbitrary bytes. Byte 0 picks the chain: its low
/// three bits the first decoder, the next three an optional second one
/// reading the first's output. Decoders: ASCIIHex, ASCII85, RunLength,
/// eexec, Flate, LZW and DCT (the JPEG marker scanner is pscat's own).
pub fn filters(data: &[u8]) {
    use crate::file::{Decoder, PsFile};
    let Some((&sel, payload)) = data.split_first() else {
        return;
    };
    let decoder = |code: u8| match code % 8 {
        0 => Some(Decoder::ascii_hex()),
        1 => Some(Decoder::ascii85()),
        2 => Some(Decoder::RunLength),
        3 => Some(Decoder::eexec()),
        4 => Some(Decoder::flate()),
        5 => Some(Decoder::lzw()),
        6 => Some(Decoder::dct()),
        _ => None,
    };
    let mut file = PsFile::from_bytes(payload.to_vec());
    let Some(first) = decoder(sel) else { return };
    file = PsFile::filtered(file, first);
    if let Some(second) = decoder(sel >> 3) {
        file = PsFile::filtered(file, second);
    }
    for _ in 0..MAX_FILTER_OUTPUT {
        match file.borrow_mut().read_byte() {
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => break,
        }
    }
}

/// Image data paths. A program that starts with `%!` is run as PostScript
/// (the seeds are the `selftest/drivers/image_*.ps` programs). Anything
/// else is a recipe: the first bytes choose an operator form (`image`,
/// `imagemask`, `colorimage` single and multi-source, the dictionary forms,
/// `ImageType 3`), a colour space, sample depth (1 to 16 bits), dimensions,
/// a Decode array and how the samples arrive (a string, a procedure, an
/// inline `currentfile`, or any decode filter), and the rest is sample data.
pub fn image(data: &[u8]) {
    let Some(mut interp) = limited_interp() else {
        return;
    };
    if data.starts_with(b"%!") {
        let _ = interp.run_source(data);
    } else {
        let _ = interp.run_source(&image_recipe(data));
    }
}

/// Reads fuzz input as a stream of choices; an exhausted input reads as
/// zeros, so every input is a valid recipe.
struct Cursor<'a> {
    data: &'a [u8],
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Cursor { data }
    }
    fn u8(&mut self) -> u8 {
        match self.data.split_first() {
            Some((&b, rest)) => {
                self.data = rest;
                b
            }
            None => 0,
        }
    }
    fn u16(&mut self) -> u16 {
        u16::from(self.u8()) << 8 | u16::from(self.u8())
    }
    fn bytes(&mut self, n: usize) -> &'a [u8] {
        let n = n.min(self.data.len());
        let (head, rest) = self.data.split_at(n);
        self.data = rest;
        head
    }
    fn rest(&mut self) -> &'a [u8] {
        self.bytes(self.data.len())
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2 + 2);
    s.push('<');
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s.push('>');
    s
}

/// Bytes of sample data a recipe passes on: more than this is only a
/// longer program.
const MAX_RECIPE_DATA: usize = 1 << 16;

fn image_recipe(data: &[u8]) -> Vec<u8> {
    use std::fmt::Write;
    let mut c = Cursor::new(data);
    let form = c.u8() % 8;
    let bits = [1u32, 2, 4, 8, 12, 16][usize::from(c.u8() % 6)];
    let space = c.u8() % 6;
    let big = c.u8() & 1 == 1;
    let (w, h) = if big {
        (u32::from(c.u16()), u32::from(c.u16()))
    } else {
        (u32::from(c.u8() % 40), u32::from(c.u8() % 40))
    };
    let (mw, mh) = (u32::from(c.u8() % 16), u32::from(c.u8() % 16));
    let decode = c.u8() % 5;
    let matrix = c.u8() % 6;
    let filter = c.u8() % 11;
    let payload = c.bytes(MAX_RECIPE_DATA);
    let ncomp = match space {
        0 | 3 | 4 => 1,
        1 | 5 => 3,
        _ => 4,
    };

    let mut p = String::from("%!\n20 20 translate 24 24 scale\n");
    p.push_str(match space {
        0 => "/DeviceGray setcolorspace\n",
        1 => "/DeviceRGB setcolorspace\n",
        2 => "/DeviceCMYK setcolorspace\n",
        3 => "[/Indexed /DeviceRGB 3 <ff0000 00ff00 0000ff ffffff>] setcolorspace\n",
        4 => "[/Separation /Spot /DeviceRGB {dup dup}] setcolorspace\n",
        _ => "[/CIEBasedABC << /RangeABC [0 1 0 1 0 1] >>] setcolorspace\n",
    });
    let (iw, ih) = (w.max(1), h.max(1));
    let image_matrix = match matrix {
        0 => format!("[{iw} 0 0 -{ih} 0 {ih}]"),
        1 => format!("[{iw} 0 0 {ih} 0 0]"),
        2 => format!("[0 {ih} {iw} 0 0 0]"),
        3 => "[0 0 0 0 0 0]".to_string(),
        4 => format!("[{iw} {ih} {ih} {iw} 1 1]"),
        _ => format!("[-{iw} 0 0 -{ih} {iw} {ih}]"),
    };
    let decode_arr = match decode {
        0 => String::new(),
        1 => "/Decode [1 0 ".to_string() + &"1 0 ".repeat(ncomp - 1) + "]",
        2 => "/Decode [0 1]".to_string(),
        3 => "/Decode [0.5 0.25 2 -1 100 0]".to_string(),
        _ => "/Decode [".to_string() + &"0 1 ".repeat(ncomp) + "]",
    };
    // How the samples arrive. `parts` is the number of independent sources
    // (one per component for the multi-source forms).
    let multi = matches!(form, 3 | 5);
    let parts = if multi { ncomp } else { 1 };
    let chunk = payload.len().div_ceil(parts).max(1);
    let mut inline = Vec::new();
    let mut sources = Vec::new();
    for part in 0..parts {
        let lo = (part * chunk).min(payload.len());
        let bytes = &payload[lo..(lo + chunk).min(payload.len())];
        sources.push(match filter {
            0 => hex(bytes),
            1 => format!("{{ {} }}", hex(bytes)),
            2 => format!("{{ {} }} bind", hex(bytes)),
            3 => {
                // Inline after the operator, through ASCIIHex.
                inline.extend(bytes.iter().flat_map(|b| format!("{b:02x}").into_bytes()));
                inline.push(b'>');
                "currentfile /ASCIIHexDecode filter".to_string()
            }
            4 => {
                // Raw bytes straight after the operator.
                inline.extend_from_slice(bytes);
                "currentfile".to_string()
            }
            n => {
                let name = [
                    "ASCIIHexDecode",
                    "ASCII85Decode",
                    "RunLengthDecode",
                    "FlateDecode",
                    "LZWDecode",
                    "DCTDecode",
                ][usize::from(n - 5)];
                format!("{} /{name} filter", hex(bytes))
            }
        });
    }
    let first = sources[0].clone();
    let dict_sources = if multi && form == 5 {
        format!(
            "/MultipleDataSources true /DataSource [ {} ]",
            sources.join(" ")
        )
    } else {
        format!("/DataSource {first}")
    };
    let dict = format!(
        "<< /ImageType 1 /Width {w} /Height {h} /BitsPerComponent {bits} \
         /ImageMatrix {image_matrix} {decode_arr} {dict_sources} >>"
    );
    match form {
        0 => {
            let _ = write!(p, "{w} {h} {bits} {image_matrix} {first} image");
        }
        1 => {
            let pol = if decode & 1 == 0 { "true" } else { "false" };
            let _ = write!(p, "{w} {h} {pol} {image_matrix} {first} imagemask");
        }
        2 | 3 => {
            let srcs = if multi {
                sources.join(" ")
            } else {
                first.clone()
            };
            let _ = write!(
                p,
                "{w} {h} {bits} {image_matrix} {srcs} {multi} {ncomp} colorimage"
            );
        }
        4 | 5 => {
            let _ = write!(p, "{dict} image");
        }
        6 => {
            let mask_decode = if decode & 1 == 0 { "[1 0]" } else { "[0 1]" };
            let mask_src = hex(&payload[..payload.len().min(64)]);
            let _ = write!(
                p,
                "<< /ImageType 3 /InterleaveType 3 /DataDict {dict} \
                 /MaskDict << /ImageType 1 /Width {mw} /Height {mh} /BitsPerComponent 1 \
                 /Decode {mask_decode} /ImageMatrix [{} 0 0 -{} 0 {}] /DataSource {mask_src} >> >> image",
                mw.max(1),
                mh.max(1),
                mh.max(1),
            );
        }
        _ => {
            let _ = write!(
                p,
                "<< /ImageType 1 /Width {w} /Height {h} /BitsPerComponent 1 /ImageMatrix {image_matrix} \
                 {decode_arr} {dict_sources} >> imagemask"
            );
        }
    }
    let mut out = p.into_bytes();
    out.push(b'\n');
    out.extend(inline);
    out
}
