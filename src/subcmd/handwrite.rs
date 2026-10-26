//! `pscat handwrite`: a string in, a handwritten PNG out (Stage 13;
//! ported from `scripts/handwrite.sh`, issue #600).
//!
//! Renders the text in the /HandScript dynamic font (`lib/handscript.ps`),
//! word-wrapped across lines like a person filling a page. The page height
//! is auto-sized: an `hs-linecount` pre-pass measures the wrap, then the
//! real render runs at exactly the height the text needs. The font is
//! lowercase-only, so ASCII capitals are lowercased; unknown characters
//! advance invisibly rather than erroring.

use super::{
    Args, Confine, Failure, Format, library, max_pixels, measure, number, page_dim, ps_escape,
    render, rgb, take_limit_flag,
};

const USAGE: &str = r#"usage: pscat handwrite [options] "text to write"

  -o, --out PATH     output PNG (default handwritten.png)
      --size N       font size in points (default 36)
      --width N      page width in points (default 612)
      --height N     page height in points (default: auto-sized to fit)
      --margin N     page margin in points (default 54)
      --leading F    baseline spacing, multiple of size (default 1.5)
      --jitter N     ink wobble in glyph units, 0=calm (default 16)
      --pen N        stroke width in glyph units (default 22)
      --ink R,G,B    ink color, components 0..1 (default 0.13,0.14,0.34)
      --paper STYLE  plain | ruled (default plain)
      --seed N       rand seed; same seed, same page (default 1985)
      --dpi N        render resolution (default 72)
      --halftone     screen the output like a mono laser printer
      --max-pixels N refuse a page of more than N device pixels (width x
                     height at --dpi) or over 8000 a side, before drawing;
                     1..64000000 (default: no cap beyond the page limits)
      --exit-with-parent PID  stop once PID is no longer pscat's parent
                     (Unix; pscat-mcp passes its own pid)
      --read-root DIR  let the render read files only inside DIR
                     (repeatable; pscat-mcp passes its read roots)
      --confine-reads  as --read-root, with no directories
      --max-steps N  stop a program after N interpreter steps (0 = no limit)
      --max-time SECS  stop a program after SECS of wall-clock time (0 = no limit)
      --max-memory MIB  stop a program that grows the heap by more than MIB
                     (0 = no limit). The limits apply to each of the two
                     programs this runs, the measuring pass and the render, not
                     to the pair; a stop exits non-zero with `limitexceeded`
                     (pscat-mcp passes them)
  -h, --help         this text

Newlines in the text force line breaks; blank lines are kept. A text that
starts with "-" goes after "--"."#;

struct Opts {
    text: String,
    out: String,
    size: String,
    width: String,
    height: Option<String>,
    margin: String,
    leading: String,
    jitter: String,
    pen: String,
    ink: String,
    paper: &'static str,
    seed: String,
    dpi: f32,
    halftone: bool,
    max_pixels: Option<u64>,
    exit_with_parent: Option<u32>,
    /// `--read-root`/`--confine-reads` (issue #763) and `--max-steps`,
    /// `--max-time`, `--max-memory` (issue #701).
    confine: Confine,
}

