//! `pscat handwrite`: a string in, a handwriting-style PNG out (Stage 13;
//! ported from `scripts/handwrite.sh`, issue #600).
//!
//! Renders the text in the /HandScript dynamic font (`lib/handscript.ps`),
//! word-wrapped across lines like a person filling a page. The page height
//! is auto-sized: an `hs-linecount` pre-pass measures the wrap, then the
//! real render runs at exactly the height the text needs. The font is
//! lowercase-only, so ASCII capitals are lowercased; unknown characters
//! advance invisibly rather than erroring.

use pscat::handwriting::{HandOptions, Paper, measure_source, parse_line_count, render_source};

use super::{
    Args, Confine, Failure, Format, check_output_name, max_pixels, measure, number, page_dim,
    render, rgb, take_limit_flag,
};

const USAGE: &str = r#"usage: pscat handwrite [options] "text to write"

  Hand-drawn styles for notes, cards and art, not for imitating a
  specific person's handwriting.

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
    out: String,
    /// The note: the text and the look, in the library's own types.
    hand: HandOptions,
    /// `--width` as given: it is range-checked after the output name, so an
    /// error there keeps its place.
    width: String,
    height: Option<String>,
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
    let Some(mut opts) = parse(args).map_err(usage)? else {
        println!("{USAGE}");
        return Err(Failure::Done);
    };
    // Before the library is read or anything measured: `-o` is always a PNG,
    // so a name for another image or document type is refused (#938).
    check_output_name("-o", &opts.out, Format::Png).map_err(usage)?;
    if let Some(parent) = opts.exit_with_parent {
        crate::exit_with_parent(parent);
    }
    let width = page_dim("--width", &opts.width).map_err(usage)?;
    opts.hand.width = width;
    // The library's own range checks; its field names are the flag names.
    opts.hand.validate().map_err(|e| usage(format!("--{e}")))?;
    let hand = &opts.hand;
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
            // Height doesn't affect wrapping, so the measuring program needs none.
            let printed = measure(measure_source(hand).as_bytes(), &opts.confine)?;
            let lines = parse_line_count(&printed).ok_or_else(|| {
                Failure::Msg(format!(
                    "could not count lines (hs-linecount gave: {printed:?})"
                ))
            })?;
            let h = hand.height_pt(lines) as u64;
            too_big(width.into(), h)?;
            page_dim("the computed page height", &h.to_string()).map_err(usage)?
        }
    };

    render(
        render_source(hand, height).as_bytes(),
        (width, height),
        opts.dpi,
        opts.halftone,
        Format::Png,
        &opts.out,
        &opts.confine,
    )
}

/// A `--size`-style value: a plain decimal number, as an `f64`.
fn float(flag: &str, v: &str) -> Result<f64, String> {
    number(flag, v)?
        .parse()
        .map_err(|_| format!("{flag} needs a number, got: {v}"))
}

/// A whole-number value (`--jitter`, `--seed`): the font and `srand` take
/// integers, so a fraction is refused here instead of failing mid-render.
fn whole<T: std::str::FromStr>(flag: &str, v: &str) -> Result<T, String> {
    number(flag, v)?
        .parse()
        .map_err(|_| format!("{flag} needs a whole number in range, got: {v}"))
}

