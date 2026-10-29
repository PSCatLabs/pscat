//! `pscat photo-etch`: a photo in, a line engraving out (issue #15;
//! ported from `scripts/photo_etch.sh`, issue #600).
//!
//! Renders a tone-driven line engraving of a JPEG photo (`lib/etching.ps`),
//! the reproduction technique old newspapers and books used before halftone
//! screens, not edge detection. An `et-dims` pre-pass reads the photo's own
//! dimensions (from its SOF marker, no decode), sizing the output page to
//! match its aspect ratio; the real render then reads and hatches it. JPEG
//! input only: this interpreter has no PNG decode path in PostScript (see
//! `lib/etching.ps`'s header comment for why that is not a gap), so any other
//! format is refused up front by its first bytes (#939).

use super::{
    Args, Confine, Failure, dpi, library, measure, number, output_format, page_dim, ps_escape,
    render, rgb, take_limit_flag,
};

const USAGE: &str = r#"usage: pscat photo-etch [options] photo.jpg

  The photo must be a JPEG; a PNG or other format is refused (convert it first).

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
      --max-steps N     stop a program after N interpreter steps (0 = no limit)
      --max-time SECS   stop a program after SECS of wall-clock time (0 = no limit)
      --max-memory MIB  stop a program that grows the heap by more than MIB
                        (0 = no limit). They apply to each of the two programs
                        this runs, the dimensions pass and the render, not to the
                        pair; a stop exits non-zero with `limitexceeded`
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
    /// `--max-steps`, `--max-time`, `--max-memory` (issue #701).
    confine: Confine,
}

pub(super) fn run(args: Args) -> Result<(), Failure> {
    let usage = |m: String| Failure::Usage(m, USAGE);
    let Some(o) = parse(args).map_err(usage)? else {
        println!("{USAGE}");
        return Err(Failure::Done);
    };
    let max_dim = page_dim("--max-dim", &o.max_dim).map_err(usage)?;
    let format = output_format(&o.out).map_err(usage)?;
    if !std::path::Path::new(&o.photo).is_file() {
        return Err(Failure::Msg(format!("no such file: {}", o.photo)));
    }
    if let Some(found) = not_a_jpeg(&o.photo) {
        return Err(Failure::Msg(format!(
            "{} is {found}; only JPEG photos are supported (convert it to JPEG first)",
            o.photo
        )));
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
    let printed = measure(&dims, &o.confine)?;
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
    render(
        &driver,
        (page_w, page_h),
        o.dpi,
        false,
        format,
        &o.out,
        &o.confine,
    )
}

/// What a file is, when its first bytes are not a JPEG's: the library's own
/// check (`et-scan`) fails on a PNG with an internal procedure name, which
/// tells the user nothing. `None` for a JPEG (`FF D8`, all `et-scan` looks
/// at, so this is never stricter than the library) or a file that cannot be
/// read, which the render reports as before.
fn not_a_jpeg(photo: &str) -> Option<String> {
    use std::io::Read as _;
    let mut head = [0u8; 12];
    let n = std::fs::File::open(photo).ok()?.read(&mut head).ok()?;
    sniff_non_jpeg(&head[..n])
}

fn sniff_non_jpeg(head: &[u8]) -> Option<String> {
    if head.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let kind = if head.is_empty() {
        "an empty file"
    } else if head.starts_with(b"\x89PNG") {
        "a PNG"
    } else if head.starts_with(b"GIF8") {
        "a GIF"
    } else if head.starts_with(b"BM") {
        "a BMP"
    } else if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WEBP") {
        "a WebP"
    } else if head.starts_with(b"II*\0") || head.starts_with(b"MM\0*") {
        "a TIFF"
    } else if head.get(4..8) == Some(b"ftyp") {
        "an ISO media file (HEIC, AVIF or similar)"
    } else if head.starts_with(b"%PDF") {
        "a PDF"
    } else {
        "not a JPEG (it does not start with the JPEG marker)"
    };
    Some(kind.to_string())
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
        confine: Confine::default(),
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
            _ if take_limit_flag(&mut o.confine.limits, &arg, &mut args)? => {}
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
}
