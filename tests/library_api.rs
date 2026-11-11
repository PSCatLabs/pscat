//! The supported library API for servers (issue #1098): halftone and page
//! access, review sheets, chalkboards and the art catalog.
//!
//! pscat-mcp (PSCatLabs/pscat-mcp) built these features on `#[doc(hidden)]`
//! modules, wrapped in its `src/pscat_extras.rs`. Each test here does one of
//! those hidden uses the way pscat-mcp does it today, then the supported way,
//! and checks the two give the same bytes. So a caller can switch from one to
//! the other and get the same output, and a change that splits them fails
//! here. The supported side of each test names only documented items.

use serde_json::json;

mod common;

/// A program with two shown pages and art after the last `showpage`.
const THREE_PAGES: &str = "0.2 setgray 0 0 30 30 rectfill showpage \
     1 0 0 setrgbcolor 10 10 40 20 rectfill showpage \
     0 0 1 setrgbcolor 0.5 setgray 20 0 20 60 rectfill";

fn interp(src: &str, w: u32, h: u32, scale: f32) -> pscat::Interp {
    let mut interp = pscat::Interp::with_page_scaled(w, h, scale).expect("valid page");
    interp.run_str(src).expect("program runs");
    interp
}

// --- 1. pages, halftone and PNG -------------------------------------------

/// Before: `halftone::screen` + `png::encode` over `Interp::gfx()`'s
/// `pages()`, `pixmap` and `has_trailing_art()`, with the scale passed in.
/// After: `Interp::page(i)` + `Page::halftone()` + `Page::to_png()`.
#[test]
fn halftone_pages_match_the_hidden_screen_and_encoder() {
    for src in [THREE_PAGES, "0.5 setgray 0 0 60 60 rectfill showpage", ""] {
        let scale = 150.0 / 72.0;
        let interp = interp(src, 60, 60, scale);

        let gfx = interp.gfx();
        let mut hidden_pages: Vec<_> = gfx.pages().iter().collect();
        if hidden_pages.is_empty() || gfx.has_trailing_art() {
            hidden_pages.push(&gfx.pixmap);
        }
        let hidden: Vec<Vec<u8>> = hidden_pages
            .into_iter()
            .map(|p| pscat::png::encode(&pscat::halftone::screen(p), scale).unwrap())
            .collect();

        let supported: Vec<Vec<u8>> = (0..interp.page_count())
            .map(|i| interp.page(i).unwrap().halftone().to_png().unwrap())
            .collect();
        assert_eq!(hidden, supported, "program {src:?}");
        assert!(interp.page(interp.page_count()).is_none());
        // `pages()` agrees with `page(i)` page by page.
        let all: Vec<Vec<u8>> = interp.pages().iter().map(|p| p.rgba8()).collect();
        let one_by_one: Vec<Vec<u8>> = (0..interp.page_count())
            .map(|i| interp.page(i).unwrap().rgba8())
            .collect();
        assert_eq!(all, one_by_one);
    }
}

/// `Page::halftone().to_png()` is the file `pscat --halftone --png` writes,
/// byte for byte (the claim in its documentation).
#[test]
fn halftone_png_is_what_the_cli_writes() {
    use common::CommandDeadline as _;
    let src = "%!PS\n0.3 0.6 0.9 setrgbcolor 5 5 40 25 rectfill \
               0.7 setgray 30 10 moveto 55 35 lineto 6 setlinewidth stroke showpage\n";
    let dir = common::scratch_dir("library-api-halftone");
    let (ps, png) = (dir.join("in.ps"), dir.join("out.png"));
    std::fs::write(&ps, src).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_pscat"))
        .args([
            "--headless",
            "--halftone",
            "--page",
            "60x40",
            "--dpi",
            "150",
            "--png",
        ])
        .arg(&png)
        .arg(&ps)
        .output_deadline()
        .expect("run pscat");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cli = std::fs::read(&png).expect("pscat wrote the PNG");

    let interp = interp(src, 60, 40, 150.0 / 72.0);
    assert_eq!(interp.page_count(), 1);
    let library = interp.page(0).unwrap().halftone().to_png().unwrap();
    assert!(
        cli == library,
        "the library's halftone PNG differs from the CLI's"
    );
}

