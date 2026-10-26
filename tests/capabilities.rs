//! The machine-readable capability catalog (issue #39) stays honest
//! about the PostScript libraries it describes, checked in both
//! directions:
//!   - every cataloged palette/template/procedure name must still be
//!     defined where the catalog says it is (a name gets renamed or
//!     removed -> this fails, not `--capabilities` silently);
//!   - every top-level name those libraries actually define must be
//!     either cataloged or explicitly listed as an internal helper in
//!     `capabilities::{ARTKIT_INTERNAL, PAGEKIT_INTERNAL}` -- so a
//!     newly added public palette/proc that nobody registered fails
//!     here instead of just being missing from `--capabilities`.

use std::collections::BTreeSet;
use std::process::Command;

use pscat::Interp;
use pscat::capabilities::{
    self, ARTKIT_INTERNAL, CapabilityKind, HANDSCRIPT_INTERNAL, HANGUL_INTERNAL, PAGEKIT_INTERNAL,
};

fn names_by(kind: CapabilityKind, source: &str) -> BTreeSet<String> {
    capabilities::catalog()
        .into_iter()
        .filter(|c| c.kind == kind && c.source == source)
        .map(|c| c.name)
        .collect()
}

/// Every top-level name currently bound in `userdict` -- `{ pop }
/// forall` leaves each key as an operand (value popped, key kept), so
/// once the loop finishes the whole enumeration sits on the operand
/// stack in one call, no printing/parsing needed.
fn userdict_names(it: &mut Interp) -> BTreeSet<String> {
    it.run_str("userdict { pop } forall")
        .unwrap_or_else(|e| panic!("enumerate userdict failed: {}", it.error_report(&e)));
    it.operand_stack()
        .iter()
        .map(|o| o.repr().trim_start_matches('/').to_string())
        .collect()
}

fn load(it: &mut Interp, path: &str) {
    let src = std::fs::read(path).unwrap_or_else(|e| panic!("{path} present: {e}"));
    it.run_source(&src)
        .unwrap_or_else(|e| panic!("{path} failed to load: {}", it.error_report(&e)));
}

fn assert_name_sets_match(context: &str, got: &BTreeSet<String>, expected: &BTreeSet<String>) {
    let missing_from_catalog: Vec<_> = got.difference(expected).collect();
    let stale_in_catalog: Vec<_> = expected.difference(got).collect();
    assert!(
        missing_from_catalog.is_empty() && stale_in_catalog.is_empty(),
        "{context}: names defined but not cataloged/allowlisted: {missing_from_catalog:?}; \
         names cataloged/allowlisted but not actually defined: {stale_in_catalog:?}"
    );
}

#[test]
fn artkit_names_match_the_catalog_exactly() {
    let mut it = Interp::new();
    load(&mut it, "lib/artkit.ps");
    let got = userdict_names(&mut it);

    let mut expected = names_by(CapabilityKind::Procedure, "lib/artkit.ps");
    expected.extend(ARTKIT_INTERNAL.iter().map(|s| s.to_string()));

    assert_name_sets_match("lib/artkit.ps", &got, &expected);
}

#[test]
fn style_pack_names_match_the_catalog_exactly() {
    let mut base_it = Interp::new();
    load(&mut base_it, "lib/artkit.ps");
    let artkit_baseline = userdict_names(&mut base_it);

    for (file, source) in [
        ("lib/styles/steampunk.ps", "lib/styles/steampunk.ps"),
        ("lib/styles/psychedelic.ps", "lib/styles/psychedelic.ps"),
        ("lib/styles/scifi.ps", "lib/styles/scifi.ps"),
        ("lib/styles/toon.ps", "lib/styles/toon.ps"),
    ] {
        let mut it = Interp::new();
        load(&mut it, "lib/artkit.ps");
        load(&mut it, file);
        let all = userdict_names(&mut it);
        // Palettes are registered with `Palettes /name [...] put` (a
        // mutation of the existing dict), not `def`, so they never
        // land in userdict -- pack_specific is procedures/dials only,
        // checked against tests::palettes_match_the_catalog_exactly
        // separately. Set-difference also means a pack that *redefined*
        // an artkit name (shadowing it with its own `def` of the same
        // name) would be swallowed here rather than flagged -- no pack
        // does that today, and it's an accepted gap in this reverse
        // check, not a case this test is designed to catch.
        let pack_specific: BTreeSet<String> = all.difference(&artkit_baseline).cloned().collect();

        let mut expected = names_by(CapabilityKind::Procedure, source);
        expected.extend(names_by(CapabilityKind::Dial, source));
        assert_name_sets_match(file, &pack_specific, &expected);
    }
}

