//! Font machinery: the bundled-font registry, the current-font graphics
//! state, and the glyph → path engine behind `show`/`stringwidth`/
//! `charpath`. The architecture (and its deliberate deviations) is
//! documented in `docs/dev/FONTS.md` at the repo root.
//!
//! The seam between PostScript font *dicts* (which programs own and
//! mutate) and Rust-side glyph data is the `FID` entry: an index into
//! `BUILTINS`. Faces are re-parsed per show call rather than cached —
//! `ttf_parser::Face` borrows its data, and parsing is table-directory
//! validation, not glyph work; the Stage 6 glyph-cache task measures
//! whether even that matters.

use std::cell::RefCell;
use std::rc::Rc;

use ttf_parser::{Face, GlyphId};

use crate::encodings;
use crate::error::PsError;
use crate::gfx::{DevPoint, Gfx, PsPath};
use crate::object::{Dict, Object, PsString, Value};

/// `FID` value for fonts with no outline source we can drive yet
/// (Type 3 dicts accepted by `definefont`); `show` reports invalidfont.
pub(crate) const FID_NONE: i64 = -1;

/// Registry index used when `findfont` substitutes for an unknown name.
pub(crate) const SUBSTITUTE: i64 = 0;

struct Builtin {
    ps_name: &'static str,
    data: &'static [u8],
}

/// The standard base-13 names minus Symbol, backed by Liberation faces,
/// which are metric-compatible with the Adobe originals. Symbol and the
/// rest of the standard 35 are in [`EMBEDDED`].
static BUILTINS: [Builtin; 12] = [
    Builtin {
        ps_name: "Helvetica",
        data: include_bytes!("../fonts/LiberationSans-Regular.ttf"),
    },
    Builtin {
        ps_name: "Helvetica-Bold",
        data: include_bytes!("../fonts/LiberationSans-Bold.ttf"),
    },
    Builtin {
        ps_name: "Helvetica-Oblique",
        data: include_bytes!("../fonts/LiberationSans-Italic.ttf"),
    },
    Builtin {
        ps_name: "Helvetica-BoldOblique",
        data: include_bytes!("../fonts/LiberationSans-BoldItalic.ttf"),
    },
    Builtin {
        ps_name: "Times-Roman",
        data: include_bytes!("../fonts/LiberationSerif-Regular.ttf"),
    },
    Builtin {
        ps_name: "Times-Bold",
        data: include_bytes!("../fonts/LiberationSerif-Bold.ttf"),
    },
    Builtin {
        ps_name: "Times-Italic",
        data: include_bytes!("../fonts/LiberationSerif-Italic.ttf"),
    },
    Builtin {
        ps_name: "Times-BoldItalic",
        data: include_bytes!("../fonts/LiberationSerif-BoldItalic.ttf"),
    },
    Builtin {
        ps_name: "Courier",
        data: include_bytes!("../fonts/LiberationMono-Regular.ttf"),
    },
    Builtin {
        ps_name: "Courier-Bold",
        data: include_bytes!("../fonts/LiberationMono-Bold.ttf"),
    },
    Builtin {
        ps_name: "Courier-Oblique",
        data: include_bytes!("../fonts/LiberationMono-Italic.ttf"),
    },
    Builtin {
        ps_name: "Courier-BoldOblique",
        data: include_bytes!("../fonts/LiberationMono-BoldItalic.ttf"),
    },
];

/// The other 23 faces of the standard 35 (issue #757): the six TeX Gyre
/// families behind Palatino, Bookman, New Century Schoolbook, Avant
/// Garde, Zapf Chancery and Helvetica-Narrow (GUST Font License), and
/// the URW Symbol and ZapfDingbats faces (OFL-1.1). Keyed by file stem,
/// the same key a pack file of that name loads under, so `ALIASES`, the
/// Symbol/Dingbats encodings and `FontName` are what an installed pack
/// gave. They register as catalog faces on first use
/// ([`embedded_fid`]), after any font file on disk: an override or an
/// installed pack holding the same stem still wins.
static EMBEDDED: [(&str, &[u8]); 23] = [
    (
        "texgyrepagella-regular",
        include_bytes!("../fonts/base35/TeXGyre-pagella/texgyrepagella-regular.otf"),
    ),
    (
        "texgyrepagella-bold",
        include_bytes!("../fonts/base35/TeXGyre-pagella/texgyrepagella-bold.otf"),
    ),
    (
        "texgyrepagella-italic",
        include_bytes!("../fonts/base35/TeXGyre-pagella/texgyrepagella-italic.otf"),
    ),
    (
        "texgyrepagella-bolditalic",
        include_bytes!("../fonts/base35/TeXGyre-pagella/texgyrepagella-bolditalic.otf"),
    ),
    (
        "texgyrebonum-regular",
        include_bytes!("../fonts/base35/TeXGyre-bonum/texgyrebonum-regular.otf"),
    ),
    (
        "texgyrebonum-bold",
        include_bytes!("../fonts/base35/TeXGyre-bonum/texgyrebonum-bold.otf"),
    ),
    (
        "texgyrebonum-italic",
        include_bytes!("../fonts/base35/TeXGyre-bonum/texgyrebonum-italic.otf"),
    ),
    (
        "texgyrebonum-bolditalic",
        include_bytes!("../fonts/base35/TeXGyre-bonum/texgyrebonum-bolditalic.otf"),
    ),
    (
        "texgyreschola-regular",
        include_bytes!("../fonts/base35/TeXGyre-schola/texgyreschola-regular.otf"),
    ),
    (
        "texgyreschola-bold",
        include_bytes!("../fonts/base35/TeXGyre-schola/texgyreschola-bold.otf"),
    ),
    (
        "texgyreschola-italic",
        include_bytes!("../fonts/base35/TeXGyre-schola/texgyreschola-italic.otf"),
    ),
    (
        "texgyreschola-bolditalic",
        include_bytes!("../fonts/base35/TeXGyre-schola/texgyreschola-bolditalic.otf"),
    ),
    (
        "texgyreadventor-regular",
        include_bytes!("../fonts/base35/TeXGyre-adventor/texgyreadventor-regular.otf"),
    ),
    (
        "texgyreadventor-bold",
        include_bytes!("../fonts/base35/TeXGyre-adventor/texgyreadventor-bold.otf"),
    ),
    (
        "texgyreadventor-italic",
        include_bytes!("../fonts/base35/TeXGyre-adventor/texgyreadventor-italic.otf"),
    ),
    (
        "texgyreadventor-bolditalic",
        include_bytes!("../fonts/base35/TeXGyre-adventor/texgyreadventor-bolditalic.otf"),
    ),
    (
        "texgyrechorus-mediumitalic",
        include_bytes!("../fonts/base35/TeXGyre-chorus/texgyrechorus-mediumitalic.otf"),
    ),
    (
        "texgyreheroscn-regular",
        include_bytes!("../fonts/base35/TeXGyre-heroscn/texgyreheroscn-regular.otf"),
    ),
    (
        "texgyreheroscn-bold",
        include_bytes!("../fonts/base35/TeXGyre-heroscn/texgyreheroscn-bold.otf"),
    ),
    (
        "texgyreheroscn-italic",
        include_bytes!("../fonts/base35/TeXGyre-heroscn/texgyreheroscn-italic.otf"),
    ),
    (
        "texgyreheroscn-bolditalic",
        include_bytes!("../fonts/base35/TeXGyre-heroscn/texgyreheroscn-bolditalic.otf"),
    ),
    (
        "StandardSymbolsPS",
        include_bytes!("../fonts/base35/URW-symbols/StandardSymbolsPS.otf"),
    ),
    (
        "D050000L",
        include_bytes!("../fonts/base35/URW-symbols/D050000L.otf"),
    ),
];

pub(crate) fn builtin_index(name: &str) -> Option<i64> {
    BUILTINS
        .iter()
        .position(|b| b.ps_name == name)
        .map(|i| i as i64)
}

/// Which Encoding array a catalog face's font dict gets. The two URW
/// symbol faces carry their own PLRM encodings; everything else is
/// StandardEncoding like the builtins. `Unicode` is a deliberate
/// deviation from the byte-indexed model (see the "Unicode-mode
/// catalog faces" addendum in docs/dev/FONTS.md): fonts whose native script
/// can't fit in a 256-entry Encoding (Hangul, kana/kanji, Thai) get a
/// `show` pipeline that decodes the string as UTF-8 and maps each
/// codepoint straight to a glyph via the face's `cmap`, bypassing
/// Encoding/glyph-name resolution entirely. The dict still carries a
/// StandardEncoding array for shape consistency, but it's unread.
#[derive(Clone, Copy)]
enum CatalogEncoding {
    Standard,
    Symbol,
    Dingbats,
    Unicode,
}

/// A face loaded from `fonts/catalog/` at findfont time. The file bytes
/// and the parsed `Face` are deliberately leaked: a catalog font loads
/// once and lives for the process, giving it the same `'static` shape
/// as the builtins (the systemdict-cycle doctrine — bounded,
/// process-lifetime, not a churn leak).
struct CatalogFace {
    /// File stem, the lookup key (`texgyrepagella-regular`,
    /// `EBGaramond-Regular`, …). Matched case-insensitively.
    key: String,
    /// The face's real PostScript name (name table id 6) when present,
    /// else the stem; becomes `FontName` in the font dict.
    ps_name: String,
    face: &'static Face<'static>,
    encoding: CatalogEncoding,
}

/// Loaded catalog faces; index i is FID `BUILTINS.len() + i`. RwLock
/// only because statics must be Sync — the interpreter is
/// single-threaded.
static CATALOG: std::sync::RwLock<Vec<CatalogFace>> = std::sync::RwLock::new(Vec::new());

