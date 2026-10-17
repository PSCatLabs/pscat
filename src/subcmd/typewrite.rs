//! `pscat typewrite`: a text file in, a typewritten page out (#213;
//! ported from `scripts/typewrite.sh`, issue #600).
//!
//! Types the text with the /Typewriter font (`lib/typewriter.ps`):
//! word-wrapped to the margins, with tab stops, line spacing, underlines
//! and strikeovers by markup, an optional memo header and page numbers, in
//! a machine's own per-strike variation and ribbon ink. All business logic
//! lives in the PostScript library (`tw-type`); this builds the options
//! dict and renders it. `--height fit` runs a headless `tw-pageheight`
//! pre-pass first, so the page is as tall as its text. `--ps PATH` writes
//! the self-contained driver instead of rendering (it runs in Ghostscript
//! with `gs -dNOSAFER`).
//!
//! Markup in the text (no escapes needed): `<u>underlined</u>`,
//! `<x>crossed out</x>`, `<o>old|new</o>` (new overtyped on old),
//! `<r>redacted</r>`. Typographic quotes, dashes and the ellipsis become
//! their ASCII forms; any other non-ASCII character becomes `?`: the
//! machine has no such key.

use std::io::{IsTerminal, Read};

use super::{Args, Failure, Format, dpi, library, measure, number, page_dim, ps_escape, render};

const USAGE: &str = r#"usage: pscat typewrite [options] [FILE | -]      (text from FILE, stdin, or --text)

  -o, --out PATH        output: .png (default typed.png) or .pdf
      --text STR        the text, instead of FILE
      --reflow          join the lines of each paragraph (a paragraph ends at
                        a blank line), for text wrapped by an editor
      --ps PATH         write the self-contained PostScript driver to PATH
                        and stop (it runs in Ghostscript: gs -dNOSAFER)

  page
      --width N         page width in points (default 612)
      --height N|fit    page height in points, or "fit" to size the page to
                        its text (default 792); longer text runs onto more
                        pages (PNG: out-001.png, out-002.png, ...)
      --margin N        all four margins in points (default 72)
      --margin-left N  --margin-right N  --margin-top N  --margin-bottom N
      --spacing F       1 single, 1.5, 2 double (default 1)
      --indent N        first-line paragraph indent in columns (default 0)
      --tabs N|A,B,C    a stop every N columns, or at these columns (default 5)
      --page-numbers P  none | top | bottom (default none)

  memo header (any of these types one above the text)
      --memo            a header with just the title and rule
      --memo-to S  --memo-from S  --memo-cc S  --memo-subject S  --memo-date S
      --memo-title S    default MEMORANDUM; "" for none
      --no-rule         leave out the underscore rule under the header

  machine
      --pitch N         10 or pica, 12 or elite (default pica)
      --size N          type size in points (default 12.5)
      --machine N       the machine: its keys' fixed quirks (default 1)
      --seed N          the document: same seeds, same page (default 1)
      --wear F          0 perfect .. 1 good manual .. 1.5 old (default 1)
      --preset P        manual | electric | worn
      --ink R,G,B       ribbon colour, components 0..1
      --ribbon F        ribbon wetness 0..1 (default 0.7)
      --dpi N           render resolution (default 72)
  -h, --help            this text

Without --reflow newlines break lines; a blank line stays blank."#;

#[derive(Default)]
struct Opts {
    text: Vec<u8>,
    have_text: bool,
    reflow: bool,
    file: Option<String>,
    out: String,
    ps_out: Option<String>,
    width: String,
    height: String,
    margin: Option<String>,
    margin_left: Option<String>,
    margin_right: Option<String>,
    margin_top: Option<String>,
    margin_bottom: Option<String>,
    spacing: Option<String>,
    indent: Option<String>,
    tabs: Option<String>,
    page_numbers: Option<String>,
    memo: bool,
    memo_to: Vec<u8>,
    memo_from: Vec<u8>,
    memo_cc: Vec<u8>,
    memo_subject: Vec<u8>,
    memo_date: Vec<u8>,
    memo_title: Option<Vec<u8>>,
    no_rule: bool,
    pitch: Option<String>,
    size: Option<String>,
    machine: Option<String>,
    seed: Option<String>,
    wear: Option<String>,
    preset: Option<String>,
    ink: Option<String>,
    ribbon: Option<String>,
    dpi: f32,
}

