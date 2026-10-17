//! `pscat photo-etch`: a photo in, a line engraving out (issue #15;
//! ported from `scripts/photo_etch.sh`, issue #600).
//!
//! Renders a tone-driven line engraving of a JPEG photo (`lib/etching.ps`),
//! the reproduction technique old newspapers and books used before halftone
//! screens, not edge detection. An `et-dims` pre-pass reads the photo's own
//! dimensions (from its SOF marker, no decode), sizing the output page to
//! match its aspect ratio; the real render then reads and hatches it. JPEG
//! input only: this interpreter has no PNG decode path in PostScript (see
//! `lib/etching.ps`'s header comment for why that is not a gap).

use super::{
    Args, Failure, Format, dpi, library, measure, number, page_dim, ps_escape, render, rgb,
};

const USAGE: &str = r#"usage: pscat photo-etch [options] photo.jpg

  -o, --out PATH     output file; format from extension: .png/.svg/.pdf
                      (default etching.png)
      --max-dim N     longest output page dimension, points (default 480)
      --spacing N     device units between hatch lines (default 3)
      --angle DEG     primary hatch angle (default -15)
      --threshold2 F  darkness above which crosshatch kicks in, 0..1
                      (default 0.55)
      --ink R,G,B     stroke color, components 0..1 (default 0.08,0.07,0.05)
      --paper R,G,B   background color, components 0..1 (default 0.96,0.94,0.88)
      --dpi N         render resolution (default 72)
  -h, --help          this text"#;

struct Opts {
    photo: String,
    out: String,
    max_dim: String,
    spacing: String,
    angle: String,
    threshold2: String,
    ink: String,
    paper: String,
    dpi: f32,
}

pub(super) fn run(args: Args) -> Result<(), Failure> {
    let usage = |m: String| Failure::Usage(m, USAGE);
    let Some(o) = parse(args).map_err(usage)? else {
        println!("{USAGE}");
        return Err(Failure::Done);
    };
    let max_dim = page_dim("--max-dim", &o.max_dim).map_err(usage)?;
    let format = match output_format(&o.out) {
        Some(f) => f,
        None => {
            return Err(usage(format!(
                "--out must end in .png, .svg, or .pdf, got: {}",
                o.out
            )));
        }
    };
    if !std::path::Path::new(&o.photo).is_file() {
        return Err(Failure::Msg(format!("no such file: {}", o.photo)));
    }
    // Absolute: a relative name would be looked up by the resolver
    // (`$PSCAT_HOME` first), not against the directory the user is in.
    // Not canonicalised: that adds a `\\?\` prefix on Windows.
    let abs = std::path::absolute(&o.photo)
        .map_err(|e| Failure::Msg(format!("cannot resolve {}: {e}", o.photo)))?;
    let abs = abs
        .to_str()
        .ok_or_else(|| Failure::Msg(format!("{}: path is not valid UTF-8", o.photo)))?;
    let mut photo = b"(".to_vec();
    photo.extend(ps_escape(abs.as_bytes()));
    photo.push(b')');

    let lib = library("lib/etching.ps")?;

    // Pre-pass: width, height and component count straight from the
    // JPEG's own SOF marker, so the page matches the photo's aspect ratio.
    let mut dims = lib.clone();
    dims.extend(b"<< /Photo ");
    dims.extend(&photo);
    dims.extend(b" >> et-dims\n/n exch def /h exch def /w exch def\nw == h == n ==\n");
    let printed = measure(&dims)?;
    let sizes: Vec<f64> = printed
        .split_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    let [w, h, _n] = sizes[..] else {
        return Err(Failure::Msg(format!(
            "could not read the photo's dimensions (et-dims gave: {printed:?})"
        )));
    };
    let (page_w, page_h) = page_for(w, h, f64::from(max_dim));
    let (page_w, page_h) = (
        page_dim("the computed page width", &page_w.to_string()).map_err(usage)?,
        page_dim("the computed page height", &page_h.to_string()).map_err(usage)?,
    );

    let mut driver = lib;
    driver.extend(b"<< /Photo ");
    driver.extend(&photo);
    driver.extend(
        format!(
            "\n   /PageWidth {page_w}\n   /PageHeight {page_h}\n   /Spacing {}\n   /Angle {}\n   \
             /Threshold2 {}\n   /Ink [ {} ]\n   /Paper [ {} ]\n>> et-draw showpage\n",
            o.spacing, o.angle, o.threshold2, o.ink, o.paper
        )
        .into_bytes(),
    );
    render(&driver, (page_w, page_h), o.dpi, false, format, &o.out)
}