/// A transparent page is screened as if on white paper, not read as solid
/// ink (the hidden `screen` assumes an opaque page).
#[test]
fn halftone_of_a_transparent_page_is_on_white() {
    let mut clear = pscat::Interp::with_page(40, 40).unwrap();
    clear.set_transparent_background();
    clear.run_str("0 0 10 10 rectfill").unwrap();
    let screened = clear.into_last_page().halftone();
    let px = screened.rgba8();
    let at = |x: usize, y: usize| px[(y * 40 + x) * 4];
    // The unpainted (transparent) top-left is paper; the black square at
    // the bottom-left is ink.
    assert_eq!(at(2, 2), 255, "transparent paper screens white");
    assert_eq!(at(5, 35), 0, "black stays black");
    // An opaque page is untouched by the compositing step.
    let opaque = interp("0.5 setgray 0 0 40 40 rectfill", 40, 40, 1.0);
    let hidden = pscat::png::encode(&pscat::halftone::screen(&opaque.gfx().pixmap), 1.0);
    assert_eq!(
        opaque.into_last_page().halftone().to_png().unwrap(),
        hidden.unwrap()
    );
}

/// Before: pscat-mcp's `render_sheet_cell`, which swaps `gfx_mut().pixmap`
/// out and takes the canvas or the last shown page. After:
/// `Interp::into_last_page()`.
#[test]
fn into_last_page_matches_the_hidden_canvas_swap() {
    for src in [
        THREE_PAGES,
        "1 0 0 setrgbcolor 0 0 9 9 rectfill showpage",
        "0 1 0 setrgbcolor 0 0 9 9 rectfill copypage 0 0 1 setrgbcolor 0 0 4 4 rectfill",
        "",
        "0 0 1 setrgbcolor 0 0 9 9 rectfill nosuchop",
    ] {
        let run = |src: &str| {
            let mut i = pscat::Interp::with_page_scaled(20, 20, 2.0).unwrap();
            let _ = i.run_str(src);
            i
        };
        let mut hidden = run(src);
        let canvas_is_last = hidden.gfx().pages().is_empty() || hidden.gfx().has_trailing_art();
        let canvas = std::mem::replace(
            &mut hidden.gfx_mut().pixmap,
            tiny_skia::Pixmap::new(1, 1).unwrap(),
        );
        let hidden_last = if canvas_is_last {
            canvas
        } else {
            hidden.gfx().pages().last().cloned().unwrap()
        };
        let hidden_png = pscat::png::encode(&hidden_last, 2.0).unwrap();

        let all = run(src).pages();
        let last = run(src).into_last_page();
        assert_eq!(last.to_png().unwrap(), hidden_png, "program {src:?}");
        assert_eq!(last.rgba8(), all.last().unwrap().rgba8());
    }
}

#[test]
fn blank_pages() {
    let page = pscat::Page::blank(3, 2).unwrap();
    assert!(page.rgba8().iter().all(|&b| b == 255));
    assert!(pscat::Page::blank(0, 1).is_none());
    assert!(pscat::Page::blank(1, 0).is_none());
    assert!(pscat::Page::blank(8001, 1).is_none());
    assert!(pscat::Page::blank(8000, 1).is_some());
}

// --- 2. review sheets -----------------------------------------------------