#[test]
fn palettes_match_the_catalog_exactly() {
    let mut it = Interp::new();
    for file in [
        "lib/artkit.ps",
        "lib/styles/steampunk.ps",
        "lib/styles/psychedelic.ps",
        "lib/styles/scifi.ps",
        "lib/styles/toon.ps",
        "lib/pagekit.ps",
    ] {
        load(&mut it, file);
    }
    it.run_str("Palettes { pop } forall")
        .unwrap_or_else(|e| panic!("enumerate Palettes failed: {}", it.error_report(&e)));
    let got: BTreeSet<String> = it
        .operand_stack()
        .iter()
        .map(|o| o.repr().trim_start_matches('/').to_string())
        .collect();

    let expected: BTreeSet<String> = capabilities::catalog()
        .into_iter()
        .filter(|c| c.kind == CapabilityKind::Palette)
        .map(|c| c.name)
        .collect();

    assert_name_sets_match("Palettes dict (all contributors)", &got, &expected);
}

#[test]
fn pagekit_names_match_the_catalog_exactly() {
    let mut base_it = Interp::new();
    load(&mut base_it, "lib/artkit.ps");
    let artkit_baseline = userdict_names(&mut base_it);

    let mut it = Interp::new();
    load(&mut it, "lib/artkit.ps");
    load(&mut it, "lib/pagekit.ps");
    let all = userdict_names(&mut it);
    let pagekit_specific: BTreeSet<String> = all.difference(&artkit_baseline).cloned().collect();

    let mut expected = names_by(CapabilityKind::Template, "lib/pagekit.ps");
    expected.extend(PAGEKIT_INTERNAL.iter().map(|s| s.to_string()));

    assert_name_sets_match("lib/pagekit.ps", &pagekit_specific, &expected);
}

/// Runs the same forward/reverse name cross-check as the per-file
/// tests above, but generically, over every file `capabilities::
/// migrated_files()` reports -- so migrating the *next* `lib/*.ps`
/// file to the `% @...` tag convention gets this protection
/// automatically, with no new hand-written test function to remember.
/// That's the actual gap `docs/dev/PS_LIBRARY_COUPLING.md` (issue #94's
/// spec) calls out: a near-duplicate per-file test is exactly as easy
/// to forget as the catalog registration itself. `build.rs` only
/// guarantees a new file gets *noticed* (tagged or in `LEGACY_FILES`)
/// -- per-*name* coverage for a migrated file still rests on its
/// tokenizer being right, and this is the independent ground truth
/// (a real `Interp`'s `userdict`) that catches it if not.
#[test]
fn every_migrated_file_names_match_the_catalog_exactly() {
    for mf in capabilities::migrated_files() {
        let mut base_it = Interp::new();
        if !mf.requires.is_empty() {
            base_it.run_str(mf.requires).unwrap_or_else(|e| {
                panic!(
                    "{}: @requires {:?} failed: {}",
                    mf.source,
                    mf.requires,
                    base_it.error_report(&e)
                )
            });
        }
        let baseline = userdict_names(&mut base_it);

        let mut it = Interp::new();
        if !mf.requires.is_empty() {
            it.run_str(mf.requires).unwrap_or_else(|e| {
                panic!(
                    "{}: @requires {:?} failed: {}",
                    mf.source,
                    mf.requires,
                    it.error_report(&e)
                )
            });
        }
        load(&mut it, mf.source);
        let all = userdict_names(&mut it);
        let file_specific: BTreeSet<String> = all.difference(&baseline).cloned().collect();

        // Palettes are `Palettes /name [...] put` dict mutations, not
        // `def`s, so they never land in userdict -- same exclusion
        // style_pack_names_match_the_catalog_exactly above makes (no
        // migrated file defines a palette yet, but this stays correct
        // if/when one does).
        let mut expected: BTreeSet<String> = capabilities::catalog()
            .into_iter()
            // a Type 3 face lives in FontDirectory, not userdict (lib/typewriter.ps
            // is migrated and also holds the hand-written /Typewriter row)
            .filter(|c| {
                c.source == mf.source
                    && c.kind != CapabilityKind::Palette
                    && c.kind != CapabilityKind::Type3Face
            })
            .map(|c| c.name)
            .collect();
        expected.extend(mf.internal_names.iter().map(|s| s.to_string()));

        assert_name_sets_match(mf.source, &file_specific, &expected);
    }
}

