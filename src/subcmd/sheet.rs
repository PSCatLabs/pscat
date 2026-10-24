//! `pscat sheet`: a labelled review sheet from several files at several
//! sizes (issue #693).
//!
//! `--contact-sheet` composites one program's sweep frames, which are all
//! the same size and unlabelled. A review sheet, such as logo candidates
//! each at 512 px and at 48 px with their names, mixes files, page sizes
//! and dpis, and needs captions. Each file given here is a row; each of
//! its dpis is a cell, drawn at its own pixel size (a 48 px render stays
//! 48 px beside a 512 px one) and centred in its column. Labels are set
//! with pscat's own text rendering (`pscat::contact_sheet::render_text`;
//! the face is picked per character by script), and the geometry is
//! `pscat::contact_sheet::Layout`.

use pscat::Interp;
use pscat::contact_sheet::{self as sheet, CellSize, Layout, RowSize};
use tiny_skia::Pixmap;

use super::{Args, Failure, dpi, page_dim};

const USAGE: &str = r#"usage: pscat sheet [options] FILE.ps [FILE.ps ...]

Render each FILE as a row of cells and compose them into one labelled PNG.

  -o, --out PATH     output PNG (default contact-sheet.png)
      --page WxH     page size in points for the files that follow (default 612x792)
      --dpi LIST     comma-separated cells per file, for the files that follow
                     (default 72): 96,9 draws each file at 96 and at 9 dpi; 9x4
                     draws the 9 dpi render again enlarged 4x with no smoothing
                     (whole factors 2..16), so a small icon can be read pixel by pixel
      --label TEXT   the next file's row label (default: its file name without .ps);
                     Latin with accents, Korean, Japanese, Chinese and Thai are
                     drawn in a face for the script (the last three need the
                     cjk-thai font pack: `pscat fonts install`)
      --no-labels    omit the row labels
      --no-sizes     omit the pixel-size caption under each cell
  -h, --help         this text

--page and --dpi stay in force for every file after them until given again, so
one sheet can mix page sizes. Every cell is drawn at its true pixel size,
centred in its column, with a hairline around it; nothing is scaled unless an
entry in --dpi says so. A cell whose program fails is still drawn, as far as it
got, and the run exits non-zero. The sheet is at most 8000 px on a side.

Example (four logo candidates, each at 512 px and 48 px):
  pscat sheet -o sheet.png --page 384x384 --dpi 96,9,9x4 \
      --label "A: one-line cat" a.ps --label "B: drawing board" b.ps"#;

/// Most cells one sheet may ask for.
const MAX_CELLS: usize = 256;
/// Largest `--dpi` zoom factor.
const MAX_ZOOM: u32 = 16;
/// Row label and caption type sizes, in pixels.
const LABEL_PX: u32 = 16;
const CAPTION_PX: u32 = 12;

struct Entry {
    path: String,
    label: Option<String>,
    page: (u32, u32),
    /// `(dpi, zoom)` per cell.
    cells: Vec<(f32, u32)>,
}

struct Opts {
    out: String,
    entries: Vec<Entry>,
    labels: bool,
    sizes: bool,
}

