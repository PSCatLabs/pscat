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
use crate::object::{Dict, Object, Value};

/// `FID` value for fonts with no outline source we can drive yet
/// (Type 3 dicts accepted by `definefont`); `show` reports invalidfont.
pub(crate) const FID_NONE: i64 = -1;

/// Registry index used when `findfont` substitutes for an unknown name.
pub(crate) const SUBSTITUTE: i64 = 0;

/// A built-in font dict's own record of its registry face (issue #1022).
/// The standard re-encoding idiom copies every entry *except* `FID` into a
/// new dict and `definefont`s it. A real Type 42 or Type 1 dict carries its
/// outlines (`sfnts`, `CharStrings`), so the copy keeps its face; ours are
/// behind the registry, so without this entry the copy had nothing naming
/// its face and `definefont` gave it the Helvetica substitute. `FontName`
/// can't serve: the idiom often renames it.
pub(crate) const FACE_KEY: &str = "PSCatFace";

mod unicode;
pub(crate) use unicode::missing_glyph_note;

/// The font dict key that makes any outline font Unicode-mode (issue
/// #1425): with `/PSCatUnicode true`, `show`, `stringwidth` and `charpath`
/// read the string as UTF-8, look each character up by code point, fall
/// back across the compiled-in faces and the bundled colour emoji, and
/// report a character nothing has. A pscat extension, like
/// `/UnicodeBuildChar` for Type 3; Ghostscript ignores the key and shows
/// the bytes. Copied by `scalefont`, `makefont` and the copy-and-
/// `definefont` idiom, so a derived font keeps it.
pub(crate) const UNICODE_KEY: &str = "PSCatUnicode";

/// Whether an outline font (`fid >= 0`) is Unicode-mode: a Unicode-mode
/// catalog face, or a dict carrying [`UNICODE_KEY`] `true`.
fn outline_unicode(fs: &FontState) -> bool {
    is_unicode_font(fs.fid)
        || fs
            .dict
            .borrow()
            .get(UNICODE_KEY)
            .is_some_and(|o| matches!(o.value, Value::Boolean(true)))
}

/// The `FontName` of a font dict, for a message; `?` when it has none.
fn font_name(fs: &FontState) -> String {
    match fs.dict.borrow().get("FontName").map(|o| o.value.clone()) {
        Some(Value::Name(n)) => n.to_string(),
        Some(Value::String(s)) => s.text(),
        _ => "?".to_string(),
    }
}

/// Whether `fs` decodes its strings as UTF-8: an outline font by
/// [`outline_unicode`], a Type 3 dict by `/UnicodeBuildChar`.
fn font_unicode_mode(fs: &FontState) -> bool {
    if fs.fid >= 0 {
        outline_unicode(fs)
    } else {
        is_unicode_type3(&fs.dict)
    }
}

/// The registry face a font dict's [`FACE_KEY`] names, if it names one that
/// exists. A program can write anything there, so anything that is not an
/// integer naming a loaded face is `None` (the caller substitutes).
pub(crate) fn face_entry_fid(d: &Dict) -> Option<i64> {
    match d.get(FACE_KEY).as_ref().map(|o| &o.value) {
        Some(Value::Integer(fid)) if face_for(*fid).is_ok() => Some(*fid),
        _ => None,
    }
}

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

/// The PostScript name of built-in face `fid` (one of the twelve
/// Liberation-backed standard faces); `None` for any other FID.
pub(crate) fn builtin_ps_name(fid: i64) -> Option<&'static str> {
    usize::try_from(fid)
        .ok()
        .and_then(|i| BUILTINS.get(i))
        .map(|b| b.ps_name)
}

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

/// The FID of the face `requested` names when one exists: a builtin, a
/// disk-catalog (or wasm-registered) face, or an embedded standard-35
/// face. `None` for a name nothing provides, including a face of a font
/// pack that is not installed, where [`resolve`] falls back to a
/// stand-in. [`resolve`] and `resourcestatus` share it, so the two
/// cannot disagree about which names are available.
pub(crate) fn locate(requested: &str) -> Option<i64> {
    if let Some(fid) = builtin_index(requested) {
        return Some(fid);
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(fid) = catalog_fid(requested) {
        return Some(fid);
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
            return Some(fid);
        }
    }
    embedded_fid(requested)
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
    if let Some(fid) = locate(requested) {
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
            .is_some_and(|face| glyph_for_char(face, c).is_some())
}

/// The FID of an already-registered catalog face whose key is `want`
/// (case-insensitively) or whose PostScript name is `requested`. An
/// empty `requested` matches by key only (`load_catalog_face` passes ""
/// for a key-only lookup), so a face with an empty name record can't
/// stand in for an unrelated file (Codex, PR #1216).
fn loaded_fid(want: &str, requested: &str) -> Option<i64> {
    let cat = CATALOG.read().unwrap();
    cat.iter()
        .position(|c| catalog_entry_matches(&c.key, &c.ps_name, want, requested))
        .map(|i| (BUILTINS.len() + i) as i64)
}

/// The match rule behind [`loaded_fid`]: the key case-insensitively, or
/// a non-empty PostScript name exactly.
fn catalog_entry_matches(key: &str, ps_name: &str, want: &str, requested: &str) -> bool {
    key.eq_ignore_ascii_case(want) || (!requested.is_empty() && ps_name == requested)
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
    // A family name (`/NotoSansKR`) finds the file `NotoSansKR-Regular`,
    // so the cache probe in `catalog_fid`, made under the requested stem,
    // misses on every later call and the face would be read, parsed and
    // leaked again (issue #1214: ~10 MB per `resourcestatus` of a CJK
    // family name). Dedupe on the file's own stem, the key it registers
    // under. Which file wins is still decided by the scan, unchanged.
    if let Some(fid) = loaded_fid(&key, "") {
        return Some(fid);
    }
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
    // The axis, name and variation calls below read the same hostile tables.
    crate::guard::catch(|| prepare_face_unguarded(key, data)).flatten()
}

fn prepare_face_unguarded<'a>(
    key: &str,
    data: &'a [u8],
) -> Option<(Face<'a>, CatalogEncoding, String)> {
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
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| key.to_string())
    };
    Some((face, encoding, ps_name))
}

/// What the `truetype` fuzz target runs on a font file's bytes: the load
/// path (`prepare_face`, as `register_font_bytes` and the catalog do it),
/// then glyph lookup by name, by code point and by id, widths, and a sample of
/// outlines (about 24 glyph ids spread over the face, plus a few names and code
/// points) painted, appended to a path and measured through the same
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
        paint_resolved_glyph(
            gfx,
            face,
            fm,
            Some(GlyphId(g)),
            &mode,
            pen,
            None,
            GlyphId(0),
        );
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
            None,
            GlyphId(0),
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
        let gid = glyph_for_char(face, c);
        if matches!(encoding, CatalogEncoding::Unicode) || gid.is_some() {
            paint_resolved_glyph(
                gfx,
                face,
                fm,
                gid,
                &ShowMode::Paint,
                DevPoint { x: 8.0, y: 40.0 },
                None,
                GlyphId(0),
            );
        }
    }
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