/// Requested-name → catalog file stem for the names found PostScript
/// actually uses: the rest of the standard 35 (Palatino, Bookman,
/// New Century Schoolbook, Avant Garde, Zapf Chancery,
/// Helvetica-Narrow, Symbol, ZapfDingbats — backed by the TeX Gyre and
/// URW libre metric-compatible faces), plus a few tasteful shorthands.
static ALIASES: &[(&str, &str)] = &[
    ("Palatino-Roman", "texgyrepagella-regular"),
    ("Palatino-Bold", "texgyrepagella-bold"),
    ("Palatino-Italic", "texgyrepagella-italic"),
    ("Palatino-BoldItalic", "texgyrepagella-bolditalic"),
    ("Palatino", "texgyrepagella-regular"),
    ("Bookman-Light", "texgyrebonum-regular"),
    ("Bookman-Demi", "texgyrebonum-bold"),
    ("Bookman-LightItalic", "texgyrebonum-italic"),
    ("Bookman-DemiItalic", "texgyrebonum-bolditalic"),
    ("Bookman", "texgyrebonum-regular"),
    ("NewCenturySchlbk-Roman", "texgyreschola-regular"),
    ("NewCenturySchlbk-Bold", "texgyreschola-bold"),
    ("NewCenturySchlbk-Italic", "texgyreschola-italic"),
    ("NewCenturySchlbk-BoldItalic", "texgyreschola-bolditalic"),
    ("NewCenturySchoolbook", "texgyreschola-regular"),
    ("AvantGarde-Book", "texgyreadventor-regular"),
    ("AvantGarde-Demi", "texgyreadventor-bold"),
    ("AvantGarde-BookOblique", "texgyreadventor-italic"),
    ("AvantGarde-DemiOblique", "texgyreadventor-bolditalic"),
    ("AvantGarde", "texgyreadventor-regular"),
    ("ZapfChancery-MediumItalic", "texgyrechorus-mediumitalic"),
    ("ZapfChancery", "texgyrechorus-mediumitalic"),
    ("Helvetica-Narrow", "texgyreheroscn-regular"),
    ("Helvetica-Narrow-Bold", "texgyreheroscn-bold"),
    ("Helvetica-Narrow-Oblique", "texgyreheroscn-italic"),
    ("Helvetica-Narrow-BoldOblique", "texgyreheroscn-bolditalic"),
    ("Symbol", "StandardSymbolsPS"),
    ("ZapfDingbats", "D050000L"),
    ("Garamond", "EBGaramond-Regular"),
    ("Baskerville", "LibreBaskerville-Regular"),
    ("UnifrakturMaguntia", "UnifrakturMaguntia-Book"),
];

/// What `findfont` found for a name: the face's `FID`, and, when the
/// name belongs to a font pack that is not installed, the note
/// `findfont` prints about the stand-in it used.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Resolved {
    pub fid: i64,
    pub stand_in_note: Option<String>,
}

impl Resolved {
    fn found(fid: i64) -> Self {
        Resolved {
            fid,
            stand_in_note: None,
        }
    }
}

/// findfont's name resolution: builtins, then the disk catalog (via
/// aliases and file stems), then the embedded rest of the standard 35
/// ([`EMBEDDED`]), then a stand-in face. The disk catalog is
/// filesystem-backed, so wasm builds check the faces registered with
/// [`register_font_bytes`] in its place.
///
/// A name that belongs to a font pack (`fonts/packs.json`, issue #599)
/// but is not on disk gets the closest built-in face instead of an error
/// (issue #728; see [`stand_in_for`] for the table), with a note naming
/// the pack to install; installed, the pack's own face is used and
/// nothing changes. Names no pack has substitute Helvetica without a
/// note, Ghostscript-style.
pub(crate) fn resolve(requested: &str) -> Resolved {
    if let Some(fid) = builtin_index(requested) {
        return Resolved::found(fid);
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(fid) = catalog_fid(requested) {
        return Resolved::found(fid);
    }
    // wasm has no disk catalog; its catalog is only the faces the caller
    // registered with `register_font_bytes` (the npm package's
    // `addFont`, #566). Same stems as the disk lookup, so an alias like
    // `/Garamond` finds an `EBGaramond-Regular` registered under that name.
    #[cfg(target_arch = "wasm32")]
    {
        let (stem, fallback) = wanted_stems(requested);
        if let Some(fid) = loaded_fid(stem, requested).or_else(|| loaded_fid(&fallback, requested))
        {
            return Resolved::found(fid);
        }
    }
    if let Some(fid) = embedded_fid(requested) {
        return Resolved::found(fid);
    }
    if let Some((pack, stem)) = missing_face(requested) {
        let stand_in = stand_in_for(requested);
        let fid = builtin_index(stand_in).unwrap_or(SUBSTITUTE);
        return Resolved {
            fid,
            stand_in_note: Some(stand_in_note(requested, pack, &stem, stand_in)),
        };
    }
    Resolved::found(SUBSTITUTE)
}

/// The embedded standard-35 face ([`EMBEDDED`]) for a requested name,
/// registering it on first use. Looked up by the same stems as the disk
/// catalog, case-insensitively, so `/Palatino-Roman`, `/Symbol` and
/// `/texgyrepagella-regular` all find their face.
/// Whether `stem` names an embedded standard-35 face, case-insensitively
/// (for `--capabilities`: an alias to one resolves everywhere).
pub(crate) fn is_embedded_stem(stem: &str) -> bool {
    EMBEDDED.iter().any(|(k, _)| k.eq_ignore_ascii_case(stem))
}

fn embedded_fid(requested: &str) -> Option<i64> {
    let (stem, fallback) = wanted_stems(requested);
    let (key, data) = EMBEDDED
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(stem))
        .or_else(|| {
            EMBEDDED
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(&fallback))
        })?;
    loaded_fid(key, key).or_else(|| register_face((*key).to_string(), data))
}

/// Whether the face `requested` names has a glyph for `c`, by the
/// face's `cmap`. For the Unicode-mode catalog faces, which draw a
/// missing character as nothing at all; `false` when the face does not
/// resolve, including a face whose pack is not installed (its stand-in
/// has no say about the real face's coverage).
pub(crate) fn has_glyph(requested: &str, c: char) -> bool {
    let r = resolve(requested);
    r.stand_in_note.is_none()
        && face_for(r.fid)
            .ok()
            .is_some_and(|face| face.glyph_index(c).is_some())
}

/// The FID of an already-registered catalog face whose key is `want`
/// (case-insensitively) or whose PostScript name is `requested`.
fn loaded_fid(want: &str, requested: &str) -> Option<i64> {
    let cat = CATALOG.read().unwrap();
    cat.iter()
        .position(|c| c.key.eq_ignore_ascii_case(want) || c.ps_name == requested)
        .map(|i| (BUILTINS.len() + i) as i64)
}

/// The file stems a requested name is looked up by: the alias target
/// (or the name itself), then that with `-Regular`, so `/Bangers
/// findfont` means the family's regular face. Shared by the lookup and
/// the missing-pack check so the two cannot disagree.
fn wanted_stems(requested: &str) -> (&str, String) {
    let stem = ALIASES
        .iter()
        .find(|(k, _)| *k == requested)
        .map_or(requested, |(_, v)| *v);
    (stem, format!("{stem}-Regular"))
}

/// The pack holding the face `requested` names, if any pack does.
#[cfg(not(target_arch = "wasm32"))]
fn missing_pack(requested: &str) -> Option<&'static str> {
    missing_face(requested).map(|(pack, _)| pack)
}

/// The pack holding the face `requested` names and the font file stem
/// it matched (`Bangers-Regular` for `/Bangers`): what `addFont` must
/// be given to supply the face.
fn missing_face(requested: &str) -> Option<(&'static str, String)> {
    let (stem, fallback) = wanted_stems(requested);
    [stem.to_string(), fallback]
        .into_iter()
        .find_map(|s| crate::fontpacks::pack_for_stem(&s).map(|pack| (pack, s)))
}

/// The built-in face standing in for `requested`, a name whose font
/// pack is not installed (issue #728). The closest family keeping the
/// weight and slant, the way Ghostscript's Fontmap does:
///
/// - the monospaced faces (Courier Prime, Special Elite, JetBrains Mono)
///   become Courier, so column layouts hold;
/// - every other pack face (display, Korean, Japanese, Thai) becomes
///   Helvetica, as an unknown name does.
///
/// The standard 35 never get here: all of them are compiled in
/// (`BUILTINS` and [`EMBEDDED`], issue #757).
fn stand_in_for(requested: &str) -> &'static str {
    let (stem, _) = wanted_stems(requested);
    let stem = stem.to_ascii_lowercase();
    let bold = stem.contains("bold") || stem.contains("demi");
    let italic = stem.ends_with("italic") || stem.ends_with("oblique");
    let pick = |plain, b, i, bi| match (bold, italic) {
        (false, false) => plain,
        (true, false) => b,
        (false, true) => i,
        (true, true) => bi,
    };
    let courier = || {
        pick(
            "Courier",
            "Courier-Bold",
            "Courier-Oblique",
            "Courier-BoldOblique",
        )
    };
    let helvetica = || {
        pick(
            "Helvetica",
            "Helvetica-Bold",
            "Helvetica-Oblique",
            "Helvetica-BoldOblique",
        )
    };
    if ["courierprime", "specialelite", "jetbrainsmono"]
        .iter()
        .any(|f| stem.starts_with(f))
    {
        courier()
    } else {
        // Display, Korean, Japanese and Thai faces.
        helvetica()
    }
}

/// The hint for a face that no installed pack or override provides but a
/// font pack lists, or `None` when the face loads (builtin, catalog or
/// installed pack) or no pack has it. For callers that must know a face
/// is truly missing rather than stood in for: `pscat sheet`'s script
/// check.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn uninstalled_pack_hint(requested: &str) -> Option<String> {
    if builtin_index(requested).is_some() || catalog_fid(requested).is_some() {
        return None;
    }
    missing_pack(requested).map(|pack| {
        format!(
            "pscat: font /{requested} is in the {pack} font pack, which is not installed; \
             run `pscat fonts install` (or `pscat fonts install --pack {pack}`)"
        )
    })
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn uninstalled_pack_hint(_requested: &str) -> Option<String> {
    None
}

/// The one-line note for a face whose pack is not installed. The wasm
/// build has no packs to install (and no stderr): its note, which the
/// npm package returns in `warnings` (issue #750), says to load the
/// face with `addFont` under its file stem instead.
#[cfg(not(target_arch = "wasm32"))]
fn stand_in_note(requested: &str, pack: &str, _stem: &str, stand_in: &str) -> String {
    format!(
        "pscat: warning: font /{requested} is in the {pack} font pack, which is not installed; \
         using {stand_in}. Run `pscat fonts install --pack {pack}` (or `pscat fonts install`) for the real face"
    )
}