/// `lib/paintkit.ps` is the first file migrated to the `% @...`
/// doc-comment tag convention (issue #94) -- `paintkit_names_match_the
/// _catalog_exactly` above already proves every name round-trips, but
/// that name-set check alone wouldn't catch a `build.rs` parser bug
/// that gets a *value* wrong (e.g. an off-by-one in the `@requires`
/// splice, or a `@param` default parsed from the wrong side of
/// `(default ...)`) while still producing the right set of names. This
/// checks specific field values directly against what `lib/paintkit.ps`'s
/// tags actually say.
#[test]
fn generated_paintkit_entries_have_the_right_fields() {
    let caps = capabilities::catalog();

    let pkoil = caps
        .iter()
        .find(|c| c.name == "pkoil" && c.source == "lib/paintkit.ps")
        .expect("pkoil in the generated catalog");
    assert!(pkoil.kind == CapabilityKind::Procedure);
    assert_eq!(pkoil.load, "(lib/artkit.ps) run (lib/paintkit.ps) run");
    assert_eq!(
        pkoil.example,
        "newpath ... << /Width 16 /Ridges 12 /Load 0.9 >> pkoil"
    );
    let width = pkoil
        .parameters
        .iter()
        .find(|p| p.name == "Width")
        .expect("pkoil has a /Width parameter");
    assert_eq!(width.default, Some("14"));

    let pkflat = caps
        .iter()
        .find(|c| c.name == "pkflat" && c.source == "lib/paintkit.ps")
        .expect("pkflat in the generated catalog");
    assert!(pkflat.kind == CapabilityKind::Procedure);
    assert_eq!(pkflat.load, "(lib/artkit.ps) run (lib/paintkit.ps) run");
    assert!(pkflat.parameters.is_empty());

    // `/Density`'s @param description has its own parenthesized aside
    // ("(0 renders nothing)") before the trailing `(default ...)` --
    // pins that `parse_param`'s `rfind("(default ")` picks the
    // *trailing* parenthetical, not the first one, and doesn't let it
    // leak into either field.
    let pkspray = caps
        .iter()
        .find(|c| c.name == "pkspray" && c.source == "lib/paintkit.ps")
        .expect("pkspray in the generated catalog");
    let density = pkspray
        .parameters
        .iter()
        .find(|p| p.name == "Density")
        .expect("pkspray has a /Density parameter");
    assert_eq!(density.default, Some("30"));
    assert!(density.description.ends_with(
        "(0 renders nothing); uncapped, bounded by the deposit-budget safety limit instead"
    ));
}

/// Issue #141 (paintkit), generalised in #195 to every migrated library:
/// every parameter's description starts with its unit in brackets, so an
/// agent reading `--capabilities` can tell "Offset 14" (points) from
/// "Spread 0.8" (a fraction of /Width) without opening the library. The
/// vocabulary is closed on purpose: a check for a leading `[` alone would
/// accept `/Under [r g b] ...` with no unit added, and free-form units
/// drift (`pt`, `pts`, `user units`). A lone unit is a fixed token or one
/// of the prefix forms; `array of U` and `U or V` combine known units, so
/// `[x0 y0 x1 y1]` is `array of points` and a `/Palette` is `name or array
/// of rgb arrays`. Add a token here only when no existing one says what the
/// number is; lengths in user space are `points`, as in paintkit.
fn unit_is_known(unit: &str) -> bool {
    const FIXED: &[&str] = &[
        "points",
        "degrees",
        "count",
        "0..1",
        "ratio",
        "proc",
        "name",
        "rgb array",
        "array of rgb arrays",
        "boolean",
        "integer",
        "string",
        "dict",
        "array",
    ];
    if FIXED.contains(&unit) {
        return true;
    }
    if ["fraction of ", "per ", "multiple of ", "count per "]
        .iter()
        .any(|form| unit.strip_prefix(form).is_some_and(|rest| !rest.is_empty()))
    {
        return true;
    }
    if let Some(inner) = unit.strip_prefix("array of ") {
        return unit_is_known(inner);
    }
    unit.split_once(" or ")
        .is_some_and(|(a, b)| unit_is_known(a) && unit_is_known(b))
}