/// The character a Symbol or ZapfDingbats glyph name (`alpha`,
/// `summation`, `a12` ...) stands for, when the face is the Symbol or
/// Dingbats catalog face and PDF text is being written (#1026). It comes
/// before `name_to_char`, which reads `mu` as the Latin-1 micro sign: in
/// Symbol it is the Greek letter. Only the text a PDF reader extracts
/// uses it: SVG text never takes these faces (`textrun::standard_face`),
/// and the glyph itself is still found by its name in the face. Asked
/// only with `--pdf-text` on, so other output does not pay for the
/// catalog lock per glyph.
fn symbolic_text_char(gfx: &Gfx, fid: i64, name: &str) -> Option<char> {
    if !gfx.pdf_text_wanted() {
        return None;
    }
    let idx = usize::try_from(fid).ok()?.checked_sub(BUILTINS.len())?;
    let encoding = CATALOG.read().ok()?.get(idx)?.encoding;
    match encoding {
        CatalogEncoding::Symbol => encodings::symbol_name_to_char(name),
        CatalogEncoding::Dingbats => encodings::dingbat_name_to_char(name),
        CatalogEncoding::Standard | CatalogEncoding::Unicode => None,
    }
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
    d.put(FACE_KEY.into(), Object::int(fid));
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

/// Which way each glyph of `xshow`, `yshow` or `xyshow` moves (#1180).
#[derive(Clone, Copy)]
pub(crate) enum Axes {
    /// `xshow`: one number per glyph, the x displacement.
    X,
    /// `yshow`: one number per glyph, the y displacement.
    Y,
    /// `xyshow`: two numbers per glyph, x then y.
    XY,
}

impl Axes {
    pub(crate) fn per_glyph(self) -> usize {
        match self {
            Axes::XY => 2,
            Axes::X | Axes::Y => 1,
        }
    }
}

/// The user-space displacements of an `xshow`-family show: each glyph
/// moves the pen by its own entry, never by the glyph's width.
pub(crate) struct Displacements {
    nums: Vec<f64>,
    axes: Axes,
    next: usize,
}

impl Displacements {
    pub(crate) fn new(nums: Vec<f64>, axes: Axes) -> Self {
        Displacements {
            nums,
            axes,
            next: 0,
        }
    }

    /// The next glyph's displacement. The operator checked that there are
    /// numbers for every glyph of the string under the font it shows with,
    /// and that font cannot change mid-show (no kshow proc, and a
    /// BuildChar's `setfont` is rolled back), so the fallback of zero is
    /// never reached; it is there so a miscount can never panic.
    fn take(&mut self) -> (f64, f64) {
        let at = |i: usize| self.nums.get(i).copied().unwrap_or(0.0);
        let d = match self.axes {
            Axes::X => (at(self.next), 0.0),
            Axes::Y => (0.0, at(self.next)),
            Axes::XY => (at(self.next), at(self.next + 1)),
        };
        self.next += self.axes.per_glyph();
        d
    }
}

/// `proc string cshow` (#1180): measure each character, then call `proc`
/// with its code and width. Runs in [`ShowMode::Width`], so nothing is
/// painted and the current point stays where it is.
struct Cshow {
    proc: Object,
    /// The font the show started with. pscat has no composite fonts, so
    /// it is the base font the PLRM sets for the proc. Ghostscript sets it
    /// back after every call, and after a `stop` out of the proc, so a
    /// proc's `setfont` lasts only until the next character.
    base: FontState,
    /// The proc is on the stack above the show.
    in_proc: bool,
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

/// One outline walk feeding the device path and an embedded charstring
/// (#1026).
struct Tee<'a> {
    sink: &'a mut PathSink,
    cs: &'a mut crate::pdf_font::Charstring,
}

impl ttf_parser::OutlineBuilder for Tee<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.sink.move_to(x, y);
        self.cs.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.sink.line_to(x, y);
        self.cs.line_to(x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.sink.quad_to(x1, y1, x, y);
        self.cs.quad_to(x1, y1, x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.sink.curve_to(x1, y1, x2, y2, x, y);
        self.cs.curve_to(x1, y1, x2, y2, x, y);
    }

    fn close(&mut self) {
        self.sink.close();
        self.cs.close();
    }
}

/// Glyph name → glyph id: the face's own name table first (Liberation
/// carries full `post` names), then the Adobe-glyph-name → Unicode →
/// `cmap` fallback.
fn resolve_glyph(face: &Face, name: &str) -> Option<GlyphId> {
    if name == ".notdef" {
        return None;
    }
    crate::guard::catch(|| face.glyph_index_by_name(name))
        .flatten()
        .or_else(|| encodings::name_to_char(name).and_then(|c| glyph_for_char(face, c)))
}

/// `Face::glyph_index`, for a face from a file we did not ship: a panic in
/// the `cmap` parser (see [`parse_face`]) is a missing glyph.
fn glyph_for_char(face: &Face, c: char) -> Option<GlyphId> {
    crate::guard::catch(|| face.glyph_index(c)).flatten()
}

/// `Face::glyph_hor_advance`, likewise guarded.
fn advance_of(face: &Face, gid: GlyphId) -> Option<u16> {
    crate::guard::catch(|| face.glyph_hor_advance(gid)).flatten()
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
///
/// `text` is the FID and character the glyph stands for, when the glyph
/// was reached through a name that has one; with SVG text output on
/// (#1119) a glyph of a standard face is recorded as that character
/// instead of its outline (`crate::textrun`).
///
/// `missing` is the glyph whose advance a missing glyph (`gid` is `None`)
/// takes: `.notdef` (glyph 0) for most faces, see [`missing_glyph`].
#[allow(clippy::too_many_arguments)]
fn paint_resolved_glyph(
    gfx: &mut Gfx,
    face: &Face,
    fm: [f64; 6],
    gid: Option<GlyphId>,
    mode: &ShowMode,
    pen: DevPoint,
    text: Option<(i64, char)>,
    missing: GlyphId,
) -> (f64, f64) {
    // No limit to exceed, so it cannot fail.
    paint_resolved_glyph_limited(
        gfx,
        face,
        fm,
        gid,
        mode,
        pen,
        text,
        missing,
        &crate::limits::LimitProbe::NONE,
        &mut 0,
    )
    .unwrap_or((0.0, 0.0))
}

/// Glyph units of `face` → device space for a glyph at `pen`: the em
/// normalisation, the FontMatrix (its translation a per-glyph user-space
/// offset) and the CTM, composed once.
fn glyph_transform(gfx: &Gfx, face: &Face, fm: [f64; 6], pen: DevPoint) -> GlyphTransform {
    let k = 1000.0 / f64::from(face.units_per_em());
    let g = [fm[0] * k, fm[1] * k, fm[2] * k, fm[3] * k];
    let ctm = gfx.ctm();
    let (c00, c01) = (f64::from(ctm.sx), f64::from(ctm.kx));
    let (c10, c11) = (f64::from(ctm.ky), f64::from(ctm.sy));
    let fm_off = ctm_lin(gfx, (fm[4], fm[5]));
    GlyphTransform {
        a: c00 * g[0] + c01 * g[1],
        b: c10 * g[0] + c11 * g[1],
        c: c00 * g[2] + c01 * g[3],
        d: c10 * g[2] + c11 * g[3],
        tx: f64::from(pen.x) + fm_off.0,
        ty: f64::from(pen.y) + fm_off.1,
    }
}

/// [`paint_resolved_glyph`] under the limits (#1332): `probe` is looked at
/// as the glyph is painted and its work (the rasteriser's and a pattern
/// layout's) is added to `work`, for the show to charge.
#[allow(clippy::too_many_arguments)]
fn paint_resolved_glyph_limited(
    gfx: &mut Gfx,
    face: &Face,
    fm: [f64; 6],
    gid: Option<GlyphId>,
    mode: &ShowMode,
    pen: DevPoint,
    text: Option<(i64, char)>,
    missing: GlyphId,
    probe: &crate::limits::LimitProbe,
    work: &mut u64,
) -> Result<(f64, f64), PsError> {
    let k = 1000.0 / f64::from(face.units_per_em());
    // Glyph-space → user-space linear part (FontMatrix over 1000-unit
    // glyph space, with the em normalization folded in).
    let g = [fm[0] * k, fm[1] * k, fm[2] * k, fm[3] * k];

    if let Some(gid) = gid
        && !matches!(mode, ShowMode::Width)
    {
        let m = glyph_transform(gfx, face, fm, pen);
        let mut sink = PathSink {
            path: PsPath::default(),
            m,
            cur: (0.0, 0.0),
        };
        let as_text = match (mode, text) {
            (ShowMode::Paint, Some((fid, ch))) => {
                let pdf = gfx.pdf_text_wanted();
                (pdf || gfx.svg_text_wanted()).then(|| glyph_text(fid, ch, gid, &m, face, pdf))
            }
            _ => None,
        };
        // A glyph the PDF font does not hold yet gets its embedded
        // charstring from this same outline walk, so the outline is walked
        // once, here, under the limits; the PDF writer never walks one
        // (#1026). The builder holds its whole 64 KiB budget, so the
        // memory limit is asked first, before anything is painted.
        let mut cs = match &as_text {
            Some(g) if gfx.pdf_would_build(g) => {
                probe.check_alloc(crate::pdf_font::CHARSTRING_BUDGET)?;
                Some(crate::pdf_font::Charstring::for_face(face))
            }
            _ => None,
        };
        let drawn = match cs.as_mut() {
            Some(cs) => face
                .outline_glyph(
                    gid,
                    &mut Tee {
                        sink: &mut sink,
                        cs,
                    },
                )
                .is_some(),
            None => face.outline_glyph(gid, &mut sink).is_some(),
        };
        let built = cs.map_or(crate::pdf_font::Built::No, |cs| cs.built(drawn));
        if let Some(g) = as_text {
            // Spaces included: an empty outline still places a character,
            // which is what keeps the words apart when the text is copied.
            // `paint_glyph` decides per recorder whether it takes the text.
            gfx.paint_glyph(&sink.path, Some(&g), built, probe, work)?;
        } else if !sink.path.is_empty() {
            match mode {
                ShowMode::Paint => gfx.fill_path_direct(&sink.path, probe, work)?,
                ShowMode::Charpath => gfx.append_path(&sink.path),
                ShowMode::Width => unreachable!("checked above"),
            }
        }
    }

    // Missing glyphs still advance — by .notdef's width, like a real
    // interpreter showing an unencoded character (`missing` says whose
    // width stands for it). Width is in raw glyph units; `g` already
    // carries the em normalization.
    let w = f64::from(
        gid.and_then(|g| advance_of(face, g))
            .or_else(|| advance_of(face, missing))
            .unwrap_or(0),
    );
    Ok((g[0] * w, g[1] * w))
}

/// The text form of a glyph (#1119, #1193). Whether a recorder takes it
/// is its own call (`textrun::svg_eligible`, `textrun::pdf_eligible`);
/// the face's embedding bits and the advance are only read when PDF text
/// output is on (`pdf`).
fn glyph_text(
    fid: i64,
    ch: char,
    gid: GlyphId,
    m: &GlyphTransform,
    face: &Face,
    pdf: bool,
) -> crate::textrun::GlyphText {
    // `m` maps font units to device; an em is `units_per_em` of them.
    let u = f64::from(face.units_per_em());
    let embeddable = pdf && crate::pdf_font::embedding_allowed(face);
    crate::textrun::GlyphText {
        ch,
        gid: gid.0,
        fid,
        em: [m.a * u, m.b * u, m.c * u, m.d * u, m.tx, m.ty],
        advance: if embeddable {
            crate::pdf_font::advance_1000(face, gid.0)
        } else {
            0.0
        },
        embeddable,
    }
}

/// The face behind `fid`, for the PDF font writer (#1193).
pub(crate) fn pdf_face(fid: i64) -> Option<&'static Face<'static>> {
    face_for(fid).ok()
}

/// The face behind `fid` and the name its embedded subset goes by (#1193):
/// the face's own PostScript name for the built-in faces
/// (`LiberationSans`, not the `Helvetica` it stands in for), the catalog's
/// `FontName` for the rest (for the pinned variable faces that is the
/// file stem, since their name table names the Thin master).
pub(crate) fn embed_face(fid: i64) -> Option<(&'static Face<'static>, String)> {
    let face = face_for(fid).ok()?;
    let idx = usize::try_from(fid).ok()?;
    let name = match BUILTINS.get(idx) {
        // The first entry may be a Mac-platform one `to_string` cannot read.
        Some(b) => crate::guard::catch(|| {
            face.names()
                .into_iter()
                .filter(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME)
                .find_map(|n| n.to_string())
        })
        .flatten()
        .unwrap_or_else(|| b.ps_name.to_string()),
        None => {
            let cat = CATALOG.read().ok()?;
            cat.get(idx - BUILTINS.len())?.ps_name.clone()
        }
    };
    Some((face, name))
}

/// The glyph whose advance a missing glyph takes on face `fid` (#1115).
/// Ghostscript's URW faces give `.notdef` the width of a space (278 for
/// Helvetica, 250 for Times, 600 for Courier, 250 for Palatino, ...), where
/// the faces that stand in for them have a wider `.notdef` box (Liberation
/// 750 and 778, TeX Gyre 500 and 280). So a missing glyph on one of the
/// standard 35 (`BUILTINS` and [`EMBEDDED`]) takes the `space` advance
/// instead. Every other face (user and pack fonts) keeps its own `.notdef`.
fn missing_glyph(face: &Face, fid: i64) -> GlyphId {
    let standard = match usize::try_from(fid) {
        Ok(i) if i < BUILTINS.len() => true,
        Ok(i) => CATALOG
            .read()
            .unwrap()
            .get(i - BUILTINS.len())
            .is_some_and(|c| EMBEDDED.iter().any(|(k, _)| k.eq_ignore_ascii_case(&c.key))),
        Err(_) => false,
    };
    standard
        .then(|| resolve_glyph(face, "space"))
        .flatten()
        .unwrap_or(GlyphId(0))
}

/// One outline-font glyph by name: the Encoding name of a shown byte
/// (live dict read), or `glyphshow`'s operand → glyph id.
fn outline_glyph(
    gfx: &mut Gfx,
    fs: &FontState,
    name: &str,
    mode: &ShowMode,
    pen: DevPoint,
    probe: &crate::limits::LimitProbe,
    work: &mut u64,
) -> Result<(f64, f64), PsError> {
    let face = face_for(fs.fid)?;
    let gid = resolve_glyph(face, name);
    let text = symbolic_text_char(gfx, fs.fid, name)
        .or_else(|| encodings::name_to_char(name))
        .map(|c| (fs.fid, c));
    // Only a missing glyph needs it: skip the name lookup per shown glyph.
    let missing = if gid.is_none() {
        missing_glyph(face, fs.fid)
    } else {
        GlyphId(0)
    };
    paint_resolved_glyph_limited(
        gfx, face, fs.matrix, gid, mode, pen, text, missing, probe, work,
    )
}

/// One Type 1 glyph: decrypt the charstring, interpret it (hints
/// ignored, flex as curves — see `src/type1.rs`), and run the outline
/// through the same FontMatrix∘CTM pipeline as every other glyph
/// source. Charspace units go through FontMatrix directly (no em
/// normalization — the font's own 0.001 matrix is the convention).
///
/// Bytes a Type 1 glyph decrypts between looks at the limits (#1188).
const DECRYPT_POLL: usize = 64 << 10;

/// Under the limits (#1188): `show` paints a glyph a step and a slice is
/// 512 steps, so 500 glyphs each calling a 16 MB subroutine were 12 s in
/// one slice. Each time the glyph has decrypted another [`DECRYPT_POLL`]
/// bytes (charstrings and subroutines together), `probe` (the limits when
/// the show began) is looked at, the decryption about to be made is held
/// against the heap cap first, and the heap is looked at again once it is
/// made. An ordinary glyph decrypts a few hundred bytes and reads no clock.
fn type1_glyph(
    gfx: &mut Gfx,
    fs: &FontState,
    name: &str,
    mode: &ShowMode,
    pen: DevPoint,
    probe: &crate::limits::LimitProbe,
    work: &mut u64,
) -> Result<(f64, f64), PsError> {
    let (charstrings, len_iv, subr_array) = {
        let d = fs.dict.borrow();
        let cs = match d.get("CharStrings").as_ref().map(|o| &o.value) {
            Some(Value::Dict(cs)) => cs.clone(),
            _ => return Err(PsError::InvalidFont),
        };
        let mut len_iv = 4usize;
        let mut subr_array = None;
        if let Some(Value::Dict(private)) = d.get("Private").as_ref().map(|o| &o.value) {
            let p = private.borrow();
            if let Some(Value::Integer(n)) = p.get("lenIV").as_ref().map(|o| &o.value) {
                len_iv = usize::try_from(*n).unwrap_or(4);
            }
            if let Some(Value::Array(a)) = p.get("Subrs").as_ref().map(|o| &o.value) {
                subr_array = Some(a.clone());
            }
        }
        (cs, len_iv, subr_array)
    };
    // A limit found while decrypting stops the charstring (as a missing
    // subroutine would) and is raised in its place.
    let limit: std::cell::Cell<Option<PsError>> = std::cell::Cell::new(None);
    let bytes = std::cell::Cell::new(0usize);
    let next_look = std::cell::Cell::new(DECRYPT_POLL);
    // Decrypt `s`, looking at the limits as above; `None` on a limit.
    let decrypt = |s: &crate::object::PsString| -> Option<Vec<u8>> {
        // Only the bytes the operator budget can reach (#1219): the first
        // `len_iv` are the cipher's lead, dropped from the plaintext.
        let len = s
            .len()
            .min(len_iv.saturating_add(crate::type1::MAX_CODE_BYTES));
        let total = bytes.get().saturating_add(len);
        bytes.set(total);
        let look = total >= next_look.get();
        let fail = |e: PsError| {
            limit.set(Some(e));
            None
        };
        if look {
            next_look.set(total.saturating_add(DECRYPT_POLL));
            // The decrypted bytes, and their shared copy.
            if let Err(e) = probe
                .check(0)
                .and_then(|()| probe.check_alloc(len.saturating_mul(2)))
            {
                return fail(e);
            }
        }
        let code = crate::file::decrypt_charstring(&s.borrow_bytes()[..len], len_iv);
        if look && let Err(e) = probe.check(0) {
            return fail(e);
        }
        Some(code)
    };
    let fetch = |n: &str| -> Option<Vec<u8>> {
        match charstrings.borrow().get(n).as_ref().map(|o| &o.value) {
            Some(Value::String(s)) => decrypt(s),
            _ => None,
        }
    };
    let Some(code) = fetch(name).or_else(|| fetch(".notdef")) else {
        return match limit.take() {
            Some(e) => Err(e),
            None => Ok((0.0, 0.0)),
        };
    };
    // Each subroutine is decrypted the first time this glyph calls it, and
    // once (#1188); an entry that is not a string is an empty subroutine.
    let decrypted: RefCell<std::collections::HashMap<usize, Rc<[u8]>>> = RefCell::default();
    let subr = |n: usize| -> Option<Rc<[u8]>> {
        if let Some(code) = decrypted.borrow().get(&n) {
            return Some(code.clone());
        }
        let a = subr_array.as_ref().filter(|a| n < a.len())?;
        let code: Rc<[u8]> = match a.get(n).as_ref().map(|o| &o.value) {
            Some(Value::String(s)) => decrypt(s)?.into(),
            _ => Rc::from(&[][..]),
        };
        decrypted.borrow_mut().insert(n, code.clone());
        Some(code)
    };
    let glyph = crate::type1::run(
        &code,
        &crate::type1::FontData {
            subr: &subr,
            charstring_by_name: &fetch,
        },
    );
    if let Some(e) = limit.take() {
        return Err(e);
    }
    let glyph = glyph?;

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
                ShowMode::Paint => gfx.fill_path_direct(&path, probe, work)?,
                ShowMode::Charpath => gfx.append_path(&path),
                ShowMode::Width => unreachable!("checked above"),
            }
        }
    }
    Ok((fm[0] * glyph.width, fm[1] * glyph.width))
}