#[cfg(target_arch = "wasm32")]
fn stand_in_note(requested: &str, pack: &str, stem: &str, stand_in: &str) -> String {
    browser_stand_in_note(requested, pack, stem, stand_in)
}

/// The browser build's note (see [`stand_in_note`]); not target-gated so
/// a native test can pin its wording.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn browser_stand_in_note(requested: &str, pack: &str, stem: &str, stand_in: &str) -> String {
    format!(
        "pscat: warning: font /{requested} is in the {pack} font pack, which is not built into the browser build; \
         using {stand_in}. Load the real face with addFont('{stem}', bytes)"
    )
}

/// The directories catalog faces load from, in priority order:
/// `fonts/catalog/` overrides, then installed packs
/// (`crate::fontpacks` module doc).
#[cfg(not(target_arch = "wasm32"))]
fn catalog_roots() -> Vec<std::path::PathBuf> {
    crate::fontpacks::font_dirs()
        .into_iter()
        .map(|d| d.path)
        .collect()
}

/// Find (loading if necessary) the catalog face for a requested name.
/// Lookup: alias → stem, already-loaded (by key or PostScript name),
/// then, root by root in priority order, a scan of its family
/// directories for `<stem>.ttf`/`.otf`/`.ttc`, then `<stem>-Regular`.
#[cfg(not(target_arch = "wasm32"))]
fn catalog_fid(requested: &str) -> Option<i64> {
    let (stem, fallback) = wanted_stems(requested);
    if let Some(fid) = loaded_fid(stem, requested) {
        return Some(fid);
    }
    let roots = catalog_roots();
    let mut found: Option<std::path::PathBuf> = None;
    // Roots outer, stems inner: a higher-priority directory's
    // `<stem>-Regular` beats a lower one's exact `<stem>`.
    'outer: for root in &roots {
        for want in [stem, fallback.as_str()] {
            for family in std::fs::read_dir(root).into_iter().flatten().flatten() {
                let dir = family.path();
                if !dir.is_dir() {
                    continue;
                }
                for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                    let path = entry.path();
                    let ext = path.extension().and_then(|e| e.to_str());
                    if !matches!(ext, Some("ttf" | "otf" | "ttc")) {
                        continue;
                    }
                    let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    if file_stem.eq_ignore_ascii_case(want) {
                        found = Some(path);
                        break 'outer;
                    }
                }
            }
        }
    }
    load_catalog_face(&found?)
}

/// Read, leak, parse, register. Returns None (→ substitution) on any
/// I/O or parse failure — catalog fonts are data, and found files
/// should render regardless. `.ttc` collections always parse face
/// index 0 — there's no naming convention here for picking a
/// different face out of a collection, so a multi-face `.ttc` only
/// ever exposes its first face. Verified against a synthetic two-face
/// `.ttc` (not checked in — built with `fonttools`' `TTCollection`):
/// face 0's outlines render correctly. One caveat found doing that:
/// `face.names()` came back empty for the synthetic collection's
/// sub-face, so `FontName` fell back to the file stem rather than the
/// face's own PostScript name — cosmetic only (glyph resolution was
/// unaffected), but means a real bundled `.ttc`'s `FontName` should be
/// spot-checked rather than assumed. No collection is bundled today.
#[cfg(not(target_arch = "wasm32"))]
fn load_catalog_face(path: &std::path::Path) -> Option<i64> {
    let key = path.file_stem()?.to_str()?.to_string();
    let data: &'static [u8] = Box::leak(std::fs::read(path).ok()?.into_boxed_slice());
    register_face(key, data)
}

/// Register a font file's bytes as a catalog face under `name`, the
/// way a file `<name>.ttf` in a font pack would load: `name` is the
/// lookup key (so the pack's file stem, e.g. `NotoSansKR-Regular`,
/// gets the same aliases and encoding as the installed pack would),
/// and the face's own PostScript name also finds it. This is how the
/// wasm build, which has no disk, gets faces beyond the embedded
/// Liberation core (`ps_add_font`, the npm package's `addFont`, #566).
///
/// Registering a name that is already loaded keeps the first face and
/// returns its FID. `Err` when the bytes are not a font. The bytes are
/// leaked like every catalog face's (process lifetime, loaded once).
#[cfg_attr(not(any(target_arch = "wasm32", test)), allow(dead_code))]
pub(crate) fn register_font_bytes(name: &str, data: Vec<u8>) -> Result<i64, String> {
    if name.is_empty() {
        return Err("empty font name".to_string());
    }
    if let Some(fid) = loaded_fid(name, name) {
        return Ok(fid);
    }
    if parse_face(&data, 0).is_none() {
        return Err(format!("{name}: not a TrueType/OpenType font"));
    }
    let data: &'static [u8] = Box::leak(data.into_boxed_slice());
    register_face(name.to_string(), data).ok_or_else(|| format!("{name}: cannot parse the font"))
}

/// `Face::parse` for bytes we did not ship (a found font file, `addFont`).
///
/// A collection (`ttcf`) header that claims more offsets than the file has
/// bytes is refused here, before ttf-parser sees it: its offset-table read
/// carries a `debug_assert!` that a hostile count trips (found by the
/// `truetype` fuzz target, issue #704). A release build would return `None`
/// anyway; a debug build, which is what `cargo test` and the fuzzer run,
/// panicked.
fn parse_face(data: &[u8], index: u32) -> Option<Face<'_>> {
    if data.starts_with(b"ttcf") {
        let count = data.get(8..12)?;
        let count = u64::from(u32::from_be_bytes([count[0], count[1], count[2], count[3]]));
        if 12 + 4 * count > data.len() as u64 {
            return None;
        }
    }
    // A panic inside the parser (a `debug_assert!` on an offset in a debug
    // build; the fuzzer found several) is a font that does not load, not a
    // dead interpreter. Release builds compile those asserts out.
    crate::guard::catch(|| Face::parse(data, index).ok()).flatten()
}

/// Parse, pin a variable font to Regular, classify, register. Shared by
/// the disk catalog and `register_font_bytes`.
fn register_face(key: String, data: &'static [u8]) -> Option<i64> {
    let (face, encoding, ps_name) = prepare_face(&key, data)?;
    let face: &'static Face<'static> = Box::leak(Box::new(face));
    let mut cat = CATALOG.write().unwrap();
    cat.push(CatalogFace {
        key,
        ps_name,
        face,
        encoding,
    });
    Some((BUILTINS.len() + cat.len() - 1) as i64)
}

/// The pure half of [`register_face`]: parse `data` as face 0, pin a
/// variable font's `wght` axis to Regular, and work out the encoding mode
/// and `FontName` for the file stem `key`. Nothing is leaked or registered,
/// so the fuzz target (`fuzzing::truetype`) can run it on every input
/// without growing the process.
fn prepare_face<'a>(key: &str, data: &'a [u8]) -> Option<(Face<'a>, CatalogEncoding, String)> {
    let mut face = parse_face(data, 0)?;
    // Some catalog files are variable fonts whose *default* named
    // instance isn't Regular — Noto Sans KR/JP ship with wght's default
    // at Thin (100), not 400. Without pinning it, the catalog would
    // silently render (and report) the file's default weight rather
    // than the Regular cut the `-Regular` filename promises. This is a
    // general policy for any variable-font catalog addition, not
    // special-cased to today's three files.
    if face.is_variable() {
        let wght = ttf_parser::Tag::from_bytes(b"wght");
        if let Some(axis) = face.variation_axes().into_iter().find(|a| a.tag == wght) {
            // Not `f32::clamp`: it panics when a hostile `fvar` has min > max
            // (or a NaN bound). Taking the bounds one at a time cannot.
            let target = 400.0f32.max(axis.min_value).min(axis.max_value);
            face.set_variation(wght, target);
        }
    }
    let encoding = match key {
        "StandardSymbolsPS" => CatalogEncoding::Symbol,
        "D050000L" => CatalogEncoding::Dingbats,
        "NotoSansKR-Regular"
        | "NotoSansJP-Regular"
        | "NotoSansThai-Regular"
        | "NanumBrushScript-Regular" => CatalogEncoding::Unicode,
        _ => CatalogEncoding::Standard,
    };
    // The name table's PostScript name (id 6) is a fixed string baked
    // into the file at whatever instance the font author called
    // "default" — for the variable fonts above that's "…-Thin", which
    // would contradict the pinned-Regular rendering above. The file
    // stem (our own naming convention) is the honest `FontName` for
    // those; everything else keeps trusting the face's own name table.
    let ps_name = if matches!(encoding, CatalogEncoding::Unicode) {
        key.to_string()
    } else {
        face.names()
            .into_iter()
            .find(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME)
            .and_then(|n| n.to_string())
            .unwrap_or_else(|| key.to_string())
    };
    Some((face, encoding, ps_name))
}

/// What the `truetype` fuzz target runs on a font file's bytes: the load
/// path (`prepare_face`, as `register_font_bytes` and the catalog do it),
/// then glyph lookup by name, by code point and by id, widths, and every
/// outline painted, stroked into a path and measured through the same
/// pipeline `show`, `charpath` and `stringwidth` use. Registers nothing.
pub(crate) fn fuzz_exercise_face(gfx: &mut Gfx, data: &[u8], key: &str) {
    if let Some((face, encoding, _name)) = prepare_face(key, data) {
        exercise_face(gfx, &face, encoding);
    }
    // `prepare_face` loads face 0 only, as the catalog does; the other
    // faces of a collection get the plain parse.
    for index in 1..ttf_parser::fonts_in_collection(data).unwrap_or(1).min(4) {
        if let Some(face) = parse_face(data, index) {
            exercise_face(gfx, &face, CatalogEncoding::Standard);
        }
    }
}