/// Before: `contact_sheet::{render_text, Layout, new_sheet, frame, blit_at,
/// zoom_nearest}` and `png::encode(&sheet, 1.0)` on tiny-skia pixmaps.
/// After: `pscat::sheet` on `Page`s. The sheet's PNG is the same, byte for
/// byte, and so are the positions.
#[test]
fn sheet_matches_the_hidden_contact_sheet() {
    use pscat::contact_sheet as cs;
    use pscat::sheet::{self, CellSize, RowSize, Sheet, SheetLayout};

    // Two rows: one with a label and two cells (one zoomed 3x), one with
    // no label and a single cell; captions on all but one cell.
    let programs = [
        (
            "first",
            "1 0 0 setrgbcolor 5 5 30 20 rectfill",
            (40, 30),
            1.0,
            1,
        ),
        (
            "first",
            "0 0 1 setrgbcolor 0 0 8 8 rectfill",
            (10, 10),
            1.0,
            3,
        ),
        (
            "",
            "0 0.6 0 setrgbcolor 0 0 50 25 rectfill",
            (50, 25),
            2.0,
            1,
        ),
    ];
    let rows_of = [0usize, 0, 1];
    let captions = ["40x30 px", "", "100x50 px"];

    // Hidden.
    let label_h = cs::render_text("first", 16, true).unwrap().unwrap();
    let mut cells_h = Vec::new();
    for (_, src, (w, h), scale, zoom) in programs {
        let mut i = pscat::Interp::with_page_scaled(w, h, scale).unwrap();
        i.run_str(src).unwrap();
        let px = i.gfx().pixmap.clone();
        cells_h.push(if zoom > 1 {
            cs::zoom_nearest(&px, zoom).unwrap()
        } else {
            px
        });
    }
    let caps_h: Vec<Option<tiny_skia::Pixmap>> = captions
        .iter()
        .map(|c| cs::render_text(c, 12, false).unwrap())
        .collect();
    let dims = |p: Option<&tiny_skia::Pixmap>| p.map_or((0, 0), |p| (p.width(), p.height()));
    let mut rows_h = vec![
        cs::RowSize {
            header: dims(Some(&label_h)),
            cells: vec![],
        },
        cs::RowSize {
            header: (0, 0),
            cells: vec![],
        },
    ];
    for (n, cell) in cells_h.iter().enumerate() {
        rows_h[rows_of[n]].cells.push(cs::CellSize {
            image: (cell.width(), cell.height()),
            caption: dims(caps_h[n].as_ref()),
        });
    }
    let layout_h = cs::Layout::new(&rows_h).unwrap();
    let mut sheet_h = layout_h.new_sheet().unwrap();
    let (x, y) = layout_h.header_xy[0];
    cs::blit_at(&mut sheet_h, x, y, &label_h);
    let mut slot = [0usize, 0];
    let mut slots = Vec::new();
    for (n, cell) in cells_h.iter().enumerate() {
        let (r, c) = (rows_of[n], slot[rows_of[n]]);
        slot[r] += 1;
        slots.push((r, c));
        let (x, y) = layout_h.image_xy[r][c];
        cs::frame(&mut sheet_h, x, y, cell.width(), cell.height());
        cs::blit_at(&mut sheet_h, x, y, cell);
        if let Some(cap) = &caps_h[n] {
            let (x, y) = layout_h.caption_xy[r][c];
            cs::blit_at(&mut sheet_h, x, y, cap);
        }
    }
    let hidden_png = pscat::png::encode(&sheet_h, 1.0).unwrap();

    // Supported.
    let label = sheet::text_strip("first", 16, true).unwrap().unwrap();
    let mut cells = Vec::new();
    for (_, src, (w, h), scale, zoom) in programs {
        let mut i = pscat::Interp::with_page_scaled(w, h, scale).unwrap();
        i.run_str(src).unwrap();
        let page = i.into_last_page();
        cells.push(if zoom > 1 {
            sheet::zoom(&page, zoom).unwrap()
        } else {
            page
        });
    }
    let caps: Vec<Option<pscat::Page>> = captions
        .iter()
        .map(|c| sheet::text_strip(c, 12, false).unwrap())
        .collect();
    let size = |p: Option<&pscat::Page>| p.map_or((0, 0), |p| (p.width(), p.height()));
    let mut row_cells = vec![vec![], vec![]];
    for (n, cell) in cells.iter().enumerate() {
        row_cells[rows_of[n]].push(CellSize::new(
            (cell.width(), cell.height()),
            size(caps[n].as_ref()),
        ));
    }
    let [first, second] = row_cells.try_into().unwrap();
    let rows = [
        RowSize::new(size(Some(&label)), first),
        RowSize::new((0, 0), second),
    ];
    let layout = SheetLayout::new(&rows).unwrap();
    assert_eq!(layout.size(), (layout_h.width, layout_h.height));
    assert_eq!(layout.header_xy(0), Some(layout_h.header_xy[0]));
    for &(r, c) in &slots {
        assert_eq!(layout.image_xy(r, c), Some(layout_h.image_xy[r][c]));
        assert_eq!(layout.caption_xy(r, c), Some(layout_h.caption_xy[r][c]));
    }
    let mut sheet = Sheet::new(&layout).unwrap();
    sheet.put_header(0, &label).unwrap();
    for (n, cell) in cells.iter().enumerate() {
        let (r, c) = slots[n];
        sheet.put_image(r, c, cell).unwrap();
        if let Some(cap) = &caps[n] {
            sheet.put_caption(r, c, cap).unwrap();
        }
    }
    assert_eq!(sheet.into_page().to_png().unwrap(), hidden_png);
}