pub(super) fn run(args: Args) -> Result<(), Failure> {
    let usage = |m: String| Failure::Usage(m, USAGE);
    let Some(opts) = parse(args).map_err(usage)? else {
        println!("{USAGE}");
        return Err(Failure::Done);
    };
    if opts.entries.is_empty() {
        return Err(usage("no files given".into()));
    }
    let n_cells: usize = opts.entries.iter().map(|e| e.cells.len()).sum();
    if n_cells > MAX_CELLS {
        return Err(Failure::Msg(format!(
            "{n_cells} cells is over the {MAX_CELLS}-cell limit"
        )));
    }
    let msg = Failure::Msg;
    let mut sources = Vec::with_capacity(opts.entries.len());
    for e in &opts.entries {
        sources.push(
            std::fs::read(&e.path).map_err(|err| msg(format!("cannot read {}: {err}", e.path)))?,
        );
    }

    // Everything the layout needs is known before any cell renders, so an
    // oversized sheet fails fast: pixel sizes from a probe interpreter,
    // text from the label renderer.
    let text = |t: &str, px: u32, bold: bool| sheet::render_text(t, px, bold).map_err(msg);
    let mut headers: Vec<Option<Pixmap>> = Vec::new();
    let mut captions: Vec<Vec<Option<Pixmap>>> = Vec::new();
    let mut sizes: Vec<RowSize> = Vec::new();
    for e in &opts.entries {
        let header = if opts.labels {
            let label = e.label.clone().unwrap_or_else(|| stem(&e.path));
            text(&label, LABEL_PX, true)?
        } else {
            None
        };
        let mut row = RowSize {
            header: dims(header.as_ref()),
            cells: Vec::new(),
        };
        let mut row_captions = Vec::new();
        for &(d, zoom) in &e.cells {
            let (w, h) = probe(e.page, d)?;
            let caption = if opts.sizes {
                let note = if zoom > 1 {
                    format!(" (shown at {zoom}x)")
                } else {
                    String::new()
                };
                text(&format!("{w}x{h} px{note}"), CAPTION_PX, false)?
            } else {
                None
            };
            let (zw, zh) = (w.checked_mul(zoom), h.checked_mul(zoom));
            let image = match (zw, zh) {
                (Some(zw), Some(zh)) if zw <= sheet::MAX_SIDE_PX && zh <= sheet::MAX_SIDE_PX => {
                    (zw, zh)
                }
                _ => {
                    return Err(msg(format!(
                        "{}: a {w}x{h} px cell shown at {zoom}x is over the {}px limit",
                        e.path,
                        sheet::MAX_SIDE_PX
                    )));
                }
            };
            row.cells.push(CellSize {
                image,
                caption: dims(caption.as_ref()),
            });
            row_captions.push(caption);
        }
        headers.push(header);
        captions.push(row_captions);
        sizes.push(row);
    }
    let layout = Layout::new(&sizes).map_err(msg)?;
    let mut out = layout.new_sheet().map_err(msg)?;

    // Cells stream into the sheet, one frame in memory at a time.
    let mut any_failed = false;
    for (r, e) in opts.entries.iter().enumerate() {
        if let Some(h) = &headers[r] {
            let (x, y) = layout.header_xy[r];
            sheet::blit_at(&mut out, x, y, h);
        }
        for (c, &(d, zoom)) in e.cells.iter().enumerate() {
            let (frame, ok) = render_cell(&sources[r], &e.path, e.page, d)?;
            any_failed |= !ok;
            let frame = if zoom > 1 {
                sheet::zoom_nearest(&frame, zoom)
                    .ok_or_else(|| msg(format!("{}: cell too large to zoom", e.path)))?
            } else {
                frame
            };
            let (x, y) = layout.image_xy[r][c];
            sheet::frame(&mut out, x, y, frame.width(), frame.height());
            sheet::blit_at(&mut out, x, y, &frame);
            if let Some(cap) = &captions[r][c] {
                let (cx, cy) = layout.caption_xy[r][c];
                sheet::blit_at(&mut out, cx, cy, cap);
            }
        }
    }
    out.save_png(&opts.out)
        .map_err(|e| msg(format!("cannot write {}: {e}", opts.out)))?;
    println!("pscat: wrote {}", opts.out);
    if any_failed {
        Err(Failure::Silent)
    } else {
        Ok(())
    }
}

fn dims(p: Option<&Pixmap>) -> (u32, u32) {
    p.map_or((0, 0), |p| (p.width(), p.height()))
}

/// The file name without directory or extension: the default row label.
fn stem(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string()
}

/// The pixel size a page of `page` points renders at `dpi`, from the
/// interpreter itself rather than a copy of its rounding.
fn probe(page: (u32, u32), dpi: f32) -> Result<(u32, u32), Failure> {
    Interp::with_page_scaled(page.0, page.1, dpi / 72.0)
        .map(|i| (i.gfx().pixmap.width(), i.gfx().pixmap.height()))
        .ok_or_else(|| {
            Failure::Msg(format!(
                "a {}x{} pt page at {dpi} dpi is over 8000 px on a side",
                page.0, page.1
            ))
        })
}

