//! The tool subcommands: `pscat handwrite`, `pscat typewrite` and
//! `pscat photo-etch` (issue #600), ports of `scripts/handwrite.sh`,
//! `scripts/typewrite.sh` and `scripts/photo_etch.sh`; and `pscat chalk`
//! (issue #555), a front over the `pscat::chalk` driver generator.
//!
//! Each builds a PostScript *driver* (the library's bytes followed by an
//! options dictionary and one call into the library) and renders it
//! through the same headless path as `pscat --png out file.ps`
//! (`run_headless` / `finish_headless`), so the output bytes are those the
//! script produced. The library comes from the resource resolver
//! (`pscat::paths`), i.e. the copy compiled into the binary unless a
//! checkout or `$PSCAT_HOME` says otherwise: nothing here needs a checkout,
//! bash, awk or sed, which is what makes the subcommands (and the MCP
//! server that calls them) work from a bare install, on Windows too.
//!
//! The scripts are thin wrappers over these now. What the scripts spliced
//! into the driver unchecked (`handwrite.sh`'s `--ink`, `--size`, ...) is
//! validated here: a numeric option must be a plain number, and the token
//! the user typed is what lands in the driver (`36` and `36.0` are an
//! integer and a real in PostScript, and the library may care).

use std::process::ExitCode;

use pscat::Interp;

use crate::Options;

mod chalk;
mod handwrite;
mod photo_etch;
mod typewrite;

/// Run the subcommand named by the first argument, or `None` if it names
/// none (the caller falls through to the flag CLI). A file literally named
/// `handwrite` is reached as `./handwrite`.
pub(crate) fn dispatch(mut args: impl Iterator<Item = String>) -> Option<ExitCode> {
    let first = args.next()?;
    let run: fn(Args) -> Result<(), Failure> = match first.as_str() {
        "handwrite" => handwrite::run,
        "typewrite" => typewrite::run,
        "photo-etch" => photo_etch::run,
        "chalk" => chalk::run,
        _ => return None,
    };
    let cmd = first;
    let args = Args {
        rest: args.collect::<Vec<_>>().into_iter(),
    };
    Some(match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Done) => ExitCode::SUCCESS,
        Err(Failure::Silent) => ExitCode::FAILURE,
        Err(Failure::Usage(msg, usage)) => {
            eprintln!("pscat {cmd}: {msg}");
            eprintln!();
            eprintln!("{usage}");
            ExitCode::FAILURE
        }
        Err(Failure::Msg(msg)) => {
            eprintln!("pscat {cmd}: {msg}");
            ExitCode::FAILURE
        }
    })
}

/// Why a subcommand stopped.
pub(crate) enum Failure {
    /// `--help` printed its text; not an error.
    Done,
    /// Already reported on stderr (an interpreter error report, a write error).
    Silent,
    /// A bad option or value: the message, then the usage text.
    Usage(String, &'static str),
    /// A runtime problem: the message.
    Msg(String),
}

/// The arguments after the subcommand name.
pub(crate) struct Args {
    rest: std::vec::IntoIter<String>,
}

impl Args {
    pub(crate) fn next(&mut self) -> Option<String> {
        self.rest.next()
    }

    /// The value after `flag`.
    pub(crate) fn value(&mut self, flag: &str) -> Result<String, String> {
        self.rest
            .next()
            .ok_or_else(|| format!("missing value after {flag}"))
    }

    /// Everything not yet consumed.
    pub(crate) fn remaining(&mut self) -> Vec<String> {
        self.rest.by_ref().collect()
    }
}

/// `s` if it is a plain decimal number (`36`, `-15`, `0.55`, `.5`, `1.`):
/// the one shape that is a PostScript number and nothing else, so a value
/// spliced into a driver cannot carry code.
pub(crate) fn number<'a>(flag: &str, s: &'a str) -> Result<&'a str, String> {
    let body = s.strip_prefix('-').unwrap_or(s);
    let (int, frac) = match body.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (body, None),
    };
    let digits = |t: &str| t.bytes().all(|b| b.is_ascii_digit());
    let ok = digits(int)
        && frac.is_none_or(digits)
        && (!int.is_empty() || frac.is_some_and(|f| !f.is_empty()));
    if ok {
        Ok(s)
    } else {
        Err(format!("{flag} needs a number, got: {s}"))
    }
}