#[test]
fn sheet_errors() {
    use pscat::sheet::{
        CellSize, MAX_SIDE_PX, MAX_TEXT_CHARS, MAX_TEXT_PX, RowSize, Sheet, SheetErrorKind,
        SheetLayout, text_strip, zoom,
    };
    let kind = |r: Result<SheetLayout, pscat::sheet::SheetError>| r.unwrap_err().kind();

    assert_eq!(kind(SheetLayout::new(&[])), SheetErrorKind::Empty);
    assert_eq!(
        kind(SheetLayout::new(&[RowSize::new((10, 10), vec![])])),
        SheetErrorKind::Empty
    );
    // Over the side limit: the message names the size, without the CLI's
    // advice about --dpi and --page.
    let wide = RowSize::new((0, 0), vec![CellSize::new((MAX_SIDE_PX, 10), (0, 0))]);
    let err = SheetLayout::new(&[wide]).unwrap_err();
    assert_eq!(err.kind(), SheetErrorKind::TooLarge);
    assert!(
        err.to_string().contains("over the 8000px-per-side limit"),
        "{err}"
    );
    assert!(!err.to_string().contains("--dpi"), "{err}");

    let row = RowSize::new((50, 20), vec![CellSize::new((30, 20), (40, 12))]);
    let mut sheet = Sheet::new(&SheetLayout::new(&[row]).unwrap()).unwrap();
    let page = |w, h| pscat::Page::blank(w, h).unwrap();
    // Wrong slots.
    for e in [
        sheet.put_header(1, &page(5, 5)),
        sheet.put_image(0, 1, &page(30, 20)),
        sheet.put_image(1, 0, &page(30, 20)),
        sheet.put_caption(0, 3, &page(5, 5)),
    ] {
        assert_eq!(e.unwrap_err().kind(), SheetErrorKind::NoSuchSlot);
    }
    // An image must be the planned size; a strip must fit its slot.
    for e in [
        sheet.put_image(0, 0, &page(31, 20)),
        sheet.put_image(0, 0, &page(29, 20)),
        sheet.put_header(0, &page(51, 20)),
        sheet.put_caption(0, 0, &page(40, 13)),
    ] {
        assert_eq!(e.unwrap_err().kind(), SheetErrorKind::WrongSize);
    }
    sheet.put_image(0, 0, &page(30, 20)).unwrap();
    sheet.put_header(0, &page(50, 20)).unwrap();
    sheet.put_header(0, &page(10, 5)).unwrap();
    sheet.put_caption(0, 0, &page(40, 12)).unwrap();

    // Text bounds.
    assert!(text_strip("", 12, false).unwrap().is_none());
    let long = "x".repeat(MAX_TEXT_CHARS + 1);
    for e in [
        text_strip(&long, 12, false),
        text_strip("a", 0, false),
        text_strip("a", MAX_TEXT_PX + 1, false),
    ] {
        assert_eq!(e.unwrap_err().kind(), SheetErrorKind::Text);
    }
    // A label is data, never code.
    let strip = text_strip(") showpage (x", 12, false).unwrap().unwrap();
    assert!(strip.width() > 20);

    // Zoom bounds.
    assert!(zoom(&page(2000, 10), 4).is_some());
    assert!(zoom(&page(2001, 10), 4).is_none());
    assert!(zoom(&page(2, 2), 0).is_none());
}