fn parse(mut args: Args) -> Result<Option<Opts>, String> {
    let mut o = Opts {
        out: "handwritten.png".into(),
        hand: HandOptions::default(),
        width: pscat::handwriting::DEFAULT_WIDTH.to_string(),
        height: None,
        dpi: 72.0,
        halftone: false,
        max_pixels: None,
        exit_with_parent: None,
        confine: Confine::default(),
    };
    let mut have_text = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "-o" | "--out" => o.out = args.value(&arg)?,
            "--size" => o.hand.size = float(&arg, &args.value(&arg)?)?,
            "--width" => o.width = args.value(&arg)?,
            "--height" => o.height = Some(args.value(&arg)?),
            "--margin" => o.hand.margin = float(&arg, &args.value(&arg)?)?,
            "--leading" => o.hand.leading = float(&arg, &args.value(&arg)?)?,
            "--jitter" => o.hand.jitter = whole(&arg, &args.value(&arg)?)?,
            "--pen" => o.hand.pen = float(&arg, &args.value(&arg)?)?,
            "--ink" => {
                let ink = rgb(&arg, &args.value(&arg)?)?;
                for (slot, part) in o.hand.ink.iter_mut().zip(ink.split(' ')) {
                    *slot = float(&arg, part)?;
                }
            }
            "--paper" => {
                o.hand.paper = args
                    .value(&arg)?
                    .parse::<Paper>()
                    .map_err(|e| format!("--{e}"))?
            }
            "--seed" => o.hand.seed = whole(&arg, &args.value(&arg)?)?,
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
                o.hand.text = args.next().unwrap_or_default();
                have_text = true;
                args.remaining();
            }
            _ if arg.starts_with('-') => return Err(format!("unknown option: {arg}")),
            _ => {
                if have_text || !o.hand.text.is_empty() {
                    return Err("more than one text argument (quote your string)".into());
                }
                o.hand.text = arg;
            }
        }
    }
    if o.hand.text.is_empty() {
        return Err("no text given".into());
    }
    Ok(Some(o))
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
    fn options_that_would_carry_code_are_refused() {
        for bad in [
            &["--ink", "1,2,3 ] def (x) =", "hi"][..],
            &["--size", "36 def", "hi"][..],
            &["--seed", "1)", "hi"][..],
            &["--paper", "plain /Text (x)", "hi"][..],
            &["--jitter", "16.5", "hi"][..],
            &["--seed", "3000000000", "hi"][..],
        ] {
            assert!(parsed(bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn no_flags_gives_the_library_defaults() {
        let o = parsed(&["hi"]).unwrap().unwrap();
        assert_eq!(o.hand, HandOptions::new("hi"));
        assert_eq!(o.width, "612");
    }

    #[test]
    fn every_flag_lands_in_the_library_options() {
        let o = parsed(&[
            "--size",
            "20",
            "--margin",
            "10",
            "--leading",
            "1.25",
            "--jitter",
            "3",
            "--pen",
            "9.5",
            "--ink",
            "1,0,.5",
            "--paper",
            "ruled",
            "--seed",
            "-4",
            "hi",
        ])
        .unwrap()
        .unwrap();
        let mut want = HandOptions::new("hi");
        want.size = 20.0;
        want.margin = 10.0;
        want.leading = 1.25;
        want.jitter = 3;
        want.pen = 9.5;
        want.ink = [1.0, 0.0, 0.5];
        want.paper = Paper::Ruled;
        want.seed = -4;
        assert_eq!(o.hand, want);
    }

    #[test]
    fn the_page_limit_is_the_librarys() {
        use pscat::handwriting::MAX_PAGE_POINTS;
        assert_eq!(
            page_dim("--width", &MAX_PAGE_POINTS.to_string()),
            Ok(MAX_PAGE_POINTS)
        );
        assert!(page_dim("--width", &(MAX_PAGE_POINTS + 1).to_string()).is_err());
        assert!(
            page_dim(
                "--width",
                &(pscat::handwriting::MIN_PAGE_POINTS - 1).to_string()
            )
            .is_err()
        );
    }

    #[test]
    fn text_handling() {
        assert!(parsed(&[]).is_err(), "no text");
        assert!(parsed(&["a", "b"]).is_err(), "two texts");
        assert!(parsed(&["-x"]).is_err(), "an option, not text");
        assert_eq!(parsed(&["--", "-x"]).unwrap().unwrap().hand.text, "-x");
        assert!(parsed(&["--help"]).unwrap().is_none());
    }
}
