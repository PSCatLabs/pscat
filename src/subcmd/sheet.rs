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
//! the face is picked per character by script), and the geometry, labels,
//! captions, frames and zoom are the shared `pscat::contact_sheet`
//! composition (`SheetPlan`, `compose_sheet`), which the `review_sheet` MCP
//! tool draws with too (issue #744). This file only parses the options and
//! renders each cell.
//!
//! Issue #706 adds sweeps (one file over seeds or a parameter, each cell
//! labelled with its value; the grammars and the value formatting are the
//! flag CLI's own, `parse_seed_spec`, `parse_sweep_spec` and
//! `format_sweep_value` in `main.rs`, which a child module can call), a
//! per-cell mat, stacked columns and `--cols` wrapping.

use pscat::contact_sheet::{
    self as sheet, ComposeError, PlanError, SheetCell, SheetOptions, SheetPlan, SheetRow,
};
use pscat::{Interp, Limits};
use tiny_skia::Pixmap;

use super::{Args, Failure, Format, check_output_name, dpi, page_dim, take_limit_flag};
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
                     --sweep-seed (N, A:B, A:B:STEP, or A,B,C). The seed
                     also seeds noise (noiseseed), unless the program calls
                     noiseseed itself
      --sweep NAME=SPEC
                     the next file only: one cell per value of /NAME, which is
                     defined before the file runs (--sweep's own grammar, e.g.
                     Density=10,20,40 or Density=10:50:10). A sweep takes every
                     --dpi cell once per value, and puts the value under each
                     cell as its caption: seed=7, Density=20.
      --captions LIST
                     the next file's sweep only: a readable caption for each
                     value in place of seed=7 or Density=20, comma-separated,
                     one per value: --sweep Panel=1,2,3 --captions
                     '(a) image model,(b) pscat,(c) hybrid'. A caption cannot
                     contain a comma; an empty one (a,,c) leaves that value's
                     caption out. A failure message still names the value
      --no-values    omit the sweep value from each caption (the pixel size
                     stays, unless --no-sizes), for a sheet whose cells already
                     say what they are. --captions then has nothing to show
      --cols N       wrap a row of more than N columns onto further lines (the
                     label only above the first); columns line up across lines
      --label TEXT   the next file's row label (default: its file name without .ps);
                     Latin with accents, Korean, Japanese, Chinese and Thai are
                     drawn in a face for the script (the last three need the
                     cjk-thai font pack: `pscat fonts install`)
      --no-labels    omit the row labels
      --no-sizes     omit the pixel size from each caption (a sweep's value stays)
      --max-steps N  stop a cell's program after N interpreter steps (0 = no limit)
      --max-time SECS  stop a cell's program after SECS of wall-clock time (0 = no limit)
      --max-memory MIB  stop a cell's program that grows the heap by more than MIB
                     (0 = no limit). The limits apply to each cell, one program
                     run, as to each frame of the flag CLI's sweeps: every cell
                     gets the whole budget, so a sheet's worst case is the cell
                     count times it. A cell that passes one is drawn as far as it
                     got, reported as `limitexceeded (steps|time|memory)`, and
                     the run exits non-zero
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
    /// `--captions`: what to draw for each sweep value in place of
    /// [`Sweep::label`] (an empty entry draws none). Always as long as the
    /// sweep, which [`push`] checks.
    captions: Option<Vec<String>>,
}

/// One cell to draw: what `--dpi` says, and which sweep value it takes.
struct Planned {
    spec: CellSpec,
    value: Option<usize>,
}

impl Entry {
    /// How many cells [`Entry::plan`] makes, without making them.
    fn cell_count(&self) -> usize {
        let per_value: usize = self.columns.iter().map(Vec::len).sum();
        per_value * self.sweep.as_ref().map_or(1, Sweep::len)
    }

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
    /// `--no-values` turns this off: the sweep value in each caption.
    values: bool,
    cols: Option<usize>,
    /// `--max-steps`, `--max-time`, `--max-memory` (issue #701), per cell.
    limits: Limits,
}