pub(super) fn run(args: Args) -> Result<(), Failure> {
    let usage = |m: String| Failure::Usage(m, USAGE);
    let Some(opts) = parse(args).map_err(usage)? else {
        println!("{USAGE}");
        return Err(Failure::Done);
    };
    if let Some(parent) = opts.exit_with_parent {
        crate::exit_with_parent(parent);
    }
    let lib = library("lib/handscript.ps")?;
    let width = page_dim("--width", &opts.width).map_err(usage)?;
    // `--max-pixels`: refuse a page over the cap, or over 8000 pixels a side
    // at this dpi, before the height is range-checked or anything is drawn.
    // The interpreter's own rounding (`Interp::with_page_scaled`).
    let too_big = |w_pt: u64, h_pt: u64| -> Result<(), Failure> {
        let Some(limit) = opts.max_pixels else {
            return Ok(());
        };
        let px = |pt: u64| ((pt as f32 * (opts.dpi / 72.0)).round() as u64).max(1);
        let (w, h) = (px(w_pt), px(h_pt));
        if w > 8000 || h > 8000 || w * h > limit {
            return Err(Failure::Msg(pscat::limits::pixel_cap_message(w, h, limit)));
        }
        Ok(())
    };
    let height = match &opts.height {
        Some(h) => {
            let h = page_dim("--height", h).map_err(usage)?;
            too_big(width.into(), h.into())?;
            h
        }
        None => {
            // Height doesn't affect wrapping, so a placeholder is fine here.
            let mut count = lib.clone();
            count.extend(opts_dict(&opts, "792"));
            count.extend(b"HandOpts hs-linecount =\n");
            let printed = measure(&count, &opts.confine)?;
            let lines: f64 = printed.trim().parse().map_err(|_| {
                Failure::Msg(format!(
                    "could not count lines (hs-linecount gave: {printed:?})"
                ))
            })?;
            let h = auto_height(
                lines,
                opts.margin.parse().unwrap_or(0.0),
                opts.size.parse().unwrap_or(0.0),
                opts.leading.parse().unwrap_or(0.0),
            );
            too_big(width.into(), h)?;
            page_dim("the computed page height", &h.to_string()).map_err(usage)?
        }
    };

    let mut driver = lib;
    driver.extend(opts_dict(&opts, &height.to_string()));
    driver.extend(b"HandOpts hs-write showpage\n");
    render(
        &driver,
        (width, height),
        opts.dpi,
        opts.halftone,
        Format::Png,
        &opts.out,
        &opts.confine,
    )
}

/// The page height that holds `lines` lines: two margins, a first line's
/// 1.05 em, and a leading step for each further line, rounded up. The
/// script's awk did this in doubles, in this order.
fn auto_height(lines: f64, margin: f64, size: f64, leading: f64) -> u64 {
    let n = if lines < 1.0 { 1.0 } else { lines };
    let h = 2.0 * margin + 1.05 * size + (n - 1.0) * leading * size;
    if h.is_finite() && h > 0.0 {
        h.ceil() as u64
    } else {
        0
    }
}

fn parse(mut args: Args) -> Result<Option<Opts>, String> {
    let mut o = Opts {
        text: String::new(),
        out: "handwritten.png".into(),
        size: "36".into(),
        width: "612".into(),
        height: None,
        margin: "54".into(),
        leading: "1.5".into(),
        jitter: "16".into(),
        pen: "22".into(),
        ink: "0.13 0.14 0.34".into(),
        paper: "plain",
        seed: "1985".into(),
        dpi: 72.0,
        halftone: false,
        max_pixels: None,
        exit_with_parent: None,
        confine: Confine::default(),
    };
    let mut have_text = false;
    let num = |flag: &str, v: String| number(flag, &v).map(str::to_string);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "-o" | "--out" => o.out = args.value(&arg)?,
            "--size" => o.size = num(&arg, args.value(&arg)?)?,
            "--width" => o.width = args.value(&arg)?,
            "--height" => o.height = Some(args.value(&arg)?),
            "--margin" => o.margin = num(&arg, args.value(&arg)?)?,
            "--leading" => o.leading = num(&arg, args.value(&arg)?)?,
            "--jitter" => o.jitter = num(&arg, args.value(&arg)?)?,
            "--pen" => o.pen = num(&arg, args.value(&arg)?)?,
            "--ink" => o.ink = rgb(&arg, &args.value(&arg)?)?,
            "--paper" => {
                o.paper = match args.value(&arg)?.as_str() {
                    "plain" => "plain",
                    "ruled" => "ruled",
                    other => return Err(format!("--paper must be plain or ruled, got: {other}")),
                }
            }
            "--seed" => o.seed = num(&arg, args.value(&arg)?)?,
            "--dpi" => o.dpi = super::dpi(&args.value(&arg)?)?,
            "--halftone" => o.halftone = true,
            "--max-pixels" => o.max_pixels = Some(max_pixels(&args.value(&arg)?)?),
            "--exit-with-parent" => {
                o.exit_with_parent = Some(crate::parent_pid_arg(&args.value(&arg)?)?)
            }
            "--read-root" => {
                let dir = args.value(&arg)?;
                o.confine.read_roots.get_or_insert_with(Vec::new).push(dir);
            }
            "--confine-reads" => {
                o.confine.read_roots.get_or_insert_with(Vec::new);
            }
            _ if take_limit_flag(&mut o.confine.limits, &arg, &mut args)? => {}
            "--" => {
                // The text follows; anything after it is ignored.
                o.text = args.next().unwrap_or_default();
                have_text = true;
                args.remaining();
            }
            _ if arg.starts_with('-') => return Err(format!("unknown option: {arg}")),
            _ => {
                if have_text || !o.text.is_empty() {
                    return Err("more than one text argument (quote your string)".into());
                }
                o.text = arg;
            }
        }
    }
    if o.text.is_empty() {
        return Err("no text given".into());
    }
    Ok(Some(o))
}