#[test]
fn the_unit_vocabulary_stays_closed() {
    for ok in [
        "points",
        "array of points",
        "name or array of rgb arrays",
        "count or name",
        "array of fraction of the box",
        "count per inch",
    ] {
        assert!(unit_is_known(ok), "[{ok}] should be a known unit");
    }
    for bad in [
        "pt",
        "num",
        "user units",
        "x0 y0 x1 y1",
        "h s v",
        "array of",
        "points or",
        "points or pt",
        "fraction of ",
    ] {
        assert!(!unit_is_known(bad), "[{bad}] should not be a known unit");
    }
}

/// The unit a description leads with, or a panic naming the parameter.
fn leading_unit<'a>(owner: &str, name: &str, description: &'a str) -> &'a str {
    description
        .strip_prefix('[')
        .and_then(|rest| rest.split_once("] "))
        .map(|(unit, _)| unit)
        .unwrap_or_else(|| {
            panic!("{owner} /{name}: description must start with a [unit], got {description:?}")
        })
}

#[test]
fn every_migrated_parameter_states_its_unit() {
    let caps = capabilities::catalog();
    let mut total = 0;
    for mf in capabilities::migrated_files() {
        let rows: Vec<_> = caps.iter().filter(|c| c.source == mf.source).collect();
        let mut checked = 0;
        for cap in &rows {
            for p in &cap.parameters {
                let unit = leading_unit(&cap.name, p.name, p.description);
                assert!(
                    unit_is_known(unit),
                    "{} {} /{}: unknown unit [{unit}]",
                    mf.source,
                    cap.name,
                    p.name
                );
                // The two names #141 was about mean one thing everywhere.
                // fillkit's /Spread is the one deliberate exception (#195):
                // GIMP's name for a 0..1 sample-coordinate jitter, with no
                // /Width to be a fraction of.
                match p.name {
                    "Spread" if mf.source == "lib/fillkit.ps" => {
                        assert_eq!(unit, "fraction of the box", "{} /Spread", cap.name)
                    }
                    "Spread" => assert_eq!(unit, "fraction of /Width", "{} /Spread", cap.name),
                    "Load" => assert_eq!(unit, "per /Width", "{} /Load", cap.name),
                    _ => {}
                }
                checked += 1;
            }
        }
        // A file that declares `@param` tags must have had them all read:
        // a new migrated library is covered by construction, and one whose
        // tags never reach the catalog fails here instead of passing empty.
        let src = std::fs::read_to_string(mf.source).expect("read migrated file");
        let declared = src.lines().filter(|l| l.starts_with("% @param:")).count();
        assert_eq!(
            declared, checked,
            "{}: {declared} @param tags in the file but {checked} checked",
            mf.source
        );
        total += checked;
    }
    assert!(total > 350, "only {total} migrated parameters checked");
}

#[test]
fn every_paintkit_parameter_has_the_issue_141_shape() {
    let caps = capabilities::catalog();
    let paintkit: Vec<_> = caps
        .iter()
        .filter(|c| c.source == "lib/paintkit.ps")
        .collect();
    assert!(!paintkit.is_empty(), "no paintkit entries in the catalog");
    let checked: usize = paintkit.iter().map(|c| c.parameters.len()).sum();
    assert!(checked > 100, "only {checked} paintkit parameters checked");

    // pkcanvas (issue #182) is a setup procedure, not a preset, but its
    // four keys are lengths and fractions like any other and carry units.
    let canvas = paintkit
        .iter()
        .find(|c| c.name == "pkcanvas")
        .expect("pkcanvas in the catalog");
    let units: Vec<(&str, &str)> = canvas
        .parameters
        .iter()
        .map(|p| {
            let unit = p.description[1..].split_once("] ").unwrap().0;
            (p.name, unit)
        })
        .collect();
    assert_eq!(
        units,
        [
            ("Seed", "integer"),
            ("Weave", "points"),
            ("Mottle", "points"),
            ("Relief", "0..1"),
        ]
    );

    // The deprecated aliases still work but are not advertised: a row for
    // them would put the second meaning back into the catalog.
    for (preset, old, new) in [
        ("pkwet", "Spread", "Offset"),
        ("pktrowel", "Load", "Cover"),
        ("pkdab", "Spread", "Scatter"),
        ("pkcrisscross", "Spread", "Lean"),
    ] {
        let cap = paintkit
            .iter()
            .find(|c| c.name == preset)
            .unwrap_or_else(|| panic!("{preset} in the catalog"));
        let names: Vec<&str> = cap.parameters.iter().map(|p| p.name).collect();
        assert!(names.contains(&new), "{preset} advertises /{new}");
        assert!(
            !names.contains(&old),
            "{preset} must not advertise its deprecated /{old}"
        );
    }
}