/// `R,G,B`: three numbers, returned as the driver's `a b c`.
pub(crate) fn rgb(flag: &str, s: &str) -> Result<String, String> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 3 {
        return Err(format!("{flag} needs R,G,B, got: {s}"));
    }
    for p in &parts {
        number(flag, p).map_err(|_| format!("{flag} needs R,G,B numbers, got: {s}"))?;
    }
    Ok(parts.join(" "))
}

/// A page dimension in points: `--page`'s own limits (1..8000, whole).
pub(crate) fn page_dim(flag: &str, s: &str) -> Result<u32, String> {
    s.parse::<u32>()
        .ok()
        .filter(|n| (1..=8000).contains(n))
        .ok_or_else(|| format!("{flag} must be a whole number of points, 1..8000, got: {s}"))
}

/// `--dpi`, with the flag CLI's own range.
pub(crate) fn dpi(s: &str) -> Result<f32, String> {
    s.parse::<f32>()
        .ok()
        .filter(|d| (9.0..=1200.0).contains(d))
        .ok_or_else(|| format!("invalid --dpi value: {s} (expected 9..1200)"))
}

/// The bytes of a PostScript string literal's contents: `\`, `(` and `)`
/// backslash-escaped, everything else (newlines included) as it is.
/// Octal escapes are deliberately not used for control characters: pscat's
/// lexer reads a raw CR (or CRLF) in a literal as a newline while `\015`
/// stays a CR, so the string would change.
pub(crate) fn ps_escape(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 8);
    for &b in bytes {
        if matches!(b, b'\\' | b'(' | b')') {
            out.push(b'\\');
        }
        out.push(b);
    }
    out
}

/// A library's bytes through the resolver, e.g. `lib/handscript.ps`.
pub(crate) fn library(name: &str) -> Result<Vec<u8>, Failure> {
    pscat::paths::open(name)
        .map(|r| r.bytes.into_owned())
        .ok_or_else(|| Failure::Msg(format!("missing library file: {name}")))
}

/// Run a measuring program (`hs-linecount`, `tw-pageheight`, `et-dims`)
/// headlessly on a fresh default page and return what it printed. A fresh
/// interpreter, as the scripts' separate `pscat --headless` run was: the
/// real render must not inherit its random state.
pub(crate) fn measure(source: &[u8]) -> Result<String, Failure> {
    let mut interp = Interp::new();
    interp.capture_stdout();
    if let Err(e) = interp.run_source(source) {
        eprintln!("{}", interp.error_report(&e));
        return Err(Failure::Silent);
    }
    Ok(String::from_utf8_lossy(interp.captured_stdout()).into_owned())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Format {
    Png,
    Svg,
    Pdf,
}

/// The output format a `-o`/`--out` path asks for, from its extension
/// (case-insensitive): `.png`, `.svg` or `.pdf`. Anything else, or no
/// extension, is an error naming the choices, rather than a guess that
/// writes one format's bytes into a file named for another.
pub(crate) fn output_format(out: &str) -> Result<Format, String> {
    let ext = std::path::Path::new(out)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("png") => Ok(Format::Png),
        Some("svg") => Ok(Format::Svg),
        Some("pdf") => Ok(Format::Pdf),
        _ => Err(format!("--out must end in .png, .svg, or .pdf, got: {out}")),
    }
}