// --- 3. chalkboards -------------------------------------------------------

/// Before: `chalk::Config` built with `from_value`/`apply`/`check`, then
/// `chalk::driver`, `chalk::explain`. After: `chalkboard::BoardConfig`,
/// `Driver`, `explain`. Same program text, same page, same explanations.
#[test]
fn chalk_driver_matches_the_hidden_generator() {
    use pscat::chalkboard::BoardConfig;
    let cfg = json!({"layout": {"width": 500, "height": 240, "dpi": 72}, "seed": 9});
    let latex = r"\int_0^1 x^2\,dx = \frac{1}{3} \\ a^2 + b^2 = c^2";

    let hidden =
        pscat::chalk::driver(latex, &pscat::chalk::Config::from_value(&cfg).unwrap()).unwrap();
    let mut board = BoardConfig::from_value(&cfg).unwrap();
    board.check().unwrap();
    assert_eq!(board.page_size(), (500, 240));
    assert_eq!(board.dpi(), 72.0);
    assert!(board.has_board());
    let driver = board.driver(latex).unwrap();
    assert_eq!(driver.source(), hidden.source);
    assert_eq!(driver.page_size(), (hidden.width, hidden.height));
    assert_eq!(driver.dpi(), hidden.dpi);
    assert!(!driver.transparent());

    // Laying keys over one at a time is the same as one object.
    board = BoardConfig::default();
    board.apply(&json!({"layout": {"width": 500}})).unwrap();
    board
        .apply(&json!({"layout": {"height": 240, "dpi": 72}}))
        .unwrap();
    board.apply(&json!({"seed": 9})).unwrap();
    assert_eq!(board, BoardConfig::from_value(&cfg).unwrap());
    assert_eq!(
        BoardConfig::from_json(&cfg.to_string()).unwrap(),
        BoardConfig::from_value(&cfg).unwrap()
    );

    // explain() agrees with the hidden one.
    for err in [
        pscat::PsError::Undefined("mk-unsupported-command:\\aleph".into()),
        pscat::PsError::Undefined("mk-missing-argument:\\frac".into()),
        pscat::PsError::Undefined("mk-oddity".into()),
        pscat::PsError::Undefined("moveto2".into()),
        pscat::PsError::Typecheck,
    ] {
        let hidden = pscat::chalk::explain(&err);
        let supported = pscat::chalkboard::explain(&err);
        assert_eq!(hidden.is_some(), supported.is_some(), "{err:?}");
        if let (Some(h), Some(s)) = (hidden, supported) {
            assert_eq!(h.code(), s.code());
            assert_eq!(h.to_string(), s.to_string());
            assert!(!s.is_input_error());
        }
    }
    let e = pscat::chalkboard::explain(&pscat::PsError::Undefined(
        "mk-unsupported-character:@".into(),
    ))
    .unwrap();
    assert_eq!(
        (e.code().as_str(), e.token()),
        ("unsupported_character", Some("@"))
    );
    let e = pscat::chalkboard::explain(&pscat::PsError::Undefined("mk-oddity".into())).unwrap();
    assert_eq!(e.token(), None);
}