/// The options dictionary, as the script's heredoc wrote it. The text is
/// lowercased (ASCII: the font has no capitals), line-ending normalised
/// (a bare CR is a newline in a PostScript literal anyway), escaped for a
/// string literal, and stripped of trailing newlines (the script's command
/// substitution did).
fn opts_dict(o: &Opts, height: &str) -> Vec<u8> {
    let mut text = o
        .text
        .to_ascii_lowercase()
        .replace("\r\n", "\n")
        .into_bytes();
    for b in &mut text {
        if *b == b'\r' {
            *b = b'\n';
        }
    }
    let mut text = ps_escape(&text);
    while text.last() == Some(&b'\n') {
        text.pop();
    }
    let mut d = b"/HandOpts <<\n  /Text (".to_vec();
    d.extend(text);
    d.extend(
        format!(
            ")\n  /Size {}\n  /Width {}\n  /Height {height}\n  /Margin {}\n  /Leading {}\n  \
             /Jitter {}\n  /Pen {}\n  /Ink [ {} ]\n  /Paper /{}\n  /Seed {}\n>> def\n",
            o.size, o.width, o.margin, o.leading, o.jitter, o.pen, o.ink, o.paper, o.seed
        )
        .into_bytes(),
    );
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(args: &[&str]) -> Result<Option<Opts>, String> {
        parse(Args {
            rest: args
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .into_iter(),
        })
    }

    #[test]
    fn exit_with_parent_takes_a_pid() {
        let o = parsed(&["--exit-with-parent", "4242", "hi"])
            .unwrap()
            .unwrap();
        assert_eq!(o.exit_with_parent, Some(4242));
        assert!(parsed(&["--exit-with-parent", "me", "hi"]).is_err());
    }

    #[test]
    fn auto_height_matches_the_scripts_arithmetic() {
        // 2*54 + 1.05*36 + 0*1.5*36 = 145.8 -> 146
        assert_eq!(auto_height(1.0, 54.0, 36.0, 1.5), 146);
        assert_eq!(auto_height(0.0, 54.0, 36.0, 1.5), 146);
        // 2*54 + 37.8 + 2*54 = 253.8 -> 254
        assert_eq!(auto_height(3.0, 54.0, 36.0, 1.5), 254);
        // an exact value is not bumped
        assert_eq!(auto_height(1.0, 10.0, 20.0, 1.0), 41);
        // 20 + 10.5 + 10 = 40.5 -> 41
        assert_eq!(auto_height(2.0, 10.0, 10.0, 1.0), 41);
        assert_eq!(auto_height(2.0, 10.0, 20.0, 1.5), 20 + 21 + 30);
    }

    #[test]
    fn options_that_would_carry_code_are_refused() {
        for bad in [
            &["--ink", "1,2,3 ] def (x) =", "hi"][..],
            &["--size", "36 def", "hi"][..],
            &["--seed", "1)", "hi"][..],
            &["--paper", "plain /Text (x)", "hi"][..],
        ] {
            assert!(parsed(bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn text_handling() {
        assert!(parsed(&[]).is_err(), "no text");
        assert!(parsed(&["a", "b"]).is_err(), "two texts");
        assert!(parsed(&["-x"]).is_err(), "an option, not text");
        assert_eq!(parsed(&["--", "-x"]).unwrap().unwrap().text, "-x");
        assert!(parsed(&["--help"]).unwrap().is_none());
    }

    #[test]
    fn the_dict_lowercases_escapes_and_trims() {
        let o = parsed(&["Hi (There)\\\r\nOK\n\n"]).unwrap().unwrap();
        let d = String::from_utf8(opts_dict(&o, "792")).unwrap();
        assert!(d.starts_with("/HandOpts <<\n  /Text (hi \\(there\\)\\\\\nok)\n  /Size 36\n"));
        assert!(d.contains("  /Ink [ 0.13 0.14 0.34 ]\n  /Paper /plain\n  /Seed 1985\n>> def\n"));
    }
}