fn exercise_face(gfx: &mut Gfx, face: &Face, encoding: CatalogEncoding) {
    let n = face.number_of_glyphs();
    let step = (n / 24).max(1);
    let fm = [0.01, 0.0, 0.0, 0.01, 0.0, 0.0];
    for (i, g) in (0..n).step_by(usize::from(step)).enumerate() {
        let mode = match i % 3 {
            0 => ShowMode::Paint,
            1 => ShowMode::Charpath,
            _ => ShowMode::Width,
        };
        let pen = DevPoint { x: 8.0, y: 40.0 };
        paint_resolved_glyph(gfx, face, fm, Some(GlyphId(g)), &mode, pen);
        let _ = face.glyph_name(GlyphId(g));
        let _ = face.glyph_bounding_box(GlyphId(g));
        let _ = face.glyph_ver_advance(GlyphId(g));
    }
    // Names the encodings resolve, and code points through the cmap.
    for name in ["A", "space", "eacute", "uni0041", "g12", ".notdef", "fi"] {
        let gid = resolve_glyph(face, name);
        paint_resolved_glyph(
            gfx,
            face,
            fm,
            gid,
            &ShowMode::Charpath,
            DevPoint { x: 8.0, y: 40.0 },
        );
    }
    for c in [
        'A',
        'z',
        '\u{e9}',
        '\u{3042}',
        '\u{ac00}',
        '\u{0e01}',
        '\u{10ffff}',
    ] {
        let gid = face.glyph_index(c);
        if matches!(encoding, CatalogEncoding::Unicode) || gid.is_some() {
            paint_resolved_glyph(
                gfx,
                face,
                fm,
                gid,
                &ShowMode::Paint,
                DevPoint { x: 8.0, y: 40.0 },
            );
        }
    }
    let _ = (face.ascender(), face.descender(), face.units_per_em());
    let _ = (face.is_variable(), face.variation_axes().len());
}

/// Whether `fid` names a Unicode-mode catalog face (see the
/// `CatalogEncoding` doc comment) — builtins and Type 1/Type 3 dicts
/// (which have no registry entry, `fid < 0`) are never Unicode-mode.
pub(crate) fn is_unicode_font(fid: i64) -> bool {
    let Ok(idx) = usize::try_from(fid) else {
        return false;
    };
    if idx < BUILTINS.len() {
        return false;
    }
    CATALOG
        .read()
        .unwrap()
        .get(idx - BUILTINS.len())
        .is_some_and(|c| matches!(c.encoding, CatalogEncoding::Unicode))
}

/// Whether a Type 3 font dict opted into Unicode-mode `BuildChar` — the
/// procedural-font counterpart of `is_unicode_font`'s bundled-TrueType
/// mechanism (see `docs/dev/FONTS.md`'s "Unicode-mode Type 3 BuildChar"
/// addendum). A Type 3 dict with `/UnicodeBuildChar true` gets `show`
/// decoding its argument as UTF-8 and passing the full codepoint to
/// `BuildChar` instead of narrowing to a byte — this is what makes
/// jamo-composition faces like `lib/hangul.ps` possible without a
/// Type 0/CID font model. Opt-in and narrowly scoped: ordinary Type 3
/// dicts (no such key) are completely unaffected.
pub(crate) fn is_unicode_type3(dict: &Rc<RefCell<Dict>>) -> bool {
    dict.borrow()
        .get("UnicodeBuildChar")
        .is_some_and(|o| matches!(o.value, Value::Boolean(true)))
}

/// How a face named by [`catalog_entries`] is actually reached —
/// distinguishes what's baked into the binary from what depends on the
/// font-catalog directory being present at runtime (absent entirely on
/// wasm, and possibly absent even on desktop if the bundle/checkout is
/// incomplete). Surfaced to `--capabilities` (issue #39) so a catalog
/// consumer can tell a permanently-available face from an
/// environment-dependent one instead of assuming every listed name
/// resolves everywhere `pscat` runs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FontOrigin {
    /// Compiled into the binary; resolves identically everywhere,
    /// wasm included.
    Builtin,
    /// A file found under the catalog directory at findfont time —
    /// desktop/bundle only (`#[cfg(not(target_arch = "wasm32"))]`).
    Catalog,
    /// A `findfont` name in `ALIASES` (or a family name with its
    /// `-Regular` dropped) that resolves to a catalog stem, with the
    /// same environment dependency as `Catalog`, or to an embedded
    /// standard-35 face, which resolves everywhere.
    Alias,
}

/// One name `findfont` can currently reach without substitution, with
/// enough provenance to explain *how* (see [`FontOrigin`]). The single
/// source both `available_fonts` (plain-text `--fonts`) and
/// `crate::capabilities` (structured `--capabilities`) build from, so
/// the two can't independently drift about what's actually installed.
pub struct FontEntry {
    pub name: String,
    pub origin: FontOrigin,
    /// Set only for `FontOrigin::Alias`: the catalog stem the alias
    /// resolves to. Owned (not `&'static str`) because an implicit
    /// `-Regular`-stripped alias's target is a stem discovered at
    /// call time, not one of the fixed `ALIASES` entries.
    pub alias_target: Option<String>,
}

/// Every face findfont can currently reach without substitution:
/// builtins (the twelve Liberation faces, then the embedded rest of the
/// standard 35), then loadable catalog files (sorted), then aliases.
///
/// One known-and-accepted gap against `catalog_fid`'s exact
/// resolution behavior (raised in cross-model review, PR #74, not
/// fixed — real installs don't hit it):
/// - Two catalog files differing *only* by case (e.g. `foo.ttf` and
///   `Foo-Regular.ttf` in the same family dir) would make an implicit
///   alias here and `catalog_fid`'s fully case-insensitive stem match
///   disagree about which file a name resolves to. No shipped catalog
///   family does this; modeling it generally would mean making every
///   name comparison in this function case-insensitive-aware, a much
///   larger change for a scenario no real catalog produces.
///
/// (An unreadable family subdirectory used to be a third: both scans
/// now skip it, since issue #599 made them walk several roots.)
pub fn catalog_entries() -> Vec<FontEntry> {
    let mut entries: Vec<FontEntry> = BUILTINS
        .iter()
        .map(|b| FontEntry {
            name: b.ps_name.to_string(),
            origin: FontOrigin::Builtin,
            alias_target: None,
        })
        .collect();
    // Font files on disk; only native builds have them.
    #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
    let mut stems: Vec<String> = Vec::new();
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut stems_seen = std::collections::HashSet::new();
        for family in catalog_roots()
            .iter()
            .flat_map(|root| std::fs::read_dir(root).into_iter().flatten().flatten())
        {
            let dir = family.path();
            if !dir.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if matches!(
                    path.extension().and_then(|e| e.to_str()),
                    Some("ttf" | "otf" | "ttc")
                ) && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                    // A stem only actually resolves if the file behind it
                    // is readable and parses — an unreadable or corrupt
                    // file in an otherwise-present catalog dir would
                    // otherwise get listed as installed while findfont
                    // silently substitutes Helvetica for it.
                    // An earlier root's file shadows a same-named one
                    // later (catalog_fid takes the first it finds).
                    && !stems_seen.contains(&stem.to_ascii_lowercase())
                    && parses_as_font(&path)
                {
                    stems_seen.insert(stem.to_ascii_lowercase());
                    stems.push(stem.to_string());
                }
            }
        }
        stems.sort();
    }
    // The embedded standard-35 faces (issue #757) resolve after any file
    // on disk, so a disk file with the same stem shadows one and is the
    // entry listed (as `Catalog`).
    let disk: std::collections::HashSet<String> =
        stems.iter().map(|s| s.to_ascii_lowercase()).collect();
    let embedded: Vec<String> = EMBEDDED
        .iter()
        .map(|(k, _)| (*k).to_string())
        .filter(|k| !disk.contains(&k.to_ascii_lowercase()))
        .collect();
    entries.extend(embedded.iter().map(|name| FontEntry {
        name: name.clone(),
        origin: FontOrigin::Builtin,
        alias_target: None,
    }));
    {
        // An alias only actually resolves (see catalog_fid's own stem/
        // -Regular-fallback lookup) when its target file is present —
        // an incomplete catalog install can have the directory but be
        // missing individual files, and listing an alias that would
        // really substitute Helvetica misreports it as installed. This
        // is deliberately built from *every* scanned stem (regardless
        // of the shadowing filter just below): a stem can be a valid
        // alias *target* even if it's unreachable under its own name.
        let stem_set: std::collections::HashSet<String> = stems
            .iter()
            .chain(&embedded)
            .map(|s| s.to_ascii_lowercase())
            .collect();
        // resolve() checks builtins, then ALIASES-key remapping,
        // before ever reaching a catalog stem directly — so a stem
        // whose own name exactly matches a builtin's ps_name or an
        // ALIASES key is permanently unreachable by that name (a
        // fifth cross-model review finding, PR #74: e.g. a catalog
        // file literally named "Helvetica.ttf" would never actually
        // be selected, since `/Helvetica findfont` always resolves to
        // the builtin first). Not listing it as its own directly-
        // reachable Catalog-origin entry avoids advertising a name
        // that findfont can never actually select this way.
        let builtin_names: std::collections::HashSet<&str> =
            BUILTINS.iter().map(|b| b.ps_name).collect();
        let alias_keys: std::collections::HashSet<&str> = ALIASES.iter().map(|(k, _)| *k).collect();
        entries.extend(
            stems
                .iter()
                .filter(|name| {
                    !builtin_names.contains(name.as_str()) && !alias_keys.contains(name.as_str())
                })
                .map(|name| FontEntry {
                    name: name.clone(),
                    origin: FontOrigin::Catalog,
                    alias_target: None,
                }),
        );
        entries.extend(
            ALIASES
                .iter()
                .filter(|(_, target)| {
                    stem_set.contains(&target.to_ascii_lowercase())
                        || stem_set.contains(&format!("{target}-Regular").to_ascii_lowercase())
                })
                .map(|(k, v)| FontEntry {
                    name: k.to_string(),
                    origin: FontOrigin::Alias,
                    alias_target: Some(v.to_string()),
                }),
        );
        // catalog_fid's own -Regular fallback means a bare family name
        // (not just names explicitly listed in ALIASES) resolves too,
        // whenever the file behind it is literally "<name>-Regular" —
        // e.g. a catalog holding only Bangers-Regular.ttf makes both
        // `/Bangers-Regular findfont` (the stem above) and
        // `/Bangers findfont` (this implicit alias) succeed. Case-
        // insensitively, matching catalog_fid's own
        // `eq_ignore_ascii_case` lookup — the bundled TeX Gyre files
        // are named e.g. `texgyreadventor-regular.otf` (lowercase),
        // and an exact-case strip_suffix("-Regular") missed every one
        // of them (a fourth cross-model review finding). Omitted
        // without this: 37+ such reachable names on this repo's own
        // catalog, confirmed empirically before writing the fix.
        // `seen` is built from `entries` as they stand now (builtins,
        // stems, and curated ALIASES all already added) so an implicit
        // alias never shadows any of those.
        //
        // A short name that's also a curated ALIASES *key* is excluded
        // outright (not just guarded by `seen`), regardless of whether
        // that curated entry actually got added above: catalog_fid
        // checks ALIASES before ever trying a bare stem/-Regular
        // fallback, unconditionally -- so `/Palatino findfont` always
        // routes through ALIASES's remapping (to texgyrepagella-
        // regular, substituting Helvetica if that's missing) and would
        // never actually reach a same-named "Palatino-Regular" file
        // even in an install where the curated target is absent (a
        // sixth cross-model review finding, PR #74: `seen` alone only
        // catches the case where the curated entry *was* added).
        let mut seen: std::collections::HashSet<String> =
            entries.iter().map(|e| e.name.clone()).collect();
        for stem in stems.iter().chain(&embedded) {
            // Lowercasing preserves byte length and every char boundary
            // (ASCII case-folding never changes which bytes are UTF-8
            // continuation bytes), so `short_len` below is guaranteed a
            // valid boundary in `stem` too -- safe to slice directly,
            // unlike computing `stem.len() - 8` and indexing by hand.
            if let Some(short_lower) = stem.to_ascii_lowercase().strip_suffix("-regular") {
                let short = &stem[..short_lower.len()];
                let shadowed_by_alias_key = ALIASES.iter().any(|(k, _)| *k == short);
                if !shadowed_by_alias_key && seen.insert(short.to_string()) {
                    entries.push(FontEntry {
                        name: short.to_string(),
                        origin: FontOrigin::Alias,
                        alias_target: Some(stem.clone()),
                    });
                }
            }
        }
    }
    entries
}