/// pscat-mcp's `chalkboard_math` takes some config keys as flat arguments
/// and names them in errors by replacing the config path in the message.
/// `ChalkError::message()` is the unprefixed text that works on, and gives
/// every message pscat-mcp's own tests expect.
#[test]
fn chalk_config_errors_carry_the_key_path() {
    use pscat::chalkboard::BoardConfig;
    // (the value laid over the defaults, the config path, pscat-mcp's
    // argument name, what pscat-mcp's message must contain)
    let cases = [
        (
            json!({"layout": {"size": 0}}),
            "layout.size",
            "size",
            "size must be",
        ),
        (
            json!({"chalk": {"color": "red"}}),
            "chalk.color",
            "color",
            "color must be one of",
        ),
        (json!({"nope": 1}), "", "", "unknown key nope"),
        (json!({"layout": {"width": 9000}}), "", "", "layout.width"),
        (
            json!({"seed": 2_000_000_000_i64}),
            "seed",
            "seed",
            "seed must be",
        ),
    ];
    for (value, path, arg, want) in cases {
        let mut board = BoardConfig::default();
        let err = board.apply(&value).unwrap_err();
        assert_eq!(err.code(), "config");
        assert!(err.is_input_error());
        assert_eq!(err.to_string(), format!("config: {}", err.message()));
        let msg = if path.is_empty() {
            err.message()
        } else {
            err.message().replace(path, arg)
        };
        assert!(msg.contains(want), "{value}: {msg}");
        if want == "color must be one of" {
            assert!(msg.starts_with(want), "{msg}");
        }
    }
    // The whole-config check: a page with no room to write.
    let mut board = BoardConfig::default();
    board
        .apply(&json!({"layout": {"width": 50, "height": 50}}))
        .unwrap();
    let err = board.check().unwrap_err();
    assert!(err.message().contains("no room to write"), "{err}");
    assert_eq!(board.driver("x").unwrap_err().code(), "config");
    // No formula.
    let err = BoardConfig::default()
        .driver("  % just a comment")
        .unwrap_err();
    assert_eq!(err.code(), "empty");
    assert!(err.is_input_error());
    assert!(BoardConfig::from_json("{").unwrap_err().is_input_error());
}

/// Before: `Gfx::set_transparent_background`, `pdf::scan_document_info` and
/// `Gfx::set_pdf_info`, as pscat-mcp's worker calls them. After:
/// `Driver::prepare`, called after `enable_pdf`.
#[test]
fn prepare_sets_the_pdf_title_and_a_transparent_page() {
    use pscat::chalkboard::BoardConfig;
    let board = BoardConfig::from_value(&json!({
        "layout": {"width": 300, "height": 150, "dpi": 72},
        "board": false
    }))
    .unwrap();
    assert!(!board.has_board());
    let driver = board.driver("x^2").unwrap();
    assert!(driver.transparent());
    let (w, h) = driver.page_size();

    let mut interp = pscat::Interp::with_page_scaled(w, h, driver.dpi() / 72.0).unwrap();
    interp.enable_pdf();
    driver.prepare(&mut interp);
    interp.run_str(driver.source()).unwrap();
    let pdf = interp.pdf_document().unwrap();
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains("/Title (Chalkboard)"), "PDF has its title");
    // No board: the page is transparent where there is no chalk.
    let page = interp.into_last_page();
    assert_eq!(page.rgba8()[3], 0, "the corner is transparent");

    // The same, the hidden way, gives the same PDF.
    let mut hidden = pscat::Interp::with_page_scaled(w, h, driver.dpi() / 72.0).unwrap();
    hidden.enable_pdf();
    hidden.gfx_mut().set_transparent_background();
    let (title, author) = pscat::pdf::scan_document_info(driver.source().as_bytes());
    hidden.gfx_mut().set_pdf_info(title, author);
    hidden.run_str(driver.source()).unwrap();
    assert_eq!(hidden.pdf_document().unwrap(), pdf);
}

// --- 4. the catalog -------------------------------------------------------

/// Before: `capabilities::payload_json()`. After: `catalog::capabilities()`.
#[test]
fn catalog_is_the_capabilities_payload() {
    let catalog = pscat::catalog::capabilities();
    assert_eq!(catalog, pscat::capabilities::payload_json());
    for key in [
        "pscat_version",
        "catalog_signature",
        "font_packs",
        "capabilities",
    ] {
        assert!(catalog.get(key).is_some(), "{key}");
    }
}