pub(super) fn run(args: Args) -> Result<(), Failure> {
    let usage = |m: String| Failure::Usage(m, USAGE);
    let Some(opts) = parse(args).map_err(usage)? else {
        println!("{USAGE}");
        return Err(Failure::Done);
    };
    // The sheet is always a PNG: a name for another image or document type
    // is refused before anything renders or is written (#938, #947).
    check_output_name("-o", &opts.out, Format::Png).map_err(usage)?;
    if opts.entries.is_empty() {
        return Err(usage("no files given".into()));
    }
    let n_cells: usize = opts.entries.iter().map(Entry::cell_count).sum();
    if n_cells > MAX_CELLS {
        return Err(Failure::Msg(format!(
            "{n_cells} cells is over the {MAX_CELLS}-cell limit"
        )));
    }
    let plans: Vec<(Vec<Planned>, Vec<usize>)> = opts.entries.iter().map(Entry::plan).collect();
    let msg = Failure::Msg;
    let mut sources = Vec::with_capacity(opts.entries.len());
    for e in &opts.entries {
        sources.push(
            std::fs::read(&e.path).map_err(|err| msg(format!("cannot read {}: {err}", e.path)))?,
        );
    }

    // Everything the layout needs is known before any cell renders, so an
    // oversized sheet fails fast: pixel sizes from a probe interpreter, text
    // from the shared composition's label renderer.
    let mut rows: Vec<SheetRow> = Vec::with_capacity(opts.entries.len());
    for (e, (cells, column_len)) in opts.entries.iter().zip(&plans) {
        let mut row = SheetRow {
            label: opts
                .labels
                .then(|| e.label.clone().unwrap_or_else(|| stem(&e.path))),
            cells: Vec::with_capacity(cells.len()),
            column_len: column_len.clone(),
        };
        for planned in cells {
            let CellSpec { dpi: d, zoom, mat } = planned.spec;
            let size = probe(e.page, d)?;
            let value = match (opts.values, planned.value, &e.sweep) {
                (true, Some(v), Some(sweep)) => Some(match &e.captions {
                    Some(names) => names[v].clone(),
                    None => sweep.label(v),
                }),
                _ => None,
            };
            row.cells.push(SheetCell {
                size,
                zoom,
                mat,
                value,
            });
        }
        rows.push(row);
    }
    let sheet_opts = SheetOptions {
        sizes: opts.sizes,
        wrap: opts.cols,
    };
    let plan = SheetPlan::new(&rows, &sheet_opts).map_err(|err| match err {
        PlanError::Text(m) => msg(m),
        PlanError::CellTooLarge { row, cell } => {
            let (w, h) = rows[row].cells[cell].size;
            msg(format!(
                "{}: a {w}x{h} px cell shown at {}x is over the {}px limit",
                opts.entries[row].path,
                rows[row].cells[cell].zoom,
                sheet::MAX_SIDE_PX
            ))
        }
        PlanError::Layout(m) => msg(match opts.cols {
            None => format!("{m}; or wrap the rows with --cols N, or stack cells with +"),
            Some(_) => m,
        }),
    })?;

    // Cells stream into the sheet, one frame in memory at a time.
    let mut any_failed = false;
    let mut seed_fired = false;
    let out = sheet::compose_sheet(&plan, |r, c| {
        let e = &opts.entries[r];
        let planned = &plans[r].0[c];
        let (frame, ok, fired) = render_cell(
            &sources[r],
            &e.path,
            e.page,
            planned.spec.dpi,
            planned.spec.mat.is_some(),
            e.sweep.as_ref().zip(planned.value),
            opts.limits,
        )?;
        any_failed |= !ok;
        if c == 0 {
            seed_fired = false;
        }
        seed_fired |= fired;
        if c + 1 == plans[r].0.len()
            && matches!(e.sweep, Some(Sweep::Seed(_)))
            && !seed_fired
        {
            eprintln!(
                "pscat sheet: warning: {} never called srand or sampled noise -- --sweep-seed had no effect, \
                 every cell is identical",
                e.path
            );
        }
        Ok(frame)
    })
    .map_err(|err| match err {
        ComposeError::Render(f) => f,
        ComposeError::Zoom { row, .. } => {
            msg(format!("{}: cell too large to zoom", opts.entries[row].path))
        }
        ComposeError::Mat { row, .. } => {
            msg(format!("{}: cell too large to mat", opts.entries[row].path))
        }
        ComposeError::Sheet(m) => msg(m),
    })?;
    // Cells of any dpi share the sheet, so it records no particular
    // resolution: 72 dpi, one pixel to the point.
    pscat::png::save(&out, 1.0, &opts.out)
        .map_err(|e| msg(format!("cannot write {}: {e}", opts.out)))?;
    eprintln!("pscat: wrote {}", opts.out);
    if any_failed {
        Err(Failure::Silent)
    } else {
        Ok(())
    }
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
/// `limits` (issue #701) cover this cell's program alone: a fresh
/// interpreter, so each cell has the whole budget. Each cell runs on a
/// thread of its own, with a fresh name interner, and the parameter is
/// defined detached from the program's name ids, so a cell is
/// byte-identical to a standalone render of its seed or value (#891).
fn render_cell(
    source: &[u8],
    path: &str,
    page: (u32, u32),
    dpi: f32,
    transparent: bool,
    sweep: Option<(&Sweep, usize)>,
    limits: Limits,
) -> Result<(Pixmap, bool, bool), Failure> {
    crate::run_on_fresh_thread(|| {
        render_cell_here(source, path, page, dpi, transparent, sweep, limits)
    })
    .map_err(Failure::Msg)?
}

/// [`render_cell`] on the calling thread.
fn render_cell_here(
    source: &[u8],
    path: &str,
    page: (u32, u32),
    dpi: f32,
    transparent: bool,
    sweep: Option<(&Sweep, usize)>,
    limits: Limits,
) -> Result<(Pixmap, bool, bool), Failure> {
    let mut interp = Interp::with_page_scaled(page.0, page.1, dpi / 72.0)
        .ok_or_else(|| Failure::Msg(format!("{path}: unusable page size")))?;
    // `set_limits` refuses only when the tracking allocator is missing, and
    // this binary declares it.
    interp
        .set_limits(limits)
        .map_err(|e| Failure::Msg(e.to_string()))?;
    if transparent {
        interp.gfx_mut().set_transparent_background();
    }
    match sweep {
        Some((Sweep::Seed(seeds), i)) => interp.set_seed_override(Some(seeds[i])),
        Some((Sweep::Param(name, values), i)) => {
            let preamble = format!("/{name} {} def", format_sweep_value(values[i]));
            // Detached: the name takes no id the program's own names would
            // (`forall` order follows the ids; #868, #891).
            if let Err(e) = pscat::name::detached(|| interp.run_source(preamble.as_bytes())) {
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
        values: true,
        cols: None,
        limits: Limits::none(),
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
        captions: None,
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
                        "one sweep per file: --sweep-seed and --sweep cannot be combined or repeated"
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
            "--captions" => {
                if pending.captions.is_some() {
                    return Err("one --captions per file: it cannot be repeated".into());
                }
                pending.captions = Some(parse_captions(&args.value(&arg)?));
            }
            "--cols" => o.cols = Some(parse_cols(&args.value(&arg)?)?),
            "--no-labels" => o.labels = false,
            "--no-sizes" => o.sizes = false,
            "--no-values" => o.values = false,
            _ if take_limit_flag(&mut o.limits, &arg, &mut args)? => {}
            "--" => {
                for rest in args.remaining() {
                    push(&mut o, rest, &mut pending)?;
                }
            }
            s if s.starts_with('-') && s.len() > 1 => {
                return Err(format!("unknown option: {s}"));
            }
            _ => push(&mut o, arg, &mut pending)?,
        }
    }
    if let Some(l) = pending.label {
        return Err(format!("--label {l:?} has no file after it"));
    }
    if pending.sweep.is_some() {
        return Err("--sweep-seed/--sweep has no file after it".into());
    }
    if pending.captions.is_some() {
        return Err("--captions has no file after it".into());
    }
    Ok(Some(o))
}

/// What the files that follow take from the options so far: `--page` and
/// `--dpi` stay; `--label`, the sweep and `--captions` apply to the next
/// file only.
struct Pending {
    page: (u32, u32),
    columns: Vec<Vec<CellSpec>>,
    label: Option<String>,
    sweep: Option<Sweep>,
    captions: Option<Vec<String>>,
}

/// `--captions`: one caption per comma-separated entry. Nothing is
/// trimmed, so a space after a comma is part of the caption; an empty entry
/// is a caption left out.
fn parse_captions(list: &str) -> Vec<String> {
    list.split(',').map(str::to_string).collect()
}

fn push(o: &mut Opts, path: String, pending: &mut Pending) -> Result<(), String> {
    let sweep = pending.sweep.take();
    let captions = pending.captions.take();
    if let Some(names) = &captions {
        match &sweep {
            None => {
                return Err(format!(
                    "--captions needs a --sweep or --sweep-seed for the same file ({path})"
                ));
            }
            Some(s) if s.len() != names.len() => {
                return Err(format!(
                    "--captions has {} entries but the sweep for {path} has {} values",
                    names.len(),
                    s.len()
                ));
            }
            Some(_) => {}
        }
    }
    o.entries.push(Entry {
        path,
        label: pending.label.take(),
        page: pending.page,
        columns: pending.columns.clone(),
        sweep,
        captions,
    });
    Ok(())
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
            captions: None,
        };
        let (cells, column_len) = e.plan();
        assert_eq!(e.cell_count(), cells.len());
        assert_eq!(column_len, vec![1, 2, 1, 2]);
        assert_eq!(cells.len(), 6);
        assert_eq!(cells[3].value, Some(1));
        assert_eq!(e.sweep.as_ref().unwrap().label(1), "seed=4");
        let p = parse_param_sweep("D=10:30:10").unwrap();
        assert_eq!((p.len(), p.label(2).as_str()), (3, "D=30"));
    }

    #[test]
    fn captions_split_on_commas_and_keep_empty_entries() {
        assert_eq!(parse_captions("(a) x,(b) y"), ["(a) x", "(b) y"]);
        assert_eq!(parse_captions("a,,c,"), ["a", "", "c", ""]);
        assert_eq!(parse_captions(""), [""]);
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