/// The BuildChar operand for `name glyphshow` on a Type 3 font with no
/// BuildGlyph: the first Encoding slot holding `name`, else 0, as
/// Ghostscript does (the PLRM only defines glyphshow through BuildGlyph).
/// A `/UnicodeBuildChar true` font takes codepoints, so it gets the name's
/// Adobe Glyph List codepoint first.
fn glyphshow_code(fs: &FontState, name: &str) -> u32 {
    if is_unicode_type3(&fs.dict)
        && let Some(c) = encodings::name_to_char(name)
    {
        return u32::from(c);
    }
    (0..=255u8)
        .find(|&b| &*encoding_name(fs, b) == name)
        .map_or(0, u32::from)
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
    Ok(font_unicode_mode(fs))
}

/// How many glyphs a show of `text` paints with the current font: one per
/// byte, or one per decoded UTF-8 scalar on a Unicode-mode font, cut the
/// way the show itself cuts them (`decode_one`). `xshow` and friends
/// check their number count against it (#1180); `invalidfont` with no
/// font set.
pub(crate) fn glyph_count(gfx: &Gfx, text: &[u8]) -> Result<usize, PsError> {
    if !current_unicode_mode(gfx)? {
        return Ok(text.len());
    }
    let (mut pos, mut n) = (0, 0);
    while let Some((_, len)) = decode_one(text, pos, true) {
        pos += len;
        n += 1;
    }
    Ok(n)
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
    /// `glyphshow`'s operand: show this one glyph by name, bypassing the
    /// Encoding. `text` is then a single placeholder byte.
    glyph: Option<crate::name::PsName>,
    /// `xshow`/`yshow`/`xyshow`: per-glyph moves that replace the widths.
    displace: Option<Displacements>,
    /// `cshow`: the proc each measured character is handed to.
    cshow: Option<Cshow>,
    /// The limits when the show began, for a Type 1 glyph's decryption
    /// (#1188); looked at with no steps.
    probe: crate::limits::LimitProbe,
    /// Work the glyphs painted since the interpreter last took it (the
    /// rasteriser's and a pattern layout's, #1332), to charge as steps.
    work: u64,
    /// Unicode mode: the scalars that start before this byte offset were
    /// joined into the emoji ligature drawn before them (#1425), so they
    /// draw nothing and take no width.
    absorb_end: usize,
    /// Characters no face had, with the font's name, since the
    /// interpreter last took them (`take_missing`), for its warnings.
    missing: Vec<(u32, String)>,
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
            glyph: None,
            displace: None,
            cshow: None,
            probe: crate::limits::LimitProbe::NONE,
            work: 0,
            absorb_end: 0,
            missing: Vec::new(),
        }
    }

    /// The glyph work done since the last call.
    pub(crate) fn take_work(&mut self) -> u64 {
        std::mem::take(&mut self.work)
    }

    /// The characters with no glyph shown since the last call, each with
    /// the font's name (at most one per step).
    pub(crate) fn take_missing(&mut self) -> Vec<(u32, String)> {
        std::mem::take(&mut self.missing)
    }

    pub(crate) fn set_limit_probe(&mut self, probe: crate::limits::LimitProbe) {
        self.probe = probe;
    }

    /// `string numarray xshow` (and `yshow`, `xyshow`): paint like `show`,
    /// but move the pen by `displace` after each glyph.
    pub(crate) fn positioned(text: Vec<u8>, displace: Displacements, pen: DevPoint) -> Self {
        let mut ctx = ShowCtx::new(text, ShowParams::default(), ShowMode::Paint, None, pen);
        ctx.displace = Some(displace);
        ctx
    }

    /// `proc string cshow`: measure each character with `base` (the
    /// current font) and call `proc` with `code wx wy`. No pen: like
    /// `stringwidth`, it needs no current point and never moves it.
    pub(crate) fn cshow(text: Vec<u8>, proc: Object, base: FontState) -> Self {
        let pen = DevPoint { x: 0.0, y: 0.0 };
        let mut ctx = ShowCtx::new(text, ShowParams::default(), ShowMode::Width, None, pen);
        ctx.cshow = Some(Cshow {
            proc,
            base,
            in_proc: false,
        });
        ctx
    }

    /// `name glyphshow` (Level 2): one glyph selected by name, painted at
    /// `pen` and advanced like `show`.
    pub(crate) fn glyph(name: crate::name::PsName, pen: DevPoint) -> Self {
        let mut ctx = ShowCtx::new(vec![0], ShowParams::default(), ShowMode::Paint, None, pen);
        ctx.glyph = Some(name);
        ctx
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
        // A `stop` or an error out of a cshow proc still gets the base font
        // back, as in Ghostscript.
        self.end_cshow_proc(gfx);
    }

    /// The cshow proc returned (or is being unwound): put the base font
    /// back over whatever the proc set.
    fn end_cshow_proc(&mut self, gfx: &mut Gfx) {
        if let Some(c) = &mut self.cshow
            && c.in_proc
        {
            c.in_proc = false;
            gfx.set_font(c.base.clone());
        }
    }

    /// Whether this show paints glyphs, as `show` does (not `charpath`
    /// or `stringwidth`): the colour must be ready before it steps.
    pub(crate) fn paints(&self) -> bool {
        matches!(self.mode, ShowMode::Paint)
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
            let adv = (m[0] * wx + m[2] * wy, m[1] * wx + m[3] * wy);
            return self.finish_glyph(gfx, p.code, adv);
        }
        // A kshow proc just finished: it may have moved the current
        // point; the next glyph starts wherever it left the pen.
        if self.kshow_wait {
            self.kshow_wait = false;
            if !matches!(self.mode, ShowMode::Width) {
                self.pen = gfx.device_current_point().ok_or(PsError::NoCurrentPoint)?;
            }
        }
        // A cshow proc just finished: the next character is measured, and
        // the proc called, with the base font again.
        self.end_cshow_proc(gfx);
        // The font is read per glyph — and now so is the segmentation of
        // the remaining text: a kshow proc may have changed the font,
        // and `decode_one` re-reads the *raw bytes* against whichever
        // font is live right now rather than trusting a decision made
        // when the show began (issue #31).
        let fs = gfx.state().font.clone().ok_or(PsError::InvalidFont)?;
        let unicode = font_unicode_mode(&fs);
        // A kshow proc that switched to a byte-mode font mid-sequence ends
        // the emoji ligature: its remaining bytes are glyphs of their own.
        if !unicode {
            self.absorb_end = 0;
        }
        let start = self.pos;
        let Some((code, len)) = decode_one(&self.text, self.pos, unicode) else {
            return Ok(match self.mode {
                // cshow measures in this mode too, but returns nothing.
                ShowMode::Width if self.cshow.is_none() => {
                    ShowStep::PopPushWidth(self.total.0, self.total.1)
                }
                _ => ShowStep::Pop,
            });
        };
        self.pos += len;
        // glyphshow's name stands in for the Encoding lookup; its
        // placeholder code is 0 (only ashow/widthshow extras read codes).
        let glyph = self.glyph.clone();
        let code = if glyph.is_some() { 0 } else { code };
        if fs.fid >= 0 {
            let adv = match &glyph {
                Some(name) => outline_glyph(
                    gfx,
                    &fs,
                    name,
                    &self.mode,
                    self.pen,
                    &self.probe,
                    &mut self.work,
                )?,
                // Taken into the emoji ligature drawn before it.
                None if unicode && start < self.absorb_end => (0.0, 0.0),
                None if unicode => {
                    let shown = unicode::show_scalar(
                        gfx,
                        &fs,
                        code,
                        &self.text,
                        self.pos,
                        &self.mode,
                        self.pen,
                        &self.probe,
                        &mut self.work,
                    )?;
                    if shown.absorbed > 0 {
                        self.absorb_end = self.pos + shown.absorbed;
                    }
                    if shown.missing {
                        self.missing.push((code, font_name(&fs)));
                    }
                    shown.adv
                }
                None => {
                    let name = encoding_name(&fs, code as u8);
                    outline_glyph(
                        gfx,
                        &fs,
                        &name,
                        &self.mode,
                        self.pen,
                        &self.probe,
                        &mut self.work,
                    )?
                }
            };
            self.finish_glyph(gfx, code, adv)
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
                self.begin_type3_glyph(gfx, fs, code, glyph)
            } else {
                let name = match glyph {
                    Some(name) => name,
                    None => encoding_name(&fs, code as u8),
                };
                let adv = type1_glyph(
                    gfx,
                    &fs,
                    &name,
                    &self.mode,
                    self.pen,
                    &self.probe,
                    &mut self.work,
                )?;
                self.finish_glyph(gfx, code, adv)
            }
        }
    }

    /// A glyph is done (painted or measured, `adv` its user-space
    /// width): advance, then hand over to a kshow or cshow proc, if any.
    fn finish_glyph(
        &mut self,
        gfx: &mut Gfx,
        code: u32,
        adv: (f64, f64),
    ) -> Result<ShowStep, PsError> {
        let adv = self.advance(gfx, code, adv);
        if let Some(c) = &mut self.cshow {
            c.in_proc = true;
            return Ok(ShowStep::Exec {
                operands: vec![
                    Object::int(i64::from(code)),
                    Object::real(adv.0),
                    Object::real(adv.1),
                ],
                target: c.proc.clone(),
            });
        }
        Ok(self.maybe_kshow(gfx, code)?.unwrap_or(ShowStep::Again))
    }

    /// Apply one glyph's advance (plus ashow/widthshow extras, or in
    /// place of all of it an xshow displacement) to the running total
    /// and, when painting, to the pen and current point. Returns the
    /// advance applied.
    fn advance(&mut self, gfx: &mut Gfx, code: u32, mut adv: (f64, f64)) -> (f64, f64) {
        if let Some(d) = &mut self.displace {
            adv = d.take();
        }
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
        adv
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
        glyph: Option<crate::name::PsName>,
    ) -> Result<ShowStep, PsError> {
        // BuildGlyph resolves through the byte-oriented Encoding array,
        // so it needs `code` to fit a byte — true for every ordinary
        // Type 3 font, and for `/UnicodeBuildChar true` ones only if
        // they still define BuildGlyph (none of this repo's do).
        // Computed before the dict borrow below since `encoding_name`
        // borrows `fs.dict` too. glyphshow names the glyph directly; a
        // BuildChar-only font gets a code for it (`glyphshow_code`).
        let (name, code) = match glyph {
            Some(name) => {
                let code = glyphshow_code(&fs, &name);
                (Some(name), code)
            }
            None => (
                u8::try_from(code).ok().map(|byte| encoding_name(&fs, byte)),
                code,
            ),
        };
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

    /// A key-only lookup (empty `requested`) never matches on the
    /// PostScript name, so a face whose name record is empty can't
    /// answer for another file's stem (Codex round 2, PR #1216).
    #[test]
    fn an_empty_requested_name_matches_by_key_only() {
        assert!(!catalog_entry_matches(
            "EmptyName-Regular",
            "",
            "Other-Regular",
            ""
        ));
        assert!(catalog_entry_matches(
            "Other-Regular",
            "",
            "other-regular",
            ""
        ));
        assert!(catalog_entry_matches(
            "NotoSansKR-Regular",
            "NotoSansKR-Regular",
            "NotoSansKR",
            "NotoSansKR-Regular"
        ));
        assert!(!catalog_entry_matches(
            "A-Regular",
            "APS",
            "B-Regular",
            "BPS"
        ));
    }

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

    /// #1193: the licence bits of every compiled-in face allow a PDF to
    /// embed a subset, so `--pdf-text` covers all of the standard 35. Several
    /// TeX Gyre faces set Preview & Print and Editable together, which reads
    /// as Editable.
    #[test]
    fn every_compiled_in_face_may_be_embedded_in_a_pdf() {
        for b in &BUILTINS {
            let face = Face::parse(b.data, 0).expect("parses");
            assert!(crate::pdf_font::embedding_allowed(&face), "{}", b.ps_name);
        }
        for (key, data) in &EMBEDDED {
            let face = Face::parse(data, 0).expect("parses");
            assert!(crate::pdf_font::embedding_allowed(&face), "{key}");
        }
    }

    /// Where table `tag` starts in an sfnt.
    fn table_offset(bytes: &[u8], tag: &[u8; 4]) -> usize {
        let tables = usize::from(u16::from_be_bytes([bytes[4], bytes[5]]));
        let rec = (0..tables)
            .map(|i| 12 + 16 * i)
            .find(|&r| &bytes[r..r + 4] == tag)
            .expect("table");
        let at = [
            bytes[rec + 8],
            bytes[rec + 9],
            bytes[rec + 10],
            bytes[rec + 11],
        ];
        u32::from_be_bytes(at) as usize
    }

    /// `bytes` with table `tag` replaced by `data`, appended at the end
    /// (offset and length updated; checksums are not read).
    fn replace_table(mut bytes: Vec<u8>, tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
        while !bytes.len().is_multiple_of(4) {
            bytes.push(0);
        }
        let tables = usize::from(u16::from_be_bytes([bytes[4], bytes[5]]));
        let rec = (0..tables)
            .map(|i| 12 + 16 * i)
            .find(|&r| &bytes[r..r + 4] == tag)
            .expect("table");
        let at = u32::try_from(bytes.len()).expect("small");
        let len = u32::try_from(data.len()).expect("small");
        bytes[rec + 8..rec + 12].copy_from_slice(&at.to_be_bytes());
        bytes[rec + 12..rec + 16].copy_from_slice(&len.to_be_bytes());
        bytes.extend_from_slice(data);
        bytes
    }

    /// One closed simple TrueType contour through `pts` (font units).
    fn simple_glyph(pts: &[(i16, i16)]) -> Vec<u8> {
        let mut g = Vec::new();
        g.extend_from_slice(&1i16.to_be_bytes()); // one contour
        for v in [0i16, 0, 600, 700] {
            g.extend_from_slice(&v.to_be_bytes()); // bbox
        }
        let last = u16::try_from(pts.len() - 1).expect("few");
        g.extend_from_slice(&last.to_be_bytes());
        g.extend_from_slice(&0u16.to_be_bytes()); // no instructions
        g.extend(std::iter::repeat_n(1u8, pts.len())); // on-curve, i16 deltas
        let (mut x, mut y) = (0i16, 0i16);
        for &(px, _) in pts {
            g.extend_from_slice(&(px - x).to_be_bytes());
            x = px;
        }
        for &(_, py) in pts {
            g.extend_from_slice(&(py - y).to_be_bytes());
            y = py;
        }
        while !g.len().is_multiple_of(4) {
            g.push(0);
        }
        g
    }

    /// A composite glyph drawing glyph `child` twice.
    fn twice(child: u16) -> Vec<u8> {
        let mut g = Vec::new();
        g.extend_from_slice(&(-1i16).to_be_bytes());
        for v in [0i16, 0, 600, 700] {
            g.extend_from_slice(&v.to_be_bytes());
        }
        // ARG_1_AND_2_ARE_WORDS | ARGS_ARE_XY_VALUES, MORE_COMPONENTS on the first.
        for flags in [0x0023u16, 0x0003] {
            g.extend_from_slice(&flags.to_be_bytes());
            g.extend_from_slice(&child.to_be_bytes());
            g.extend_from_slice(&0i16.to_be_bytes());
            g.extend_from_slice(&0i16.to_be_bytes());
        }
        g
    }

    /// LiberationSans with a new `glyf`: `.notdef` (glyph 0) is a chain of
    /// `depth` composites, each drawing the next twice (2^depth triangles
    /// in all), and `A` (glyph 36) is a plain square. Glyph `depth` is the
    /// triangle; every other glyph is empty.
    fn notdef_bomb(depth: u16) -> Vec<u8> {
        let bytes = BUILTINS[0].data.to_vec();
        let n = usize::from(Face::parse(&bytes, 0).expect("face").number_of_glyphs());
        let mut glyf = Vec::new();
        let mut loca = Vec::new();
        for gid in 0..n {
            loca.extend_from_slice(&u32::try_from(glyf.len()).expect("small").to_be_bytes());
            let gid16 = u16::try_from(gid).expect("u16");
            if gid16 < depth {
                glyf.extend(twice(gid16 + 1));
            } else if gid16 == depth {
                glyf.extend(simple_glyph(&[(0, 0), (500, 0), (250, 600)]));
            } else if gid == 36 {
                glyf.extend(simple_glyph(&[(0, 0), (600, 0), (600, 700), (0, 700)]));
            }
        }
        loca.extend_from_slice(&u32::try_from(glyf.len()).expect("small").to_be_bytes());
        let mut bytes = replace_table(bytes, b"glyf", &glyf);
        bytes = replace_table(bytes, b"loca", &loca);
        let head = table_offset(&bytes, b"head");
        bytes[head + 50..head + 52].copy_from_slice(&1i16.to_be_bytes()); // long loca
        bytes
    }

    /// Codex review of #1398: `.notdef` used to be outlined while the PDF
    /// was written, outside every limit, and a face can make that walk a
    /// billion outlines long. Now nothing walks an outline at write time:
    /// a face whose `.notdef` is 30 levels of doubled composites writes its
    /// default (text) PDF at once, with `A` embedded and an empty `.notdef`.
    #[test]
    fn a_notdef_composite_bomb_is_never_walked_writing_the_pdf() {
        // The crafted structure is real: each level doubles the outline.
        let points = |depth: u16| {
            let bytes = notdef_bomb(depth);
            let face = Face::parse(&bytes, 0).expect("parses");
            let mut sink = PathSink {
                path: PsPath::default(),
                m: GlyphTransform {
                    a: 1.0,
                    b: 0.0,
                    c: 0.0,
                    d: 1.0,
                    tx: 0.0,
                    ty: 0.0,
                },
                cur: (0.0, 0.0),
            };
            assert!(face.outline_glyph(GlyphId(0), &mut sink).is_some());
            sink.path.points().len()
        };
        assert_eq!(points(5), 2 * points(4));
        assert_eq!(points(4), 16 * points(0));

        register_font_bytes("PscatNotdefBomb-Regular", notdef_bomb(30)).expect("registers");
        let t = std::time::Instant::now();
        let mut it = crate::Interp::with_page(200, 100).expect("page");
        it.enable_pdf(); // the default: text
        // `A` is text; the unmapped `B` (no glyph) draws nothing, as before.
        it.run_str("/PscatNotdefBomb-Regular findfont 20 scalefont setfont 10 50 moveto (AB) show")
            .expect("runs");
        let pdf = it.pdf_document().expect("pdf");
        assert!(t.elapsed().as_secs() < 10, "took {:?}", t.elapsed());
        let doc = String::from_utf8_lossy(&pdf);
        assert!(doc.contains("/Subtype /Type0 "), "A is embedded text");
        assert!(doc.trim_end().ends_with("%%EOF"));
    }

    /// A `cmap` with one format 4 subtable: `A` → glyph 36, U+4E00 →
    /// glyph 0 (`.notdef`), as a hostile or careless face may map it.
    fn cmap_to_notdef() -> Vec<u8> {
        let segs: [(u16, u16); 3] = [
            (0x41, 36u16.wrapping_sub(0x41)),
            (0x4E00, 0u16.wrapping_sub(0x4E00)),
            (0xFFFF, 1),
        ];
        let mut sub = Vec::new();
        let words = |v: &mut Vec<u8>, xs: &[u16]| {
            xs.iter()
                .for_each(|x| v.extend_from_slice(&x.to_be_bytes()))
        };
        words(&mut sub, &[4, 0, 0, 6, 4, 1, 2]); // format, length (below), language, segCountX2, ...
        words(&mut sub, &segs.map(|s| s.0)); // endCode
        words(&mut sub, &[0]);
        words(&mut sub, &segs.map(|s| s.0)); // startCode
        words(&mut sub, &segs.map(|s| s.1)); // idDelta
        words(&mut sub, &[0, 0, 0]); // idRangeOffset
        let len = u16::try_from(sub.len()).expect("small");
        sub[2..4].copy_from_slice(&len.to_be_bytes());
        let mut cmap = Vec::new();
        words(&mut cmap, &[0, 1, 3, 1, 0, 12]); // version, one record: 3/1 at 12
        cmap.extend(sub);
        cmap
    }

    /// Glyph 0 shown on purpose (a `cmap` that maps a character to it)
    /// still draws: in the raster, and in the PDF as its outline in the
    /// content stream, walked once at `show` like any glyph. It is never
    /// text, so the default PDF is the outline PDF byte for byte (#1026,
    /// Codex review of #1398).
    #[test]
    fn a_shown_notdef_keeps_its_outline_in_the_pdf() {
        let bytes = replace_table(notdef_bomb(0), b"cmap", &cmap_to_notdef());
        register_font_bytes("PscatShownNotdef-Regular", bytes).expect("registers");
        let src = "/PscatShownNotdef-Regular findfont 40 scalefont setfont 10 30 moveto /uni4E00 glyphshow";
        let run = |mode: crate::PdfText, svg: bool| {
            let mut it = crate::Interp::with_page(100, 100).expect("page");
            it.set_pdf_text(mode);
            it.enable_pdf();
            if svg {
                it.enable_svg();
            }
            it.run_str(src).expect("runs");
            let pdf = it.pdf_document().expect("pdf");
            let inked = it.gfx().pixmap.pixels().iter().any(|p| p.red() < 128);
            let svg = svg.then(|| it.svg_pages().expect("svg on").concat());
            (pdf, inked, svg)
        };
        let (text, inked, svg) = run(crate::PdfText::Text, true);
        let (paths, inked_paths, _) = run(crate::PdfText::Paths, false);
        assert!(inked && inked_paths, "the raster draws glyph 0");
        assert_eq!(text, paths, "glyph 0 is its outline in the default PDF");
        let doc = String::from_utf8_lossy(&text);
        assert!(!doc.contains("/Subtype /Type0 "), "no font for glyph 0");
        assert!(svg.expect("svg").contains("<path"), "the SVG draws it");
    }

    /// A glyph the PDF font does not hold yet needs a 64 KiB charstring
    /// buffer, built from the walk that draws it (#1026). The memory limit
    /// is asked before that walk: a refusal paints nothing and registers
    /// nothing; with room the glyph is drawn, embedded and charged.
    #[test]
    fn a_refused_pdf_glyph_paints_nothing() {
        let face = face_for(0).expect("first built-in face");
        let gid = glyph_for_char(face, 'A').expect("A");
        let mut gfx = Gfx::new(40, 40).expect("pixmap");
        gfx.enable_pdf();
        let paint = |gfx: &mut Gfx, probe: &crate::limits::LimitProbe, work: &mut u64| {
            paint_resolved_glyph_limited(
                gfx,
                face,
                [0.03, 0.0, 0.0, 0.03, 0.0, 0.0],
                Some(gid),
                &ShowMode::Paint,
                DevPoint { x: 4.0, y: 36.0 },
                Some((0, 'A')),
                GlyphId(0),
                probe,
                work,
            )
        };
        let probe_g = || {
            glyph_text(
                0,
                'A',
                gid,
                &GlyphTransform {
                    a: 1.0,
                    b: 0.0,
                    c: 0.0,
                    d: 1.0,
                    tx: 0.0,
                    ty: 0.0,
                },
                face,
                true,
            )
        };
        let before = gfx.pixmap.data().to_vec();
        let full = crate::limits::LimitProbe::memory_only(Some((usize::MAX / 2, 0)));
        let mut work = 0;
        let r = paint(&mut gfx, &full, &mut work);
        assert!(matches!(r, Err(PsError::MemoryExceeded)), "{r:?}");
        assert_eq!(gfx.pixmap.data(), &before[..], "nothing painted");
        assert!(!gfx.dirty);
        assert!(gfx.pdf_would_build(&probe_g()), "nothing registered");
        let mut work = 0;
        paint(&mut gfx, &crate::limits::LimitProbe::NONE, &mut work).expect("room");
        assert!(gfx.dirty);
        assert!(!gfx.pdf_would_build(&probe_g()));
        let mut again = 0;
        paint(&mut gfx, &crate::limits::LimitProbe::NONE, &mut again).expect("again");
        assert!(
            work >= again + 1024,
            "the new charstring is charged: {work} vs {again}"
        );
    }

    /// Run `src` with PDF text output on; the document as lossy text (font
    /// dicts are not compressed).
    fn pdf_text_document(src: &str) -> String {
        let mut it = crate::Interp::with_page(200, 100).expect("page");
        it.set_pdf_text(crate::PdfText::Text);
        it.enable_pdf();
        it.run_str(src).expect("runs");
        String::from_utf8_lossy(&it.pdf_document().expect("pdf")).into_owned()
    }

    /// #1193, Codex review: every committed `truetype` fuzz input (the
    /// seeds and the crash regressions that `tests/fuzz_corpus.rs` replays)
    /// that loads as a face is shown with PDF text output on, every byte,
    /// and the document finished. `hvar_region_index_overflow` panicked in
    /// the advance lookup before it was guarded.
    #[test]
    fn the_truetype_fuzz_corpus_survives_pdf_text() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fuzz");
        let mut files: Vec<_> = ["seeds/truetype", "regressions/truetype"]
            .iter()
            .flat_map(|d| std::fs::read_dir(root.join(d)).expect("corpus dir"))
            .map(|e| e.expect("entry").path())
            .collect();
        files.sort();
        assert!(files.len() >= 8, "{files:?}");
        let mut loaded = 0;
        for (i, path) in files.iter().enumerate() {
            let bytes = std::fs::read(path).expect("read seed");
            let name = format!("PscatPdfTextFuzz{i}");
            if register_font_bytes(&name, bytes).is_err() {
                continue;
            }
            loaded += 1;
            pdf_text_document(&format!(
                "/{name} findfont 20 scalefont setfont 10 50 moveto (ABC abc 0123) show \
                 0 1 255 {{ 1 string dup 0 4 -1 roll put show }} for"
            ));
        }
        assert!(loaded >= 4, "only {loaded} corpus faces loaded");
    }

    /// #1193, Codex review: a valid 16-unit em puts Liberation's outlines
    /// 62.5 times past 1000 units, beyond a Type 2 operand. Those glyphs
    /// keep their outlines (clamped, they drew at the wrong size), and no
    /// empty font is written for the face.
    #[test]
    fn outlines_past_the_type2_range_stay_paths() {
        let mut bytes = BUILTINS[0].data.to_vec();
        let head = table_offset(&bytes, b"head");
        bytes[head + 18..head + 20].copy_from_slice(&16u16.to_be_bytes());
        register_font_bytes("PscatTinyEmTest-Regular", bytes).expect("registers");
        let pdf = pdf_text_document(
            "/PscatTinyEmTest-Regular findfont 0.5 scalefont setfont 10 50 moveto (Hi) show",
        );
        assert!(!pdf.contains("/Subtype /Type0 "), "{pdf}");
    }

    /// #1193: a face whose OS/2 fsType forbids embedding keeps its outlines
    /// under PDF text output, beside a face that becomes text.
    #[test]
    fn a_restricted_licence_face_keeps_its_outlines_in_pdf_text() {
        let mut bytes = BUILTINS[0].data.to_vec();
        let at = table_offset(&bytes, b"OS/2") + 8;
        bytes[at..at + 2].copy_from_slice(&0x0002u16.to_be_bytes());
        assert!(!crate::pdf_font::embedding_allowed(
            &Face::parse(&bytes, 0).expect("parses")
        ));
        register_font_bytes("PscatRestrictedTest-Regular", bytes).expect("registers");
        let pdf = pdf_text_document(
            "/PscatRestrictedTest-Regular findfont 20 scalefont setfont 10 50 moveto (Hi) show \
             /Helvetica findfont 20 scalefont setfont (Hi) show",
        );
        // Font dicts are not compressed: one font, the Helvetica one.
        assert_eq!(pdf.matches("/Subtype /Type0 ").count(), 1, "{pdf}");
        assert!(pdf.contains("+LiberationSans /Encoding"), "{pdf}");
    }
}