/// Whether `path` is actually readable and parses as a font — the
/// gate `catalog_entries()` applies before advertising a stem/alias as
/// installed. `.ttc` collections always resolve face index 0, matching
/// `load_catalog_face`'s own convention.
#[cfg(not(target_arch = "wasm32"))]
fn parses_as_font(path: &std::path::Path) -> bool {
    std::fs::read(path).is_ok_and(|data| parse_face(&data, 0).is_some())
}

/// Names of every face findfont can currently reach without
/// substitution: builtins, loadable catalog files, and aliases (for
/// `--fonts`).
pub fn available_fonts() -> Vec<String> {
    catalog_entries()
        .into_iter()
        .map(|e| match e.alias_target {
            Some(target) => format!("{} -> {target}", e.name),
            None => e.name,
        })
        .collect()
}

/// Parsed faces, once per process — the bundled data is `'static`, so
/// the zero-copy `Face` views can live forever. (Stage 6 task 7:
/// measured before/after; numbers in docs/archive/NOTES.md.)
static FACES: std::sync::OnceLock<Vec<Face<'static>>> = std::sync::OnceLock::new();

fn face_for(fid: i64) -> Result<&'static Face<'static>, PsError> {
    let idx = usize::try_from(fid).map_err(|_| PsError::InvalidFont)?;
    if idx >= BUILTINS.len() {
        let cat = CATALOG.read().unwrap();
        return cat
            .get(idx - BUILTINS.len())
            .map(|c| c.face)
            .ok_or(PsError::InvalidFont);
    }
    let faces = FACES.get_or_init(|| {
        BUILTINS
            .iter()
            .map(|b| Face::parse(b.data, 0).expect("bundled font parses"))
            .collect()
    });
    faces.get(idx).ok_or(PsError::InvalidFont)
}

/// The current font, snapshotted by `setfont` into the graphics state
/// (so `gsave`/`grestore` handle it like any other attribute). The
/// FontMatrix is cached here per the PLRM's set-time semantics; the
/// Encoding is deliberately *not* — `show` reads it live from the dict
/// so the re-encoding idiom works even after `setfont`.
#[derive(Clone)]
pub struct FontState {
    pub dict: Rc<RefCell<Dict>>,
    pub fid: i64,
    /// Composed FontMatrix `[a b c d tx ty]`: glyph space (1000/em) →
    /// user space, scale included.
    pub matrix: [f64; 6],
}

/// Build the font dict for a built-in face. Each dict gets its own
/// fresh Encoding array so re-encoding one font can't affect another.
pub(crate) fn build_builtin_dict(fid: i64) -> Result<Rc<RefCell<Dict>>, PsError> {
    let face = face_for(fid)?;
    let idx = fid as usize; // face_for validated the range
    let k = 1000.0 / f64::from(face.units_per_em());
    let bbox = face.global_bounding_box();

    let (font_name, encoding) = if idx < BUILTINS.len() {
        (
            BUILTINS[idx].ps_name.to_string(),
            encodings::standard_encoding(),
        )
    } else {
        let cat = CATALOG.read().unwrap();
        let c = &cat[idx - BUILTINS.len()];
        let enc = match c.encoding {
            // Unicode-mode faces never read this array at show time
            // (see the CatalogEncoding doc comment); StandardEncoding
            // is just a well-formed placeholder for dict-shape
            // consistency (`dup length dict copy`-style idioms).
            CatalogEncoding::Standard | CatalogEncoding::Unicode => encodings::standard_encoding(),
            CatalogEncoding::Symbol => encodings::symbol_encoding(),
            CatalogEncoding::Dingbats => encodings::dingbats_encoding(),
        };
        (c.ps_name.clone(), enc)
    };

    let mut d = Dict::new();
    d.put("FontName".into(), Object::name(&font_name));
    // Honest about the outline source: Type 42 is PostScript's name for
    // TrueType-backed fonts.
    d.put("FontType".into(), Object::int(42));
    d.put(
        "FontMatrix".into(),
        Object::array(
            [0.001, 0.0, 0.0, 0.001, 0.0, 0.0]
                .into_iter()
                .map(Object::real)
                .collect(),
        ),
    );
    d.put(
        "FontBBox".into(),
        Object::array(
            [
                f64::from(bbox.x_min) * k,
                f64::from(bbox.y_min) * k,
                f64::from(bbox.x_max) * k,
                f64::from(bbox.y_max) * k,
            ]
            .into_iter()
            .map(Object::real)
            .collect(),
        ),
    );
    d.put(
        "Encoding".into(),
        Object::array(encoding.into_iter().map(Object::name).collect()),
    );
    d.put("FID".into(), Object::int(fid));
    Ok(Rc::new(RefCell::new(d)))
}

/// Row-vector affine composition: apply `first`, then `second`
/// (PS matrix layout `[a b c d tx ty]`, x' = ax + cy + tx).
pub(crate) fn compose(first: [f64; 6], second: [f64; 6]) -> [f64; 6] {
    let [a1, b1, c1, d1, tx1, ty1] = first;
    let [a2, b2, c2, d2, tx2, ty2] = second;
    [
        a1 * a2 + b1 * c2,
        a1 * b2 + b1 * d2,
        c1 * a2 + d1 * c2,
        c1 * b2 + d1 * d2,
        tx1 * a2 + ty1 * c2 + tx2,
        tx1 * b2 + ty1 * d2 + ty2,
    ]
}

/// Per-glyph text-placement extras: `ashow` adds a vector to every
/// glyph's advance; `widthshow` adds one to a designated character
/// code's (a byte for ordinary fonts, a Unicode scalar for
/// Unicode-mode ones — see `CatalogEncoding`).
#[derive(Default)]
pub(crate) struct ShowParams {
    pub extra: (f64, f64),
    pub char_extra: Option<(u32, (f64, f64))>,
}

pub(crate) enum ShowMode {
    /// Paint glyphs; current point advances (`show` family).
    Paint,
    /// Append outlines to the current path (`charpath`).
    Charpath,
    /// Metrics only — no current point required (`stringwidth`).
    Width,
}

/// Full device-space affine for one glyph placement.
#[derive(Clone, Copy)]
struct GlyphTransform {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    tx: f64,
    ty: f64,
}

impl GlyphTransform {
    fn apply(&self, x: f64, y: f64) -> DevPoint {
        DevPoint {
            x: (self.a * x + self.c * y + self.tx) as f32,
            y: (self.b * x + self.d * y + self.ty) as f32,
        }
    }
}

/// Feeds `ttf-parser` outlines into a device-space `PsPath`, elevating
/// quadratic segments to the cubics our path model speaks.
struct PathSink {
    path: PsPath,
    m: GlyphTransform,
    cur: (f64, f64),
}

impl ttf_parser::OutlineBuilder for PathSink {
    fn move_to(&mut self, x: f32, y: f32) {
        self.cur = (f64::from(x), f64::from(y));
        let p = self.m.apply(self.cur.0, self.cur.1);
        self.path.move_to(p);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.cur = (f64::from(x), f64::from(y));
        let p = self.m.apply(self.cur.0, self.cur.1);
        self.path.line_to(p);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (qx, qy) = (f64::from(x1), f64::from(y1));
        let (px, py) = (f64::from(x), f64::from(y));
        let (cx, cy) = self.cur;
        // Exact degree elevation: cubic controls at ⅓ and ⅔ along the
        // quad's control polygon.
        let c1 = (cx + 2.0 / 3.0 * (qx - cx), cy + 2.0 / 3.0 * (qy - cy));
        let c2 = (px + 2.0 / 3.0 * (qx - px), py + 2.0 / 3.0 * (qy - py));
        self.path.curve_to(
            self.m.apply(c1.0, c1.1),
            self.m.apply(c2.0, c2.1),
            self.m.apply(px, py),
        );
        self.cur = (px, py);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.path.curve_to(
            self.m.apply(f64::from(x1), f64::from(y1)),
            self.m.apply(f64::from(x2), f64::from(y2)),
            self.m.apply(f64::from(x), f64::from(y)),
        );
        self.cur = (f64::from(x), f64::from(y));
    }

    fn close(&mut self) {
        self.path.close();
    }
}