/// The catalog names the loaded-colour keys (issue #183, specimen slice #227)
/// on every preset that takes them, each with the unit the design gives it
/// (PAINT_LOADING.md 2.1), and does not advertise the British spellings the
/// presets reject. `pkwet`'s `/Direction` row must tell an artist that a
/// loaded colour is graded for them, or the "paint in the colour you
/// inherit" rule reads as forbidding `/Colors`.
#[test]
fn the_catalog_names_the_loaded_color_keys_on_every_adopter() {
    let caps = capabilities::catalog();
    let unit_of = |preset: &str, key: &str| -> String {
        let cap = caps
            .iter()
            .find(|c| c.source == "lib/paintkit.ps" && c.name == preset)
            .unwrap_or_else(|| panic!("{preset} in the catalog"));
        let p = cap
            .parameters
            .iter()
            .find(|p| p.name == key)
            .unwrap_or_else(|| panic!("{preset} does not advertise /{key}"));
        p.description[1..].split_once("] ").unwrap().0.to_string()
    };
    for preset in ["pkfan", "pkbroad", "pktrowel", "pkdab"] {
        assert_eq!(unit_of(preset, "Colors"), "array of rgb arrays", "{preset}");
        assert_eq!(unit_of(preset, "ColorMix"), "0..1", "{preset}");
    }
    for preset in ["pkfan", "pkbroad", "pktrowel"] {
        assert_eq!(unit_of(preset, "Light"), "degrees", "{preset}");
    }
    for preset in ["pkdab", "pkoil"] {
        assert_eq!(unit_of(preset, "HighlightColor"), "rgb array", "{preset}");
        assert_eq!(unit_of(preset, "ShadowColor"), "rgb array", "{preset}");
    }
    for cap in caps.iter().filter(|c| c.source == "lib/paintkit.ps") {
        for p in &cap.parameters {
            assert!(
                !p.name.contains("Colour"),
                "{} advertises the British spelling /{}",
                cap.name,
                p.name
            );
        }
    }
    let pkwet = caps.iter().find(|c| c.name == "pkwet").expect("pkwet");
    let direction = pkwet
        .parameters
        .iter()
        .find(|p| p.name == "Direction")
        .expect("pkwet /Direction");
    assert!(
        direction.description.contains("loaded colors (/Colors)"),
        "pkwet /Direction does not say loaded colors are graded"
    );
}

/// drykit (issue #234) and the dry-media files built on it (`lib/dry*.ps`)
/// follow paintkit's #141 rules (docs/dev/design/DRYMEDIA.md 3): every parameter
/// states a unit (checked for every migrated file by
/// `every_migrated_parameter_states_its_unit`), and neither `/Spread` nor
/// `/Density` appears, since paintkit and stipplekit already gave those
/// names other meanings.
#[test]
fn drykit_files_do_not_reuse_reserved_parameter_names() {
    let caps = capabilities::catalog();
    let dry: Vec<_> = caps
        .iter()
        .filter(|c| c.source.starts_with("lib/dry") && c.source.ends_with(".ps"))
        .collect();
    assert!(
        dry.iter().any(|c| c.name == "dksurface"),
        "dksurface missing from the catalog"
    );
    let mut checked = 0;
    for cap in &dry {
        for p in &cap.parameters {
            assert!(
                !matches!(p.name, "Spread" | "Density"),
                "{} advertises /{}, which drykit must not use (#141)",
                cap.name,
                p.name
            );
            checked += 1;
        }
    }
    assert!(checked >= 6, "only {checked} drykit parameters checked");
}