pub(super) fn run(args: Args) -> Result<(), Failure> {
    let usage = |m: String| Failure::Usage(m, USAGE);
    let Some(mut o) = parse(args).map_err(usage)? else {
        println!("{USAGE}");
        return Err(Failure::Done);
    };

    if !o.have_text {
        if let Some(file) = o.file.as_deref().filter(|f| *f != "-") {
            o.text =
                std::fs::read(file).map_err(|_| Failure::Msg(format!("cannot read {file}")))?;
        } else if o.file.as_deref() == Some("-") || !std::io::stdin().is_terminal() {
            std::io::stdin()
                .lock()
                .read_to_end(&mut o.text)
                .map_err(|e| Failure::Msg(format!("cannot read stdin: {e}")))?;
        }
        // The script's shell variables could not hold a NUL.
        o.text.retain(|&b| b != 0);
    }
    if o.text.is_empty() && !o.memo {
        return Err(usage(
            "no text given (a FILE, - for stdin, or --text)".into(),
        ));
    }
    if o.reflow {
        o.text = reflow(&o.text);
    }

    let width = page_dim("--width", &o.width).map_err(usage)?;
    let mut libs = library("lib/typewriter_outlines.ps")?;
    libs.extend(library("lib/typewriter.ps")?);

    let height_text = if o.height == "fit" {
        // The page height that holds the text on one page. Width, not
        // height, decides the wrapping, so a placeholder height is fine.
        let mut fit = libs.clone();
        fit.extend(opts_dict(&o, "792"));
        fit.extend(b"TWOpts tw-pageheight =\n");
        let printed = measure(&fit)?;
        let printed = printed.trim_end_matches('\n');
        if printed.is_empty() || !printed.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Failure::Msg(format!(
                "could not size the page (tw-pageheight gave: {printed})"
            )));
        }
        printed.to_string()
    } else {
        o.height.clone()
    };
    let height = page_dim("--height", &height_text).map_err(usage)?;

    // The page size is declared in the driver, so `gs -dNOSAFER file.ps`
    // needs no -g.
    let mut driver = libs;
    driver.extend(
        format!(
            "/setpagedevice where {{ pop << /PageSize [ {width} {height} ] >> setpagedevice }} if\n"
        )
        .into_bytes(),
    );
    driver.extend(opts_dict(&o, &height_text));
    driver.extend(b"TWOpts tw-type showpage\n");

    if let Some(ps) = &o.ps_out {
        std::fs::write(ps, &driver).map_err(|e| Failure::Msg(format!("cannot write {ps}: {e}")))?;
        eprintln!(
            "pscat typewrite: wrote {ps} ({width}x{height} points; run with gs -dNOSAFER, or pscat --page {width}x{height})"
        );
        return Ok(());
    }

    let format = if o.out.ends_with(".pdf") || o.out.ends_with(".PDF") {
        Format::Pdf
    } else {
        Format::Png
    };
    render(&driver, (width, height), o.dpi, false, format, &o.out)
}