/// Glyph name → glyph id: the face's own name table first (Liberation
/// carries full `post` names), then the Adobe-glyph-name → Unicode →
/// `cmap` fallback.
fn resolve_glyph(face: &Face, name: &str) -> Option<GlyphId> {
    if name == ".notdef" {
        return None;
    }
    face.glyph_index_by_name(name)
        .or_else(|| encodings::name_to_char(name).and_then(|c| face.glyph_index(c)))
}

/// CTM linear part in f64 (tiny-skia rows: sx ky kx sy tx ty), applied
/// to a user-space distance.
fn ctm_lin(gfx: &Gfx, (x, y): (f64, f64)) -> (f64, f64) {
    let m = gfx.ctm();
    (
        f64::from(m.sx) * x + f64::from(m.kx) * y,
        f64::from(m.ky) * x + f64::from(m.sy) * y,
    )
}

/// The Encoding lookup, read live from the font dict per the docs/dev/FONTS.md
/// set-time-semantics decision; a font dict without an Encoding
/// (hand-built) falls back to StandardEncoding. Out-of-range or
/// non-name entries behave as .notdef.
fn encoding_name(fs: &FontState, byte: u8) -> crate::name::PsName {
    let enc = fs
        .dict
        .borrow()
        .get("Encoding")
        .and_then(|o| match &o.value {
            Value::Array(a) => Some(a.clone()),
            _ => None,
        });
    match &enc {
        Some(a) => match a.get(usize::from(byte)).as_ref().map(|o| &o.value) {
            Some(Value::Name(n)) => n.clone(),
            _ => ".notdef".into(),
        },
        None => encodings::standard_encoding()[usize::from(byte)].into(),
    }
}

/// Paint/append/measure a resolved glyph id at `pen`, returning the raw
/// user-space advance (before per-show extras). Shared by `outline_glyph`
/// (byte → Encoding name → glyph id) and `unicode_glyph` (codepoint →
/// `cmap` directly) — everything past glyph-id resolution is identical.
fn paint_resolved_glyph(
    gfx: &mut Gfx,
    face: &Face,
    fm: [f64; 6],
    gid: Option<GlyphId>,
    mode: &ShowMode,
    pen: DevPoint,
) -> (f64, f64) {
    let k = 1000.0 / f64::from(face.units_per_em());
    // Glyph-space → user-space linear part (FontMatrix over 1000-unit
    // glyph space, with the em normalization folded in).
    let g = [fm[0] * k, fm[1] * k, fm[2] * k, fm[3] * k];

    if let Some(gid) = gid
        && !matches!(mode, ShowMode::Width)
    {
        let ctm = gfx.ctm();
        let (c00, c01) = (f64::from(ctm.sx), f64::from(ctm.kx));
        let (c10, c11) = (f64::from(ctm.ky), f64::from(ctm.sy));
        // The FontMatrix translation is a per-glyph user-space offset.
        let fm_off = ctm_lin(gfx, (fm[4], fm[5]));
        let m = GlyphTransform {
            a: c00 * g[0] + c01 * g[1],
            b: c10 * g[0] + c11 * g[1],
            c: c00 * g[2] + c01 * g[3],
            d: c10 * g[2] + c11 * g[3],
            tx: f64::from(pen.x) + fm_off.0,
            ty: f64::from(pen.y) + fm_off.1,
        };
        let mut sink = PathSink {
            path: PsPath::default(),
            m,
            cur: (0.0, 0.0),
        };
        face.outline_glyph(gid, &mut sink);
        if !sink.path.is_empty() {
            match mode {
                ShowMode::Paint => gfx.fill_path_direct(&sink.path),
                ShowMode::Charpath => gfx.append_path(&sink.path),
                ShowMode::Width => unreachable!("checked above"),
            }
        }
    }

    // Missing glyphs still advance — by .notdef's width, like a real
    // interpreter showing an unencoded character. Width is in raw glyph
    // units; `g` already carries the em normalization.
    let w = f64::from(
        gid.and_then(|g| face.glyph_hor_advance(g))
            .or_else(|| face.glyph_hor_advance(GlyphId(0)))
            .unwrap_or(0),
    );
    (g[0] * w, g[1] * w)
}

/// One outline-font glyph reached the byte-oriented way: byte →
/// Encoding name (live dict read) → glyph id.
fn outline_glyph(
    gfx: &mut Gfx,
    fs: &FontState,
    byte: u8,
    mode: &ShowMode,
    pen: DevPoint,
) -> Result<(f64, f64), PsError> {
    let face = face_for(fs.fid)?;
    let name = encoding_name(fs, byte);
    let gid = resolve_glyph(face, &name);
    Ok(paint_resolved_glyph(gfx, face, fs.matrix, gid, mode, pen))
}

/// One glyph on a Unicode-mode catalog face (see `CatalogEncoding`):
/// the decoded codepoint maps straight to a glyph id via the face's
/// `cmap`, with no Encoding array or glyph-name lookup involved — the
/// 256-slot Encoding model can't address Hangul/kana/kanji, so these
/// faces skip it entirely.
fn unicode_glyph(
    gfx: &mut Gfx,
    fs: &FontState,
    code: u32,
    mode: &ShowMode,
    pen: DevPoint,
) -> Result<(f64, f64), PsError> {
    let face = face_for(fs.fid)?;
    let gid = char::from_u32(code).and_then(|c| face.glyph_index(c));
    Ok(paint_resolved_glyph(gfx, face, fs.matrix, gid, mode, pen))
}

/// One Type 1 glyph: decrypt the charstring, interpret it (hints
/// ignored, flex as curves — see `src/type1.rs`), and run the outline
/// through the same FontMatrix∘CTM pipeline as every other glyph
/// source. Charspace units go through FontMatrix directly (no em
/// normalization — the font's own 0.001 matrix is the convention).
fn type1_glyph(
    gfx: &mut Gfx,
    fs: &FontState,
    byte: u8,
    mode: &ShowMode,
    pen: DevPoint,
) -> Result<(f64, f64), PsError> {
    let name = encoding_name(fs, byte);
    let (charstrings, len_iv, subr_strings) = {
        let d = fs.dict.borrow();
        let cs = match d.get("CharStrings").as_ref().map(|o| &o.value) {
            Some(Value::Dict(cs)) => cs.clone(),
            _ => return Err(PsError::InvalidFont),
        };
        let mut len_iv = 4usize;
        let mut subr_strings: Vec<crate::object::PsString> = Vec::new();
        if let Some(Value::Dict(private)) = d.get("Private").as_ref().map(|o| &o.value) {
            let p = private.borrow();
            if let Some(Value::Integer(n)) = p.get("lenIV").as_ref().map(|o| &o.value) {
                len_iv = usize::try_from(*n).unwrap_or(4);
            }
            if let Some(Value::Array(a)) = p.get("Subrs").as_ref().map(|o| &o.value) {
                for i in 0..a.len() {
                    if let Some(Value::String(s)) = a.get(i).as_ref().map(|o| &o.value) {
                        subr_strings.push(s.clone());
                    } else {
                        subr_strings.push(PsString::from_bytes(Vec::new()));
                    }
                }
            }
        }
        (cs, len_iv, subr_strings)
    };
    let fetch = |n: &str| -> Option<Vec<u8>> {
        match charstrings.borrow().get(n).as_ref().map(|o| &o.value) {
            Some(Value::String(s)) => Some(crate::file::decrypt_charstring(&s.to_vec(), len_iv)),
            _ => None,
        }
    };
    let Some(code) = fetch(&name).or_else(|| fetch(".notdef")) else {
        return Ok((0.0, 0.0));
    };
    let subrs: Vec<Vec<u8>> = subr_strings
        .iter()
        .map(|s| crate::file::decrypt_charstring(&s.to_vec(), len_iv))
        .collect();
    let glyph = crate::type1::run(
        &code,
        &crate::type1::FontData {
            subrs: &subrs,
            charstring_by_name: &fetch,
        },
    )?;

    let fm = fs.matrix;
    if !matches!(mode, ShowMode::Width) {
        let ctm = gfx.ctm();
        let (c00, c01) = (f64::from(ctm.sx), f64::from(ctm.kx));
        let (c10, c11) = (f64::from(ctm.ky), f64::from(ctm.sy));
        let fm_off = ctm_lin(gfx, (fm[4], fm[5]));
        let m = GlyphTransform {
            a: c00 * fm[0] + c01 * fm[1],
            b: c10 * fm[0] + c11 * fm[1],
            c: c00 * fm[2] + c01 * fm[3],
            d: c10 * fm[2] + c11 * fm[3],
            tx: f64::from(pen.x) + fm_off.0,
            ty: f64::from(pen.y) + fm_off.1,
        };
        let mut path = PsPath::default();
        for cmd in &glyph.cmds {
            match *cmd {
                crate::type1::Cmd::Move(x, y) => path.move_to(m.apply(x, y)),
                crate::type1::Cmd::Line(x, y) => path.line_to(m.apply(x, y)),
                crate::type1::Cmd::Curve(x1, y1, x2, y2, x, y) => {
                    path.curve_to(m.apply(x1, y1), m.apply(x2, y2), m.apply(x, y));
                }
                crate::type1::Cmd::Close => path.close(),
            }
        }
        if !path.is_empty() {
            match mode {
                ShowMode::Paint => gfx.fill_path_direct(&path),
                ShowMode::Charpath => gfx.append_path(&path),
                ShowMode::Width => unreachable!("checked above"),
            }
        }
    }
    Ok((fm[0] * glyph.width, fm[1] * glyph.width))
}

/// A Type 3 glyph whose BuildChar/BuildGlyph procedure is currently
/// executing on the machine. Holds everything needed to seal the glyph
/// context back up, however the procedure exits.
pub(crate) struct PendingGlyph {
    /// The BuildChar operand: a raw byte for ordinary Type 3 dicts, or a
    /// full Unicode codepoint for `/UnicodeBuildChar true` dicts (see
    /// `is_unicode_type3`) — always ≤ 255 in the former case, so this
    /// widened field is a no-op for existing byte-mode fonts.
    code: u32,
    /// Glyph-space width from setcachedevice/setcharwidth; a BuildChar
    /// that never declares one advances by zero.
    width: Option<(f64, f64)>,
    /// The FontState matrix at setup, for the advance transform.
    matrix: [f64; 6],
    depth: usize,
    saved: Box<crate::gfx::GraphicsState>,
    suppressed: bool,
}