/// The output page: the longer side `max_dim`, the other scaled to the
/// photo's aspect ratio and rounded, at least 1 (an extreme aspect ratio,
/// say 1000x1, would round the short side to 0, which a page cannot be).
fn page_for(w: f64, h: f64, max_dim: f64) -> (u64, u64) {
    let short = |a: f64, b: f64| ((max_dim * a / b + 0.5).trunc().max(1.0)) as u64;
    if w >= h {
        (max_dim as u64, short(h, w))
    } else {
        (short(w, h), max_dim as u64)
    }
}

fn output_format(out: &str) -> Option<Format> {
    let ext = std::path::Path::new(out)
        .extension()?
        .to_str()?
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some(Format::Png),
        "svg" => Some(Format::Svg),
        "pdf" => Some(Format::Pdf),
        _ => None,
    }
}

fn parse(mut args: Args) -> Result<Option<Opts>, String> {
    let mut o = Opts {
        photo: String::new(),
        out: "etching.png".into(),
        max_dim: "480".into(),
        spacing: "3".into(),
        angle: "-15".into(),
        threshold2: "0.55".into(),
        ink: "0.08 0.07 0.05".into(),
        paper: "0.96 0.94 0.88".into(),
        dpi: 72.0,
    };
    let num = |flag: &str, v: String| number(flag, &v).map(str::to_string);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "-o" | "--out" => o.out = args.value(&arg)?,
            "--max-dim" => o.max_dim = args.value(&arg)?,
            "--spacing" => o.spacing = num(&arg, args.value(&arg)?)?,
            "--angle" => o.angle = num(&arg, args.value(&arg)?)?,
            "--threshold2" => o.threshold2 = num(&arg, args.value(&arg)?)?,
            "--ink" => o.ink = rgb(&arg, &args.value(&arg)?)?,
            "--paper" => o.paper = rgb(&arg, &args.value(&arg)?)?,
            "--dpi" => o.dpi = dpi(&args.value(&arg)?)?,
            _ if arg.starts_with('-') => return Err(format!("unknown option: {arg}")),
            _ => {
                if !o.photo.is_empty() {
                    return Err("more than one photo argument".into());
                }
                o.photo = arg;
            }
        }
    }
    if o.photo.is_empty() {
        return Err("no photo given".into());
    }
    Ok(Some(o))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_follows_the_photos_aspect_ratio() {
        assert_eq!(page_for(8.0, 8.0, 480.0), (480, 480));
        assert_eq!(page_for(1600.0, 900.0, 480.0), (480, 270));
        assert_eq!(page_for(900.0, 1600.0, 480.0), (270, 480));
        // 480 * 2 / 3 = 320, 480 * 1 / 3 = 160
        assert_eq!(page_for(3.0, 2.0, 480.0), (480, 320));
        assert_eq!(page_for(3.0, 1.0, 480.0), (480, 160));
        // an extreme ratio still gets a 1-point short side
        assert_eq!(page_for(1000.0, 1.0, 480.0), (480, 1));
    }

    #[test]
    fn the_output_format_comes_from_the_extension() {
        assert_eq!(output_format("a.png"), Some(Format::Png));
        assert_eq!(output_format("a.SVG"), Some(Format::Svg));
        assert_eq!(output_format("dir.d/a.pdf"), Some(Format::Pdf));
        assert_eq!(output_format("a.jpg"), None);
        assert_eq!(output_format("png"), None);
    }
}