fn parse(mut args: Args) -> Result<Option<Opts>, String> {
    let mut o = Opts {
        out: "typed.png".into(),
        width: "612".into(),
        height: "792".into(),
        dpi: 72.0,
        ..Opts::default()
    };
    let mut only_positional = false;
    let num = |flag: &str, v: String| number(flag, &v).map(str::to_string);
    let field = |v: String| v.into_bytes();
    while let Some(arg) = args.next() {
        if only_positional {
            set_file(&mut o, arg)?;
            continue;
        }
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "-o" | "--out" => o.out = args.value(&arg)?,
            "--text" => {
                o.text = args.value(&arg)?.into_bytes();
                o.have_text = true;
            }
            "--ps" => o.ps_out = Some(args.value(&arg)?),
            "--reflow" => o.reflow = true,
            "--width" => o.width = num(&arg, args.value(&arg)?)?,
            "--height" => {
                let v = args.value(&arg)?;
                o.height = if v == "fit" { v } else { num(&arg, v)? };
            }
            "--margin" => o.margin = Some(num(&arg, args.value(&arg)?)?),
            "--margin-left" => o.margin_left = Some(num(&arg, args.value(&arg)?)?),
            "--margin-right" => o.margin_right = Some(num(&arg, args.value(&arg)?)?),
            "--margin-top" => o.margin_top = Some(num(&arg, args.value(&arg)?)?),
            "--margin-bottom" => o.margin_bottom = Some(num(&arg, args.value(&arg)?)?),
            "--spacing" => o.spacing = Some(num(&arg, args.value(&arg)?)?),
            "--indent" => o.indent = Some(num(&arg, args.value(&arg)?)?),
            "--tabs" => {
                let v = args.value(&arg)?;
                let ok = !v.is_empty()
                    && v.split(',')
                        .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
                if !ok {
                    return Err(format!("--tabs needs N or A,B,C columns, got: {v}"));
                }
                o.tabs = Some(v);
            }
            "--page-numbers" => {
                let v = args.value(&arg)?;
                if !matches!(v.as_str(), "none" | "top" | "bottom") {
                    return Err(format!(
                        "--page-numbers must be none, top or bottom, got: {v}"
                    ));
                }
                o.page_numbers = Some(v);
            }
            "--memo" => o.memo = true,
            "--memo-to" => {
                o.memo_to = field(args.value(&arg)?);
                o.memo = true;
            }
            "--memo-from" => {
                o.memo_from = field(args.value(&arg)?);
                o.memo = true;
            }
            "--memo-cc" => {
                o.memo_cc = field(args.value(&arg)?);
                o.memo = true;
            }
            "--memo-subject" => {
                o.memo_subject = field(args.value(&arg)?);
                o.memo = true;
            }
            "--memo-date" => {
                o.memo_date = field(args.value(&arg)?);
                o.memo = true;
            }
            "--memo-title" => {
                o.memo_title = Some(field(args.value(&arg)?));
                o.memo = true;
            }
            "--no-rule" => {
                o.no_rule = true;
                o.memo = true;
            }
            "--pitch" => {
                let v = args.value(&arg)?;
                o.pitch = Some(match v.as_str() {
                    "pica" => "10".into(),
                    "elite" => "12".into(),
                    _ => num(&arg, v)?,
                });
            }
            "--size" => o.size = Some(num(&arg, args.value(&arg)?)?),
            "--machine" => o.machine = Some(num(&arg, args.value(&arg)?)?),
            "--seed" => o.seed = Some(num(&arg, args.value(&arg)?)?),
            "--wear" => o.wear = Some(num(&arg, args.value(&arg)?)?),
            "--preset" => {
                let v = args.value(&arg)?;
                if !matches!(v.as_str(), "manual" | "electric" | "worn") {
                    return Err(format!(
                        "--preset must be manual, electric or worn, got: {v}"
                    ));
                }
                o.preset = Some(v);
            }
            "--ink" => o.ink = Some(super::rgb(&arg, &args.value(&arg)?)?),
            "--ribbon" => o.ribbon = Some(num(&arg, args.value(&arg)?)?),
            "--dpi" => o.dpi = dpi(&args.value(&arg)?)?,
            "--" => only_positional = true,
            "-" => set_file(&mut o, arg)?,
            _ if arg.starts_with('-') => return Err(format!("unknown option: {arg}")),
            _ => set_file(&mut o, arg)?,
        }
    }
    Ok(Some(o))
}

fn set_file(o: &mut Opts, file: String) -> Result<(), String> {
    if o.file.is_some() {
        return Err("more than one input file".into());
    }
    o.file = Some(file);
    Ok(())
}