/// `lib/handscript.ps`/`lib/hangul.ps` are standalone (no artkit
/// dependency, unlike pagekit/the style packs), so each gets its own
/// fresh `Interp` rather than an artkit-baseline diff.
#[test]
fn handscript_and_hangul_names_match_the_catalog_exactly() {
    for (file, source, internal) in [
        (
            "lib/handscript.ps",
            "lib/handscript.ps",
            HANDSCRIPT_INTERNAL,
        ),
        ("lib/hangul.ps", "lib/hangul.ps", HANGUL_INTERNAL),
    ] {
        let mut it = Interp::new();
        load(&mut it, file);
        let got = userdict_names(&mut it);

        let mut expected = names_by(CapabilityKind::Procedure, source);
        expected.extend(internal.iter().map(|s| s.to_string()));

        assert_name_sets_match(file, &got, &expected);
    }
}

/// Type 3 faces don't go through `findfont`'s Rust-side registry
/// (`--fonts`/`resolve`) -- they're defined by `definefont` straight
/// into PostScript's own `FontDirectory`. `findfont` on an unknown name
/// substitutes rather than erroring (a deliberate deviation, see
/// docs/dev/HANDOFF.md), so success/failure of `findfont` itself can't tell a
/// real face from a substituted one; checking `FontDirectory` directly
/// is what actually proves the file defines the name the catalog claims.
#[test]
fn type3_faces_are_actually_defined_after_loading() {
    // Type3 entries carry per-face source paths, not one shared
    // constant, so gather (name, source) pairs from the catalog
    // instead of filtering by a fixed source string.
    let entries: Vec<(String, String)> = capabilities::catalog()
        .into_iter()
        .filter(|c| c.kind == CapabilityKind::Type3Face)
        .map(|c| (c.name, c.source))
        .collect();

    // Reverse direction: every `.ps` file under lib/fonts/ must be
    // cataloged, not just every cataloged entry checked forward --
    // otherwise a new face added to the directory and never
    // registered in capabilities.rs would pass silently. Two more
    // Type 3 faces (HandScript, HangulScript) predate the lib/fonts/
    // convention and live at lib/handscript.ps / lib/hangul.ps
    // instead -- named explicitly here rather than widening the scan
    // to all of lib/*.ps, which would also need to allowlist every
    // non-Type3 sibling library (graph.ps, dataviz.ps, etching.ps,
    // pagekit.ps, artkit.ps itself) as "not a face," muddying what
    // this check is actually verifying. A cross-model review found
    // HandScript missing here on the first PR (#74); it and
    // HangulScript's Type3-family status were confirmed the same way
    // -- FontDirectory holds their names after loading their file.
    let cataloged_sources: BTreeSet<String> =
        entries.iter().map(|(_, source)| source.clone()).collect();
    let mut on_disk: BTreeSet<String> = std::fs::read_dir("lib/fonts")
        .expect("lib/fonts present")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("ps"))
        .map(|p| format!("lib/fonts/{}", p.file_name().unwrap().to_string_lossy()))
        .collect();
    on_disk.insert("lib/handscript.ps".to_string());
    on_disk.insert("lib/hangul.ps".to_string());
    // The typewriter (#211) is a migrated library whose font row is
    // hand-written: build.rs does not take @kind: Type3Face.
    on_disk.insert("lib/typewriter.ps".to_string());
    assert_name_sets_match(
        "lib/fonts/*.ps + named outliers",
        &on_disk,
        &cataloged_sources,
    );

    for (name, source) in entries {
        let mut it = Interp::with_page(200, 200).expect("page");
        if source == "lib/typewriter.ps" {
            // the face draws from the generated outline table
            load(&mut it, "lib/typewriter_outlines.ps");
        }
        load(&mut it, &source);
        it.run_str(&format!("FontDirectory /{name} known"))
            .unwrap_or_else(|e| panic!("{source}: known check failed: {}", it.error_report(&e)));
        let stack = it.operand_stack();
        assert_eq!(
            stack.last().map(|o| o.repr()).as_deref(),
            Some("true"),
            "{source} does not define /{name} in FontDirectory"
        );
    }
}