/// What the machine should do after a ShowCtx step.
pub(crate) enum ShowStep {
    /// The show is complete; pop the frame.
    Pop,
    /// stringwidth complete; pop the frame and push wx wy.
    PopPushWidth(f64, f64),
    /// Push `operands`, then execute `target` (a BuildChar/BuildGlyph or
    /// kshow procedure); step again when it finishes.
    Exec {
        operands: Vec<Object>,
        target: Object,
    },
    /// Synchronous work done (an outline glyph painted); step again.
    Again,
}

/// Whether the font live in `gfx`'s current graphics state is
/// Unicode-mode: a `CatalogEncoding::Unicode` catalog face, or a Type 3
/// dict with `/UnicodeBuildChar true`. Re-derived fresh every time it's
/// needed — never cached across a glyph — since a kshow proc can change
/// the font between glyphs. (A `setfont` from *inside* BuildChar can't:
/// `restore_glyph_snapshot` rolls the whole `GraphicsState`, font
/// included, back to its pre-BuildChar snapshot when the glyph context
/// closes, so kshow is the only way a font change persists into the
/// next glyph of the same show.)
fn current_unicode_mode(gfx: &Gfx) -> Result<bool, PsError> {
    let fs = gfx.state().font.as_ref().ok_or(PsError::InvalidFont)?;
    Ok(if fs.fid >= 0 {
        is_unicode_font(fs.fid)
    } else {
        is_unicode_type3(&fs.dict)
    })
}

/// Decode one glyph code from `text` starting at byte offset `pos`,
/// against `unicode` (the live font's mode at this position — see
/// `current_unicode_mode`): a single raw byte in byte mode, or one
/// lossily-decoded UTF-8 scalar (invalid bytes become U+FFFD, per
/// `char::REPLACEMENT_CHARACTER`, consuming the same "maximal valid
/// subpart" `String::from_utf8_lossy` would) in Unicode mode. Returns
/// the code and how many bytes it consumed; `None` at end of string.
///
/// Because this is called once per glyph against whatever font is live
/// *right then*, a font switch mid-string re-segments the *remaining*
/// bytes rather than reinterpreting a decision made when the show
/// began — this is the fix for issue #31: with no such switch, this
/// produces byte-identical results to decoding the whole string once
/// up front.
///
/// Validates at most 4 bytes (`char::len_utf8`'s max), not the whole
/// remaining string: a valid scalar is never longer, and neither is
/// the "maximal subpart" of an ill-formed one, so this is exactly as
/// correct as validating the full remainder — just not Θ(n²) over a
/// long Unicode-mode `show`.
fn decode_one(text: &[u8], pos: usize, unicode: bool) -> Option<(u32, usize)> {
    let &first = text.get(pos)?;
    if !unicode {
        return Some((u32::from(first), 1));
    }
    let end = (pos + 4).min(text.len());
    let rest = &text[pos..end];
    match std::str::from_utf8(rest) {
        Ok(s) => {
            let ch = s.chars().next()?;
            Some((u32::from(ch), ch.len_utf8()))
        }
        Err(e) if e.valid_up_to() > 0 => {
            // Safe: `valid_up_to()` bytes are a validated UTF-8 prefix,
            // so its first scalar decodes without touching the error.
            let valid = std::str::from_utf8(&rest[..e.valid_up_to()]).ok()?;
            let ch = valid.chars().next()?;
            Some((u32::from(ch), ch.len_utf8()))
        }
        Err(e) => {
            let len = e.error_len().unwrap_or(rest.len()).max(1);
            Some((u32::from(char::REPLACEMENT_CHARACTER), len))
        }
    }
}

/// The execution-stack frame behind the whole show family (`docs/dev/FONTS.md`
/// Decision 6). One glyph per machine step: outline glyphs paint
/// synchronously, Type 3 glyphs yield to their BuildChar/BuildGlyph
/// procedure, and kshow yields to its proc between characters. Because
/// it's a frame, text draws glyph by glyph in the live window, nested
/// shows inside BuildChar just work, and `stringwidth`'s results land
/// on the operand stack before the next token runs — frame order *is*
/// program order.
pub(crate) struct ShowCtx {
    /// The string's raw bytes, untouched. Re-segmented into codes one
    /// glyph at a time (see `decode_one`) against whichever font is
    /// live *at that point* — not decoded up front under a single
    /// decision — because a kshow proc can switch between byte-oriented
    /// and Unicode-mode fonts (see `CatalogEncoding`) mid-string, and
    /// only a live byte cursor can recover the right UTF-8 boundaries
    /// for a mid-string switch into Unicode mode: bytes already split
    /// apart under a byte-mode decision can never be recombined later.
    text: Vec<u8>,
    /// Byte offset of the next undecoded code in `text`.
    pos: usize,
    /// Device-space pen. The current point is re-fetched after each
    /// kshow proc so the proc can move it — that's what kshow is *for*.
    pen: DevPoint,
    /// User-space accumulated advance (the stringwidth result).
    total: (f64, f64),
    params: ShowParams,
    mode: ShowMode,
    kshow_proc: Option<Object>,
    pending: Option<PendingGlyph>,
    /// A kshow proc is on the stack above us.
    kshow_wait: bool,
}

impl ShowCtx {
    pub(crate) fn new(
        text: Vec<u8>,
        params: ShowParams,
        mode: ShowMode,
        kshow_proc: Option<Object>,
        pen: DevPoint,
    ) -> Self {
        ShowCtx {
            text,
            pos: 0,
            pen,
            total: (0.0, 0.0),
            params,
            mode,
            kshow_proc,
            pending: None,
            kshow_wait: false,
        }
    }

    pub(crate) fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// setcachedevice/setcharwidth landing here, in glyph space.
    pub(crate) fn set_glyph_width(&mut self, w: (f64, f64)) {
        if let Some(p) = &mut self.pending {
            p.width = Some(w);
        }
    }

    /// Unwind bookkeeping when the frame is popped abnormally (stop, an
    /// uncaught error) with a glyph context still open.
    pub(crate) fn cleanup(&mut self, gfx: &mut Gfx) {
        if let Some(p) = self.pending.take() {
            if p.suppressed {
                gfx.unsuppress_painting();
            }
            gfx.restore_glyph_snapshot(p.depth, p.saved);
        }
    }

    pub(crate) fn step(&mut self, gfx: &mut Gfx) -> Result<ShowStep, PsError> {
        // A BuildChar just finished: seal the glyph context and advance.
        if let Some(p) = self.pending.take() {
            if p.suppressed {
                gfx.unsuppress_painting();
            }
            gfx.restore_glyph_snapshot(p.depth, p.saved);
            let (wx, wy) = p.width.unwrap_or((0.0, 0.0));
            let m = p.matrix;
            self.advance(gfx, p.code, (m[0] * wx + m[2] * wy, m[1] * wx + m[3] * wy));
            if let Some(step) = self.maybe_kshow(gfx, p.code)? {
                return Ok(step);
            }
            return Ok(ShowStep::Again);
        }
        // A kshow proc just finished: it may have moved the current
        // point; the next glyph starts wherever it left the pen.
        if self.kshow_wait {
            self.kshow_wait = false;
            if !matches!(self.mode, ShowMode::Width) {
                self.pen = gfx.device_current_point().ok_or(PsError::NoCurrentPoint)?;
            }
        }
        // The font is read per glyph — and now so is the segmentation of
        // the remaining text: a kshow proc may have changed the font,
        // and `decode_one` re-reads the *raw bytes* against whichever
        // font is live right now rather than trusting a decision made
        // when the show began (issue #31).
        let fs = gfx.state().font.clone().ok_or(PsError::InvalidFont)?;
        let unicode = if fs.fid >= 0 {
            is_unicode_font(fs.fid)
        } else {
            is_unicode_type3(&fs.dict)
        };
        let Some((code, len)) = decode_one(&self.text, self.pos, unicode) else {
            return Ok(match self.mode {
                ShowMode::Width => ShowStep::PopPushWidth(self.total.0, self.total.1),
                _ => ShowStep::Pop,
            });
        };
        self.pos += len;
        if fs.fid >= 0 {
            let adv = if unicode {
                unicode_glyph(gfx, &fs, code, &self.mode, self.pen)?
            } else {
                outline_glyph(gfx, &fs, code as u8, &self.mode, self.pen)?
            };
            self.advance(gfx, code, adv);
            if let Some(step) = self.maybe_kshow(gfx, code)? {
                return Ok(step);
            }
            Ok(ShowStep::Again)
        } else {
            // No registry entry: a procedural font. Type 3 glyph
            // procedures win; otherwise Type 1 CharStrings render
            // synchronously like outlines. `code` is already correctly
            // narrowed to a byte by `decode_one` for a Type 1 dict (its
            // `unicode` is always false — see `is_unicode_type3`), so
            // no further narrowing is needed here.
            let is_type3 = {
                let d = fs.dict.borrow();
                d.get("BuildGlyph").is_some() || d.get("BuildChar").is_some()
            };
            if is_type3 {
                self.begin_type3_glyph(gfx, fs, code)
            } else {
                let byte = code as u8;
                let adv = type1_glyph(gfx, &fs, byte, &self.mode, self.pen)?;
                self.advance(gfx, code, adv);
                if let Some(step) = self.maybe_kshow(gfx, code)? {
                    return Ok(step);
                }
                Ok(ShowStep::Again)
            }
        }
    }

    /// Apply one glyph's advance (plus ashow/widthshow extras) to the
    /// running total and, when painting, to the pen and current point.
    fn advance(&mut self, gfx: &mut Gfx, code: u32, mut adv: (f64, f64)) {
        adv.0 += self.params.extra.0;
        adv.1 += self.params.extra.1;
        if let Some((ch, (cx, cy))) = self.params.char_extra
            && ch == code
        {
            adv.0 += cx;
            adv.1 += cy;
        }
        self.total.0 += adv.0;
        self.total.1 += adv.1;
        if !matches!(self.mode, ShowMode::Width) {
            let (dx, dy) = ctm_lin(gfx, adv);
            self.pen.x += dx as f32;
            self.pen.y += dy as f32;
            // Advance the visible current point per glyph, not at the
            // end: BuildChar and kshow procs are allowed to observe it.
            gfx.move_to_device(self.pen);
        }
    }