/// A paragraph's lines joined by a space; its first line keeps any indent.
/// A paragraph ends at a blank (or whitespace-only) line, which is kept as
/// an empty line. Every output line ends in a newline.
fn reflow(text: &[u8]) -> Vec<u8> {
    let mut lines: Vec<&[u8]> = text.split(|&b| b == b'\n').collect();
    if lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    let blank = |l: &[u8]| l.iter().all(|&b| b == b' ' || b == b'\t');
    let mut out = Vec::with_capacity(text.len() + 1);
    let mut para: Vec<u8> = Vec::new();
    let flush = |para: &mut Vec<u8>, out: &mut Vec<u8>| {
        if !para.is_empty() {
            out.extend_from_slice(para);
            out.push(b'\n');
            para.clear();
        }
    };
    for line in lines {
        if blank(line) {
            flush(&mut para, &mut out);
            out.push(b'\n');
        } else if para.is_empty() {
            para.extend_from_slice(line);
        } else {
            let start = line
                .iter()
                .position(|&b| b != b' ' && b != b'\t')
                .unwrap_or(line.len());
            para.push(b' ');
            para.extend_from_slice(&line[start..]);
        }
    }
    flush(&mut para, &mut out);
    out
}

/// ASCII only: the typewriter has no other keys. Typographic punctuation
/// becomes its plain form, any other multi-byte character a `?`; the script's
/// `sed` passes, applied in the same order, on bytes.
fn ascii(text: &[u8]) -> Vec<u8> {
    fn replace(text: Vec<u8>, from: &[u8], with: &[u8], class: Option<&[u8]>) -> Vec<u8> {
        let n = from.len() + usize::from(class.is_some());
        let mut out = Vec::with_capacity(text.len());
        let mut i = 0;
        while i < text.len() {
            let hit = text[i..].len() >= n
                && text[i..].starts_with(from)
                && class.is_none_or(|c| c.contains(&text[i + from.len()]));
            if hit {
                out.extend_from_slice(with);
                i += n;
            } else {
                out.push(text[i]);
                i += 1;
            }
        }
        out
    }
    let t = text.to_vec();
    let t = replace(t, b"\xe2\x80", b"'", Some(b"\x98\x99"));
    let t = replace(t, b"\xe2\x80", b"\"", Some(b"\x9c\x9d"));
    let t = replace(t, b"\xe2\x80\x94", b"--", None);
    let t = replace(t, b"\xe2\x80\x93", b"-", None);
    let t = replace(t, b"\xe2\x80\xa6", b"...", None);
    let t = replace(t, b"\xe2\x80\xa2", b"*", None);
    let t = replace(t, b"\xc2\xa0", b" ", None);
    let t: Vec<u8> = t.into_iter().filter(|&b| b != b'\r').collect();
    // [\xc2-\xf4][\x80-\xbf]* -> ?
    let mut out = Vec::with_capacity(t.len());
    let mut i = 0;
    while i < t.len() {
        if (0xc2..=0xf4).contains(&t[i]) {
            i += 1;
            while i < t.len() && (0x80..=0xbf).contains(&t[i]) {
                i += 1;
            }
            out.push(b'?');
        } else {
            out.push(t[i]);
            i += 1;
        }
    }
    out
}

/// A PostScript string literal's contents: ASCII-only, `\ ( )` escaped,
/// newlines kept.
fn pstr(text: &[u8]) -> Vec<u8> {
    ps_escape(&ascii(text))
}

/// As [`pstr`], without trailing newlines: a memo field was a command
/// substitution in the script, which strips them.
fn pstr_field(text: &[u8]) -> Vec<u8> {
    let mut v = pstr(text);
    while v.last() == Some(&b'\n') {
        v.pop();
    }
    v
}