/// Render `source` on a `page`-point canvas at `dpi` and write it to
/// `out`, as `pscat --page WxH --dpi D [--halftone] --png|--svg|--pdf OUT
/// file.ps` does. The file is written even after a PostScript error (a
/// partial canvas), and the run then fails.
pub(crate) fn render(
    source: &[u8],
    page: (u32, u32),
    dpi: f32,
    halftone: bool,
    format: Format,
    out: &str,
) -> Result<(), Failure> {
    let mut options = Options::new();
    options.page = page;
    options.dpi = dpi;
    options.halftone = halftone;
    match format {
        Format::Png => options.png = Some(out.to_string()),
        Format::Svg => options.svg = Some(out.to_string()),
        Format::Pdf => options.pdf = Some(out.to_string()),
    }
    let Some(mut interp) = Interp::with_page_scaled(page.0, page.1, dpi / 72.0) else {
        return Err(Failure::Msg(format!(
            "unusable page size {}x{}",
            page.0, page.1
        )));
    };
    match format {
        Format::Png => {}
        Format::Svg => interp.gfx_mut().enable_svg(),
        Format::Pdf => {
            interp.gfx_mut().enable_pdf();
            let (title, author) = pscat::pdf::scan_document_info(source);
            interp.gfx_mut().set_pdf_info(title, author);
        }
    }
    let ok = crate::run_headless(&mut interp, source, &options);
    let declared = pscat::lint::scan_declared_pages(source);
    match crate::finish_headless(ok, &interp, &options, declared) {
        code if code == ExitCode::SUCCESS => Ok(()),
        _ => Err(Failure::Silent),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_output_format_comes_from_the_extension() {
        assert_eq!(output_format("a.png"), Ok(Format::Png));
        assert_eq!(output_format("a.SVG"), Ok(Format::Svg));
        assert_eq!(output_format("dir.d/a.pdf"), Ok(Format::Pdf));
        assert_eq!(output_format("a.PDF"), Ok(Format::Pdf));
        for bad in [
            "a.jpg",
            "png",
            "memo",
            "a.txt",
            "a.png.bak",
            "dir.png/a",
            "",
        ] {
            let e = output_format(bad).unwrap_err();
            assert!(e.contains(".png, .svg, or .pdf"), "{bad:?}: {e}");
            assert!(e.ends_with(&format!("got: {bad}")), "{bad:?}: {e}");
        }
    }

    #[test]
    fn number_accepts_only_plain_decimals() {
        for ok in ["36", "-15", "0.55", ".5", "1.", "0", "-0.5"] {
            assert_eq!(number("--x", ok), Ok(ok), "{ok}");
        }
        for bad in [
            "", "-", ".", "+1", "1e3", "1 2", "1,2", "0x10", "/Name", "1)", "1.2.3", "--1", "1\n",
            "36 def",
        ] {
            assert!(number("--x", bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn rgb_needs_three_numbers_and_cannot_carry_code() {
        assert_eq!(rgb("--ink", "0.13,0.14,0.34").unwrap(), "0.13 0.14 0.34");
        assert_eq!(rgb("--ink", "1,0,.5").unwrap(), "1 0 .5");
        for bad in [
            "1,2",
            "1,2,3,4",
            "1,2,3 ] def (x) =",
            "a,b,c",
            "",
            "1,,2",
            "1 2 3",
        ] {
            assert!(rgb("--ink", bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn page_dim_and_dpi_keep_the_cli_limits() {
        assert_eq!(page_dim("--width", "612"), Ok(612));
        for bad in ["0", "8001", "-1", "6.5", "x", ""] {
            assert!(page_dim("--width", bad).is_err(), "{bad:?}");
        }
        assert_eq!(dpi("144"), Ok(144.0));
        assert!(dpi("8").is_err() && dpi("1201").is_err() && dpi("x").is_err());
    }

    #[test]
    fn ps_escape_covers_the_three_string_metacharacters() {
        assert_eq!(ps_escape(b"a(b)c\\d\ne"), b"a\\(b\\)c\\\\d\ne");
        assert_eq!(ps_escape(b""), b"");
    }

    #[test]
    fn the_escaped_text_reads_back_as_the_original() {
        let nasty = b"a) (b \\) 100 0 moveto (c\\\n)(".to_vec();
        let mut src = b"(".to_vec();
        src.extend(ps_escape(&nasty));
        src.extend(b") dup length =");
        let mut it = Interp::new();
        it.capture_stdout();
        it.run_source(&src).unwrap();
        assert_eq!(
            String::from_utf8_lossy(it.captured_stdout()).trim(),
            nasty.len().to_string()
        );
    }
}