/// Guards a real gap Codex review found on PR #97 (issue #94): once a
/// file migrates to the `% @...` tag convention, its old hand-written
/// rows in `ENTRIES` must actually be deleted, not just left alongside
/// the new `GENERATED_ENTRIES` rows -- `capabilities::catalog()`
/// concatenates both, so a forgotten stale row produces a duplicate
/// (name, kind, source) triple. Every other cross-check here converts
/// to a `BTreeSet` before comparing, so a duplicate would pass all of
/// them silently while `--capabilities`/`describe_art_capabilities`
/// still emitted it twice.
#[test]
fn catalog_has_no_duplicate_entries() {
    // Keyed by (name, source), not (name, kind, source): one PS name
    // in one source file can't legitimately have two different kinds,
    // so including `kind` in the key would let a stale hand-written
    // row masquerading under a different kind than its generated
    // replacement (e.g. (foo, Dial, src) vs (foo, Procedure, src))
    // slip past this check as two "distinct" entries (Codex review,
    // PR #97, round 2) -- the payload would still emit the same
    // binding twice.
    let caps = capabilities::catalog();
    let mut seen = BTreeSet::new();
    let mut dupes = Vec::new();
    for c in &caps {
        let key = (c.name.clone(), c.source.clone());
        if !seen.insert(key.clone()) {
            dupes.push(key);
        }
    }
    assert!(
        dupes.is_empty(),
        "duplicate (name, source) catalog rows: {dupes:?}"
    );
}

/// Every catalog font alias -- both the curated `ALIASES` table and
/// the implicit `-Regular`-stripped ones (issue #39's third
/// cross-model review round: a catalog holding only
/// `Bangers-Regular.ttf` makes `/Bangers findfont` resolve too, which
/// a stem/ALIASES-only scan had been silently omitting) -- must
/// actually resolve to the same face as its stated target, not a
/// silent Helvetica substitution.
#[test]
fn font_aliases_resolve_to_their_stated_target() {
    let aliases: Vec<(String, String)> = pscat::font::catalog_entries()
        .into_iter()
        .filter(|e| e.origin == pscat::font::FontOrigin::Alias)
        .map(|e| (e.name, e.alias_target.expect("alias has a target")))
        .collect();
    assert!(!aliases.is_empty(), "expected at least one catalog alias");

    let mut it = Interp::new();
    for (name, target) in aliases {
        it.run_str(&format!(
            "/{name} findfont /FontName get /{target} findfont /FontName get eq"
        ))
        .unwrap_or_else(|e| panic!("resolving {name}/{target} failed: {}", it.error_report(&e)));
        let stack = it.operand_stack();
        assert_eq!(
            stack.last().map(|o| o.repr()).as_deref(),
            Some("true"),
            "/{name} findfont does not resolve to the same face as its stated target /{target}"
        );
    }
}

#[test]
fn fonts_agree_with_available_fonts() {
    let from_capabilities: BTreeSet<String> = capabilities::catalog()
        .into_iter()
        .filter(|c| c.kind == CapabilityKind::Font)
        .map(|c| c.name)
        .collect();
    let from_available: BTreeSet<String> = pscat::font::catalog_entries()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(
        from_capabilities, from_available,
        "capabilities::catalog()'s font entries must match font::catalog_entries() exactly \
         (both should derive from the same source, not drift independently)"
    );
}

#[test]
fn cli_capabilities_flag_prints_valid_matching_json() {
    let exe = env!("CARGO_BIN_EXE_pscat");
    let out = Command::new(exe)
        .arg("--capabilities")
        .output()
        .expect("run pscat --capabilities");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("--capabilities prints valid JSON");
    assert!(json["pscat_version"].is_string());
    assert!(json["catalog_signature"].is_string());
    let caps = json["capabilities"].as_array().expect("capabilities array");
    assert!(
        caps.len() > 100,
        "expected a substantial catalog, got {}",
        caps.len()
    );
    for cap in caps {
        for key in [
            "name",
            "kind",
            "description",
            "parameters",
            "source",
            "load",
            "example",
            "availability",
        ] {
            assert!(cap.get(key).is_some(), "entry missing {key:?}: {cap}");
        }
    }
}