/// The options dictionary, as the script's `opts_dict` echoed it.
fn opts_dict(o: &Opts, height: &str) -> Vec<u8> {
    let mut d: Vec<u8> = Vec::new();
    let line = |d: &mut Vec<u8>, s: String| {
        d.extend(s.into_bytes());
        d.push(b'\n');
    };
    let opt = |d: &mut Vec<u8>, key: &str, v: &Option<String>| {
        if let Some(v) = v {
            d.extend(format!("  /{key} {v}\n").into_bytes());
        }
    };
    d.extend(b"/TWOpts <<\n  /Text (");
    d.extend(pstr(&o.text));
    d.extend(b")\n");
    line(&mut d, format!("  /Width {}  /Height {height}", o.width));
    opt(&mut d, "Margin", &o.margin);
    opt(&mut d, "MarginLeft", &o.margin_left);
    opt(&mut d, "MarginRight", &o.margin_right);
    opt(&mut d, "MarginTop", &o.margin_top);
    opt(&mut d, "MarginBottom", &o.margin_bottom);
    opt(&mut d, "Spacing", &o.spacing);
    opt(&mut d, "Indent", &o.indent);
    if let Some(t) = &o.tabs {
        if t.contains(',') {
            line(&mut d, format!("  /Tabs [ {} ]", t.replace(',', " ")));
        } else {
            line(&mut d, format!("  /Tabs {t}"));
        }
    }
    if let Some(p) = &o.page_numbers {
        line(&mut d, format!("  /PageNumbers /{p}"));
    }
    if o.memo {
        d.extend(b"  /Memo <<\n");
        let mut field = |key: &str, v: &[u8]| {
            if !v.is_empty() {
                d.extend(format!("    /{key} (").into_bytes());
                d.extend(pstr_field(v));
                d.extend(b")\n");
            }
        };
        field("To", &o.memo_to);
        field("From", &o.memo_from);
        field("Cc", &o.memo_cc);
        field("Subject", &o.memo_subject);
        field("Date", &o.memo_date);
        if let Some(t) = &o.memo_title {
            d.extend(b"    /Title (");
            d.extend(pstr_field(t));
            d.extend(b")\n");
        }
        if o.no_rule {
            d.extend(b"    /Rule false\n");
        }
        d.extend(b"  >>\n");
    }
    opt(&mut d, "Pitch", &o.pitch);
    opt(&mut d, "Size", &o.size);
    opt(&mut d, "Machine", &o.machine);
    opt(&mut d, "Seed", &o.seed);
    opt(&mut d, "Wear", &o.wear);
    if let Some(p) = &o.preset {
        line(&mut d, format!("  /Preset /{p}"));
    }
    if let Some(i) = &o.ink {
        line(&mut d, format!("  /Ink [ {i} ]"));
    }
    opt(&mut d, "Ribbon", &o.ribbon);
    d.extend(b">> def\n");
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reflow_joins_paragraph_lines_and_keeps_blank_ones() {
        assert_eq!(
            reflow(b"one\ntwo\n\n\tthree\n four"),
            b"one two\n\n\tthree four\n"
        );
        assert_eq!(reflow(b""), b"");
        assert_eq!(reflow(b"a\n  \t\nb\n"), b"a\n\nb\n");
        assert_eq!(reflow(b"a\n\n\n"), b"a\n\n\n");
    }

    #[test]
    fn ascii_maps_typography_and_replaces_the_rest() {
        let src =
            "\u{201c}Quoted\u{201d} it\u{2019}s \u{2014} caf\u{e9}\u{2026} \u{2022}\u{a0}x\r\n"
                .as_bytes();
        assert_eq!(ascii(src), b"\"Quoted\" it's -- caf?... * x\n");
        // a stray continuation byte is not a lead byte: it stays, as with sed
        assert_eq!(ascii(b"a\xffb\x80c"), b"a\xffb\x80c");
        assert_eq!(ascii("\u{4e2d}\u{6587}x".as_bytes()), b"??x");
        // an en dash, and a three-byte char that merely starts like a quote
        assert_eq!(ascii("\u{2013}\u{2018}".as_bytes()), b"-'");
        assert_eq!(ascii("\u{2020}".as_bytes()), b"?");
    }

    #[test]
    fn memo_fields_lose_trailing_newlines_text_keeps_them() {
        assert_eq!(pstr_field(b"To (me)\n\n"), b"To \\(me\\)");
        assert_eq!(pstr(b"a\n\n"), b"a\n\n");
    }
}
