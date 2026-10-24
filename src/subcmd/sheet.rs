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
//!
//! Issue #706 adds sweeps (one file over seeds or a parameter, each cell
//! labelled with its value; the grammars and the value formatting are the
//! flag CLI's own, `parse_seed_spec`, `parse_sweep_spec` and
//! `format_sweep_value` in `main.rs`, which a child module can call), a
//! per-cell mat, stacked columns and `--cols` wrapping.

use pscat::Interp;
use pscat::contact_sheet::{self as sheet, CellSize, Layout, PackedRow};
use tiny_skia::Pixmap;

use super::{Args, Failure, dpi, page_dim};
use crate::{SweepValue, format_sweep_value, parse_seed_spec, parse_sweep_spec};

const USAGE: &str = r#"usage: pscat sheet [options] FILE.ps [FILE.ps ...]

Render each FILE as a row of cells and compose them into one labelled PNG.

  -o, --out PATH     output PNG (default contact-sheet.png)
      --page WxH     page size in points for the files that follow (default 612x792)
      --dpi LIST     comma-separated columns of cells, for the files that follow
                     (default 72). Each cell is DPI[xZOOM][@MAT]:
                       96,9       each file at 96 and at 9 dpi
                       9x4        the 9 dpi render again enlarged 4x with no
                                  smoothing (whole factors 2..16), to read a
                                  small icon pixel by pixel
                       9@#222     the render on a #222 mat: the page is drawn
                                  with a transparent background, set on a MAT
                                  (#rgb, #rrggbb, white or black, # optional)
                                  and padded 10 px, to judge an icon on dark
                                  or light. An opaque page just gets the frame.
                     Join cells with + to stack them in one column, so small
                     cells share a column instead of each taking a wide one:
                     96,9+9@#222 is a 96 dpi cell and, beside it, a column of
                     the 9 dpi render over its dark-mat copy.
      --sweep-seed SPEC
                     the next file only: one cell per seed, as the flag CLI's
                     --sweep-seed (N, A:B, A:B:STEP, or A,B,C)
      --sweep NAME=SPEC
                     the next file only: one cell per value of /NAME, which is
                     defined before the file runs (--sweep's own grammar, e.g.
                     Density=10,20,40 or Density=10:50:10). A sweep takes every
                     --dpi cell once per value, and puts the value under each
                     cell as its caption: seed=7, Density=20.
      --cols N       wrap a row of more than N columns onto further lines (the
                     label only above the first); columns line up across lines
      --label TEXT   the next file's row label (default: its file name without .ps);
                     Latin with accents, Korean, Japanese, Chinese and Thai are
                     drawn in a face for the script (the last three need the
                     cjk-thai font pack: `pscat fonts install`)
      --no-labels    omit the row labels
      --no-sizes     omit the pixel size from each caption (a sweep's value stays)
  -h, --help         this text

--page and --dpi stay in force for every file after them until given again, so
one sheet can mix page sizes. Every cell is drawn at its true pixel size,
centred in its column, with a hairline around it; nothing is scaled unless an
entry in --dpi says so. A cell whose program fails is still drawn, as far as it
got, and the run exits non-zero. The sheet is at most 8000 px on a side: past
that, wrap with --cols, stack with +, or use a lower --dpi.

Example (four logo candidates, each at 512 px, 48 px and 48 px on a dark mat):
  pscat sheet -o sheet.png --page 384x384 --dpi '96,9+9@#222' \
      --label "A: one-line cat" a.ps --label "B: drawing board" b.ps

Example (twelve seeds of one piece, four to a line):
  pscat sheet -o seeds.png --page 200x200 --dpi 72 --cols 4 --sweep-seed 1:12 piece.ps"#;

/// Most cells one sheet may ask for.
const MAX_CELLS: usize = 256;
/// Largest `--dpi` zoom factor.
const MAX_ZOOM: u32 = 16;
/// Row label and caption type sizes, in pixels.
const LABEL_PX: u32 = 16;
const CAPTION_PX: u32 = 12;

/// One `--dpi` cell: `DPI[xZOOM][@MAT]`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct CellSpec {
    dpi: f32,
    zoom: u32,
    mat: Option<[u8; 3]>,
}

/// The values a sweep takes.
#[derive(Clone)]
enum Sweep {
    Seed(Vec<i64>),
    Param(String, Vec<SweepValue>),
}

impl Sweep {
    fn len(&self) -> usize {
        match self {
            Sweep::Seed(v) => v.len(),
            Sweep::Param(_, v) => v.len(),
        }
    }

    /// The caption for value `i`.
    fn label(&self, i: usize) -> String {
        match self {
            Sweep::Seed(v) => format!("seed={}", v[i]),
            Sweep::Param(name, v) => format!("{name}={}", format_sweep_value(v[i])),
        }
    }
}

struct Entry {
    path: String,
    label: Option<String>,
    page: (u32, u32),
    /// `--dpi`'s columns: each a stack of one or more cells.
    columns: Vec<Vec<CellSpec>>,
    sweep: Option<Sweep>,
}

/// One cell to draw: what `--dpi` says, and which sweep value it takes.
struct Planned {
    spec: CellSpec,
    value: Option<usize>,
}

impl Entry {
    /// Every cell in order (value by value, then `--dpi` order), and how they
    /// group into columns.
    fn plan(&self) -> (Vec<Planned>, Vec<usize>) {
        let values: Vec<Option<usize>> = match &self.sweep {
            Some(s) => (0..s.len()).map(Some).collect(),
            None => vec![None],
        };
        let mut cells = Vec::new();
        let mut column_len = Vec::new();
        for &value in &values {
            for col in &self.columns {
                column_len.push(col.len());
                cells.extend(col.iter().map(|&spec| Planned { spec, value }));
            }
        }
        (cells, column_len)
    }
}

struct Opts {
    out: String,
    entries: Vec<Entry>,
    labels: bool,
    sizes: bool,
    cols: Option<usize>,
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
    let plans: Vec<(Vec<Planned>, Vec<usize>)> = opts.entries.iter().map(Entry::plan).collect();
    let n_cells: usize = plans.iter().map(|(cells, _)| cells.len()).sum();
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
    let mut rows: Vec<PackedRow> = Vec::new();
    for (e, (cells, column_len)) in opts.entries.iter().zip(&plans) {
        let header = if opts.labels {
            let label = e.label.clone().unwrap_or_else(|| stem(&e.path));
            text(&label, LABEL_PX, true)?
        } else {
            None
        };
        let mut row = PackedRow {
            header: dims(header.as_ref()),
            cells: Vec::new(),
            column_len: column_len.clone(),
        };
        let mut row_captions = Vec::new();
        for planned in cells {
            let CellSpec { dpi: d, zoom, mat } = planned.spec;
            let (w, h) = probe(e.page, d)?;
            // The caption: the sweep value if any, then the size.
            let mut parts = Vec::new();
            if let (Some(v), Some(sweep)) = (planned.value, &e.sweep) {
                parts.push(sweep.label(v));
            }
            if opts.sizes {
                let note = if zoom > 1 {
                    format!(" (shown at {zoom}x)")
                } else {
                    String::new()
                };
                parts.push(format!("{w}x{h} px{note}"));
            }
            let caption = if parts.is_empty() {
                None
            } else {
                text(&parts.join(", "), CAPTION_PX, false)?
            };
            let pad = if mat.is_some() { 2 * sheet::MAT_PAD } else { 0 };
            let (zw, zh) = (
                w.checked_mul(zoom).and_then(|v| v.checked_add(pad)),
                h.checked_mul(zoom).and_then(|v| v.checked_add(pad)),
            );
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
        rows.push(row);
    }
    let layout = Layout::packed(&rows, opts.cols).map_err(|m| {
        msg(match opts.cols {
            None => format!("{m}; or wrap the rows with --cols N, or stack cells with +"),
            Some(_) => m,
        })
    })?;
    let mut out = layout.new_sheet().map_err(msg)?;

    // Cells stream into the sheet, one frame in memory at a time.
    let mut any_failed = false;
    for (r, (e, (cells, _))) in opts.entries.iter().zip(&plans).enumerate() {
        if let Some(h) = &headers[r] {
            let (x, y) = layout.header_xy[r];
            sheet::blit_at(&mut out, x, y, h);
        }
        let mut seed_fired = false;
        for (c, planned) in cells.iter().enumerate() {
            let CellSpec { dpi: d, zoom, mat } = planned.spec;
            let sweep = e.sweep.as_ref().zip(planned.value);
            let (frame, ok, fired) =
                render_cell(&sources[r], &e.path, e.page, d, mat.is_some(), sweep)?;
            any_failed |= !ok;
            seed_fired |= fired;
            let frame = if zoom > 1 {
                sheet::zoom_nearest(&frame, zoom)
                    .ok_or_else(|| msg(format!("{}: cell too large to zoom", e.path)))?
            } else {
                frame
            };
            let frame = match mat {
                Some(rgb) => sheet::composite_on_mat(&frame, rgb, sheet::MAT_PAD)
                    .ok_or_else(|| msg(format!("{}: cell too large to mat", e.path)))?,
                None => frame,
            };
            let (x, y) = layout.image_xy[r][c];
            sheet::frame(&mut out, x, y, frame.width(), frame.height());
            sheet::blit_at(&mut out, x, y, &frame);
            if let Some(cap) = &captions[r][c] {
                let (cx, cy) = layout.caption_xy[r][c];
                sheet::blit_at(&mut out, cx, cy, cap);
            }
        }
        if matches!(e.sweep, Some(Sweep::Seed(_))) && !seed_fired {
            eprintln!(
                "pscat sheet: warning: {} never called srand -- --sweep-seed had no effect, \
                 every cell is identical",
                e.path
            );
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

/// Run `source` on a fresh page and return its last page, whether the
/// program ran to the end (the error report is printed), and whether a
/// seed override took effect. The last page, as `--sweep-seed` frames
/// use: a program that shows several pages contributes its final one.
/// `transparent` draws on a transparent page, for a cell that goes on a
/// mat. `sweep` is the sweep and the index of the value this cell takes:
/// a seed overrides `srand`, a parameter is defined before the program.
fn render_cell(
    source: &[u8],
    path: &str,
    page: (u32, u32),
    dpi: f32,
    transparent: bool,
    sweep: Option<(&Sweep, usize)>,
) -> Result<(Pixmap, bool, bool), Failure> {
    let mut interp = Interp::with_page_scaled(page.0, page.1, dpi / 72.0)
        .ok_or_else(|| Failure::Msg(format!("{path}: unusable page size")))?;
    if transparent {
        interp.gfx_mut().set_transparent_background();
    }
    match sweep {
        Some((Sweep::Seed(seeds), i)) => interp.set_seed_override(Some(seeds[i])),
        Some((Sweep::Param(name, values), i)) => {
            let preamble = format!("/{name} {} def", format_sweep_value(values[i]));
            if let Err(e) = interp.run_source(preamble.as_bytes()) {
                return Err(Failure::Msg(format!(
                    "{path}: could not define {name}: {}",
                    interp.error_report(&e)
                )));
            }
        }
        None => {}
    }
    let ok = match interp.run_source(source) {
        Ok(()) => true,
        Err(e) => {
            let at = sweep.map_or(String::new(), |(s, i)| format!(", {}", s.label(i)));
            eprintln!("pscat sheet: {path} at {dpi} dpi{at} failed:");
            eprintln!("{}", interp.error_report(&e));
            false
        }
    };
    let fired = interp.seed_override_fired();
    let gfx = interp.gfx();
    let mut pages: Vec<&Pixmap> = gfx.pages().iter().collect();
    if pages.is_empty() || gfx.has_trailing_art() {
        pages.push(&gfx.pixmap);
    }
    Ok((pages[pages.len() - 1].clone(), ok, fired))
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

/// `96,9+9@#222,9x4`: comma-separated columns, each one or more cells
/// joined by `+`, each cell `DPI[xZOOM][@MAT]`.
fn parse_dpis(list: &str) -> Result<Vec<Vec<CellSpec>>, String> {
    list.split(',')
        .map(|column| column.split('+').map(parse_cell).collect())
        .collect()
}

fn parse_cell(item: &str) -> Result<CellSpec, String> {
    let (spec, mat) = match item.split_once('@') {
        Some((spec, mat)) => (spec, Some(parse_mat(mat)?)),
        None => (item, None),
    };
    let (d, zoom) = match spec.split_once('x') {
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
        None => (spec, 1),
    };
    Ok(CellSpec {
        dpi: dpi(d)?,
        zoom,
        mat,
    })
}

/// A mat colour: `#rgb` or `#rrggbb` (the `#` optional, so a shell that
/// treats it as a pattern character is not a problem), `white` or `black`.
fn parse_mat(s: &str) -> Result<[u8; 3], String> {
    match s {
        "white" => return Ok([255, 255, 255]),
        "black" => return Ok([0, 0, 0]),
        _ => {}
    }
    let hex = s.strip_prefix('#').unwrap_or(s);
    let bad = || format!("invalid mat colour: {s:?} (expected #rgb, #rrggbb, white or black)");
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(bad());
    }
    let digit = |i: usize| u8::from_str_radix(&hex[i..i + 1], 16).map_err(|_| bad());
    let pair = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| bad());
    match hex.len() {
        3 => Ok([digit(0)? * 17, digit(1)? * 17, digit(2)? * 17]),
        6 => Ok([pair(0)?, pair(2)?, pair(4)?]),
        _ => Err(bad()),
    }
}

/// `--sweep NAME=SPEC`: the name is spliced into a `/NAME v def`, so it
/// must be an identifier (the flag CLI checks it inline in its own
/// parser, so the check is repeated here); the values are `--sweep`'s.
fn parse_param_sweep(spec: &str) -> Result<Sweep, String> {
    let (name, rest) = spec
        .split_once('=')
        .ok_or_else(|| format!("invalid --sweep value: {spec} (expected NAME=SPEC)"))?;
    let valid = name.starts_with(|c: char| c.is_ascii_alphabetic())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !valid {
        return Err(format!(
            "invalid --sweep name: {name:?} (expected an identifier)"
        ));
    }
    Ok(Sweep::Param(name.to_string(), parse_sweep_spec(rest)?))
}

/// `--cols`: a whole number of columns, 1 or more.
fn parse_cols(s: &str) -> Result<usize, String> {
    s.parse::<usize>()
        .ok()
        .filter(|&n| n >= 1)
        .ok_or_else(|| format!("--cols must be a whole number of columns, 1 or more, got: {s}"))
}

fn parse(mut args: Args) -> Result<Option<Opts>, String> {
    let mut o = Opts {
        out: "contact-sheet.png".into(),
        entries: Vec::new(),
        labels: true,
        sizes: true,
        cols: None,
    };
    let mut pending = Pending {
        page: pscat::gfx::DEFAULT_PAGE,
        columns: vec![vec![CellSpec {
            dpi: 72.0,
            zoom: 1,
            mat: None,
        }]],
        label: None,
        sweep: None,
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "-o" | "--out" => o.out = args.value(&arg)?,
            "--page" => pending.page = parse_page(&args.value(&arg)?)?,
            "--dpi" => pending.columns = parse_dpis(&args.value(&arg)?)?,
            "--label" => pending.label = Some(args.value(&arg)?),
            "--sweep-seed" | "--sweep" => {
                if pending.sweep.is_some() {
                    return Err(
                        "--sweep-seed and --sweep are mutually exclusive (one sweep axis per file)"
                            .into(),
                    );
                }
                let spec = args.value(&arg)?;
                pending.sweep = Some(if arg == "--sweep" {
                    parse_param_sweep(&spec)?
                } else {
                    Sweep::Seed(parse_seed_spec(&spec)?)
                });
            }
            "--cols" => o.cols = Some(parse_cols(&args.value(&arg)?)?),
            "--no-labels" => o.labels = false,
            "--no-sizes" => o.sizes = false,
            "--" => {
                for rest in args.remaining() {
                    push(&mut o, rest, &mut pending);
                }
            }
            s if s.starts_with('-') && s.len() > 1 => {
                return Err(format!("unknown option: {s}"));
            }
            _ => push(&mut o, arg, &mut pending),
        }
    }
    if let Some(l) = pending.label {
        return Err(format!("--label {l:?} has no file after it"));
    }
    if pending.sweep.is_some() {
        return Err("--sweep-seed/--sweep has no file after it".into());
    }
    Ok(Some(o))
}

/// What the files that follow take from the options so far: `--page` and
/// `--dpi` stay; `--label` and the sweep apply to the next file only.
struct Pending {
    page: (u32, u32),
    columns: Vec<Vec<CellSpec>>,
    label: Option<String>,
    sweep: Option<Sweep>,
}

fn push(o: &mut Opts, path: String, pending: &mut Pending) {
    o.entries.push(Entry {
        path,
        label: pending.label.take(),
        page: pending.page,
        columns: pending.columns.clone(),
        sweep: pending.sweep.take(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(dpi: f32, zoom: u32, mat: Option<[u8; 3]>) -> CellSpec {
        CellSpec { dpi, zoom, mat }
    }

    #[test]
    fn dpi_lists_carry_an_optional_zoom() {
        assert_eq!(parse_dpis("96").unwrap(), vec![vec![cell(96.0, 1, None)]]);
        assert_eq!(
            parse_dpis("96,9,9x4").unwrap(),
            vec![
                vec![cell(96.0, 1, None)],
                vec![cell(9.0, 1, None)],
                vec![cell(9.0, 4, None)]
            ]
        );
        for bad in ["", "96,", "9x1", "9x17", "9x", "9xa", "8", "x4", "96;9"] {
            assert!(parse_dpis(bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn dpi_cells_take_a_mat_and_stack_with_a_plus() {
        let dark = Some([0x22, 0x22, 0x22]);
        assert_eq!(
            parse_dpis("96,9+9@#222,9x4@222222").unwrap(),
            vec![
                vec![cell(96.0, 1, None)],
                vec![cell(9.0, 1, None), cell(9.0, 1, dark)],
                vec![cell(9.0, 4, dark)],
            ]
        );
        for bad in [
            "9+",
            "+9",
            "9@",
            "9@#12",
            "9@#gggggg",
            "9@#1234567",
            "9@@#222",
            "9@#222x4",
        ] {
            assert!(parse_dpis(bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn mat_colours_are_hex_or_named() {
        assert_eq!(parse_mat("#fff"), Ok([255, 255, 255]));
        assert_eq!(parse_mat("1a2B3c"), Ok([0x1a, 0x2b, 0x3c]));
        assert_eq!(parse_mat("black"), Ok([0, 0, 0]));
        for bad in ["", "#", "red", "#12", "#12345", "#ggg"] {
            assert!(parse_mat(bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn a_sweep_name_must_be_an_identifier() {
        assert!(matches!(
            parse_param_sweep("Density=10,20"),
            Ok(Sweep::Param(..))
        ));
        for bad in [
            "Density",
            "=1,2",
            "1x=1,2",
            "a b=1,2",
            "a;b=1,2",
            "a def (x) = 1,2",
            "/a=1,2",
            "a=",
            "a=x",
        ] {
            assert!(parse_param_sweep(bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn a_sweep_labels_each_value_and_expands_the_dpi_cells() {
        let e = Entry {
            path: "p.ps".into(),
            label: None,
            page: (10, 10),
            columns: parse_dpis("72,9+9@#222").unwrap(),
            sweep: Some(Sweep::Seed(vec![3, 4])),
        };
        let (cells, column_len) = e.plan();
        assert_eq!(column_len, vec![1, 2, 1, 2]);
        assert_eq!(cells.len(), 6);
        assert_eq!(cells[3].value, Some(1));
        assert_eq!(e.sweep.as_ref().unwrap().label(1), "seed=4");
        let p = parse_param_sweep("D=10:30:10").unwrap();
        assert_eq!((p.len(), p.label(2).as_str()), (3, "D=30"));
    }

    #[test]
    fn cols_is_a_positive_whole_number() {
        assert_eq!(parse_cols("4"), Ok(4));
        for bad in ["0", "-1", "x", "", "1.5"] {
            assert!(parse_cols(bad).is_err(), "{bad:?} must be refused");
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