/// Issue #236: `lib/strokeglyphs.ps` keeps paintkit's #141 convention
/// (its units are checked by `every_migrated_parameter_states_its_unit`)
/// and never uses `/Spread` or `/Density` (docs/dev/design/DRYMEDIA.md 3.1); nor does
/// `lib/mathkit.ps` (#237), which lays out its glyphs.
#[test]
fn strokeglyphs_does_not_reuse_reserved_parameter_names() {
    let caps = capabilities::catalog();
    let mut checked = 0;
    for cap in caps
        .iter()
        .filter(|c| c.source == "lib/strokeglyphs.ps" || c.source == "lib/mathkit.ps")
    {
        for p in &cap.parameters {
            assert!(
                p.name != "Spread" && p.name != "Density",
                "{} /{}: a name #141 reserves",
                cap.name,
                p.name
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 40,
        "only {checked} strokeglyphs and mathkit parameters checked"
    );
}

/// Issue #754: pscat's native extension operators are cataloged with
/// `kind: "operator"`, through the serialized payload an agent reads (not
/// just the Rust enum), and every entry has a kind from the closed set.
#[test]
fn payload_lists_native_operators_with_kind_operator() {
    let payload = capabilities::payload_json();
    let caps = payload["capabilities"].as_array().expect("array");
    let find = |name: &str| {
        caps.iter()
            .find(|c| c["name"] == name)
            .unwrap_or_else(|| panic!("{name} missing from the catalog"))
    };
    let g = find("gnoise2");
    assert_eq!(g["kind"], "operator");
    assert_eq!(g["source"], "systemdict");
    assert_eq!(g["load"], "");
    assert_eq!(g["example"], "x y gnoise2 n");
    assert!(!g["description"].as_str().unwrap().is_empty());
    for name in [
        "noiseseed",
        "gnoise3",
        "gnoise2d",
        "vnoise2",
        "vnoise3",
        "fbm2",
        "fbm3",
        "vfbm2",
        "vfbm3",
        "ridged2",
        "ridged3",
        "ridgedmf2",
        "erodedfbm2",
        "setalpha",
        "currentalpha",
        "setblendmode",
        "currentblendmode",
        ".sameclip",
        "writereals",
        "readreals",
    ] {
        assert_eq!(find(name)["kind"], "operator", "{name}");
    }
    let kinds: BTreeSet<&str> = caps.iter().map(|c| c["kind"].as_str().unwrap()).collect();
    let known: BTreeSet<&str> = [
        "font",
        "type3_face",
        "palette",
        "template",
        "procedure",
        "operator",
        "dial",
    ]
    .into();
    assert!(kinds.is_subset(&known), "unexpected kinds: {kinds:?}");
    // The library procedures keep their kind.
    assert_eq!(find("nrgnoise2")["kind"], "procedure");
}

/// Every cataloged operator is a real native operator in `systemdict`
/// (not a missing name, and not a library procedure shadowing one).
#[test]
fn every_cataloged_operator_exists_in_systemdict() {
    let mut it = Interp::new();
    let ops: Vec<String> = capabilities::catalog()
        .into_iter()
        .filter(|c| c.kind == CapabilityKind::Operator)
        .map(|c| c.name)
        .collect();
    assert!(ops.len() >= 18, "operators: {ops:?}");
    for name in &ops {
        it.run_str(&format!(
            "systemdict /{name} known {{ systemdict /{name} get type /operatortype eq }} {{ false }} ifelse"
        ))
        .unwrap_or_else(|e| panic!("probe {name}: {}", it.error_report(&e)));
        let top = it.operand_stack().last().expect("result").repr();
        assert_eq!(top, "true", "{name} is not an operator in systemdict");
    }
}

/// Every `nr*` noise reference procedure in the catalog names its native
/// operator, and that operator is cataloged as an operator.
#[test]
fn noiseref_procedures_point_to_cataloged_native_operators() {
    let caps = capabilities::catalog();
    let operators: BTreeSet<String> = caps
        .iter()
        .filter(|c| c.kind == CapabilityKind::Operator)
        .map(|c| c.name.clone())
        .collect();
    let refs: Vec<_> = caps
        .iter()
        .filter(|c| c.source == "lib/noiseref.ps" && c.kind == CapabilityKind::Procedure)
        .collect();
    assert_eq!(refs.len(), 12, "the twelve nr* twins");
    for c in refs {
        let stem = c.name.strip_prefix("nr").expect("nr prefix");
        // nrseed is the twin of noiseseed; every other twin drops only the prefix.
        let native = if stem == "seed" { "noiseseed" } else { stem };
        assert!(
            operators.contains(native),
            "{} has no cataloged native operator {native}",
            c.name
        );
        assert!(
            c.description.contains(&format!("native {native} operator")),
            "{}: summary must name the native operator: {}",
            c.name,
            c.description
        );
    }
}