    /// Between two characters (never before the first or after the
    /// last), kshow pushes their codes and runs its proc. `prev` is the
    /// code just shown; `next` is peeked (without consuming it — the
    /// real decode happens on the following `step`) against the font
    /// still live right now, since the kshow proc that might change it
    /// hasn't run yet.
    fn maybe_kshow(&mut self, gfx: &Gfx, prev: u32) -> Result<Option<ShowStep>, PsError> {
        let Some(proc) = self.kshow_proc.clone() else {
            return Ok(None);
        };
        if self.pos >= self.text.len() {
            return Ok(None);
        }
        let unicode = current_unicode_mode(gfx)?;
        let Some((next, _)) = decode_one(&self.text, self.pos, unicode) else {
            return Ok(None);
        };
        self.kshow_wait = true;
        Ok(Some(ShowStep::Exec {
            operands: vec![Object::int(i64::from(prev)), Object::int(i64::from(next))],
            target: proc,
        }))
    }

    /// Open a Type 3 glyph context: snapshot the graphics state, set the
    /// CTM so BuildChar draws in glyph space at the pen, start a fresh
    /// path, and hand the machine the procedure to run. Metrics-only
    /// modes run the procedure with painting suppressed — that's how
    /// stringwidth learns a Type 3 width (charpath on Type 3 advances
    /// without appending outlines; documented in docs/dev/FONTS.md).
    fn begin_type3_glyph(
        &mut self,
        gfx: &mut Gfx,
        fs: FontState,
        code: u32,
    ) -> Result<ShowStep, PsError> {
        // BuildGlyph resolves through the byte-oriented Encoding array,
        // so it needs `code` to fit a byte — true for every ordinary
        // Type 3 font, and for `/UnicodeBuildChar true` ones only if
        // they still define BuildGlyph (none of this repo's do).
        // Computed before the dict borrow below since `encoding_name`
        // borrows `fs.dict` too.
        let name = u8::try_from(code).ok().map(|byte| encoding_name(&fs, byte));
        let (target, char_operand) = {
            let d = fs.dict.borrow();
            // BuildGlyph (Level 2, name-driven) wins over BuildChar
            // (Level 1, code-driven), per the PLRM.
            if let Some(bg) = d.get("BuildGlyph") {
                (
                    bg,
                    Object::lit(Value::Name(name.ok_or(PsError::Rangecheck)?)),
                )
            } else if let Some(bc) = d.get("BuildChar") {
                (bc, Object::int(i64::from(code)))
            } else {
                return Err(PsError::InvalidFont);
            }
        };
        let (depth, saved) = gfx.glyph_enter();
        let suppressed = !matches!(self.mode, ShowMode::Paint);
        if suppressed {
            gfx.suppress_painting();
        }
        // Glyph space → FontMatrix → user space at the pen → device:
        // same pipeline as outline glyphs, installed as the CTM so
        // BuildChar's ordinary path operators land in the right place.
        let fm = fs.matrix;
        let ctm = gfx.ctm();
        let (c00, c01) = (f64::from(ctm.sx), f64::from(ctm.kx));
        let (c10, c11) = (f64::from(ctm.ky), f64::from(ctm.sy));
        let off = ctm_lin(gfx, (fm[4], fm[5]));
        gfx.set_ctm(tiny_skia::Transform::from_row(
            (c00 * fm[0] + c01 * fm[1]) as f32,
            (c10 * fm[0] + c11 * fm[1]) as f32,
            (c00 * fm[2] + c01 * fm[3]) as f32,
            (c10 * fm[2] + c11 * fm[3]) as f32,
            (f64::from(self.pen.x) + off.0) as f32,
            (f64::from(self.pen.y) + off.1) as f32,
        ));
        gfx.newpath();
        self.pending = Some(PendingGlyph {
            code,
            width: None,
            matrix: fs.matrix,
            depth,
            saved,
            suppressed,
        });
        Ok(ShowStep::Exec {
            operands: vec![Object::lit(Value::Dict(fs.dict.clone())), char_operand],
            target,
        })
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    /// The stand-in table (issue #728): weight and slant survive, and
    /// each pack names itself in the note.
    #[test]
    fn pack_faces_stand_in_with_the_closest_builtin() {
        for (name, pack, stand_in) in [
            ("Bangers", "extras", "Helvetica"),
            ("bangers-regular", "extras", "Helvetica"),
            ("CourierPrime", "extras", "Courier"),
            ("JetBrainsMono-Regular", "extras", "Courier"),
            ("NotoSansKR-Regular", "cjk-thai", "Helvetica"),
        ] {
            assert_eq!(missing_pack(name), Some(pack), "{name}");
            assert_eq!(stand_in_for(name), stand_in, "{name}");
            let note = stand_in_note(name, pack, "stem", stand_in);
            assert!(
                !note.contains('\n')
                    && note.contains(&format!("/{name} is in the {pack} font pack"))
                    && note.contains(&format!("using {stand_in}"))
                    && note.contains(&format!("`pscat fonts install --pack {pack}`")),
                "{note}"
            );
        }
        for name in ["Helvetica", "Courier-Bold", "NoSuchFace", "Arial"] {
            assert_eq!(missing_pack(name), None, "{name}");
        }
    }

    /// The browser build's note (issue #750): one line naming the face,
    /// the stand-in and the `addFont` stem, with no `pscat fonts install`
    /// advice, which a browser cannot follow.
    #[test]
    fn the_browser_note_says_to_use_addfont_with_the_file_stem() {
        assert_eq!(
            missing_face("Bangers"),
            Some(("extras", "Bangers-Regular".to_string()))
        );
        assert_eq!(
            missing_face("NotoSansKR-Regular"),
            Some(("cjk-thai", "NotoSansKR-Regular".to_string()))
        );
        let note = browser_stand_in_note("Bangers", "extras", "Bangers-Regular", "Helvetica");
        assert!(
            !note.contains('\n')
                && note.contains("/Bangers is in the extras font pack")
                && note.contains("using Helvetica")
                && note.contains("addFont('Bangers-Regular', bytes)")
                && !note.contains("fonts install"),
            "{note}"
        );
    }

    /// All of the standard 35 resolve with no font on disk (issue #757):
    /// no pack lists them, and each name lands on its own embedded face,
    /// registered under the pack file's stem, not on the substitute or a
    /// stand-in.
    #[test]
    fn the_standard_35_are_compiled_in() {
        let names = [
            ("Palatino-Roman", "texgyrepagella-regular"),
            ("Palatino-Bold", "texgyrepagella-bold"),
            ("Palatino-Italic", "texgyrepagella-italic"),
            ("Palatino-BoldItalic", "texgyrepagella-bolditalic"),
            ("Bookman-Light", "texgyrebonum-regular"),
            ("Bookman-Demi", "texgyrebonum-bold"),
            ("Bookman-LightItalic", "texgyrebonum-italic"),
            ("Bookman-DemiItalic", "texgyrebonum-bolditalic"),
            ("NewCenturySchlbk-Roman", "texgyreschola-regular"),
            ("NewCenturySchlbk-Bold", "texgyreschola-bold"),
            ("NewCenturySchlbk-Italic", "texgyreschola-italic"),
            ("NewCenturySchlbk-BoldItalic", "texgyreschola-bolditalic"),
            ("AvantGarde-Book", "texgyreadventor-regular"),
            ("AvantGarde-Demi", "texgyreadventor-bold"),
            ("AvantGarde-BookOblique", "texgyreadventor-italic"),
            ("AvantGarde-DemiOblique", "texgyreadventor-bolditalic"),
            ("ZapfChancery-MediumItalic", "texgyrechorus-mediumitalic"),
            ("Helvetica-Narrow", "texgyreheroscn-regular"),
            ("Helvetica-Narrow-Bold", "texgyreheroscn-bold"),
            ("Helvetica-Narrow-Oblique", "texgyreheroscn-italic"),
            ("Helvetica-Narrow-BoldOblique", "texgyreheroscn-bolditalic"),
            ("Symbol", "StandardSymbolsPS"),
            ("ZapfDingbats", "D050000L"),
        ];
        assert_eq!(names.len() + BUILTINS.len(), 35);
        for (name, stem) in names {
            assert_eq!(missing_pack(name), None, "{name}");
            let fid = embedded_fid(name).unwrap_or_else(|| panic!("{name} is not embedded"));
            assert!(fid >= BUILTINS.len() as i64, "{name}");
            let cat = CATALOG.read().unwrap();
            let face = &cat[fid as usize - BUILTINS.len()];
            assert_eq!(face.key, stem, "{name}");
        }
        for (k, _) in &EMBEDDED {
            assert!(
                names.iter().any(|(_, s)| s == k),
                "{k} has no standard name"
            );
        }
    }

    /// Every stand-in is one of the twelve built-ins, so `resolve` never
    /// lands on the generic substitute by accident.
    #[test]
    fn every_stand_in_is_a_builtin() {
        for name in crate::fontpacks::packs()
            .iter()
            .flat_map(|p| p.families.iter().flat_map(|(_, f)| f))
            .map(|f| f.rsplit_once('.').map_or(f.as_str(), |(s, _)| s))
            .chain(ALIASES.iter().map(|(k, _)| *k))
        {
            assert!(builtin_index(stand_in_for(name)).is_some(), "{name}");
        }
    }

    /// `register_font_bytes` (the wasm `ps_add_font`): a registered face
    /// resolves by its key, a second registration keeps the first, and
    /// bytes that are not a font are refused.
    #[test]
    fn registered_font_bytes_resolve_by_name() {
        let bytes = BUILTINS[0].data.to_vec();
        let fid = register_font_bytes("PscatRegisterTest-Regular", bytes.clone()).unwrap();
        assert!(fid >= BUILTINS.len() as i64);
        assert_eq!(resolve("PscatRegisterTest-Regular").fid, fid);
        assert_eq!(
            register_font_bytes("pscatregistertest-regular", bytes),
            Ok(fid)
        );
        assert!(register_font_bytes("PscatRegisterJunk", b"not a font".to_vec()).is_err());
        assert!(register_font_bytes("", BUILTINS[0].data.to_vec()).is_err());
    }
}