/// Run `source` on a fresh page and return its last page, and whether
/// the program ran to the end (the error report is printed). The last
/// page, as `--sweep-seed` frames use: a program that shows several
/// pages contributes its final one.
fn render_cell(
    source: &[u8],
    path: &str,
    page: (u32, u32),
    dpi: f32,
) -> Result<(Pixmap, bool), Failure> {
    let mut interp = Interp::with_page_scaled(page.0, page.1, dpi / 72.0)
        .ok_or_else(|| Failure::Msg(format!("{path}: unusable page size")))?;
    let ok = match interp.run_source(source) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("pscat sheet: {path} at {dpi} dpi failed:");
            eprintln!("{}", interp.error_report(&e));
            false
        }
    };
    let gfx = interp.gfx();
    let mut pages: Vec<&Pixmap> = gfx.pages().iter().collect();
    if pages.is_empty() || gfx.has_trailing_art() {
        pages.push(&gfx.pixmap);
    }
    Ok((pages[pages.len() - 1].clone(), ok))
}

/// `WxH` points, each 1..8000 (`--page`'s own limits).
fn parse_page(spec: &str) -> Result<(u32, u32), String> {
    let bad = || format!("invalid --page value: {spec} (expected WxH)");
    let (w, h) = spec.split_once('x').ok_or_else(bad)?;
    Ok((
        page_dim("--page width", w).map_err(|_| bad())?,
        page_dim("--page height", h).map_err(|_| bad())?,
    ))
}

/// `96,9,9x4`: each entry a dpi, optionally `xZOOM`.
fn parse_dpis(list: &str) -> Result<Vec<(f32, u32)>, String> {
    let mut cells = Vec::new();
    for item in list.split(',') {
        let (d, zoom) = match item.split_once('x') {
            Some((d, z)) => {
                let zoom = z
                    .parse::<u32>()
                    .ok()
                    .filter(|z| (2..=MAX_ZOOM).contains(z))
                    .ok_or_else(|| {
                        format!("--dpi zoom must be a whole number 2..{MAX_ZOOM}, got: {item}")
                    })?;
                (d, zoom)
            }
            None => (item, 1),
        };
        cells.push((dpi(d)?, zoom));
    }
    Ok(cells)
}

fn parse(mut args: Args) -> Result<Option<Opts>, String> {
    let mut o = Opts {
        out: "contact-sheet.png".into(),
        entries: Vec::new(),
        labels: true,
        sizes: true,
    };
    let mut page = pscat::gfx::DEFAULT_PAGE;
    let mut cells = vec![(72.0, 1)];
    let mut label: Option<String> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "-o" | "--out" => o.out = args.value(&arg)?,
            "--page" => page = parse_page(&args.value(&arg)?)?,
            "--dpi" => cells = parse_dpis(&args.value(&arg)?)?,
            "--label" => label = Some(args.value(&arg)?),
            "--no-labels" => o.labels = false,
            "--no-sizes" => o.sizes = false,
            "--" => {
                for rest in args.remaining() {
                    push(&mut o, rest, &mut label, page, &cells);
                }
            }
            s if s.starts_with('-') && s.len() > 1 => {
                return Err(format!("unknown option: {s}"));
            }
            _ => push(&mut o, arg, &mut label, page, &cells),
        }
    }
    if let Some(l) = label {
        return Err(format!("--label {l:?} has no file after it"));
    }
    Ok(Some(o))
}

fn push(
    o: &mut Opts,
    path: String,
    label: &mut Option<String>,
    page: (u32, u32),
    cells: &[(f32, u32)],
) {
    o.entries.push(Entry {
        path,
        label: label.take(),
        page,
        cells: cells.to_vec(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpi_lists_carry_an_optional_zoom() {
        assert_eq!(parse_dpis("96").unwrap(), vec![(96.0, 1)]);
        assert_eq!(
            parse_dpis("96,9,9x4").unwrap(),
            vec![(96.0, 1), (9.0, 1), (9.0, 4)]
        );
        for bad in ["", "96,", "9x1", "9x17", "9x", "9xa", "8", "x4", "96;9"] {
            assert!(parse_dpis(bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn page_takes_whole_points_within_the_cli_limits() {
        assert_eq!(parse_page("384x384"), Ok((384, 384)));
        for bad in ["384", "0x5", "8001x5", "5x", "x5", "1.5x2", "5x5x5"] {
            assert!(parse_page(bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn stem_drops_directory_and_extension() {
        assert_eq!(stem("art/logo/pscat-logo-a.ps"), "pscat-logo-a");
        assert_eq!(stem("plain"), "plain");
    }
}
