//! QR data, correction, matrix, mask-selection and decode regression tests. Frozen fixtures come from an independent,
//! pinned encoder, not a second implementation of our packing algorithm.

mod common;
use common::CommandDeadline as _;
use pscat::{Interp, Value};
use sha2::{Digest, Sha256};
use std::process::Command;

const LIB: &str = include_str!("../lib/qr.ps");
const VECTORS: &str = include_str!("data/qr_byte_vectors.json");
const PLANNING: &str = include_str!("data/qr_planning_vectors.json");
const CORRECTION: &str = include_str!("data/qr_correction_vectors.json");
const MATRIX: &str = include_str!("data/qr_matrix_vectors.json");
const MASKS: &str = include_str!("data/qr_mask_vectors.json");
const GITHUB_URL: &str = "https://github.com/PSCatLabs/pscat";

fn interpreter() -> Interp {
    let mut it = Interp::with_page(8, 8).expect("small test page");
    it.run_str(LIB)
        .unwrap_or_else(|e| panic!("load qr.ps: {}", it.error_report(&e)));
    assert!(
        it.operand_stack().is_empty(),
        "library load leaked operands"
    );
    it
}

#[test]
fn byte_library_is_embedded_and_loads_outside_the_checkout() {
    assert_eq!(pscat::paths::embedded("lib/qr.ps"), Some(LIB.as_bytes()));
    let dir = common::scratch_dir("qr-embedded");
    let out = Command::new(env!("CARGO_BIN_EXE_pscat"))
        .current_dir(dir)
        .env_remove("PSCAT_HOME")
        .env_remove("PSCAT_ROOT")
        .env_remove("PSCAT_LIB")
        .env("PSCAT_TRACE_RESOURCES", "1")
        .args(["-e", "(lib/qr.ps) run (A) 1 3 qr--byte-data <401410> eq ="])
        .output_deadline()
        .expect("run pscat outside checkout");
    assert!(out.status.success(), "{:?}", out.stderr);
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "true");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("lib/qr.ps <- embedded"),
        "library did not resolve from embedded resources: {:?}",
        out.stderr
    );
}

fn assert_true(it: &mut Interp, src: &str, label: &str) {
    it.run_str(src)
        .unwrap_or_else(|e| panic!("{label}: {}", it.error_report(&e)));
    let got: Vec<_> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(got, ["true"], "{label}: unexpected values/stack leak");
    it.run_str("pop").expect("pop assertion result");
}

fn pop_string(it: &mut Interp, label: &str) -> Vec<u8> {
    assert_eq!(it.operand_stack().len(), 1, "{label}: stack leak");
    let bytes = match &it.operand_stack()[0].value {
        Value::String(s) => s.borrow_bytes().to_vec(),
        _ => panic!(
            "{label}: expected string, got {}",
            it.operand_stack()[0].repr()
        ),
    };
    it.run_str("pop").expect("pop string");
    bytes
}

fn digest_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn vectors() -> serde_json::Value {
    serde_json::from_str(VECTORS).expect("checked-in fixture JSON")
}

fn source(f: &serde_json::Value) -> String {
    format!(
        "<{}> {} {} qr--byte-data <{}> eq",
        f["payload"].as_str().expect("payload"),
        f["version"],
        f["capacity"],
        f["codewords"].as_str().expect("codewords")
    )
}

fn guard(src: &str, name: &str, it: &mut Interp) {
    // Unlike mustguard, this checks the actual post-error dictionary stack
    // before any harness cleanup can hide a leaked private begin frame.
    assert_true(
        it,
        &format!(
            "/dictdepth countdictstack def {{ {src} }} stopped \
             $error /command get /{name} eq and \
             countdictstack dictdepth eq and \
             4 1 roll pop pop pop"
        ),
        src,
    );
}

#[test]
fn byte_data_matches_independent_reference_at_versions_levels_and_boundaries() {
    let mut it = interpreter();
    let data = vectors();
    let fixtures = data["fixtures"].as_array().expect("fixtures");
    assert!(fixtures.len() >= 90, "fixture corpus unexpectedly shrank");
    for f in fixtures {
        assert_true(&mut it, &source(f), f["name"].as_str().expect("name"));
        if f["name"].as_str().expect("name").ends_with("-maximum") {
            guard(
                &format!(
                    "<{}00> {} {} qr--byte-data",
                    f["payload"].as_str().expect("payload"),
                    f["version"],
                    f["capacity"]
                ),
                "qr-data-does-not-fit-capacity",
                &mut it,
            );
        }
    }
}

#[test]
fn guards_preserve_dictionary_depth_and_name_the_bad_argument() {
    let mut it = interpreter();
    for (src, guardname) in [
        ("() 1 19.0", "qr-data-capacity-must-be-integer"),
        ("() 1 true", "qr-data-capacity-must-be-integer"),
        ("() 1 -1", "qr-data-capacity-must-be-in-1-2956"),
        ("() 1 0", "qr-data-capacity-must-be-in-1-2956"),
        ("() 1 2957", "qr-data-capacity-must-be-in-1-2956"),
        ("() 1.0 19", "qr-data-version-must-be-integer"),
        ("() /one 19", "qr-data-version-must-be-integer"),
        ("() 0 19", "qr-data-version-must-be-in-1-40"),
        ("() 41 19", "qr-data-version-must-be-in-1-40"),
        ("[] 1 19", "qr-data-must-be-a-string"),
        ("{} 1 19", "qr-data-must-be-a-string"),
        ("256 string 9 300", "qr-data-count-does-not-fit-version"),
        ("() 1 1", "qr-data-does-not-fit-capacity"),
        ("(AB) 1 3", "qr-data-does-not-fit-capacity"),
    ] {
        guard(&format!("{src} qr--byte-data"), guardname, &mut it);
    }
}

#[test]
fn count_width_ownership_and_repeated_calls_do_not_depend_on_caller_names() {
    let mut it = interpreter();
    assert_true(
        &mut it,
        "/dictdepth countdictstack def \
         /out (caller) def /n 42 def /capacity 7 def \
         /input <00ff41> readonly def \
         /first input 1 19 qr--byte-data def \
         /second input 1 19 qr--byte-data def \
         first 0 0 put \
         input <00ff41> eq second 0 get 64 eq and \
         out (caller) eq n 42 eq and capacity 7 eq and and \
         255 string 9 300 qr--byte-data length 300 eq and \
         256 string 10 300 qr--byte-data length 300 eq and \
         countdictstack dictdepth eq and",
        "ownership/count-width/reentrancy",
    );
    assert!(
        it.gfx().pixmap.pixels().iter().all(|p| p.red() == 255),
        "data stage drew on the page"
    );
    assert!(!it.gfx().page_shown);
}

#[test]
fn executable_strings_are_packed_as_bytes_without_executing_or_changing_them() {
    let mut it = interpreter();
    assert_true(
        &mut it,
        "/dictdepth countdictstack def \
         /executable (pop) cvx def \
         /executable load 1 5 qr--byte-data <403706f700> eq \
         /executable load xcheck and \
         () cvx 1 2 qr--byte-data <4000> eq and \
         countdictstack dictdepth eq and",
        "executable payload is raw data",
    );
}

#[test]
fn ghostscript_matches_every_reference_fixture_and_access_contract() {
    // Existing repository gs tests permit an unavailable optional Ghostscript.
    // Required rendered-QR decoding later has a separate, non-skipping gate.
    if Command::new("gs")
        .arg("--version")
        .output_deadline()
        .is_err()
    {
        eprintln!("gs unavailable; QR data portability check not run");
        return;
    }
    let data = vectors();
    let mut src = String::from(LIB);
    for f in data["fixtures"].as_array().expect("fixtures") {
        src.push_str(&format!(
            "\n{} not {{ (FAIL {}) = 1 .quit }} if\n",
            source(f),
            f["name"].as_str().expect("name")
        ));
    }
    src.push_str(
        "\n/dictdepth countdictstack def \
         (pop) cvx 1 5 qr--byte-data <403706f700> eq \
         () cvx 1 2 qr--byte-data <4000> eq and \
         countdictstack dictdepth eq and \
         not { (FAIL executable string/frame) = 1 .quit } if \
         { (secret) noaccess 1 19 qr--byte-data } stopped \
         $error /command get /qr-data-must-be-readable eq and \
         countdictstack dictdepth eq and \
         not { (FAIL string access/frame) = 1 .quit } if \
         pop pop pop (QR-REFERENCE-PASS) =\n",
    );
    let out = Command::new("gs")
        .args(["-q", "-dNODISPLAY", "-dBATCH", "-dNOPAUSE", "-"])
        .output_with_input(src.as_bytes())
        .expect("run Ghostscript");
    assert!(
        out.status.success(),
        "Ghostscript reference failure: {} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "QR-REFERENCE-PASS"
    );
}

fn specification_check(f: &serde_json::Value) -> String {
    let mut src = format!(
        "/spec {} /{} qr--block-spec def true",
        f["Version"],
        f["Level"].as_str().expect("level")
    );
    for key in [
        "Version",
        "Size",
        "RawCodewords",
        "RemainderBits",
        "DataCodewords",
        "ECCodewordsPerBlock",
        "Blocks",
    ] {
        src.push_str(&format!(" spec /{key} get {} eq and", f[key]));
    }
    src.push_str(&format!(
        " spec /Level get /{} eq and",
        f["Level"].as_str().expect("level")
    ));
    src
}

fn plan_source(length: u64, level: &str) -> String {
    format!(
        "/payload {length} string def 0 1 {length} 1 sub {{ /i exch def payload i i 256 mod put }} for payload /{level} qr--byte-plan"
    )
}

#[test]
fn all_160_block_specifications_match_independent_reference() {
    let data: serde_json::Value = serde_json::from_str(PLANNING).expect("planning vectors");
    let specs = data["specifications"].as_array().expect("specifications");
    assert_eq!(specs.len(), 160);
    let mut it = interpreter();
    for spec in specs {
        assert_true(
            &mut it,
            &specification_check(spec),
            &format!("specification {spec}"),
        );
    }
}

#[test]
fn automatic_byte_plans_match_independent_selected_versions_and_padded_data() {
    let data: serde_json::Value = serde_json::from_str(PLANNING).expect("planning vectors");
    let plans = data["plans"].as_array().expect("plans");
    assert!(plans.len() > 480, "planning boundary corpus shrank");
    let mut it = interpreter();
    for plan in plans {
        let length = plan["length"].as_u64().expect("length");
        let level = plan["level"].as_str().expect("level");
        let label = format!("length {length}, level {level}");
        let src = plan_source(length, level);
        if plan["version"].is_null() {
            // Planner failures leave two input operands (the packer leaves three).
            assert_true(
                &mut it,
                &format!(
                    "/dictdepth countdictstack def {{ {src} }} stopped \
                 $error /command get /qr-data-too-long-for-level eq and \
                 countdictstack dictdepth eq and 3 1 roll pop pop"
                ),
                &label,
            );
            continue;
        }
        assert_true(
            &mut it,
            &format!(
                "/dictdepth countdictstack def {src} /plan exch def \
             plan /Version get {} eq plan /Level get /{level} eq and \
             plan /DataCodewords get {} eq and \
             plan /Data get length {} eq and \
             countdictstack dictdepth eq and",
                plan["version"], plan["capacity"], plan["capacity"]
            ),
            &label,
        );
        it.run_str("plan /Data get")
            .expect("retrieve planned bytes");
        let hash = digest_hex(&pop_string(&mut it, &label));
        assert_eq!(
            hash,
            plan["data_sha256"].as_str().expect("digest"),
            "{label}: padded bytes differ"
        );
    }
}

fn correction_components(data: &serde_json::Value) -> Vec<(String, String)> {
    let mut cases = Vec::new();
    for f in data["divisors"].as_array().expect("divisors") {
        cases.push((
            format!(
                "{} qr--rs-divisor <{}> eq",
                f["degree"],
                f["hex"].as_str().expect("hex")
            ),
            format!("divisor degree {}", f["degree"]),
        ));
    }
    for f in data["remainders"].as_array().expect("remainders") {
        cases.push((
            format!(
                "<{}> {} qr--rs-divisor qr--rs-remainder <{}> eq",
                f["data"].as_str().expect("data"),
                f["degree"],
                f["hex"].as_str().expect("hex")
            ),
            format!(
                "remainder degree {}, input hex length {}",
                f["degree"],
                f["data"].as_str().expect("data").len()
            ),
        ));
    }
    cases
}

#[test]
fn finite_field_and_reed_solomon_match_independent_reference() {
    let data: serde_json::Value = serde_json::from_str(CORRECTION).expect("correction vectors");
    let mut it = interpreter();
    let mut hash = Sha256::new();
    for a in 0..256 {
        it.run_str(&format!(
            "/row 256 string def 0 1 255 {{ /b exch def row b {a} b qr--gf-mul put }} for row"
        ))
        .expect("GF product row");
        hash.update(pop_string(&mut it, "GF products"));
    }
    let actual: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(
        actual,
        data["gf_products_sha256"].as_str().expect("GF digest")
    );
    assert_eq!(data["divisors"].as_array().expect("divisors").len(), 30);
    assert_eq!(data["remainders"].as_array().expect("remainders").len(), 91);
    for (src, label) in correction_components(&data) {
        assert_true(&mut it, &src, &label);
    }
}

#[test]
fn every_version_level_interleaves_short_and_long_blocks_correctly() {
    let data: serde_json::Value = serde_json::from_str(CORRECTION).expect("correction vectors");
    let cases = data["interleaved"]
        .as_array()
        .expect("interleaving fixtures");
    assert_eq!(cases.len(), 160);
    let mut it = interpreter();
    for f in cases {
        let version = f["version"].as_u64().expect("version");
        let level = f["level"].as_str().expect("level");
        let ordinal = match level {
            "L" => 0,
            "M" => 1,
            "Q" => 2,
            "H" => 3,
            _ => panic!("fixture level"),
        };
        let capacity = f["capacity"].as_u64().expect("capacity");
        it.run_str(&format!(
            "/data {capacity} string def 0 1 {capacity} 1 sub {{ /i exch def \
             data i i 149 mul i 251 idiv add {version} 17 mul add {ordinal} 23 mul add 256 mod put \
             }} for data {version} /{level} qr--interleave"
        ))
        .unwrap_or_else(|e| panic!("v{version}-{level}: {}", it.error_report(&e)));
        let bytes = pop_string(&mut it, &format!("interleave v{version}-{level}"));
        assert_eq!(bytes.len() as u64, f["length"].as_u64().expect("length"));
        assert_eq!(
            digest_hex(&bytes),
            f["sha256"].as_str().expect("digest"),
            "interleave v{version}-{level}"
        );
    }
}

#[test]
fn correction_guards_and_fresh_outputs_preserve_caller_state() {
    let mut it = interpreter();
    for (src, name) in [
        ("7.0 qr--rs-divisor", "qr-ecc-degree-must-be-integer"),
        ("0 qr--rs-divisor", "qr-ecc-degree-must-be-in-1-30"),
        ("31 qr--rs-divisor", "qr-ecc-degree-must-be-in-1-30"),
        ("() [] qr--rs-remainder", "qr-ecc-divisor-must-be-a-string"),
        (
            "() () qr--rs-remainder",
            "qr-ecc-divisor-length-must-be-in-1-30",
        ),
        (
            "() 31 string qr--rs-remainder",
            "qr-ecc-divisor-length-must-be-in-1-30",
        ),
        ("[] <01> qr--rs-remainder", "qr-data-must-be-a-string"),
        ("2957 string <01> qr--rs-remainder", "qr-ecc-data-too-long"),
        (
            "() 1 /L qr--interleave",
            "qr-ecc-data-length-must-match-capacity",
        ),
        (
            "20 string 1 /L qr--interleave",
            "qr-ecc-data-length-must-match-capacity",
        ),
        ("[] 1 /L qr--interleave", "qr-data-must-be-a-string"),
        (
            "19 string 1 /bad qr--interleave",
            "qr-level-must-be-L-M-Q-or-H",
        ),
        (
            "19 string 0 /L qr--interleave",
            "qr-version-must-be-in-1-40",
        ),
    ] {
        assert_true(
            &mut it,
            &format!(
                "/dictdepth countdictstack def mark {{ {src} }} stopped \
             $error /command get /{name} eq and \
             countdictstack dictdepth eq and /guard-ok exch def cleartomark guard-ok"
            ),
            src,
        );
    }
    assert_true(
        &mut it,
        "/dictdepth countdictstack def /result 42 def /root 17 def \
         /a 7 qr--rs-divisor def /b 7 qr--rs-divisor def a 0 0 put b 0 get 127 eq \
         /divisor <7f7a9aa40b4475> readonly def \
         /a () cvx divisor cvx qr--rs-remainder def \
         /b () cvx divisor cvx qr--rs-remainder def \
         a 0 99 put b 0 get 0 eq and divisor <7f7a9aa40b4475> eq and \
         /input 19 string cvx def \
         /a /input load 1 /L cvx qr--interleave def \
         /b /input load 1 /L cvx qr--interleave def a 0 99 put b 0 get 0 eq and \
         /input load xcheck and /input load 0 get 0 eq and \
         result 42 eq root 17 eq and and countdictstack dictdepth eq and",
        "correction output ownership/caller state",
    );
}

fn github_stream_check(f: &serde_json::Value) -> String {
    format!(
        "/plan (https://github.com/PSCatLabs/pscat) /{} qr--byte-plan def \
         plan /Version get {} eq \
         plan /Data get plan /Version get plan /Level get qr--interleave <{}> eq and",
        f["level"].as_str().expect("level"),
        f["version"],
        f["hex"].as_str().expect("codewords")
    )
}

#[test]
fn github_url_plans_pack_and_correct_to_independent_complete_streams() {
    let data: serde_json::Value = serde_json::from_str(CORRECTION).expect("correction vectors");
    let cases = data["github"].as_array().expect("GitHub streams");
    assert_eq!(cases.len(), 4);
    let mut it = interpreter();
    for f in cases {
        assert_true(
            &mut it,
            &github_stream_check(f),
            "GitHub complete codewords",
        );
    }
}

#[test]
fn ghostscript_matches_correction_vectors_and_access_guards() {
    if Command::new("gs")
        .arg("--version")
        .output_deadline()
        .is_err()
    {
        eprintln!("gs unavailable; QR correction portability check not run");
        return;
    }
    let data: serde_json::Value = serde_json::from_str(CORRECTION).expect("correction vectors");
    let mut src = String::from(LIB);
    for f in data["github"].as_array().expect("GitHub streams") {
        src.push_str(&format!(
            "\n{} not {{ (FAIL GitHub complete stream) = 1 .quit }} if\n",
            github_stream_check(f)
        ));
    }
    for (code, label) in correction_components(&data) {
        src.push_str(&format!("\n{code} not {{ (FAIL {label}) = 1 .quit }} if\n"));
    }
    for f in data["interleaved"].as_array().expect("interleaved") {
        if f.get("hex").is_none() {
            continue;
        }
        src.push_str(&format!(
            "\n<{}> {} /{} qr--interleave <{}> eq not {{ (FAIL interleave {} {}) = 1 .quit }} if\n",
            f["data"].as_str().expect("data"),
            f["version"],
            f["level"].as_str().expect("level"),
            f["hex"].as_str().expect("hex"),
            f["version"],
            f["level"].as_str().expect("level")
        ));
    }
    for (code, name) in [
        (
            "() <01> noaccess qr--rs-remainder",
            "qr-ecc-divisor-must-be-readable",
        ),
        (
            "() noaccess <01> qr--rs-remainder",
            "qr-data-must-be-readable",
        ),
        (
            "19 string noaccess 1 /L qr--interleave",
            "qr-data-must-be-readable",
        ),
    ] {
        src.push_str(&format!(
            "\n/dictdepth countdictstack def mark {{ {code} }} stopped \
            $error /command get /{name} eq and countdictstack dictdepth eq and \
            not {{ (FAIL access/frame {name}) = 1 .quit }} if cleartomark\n"
        ));
    }
    src.push_str("\n19 string cvx 1 /L cvx qr--interleave length 26 ne { (FAIL executable bytes) = 1 .quit } if (QR-CORRECTION-PASS) =\n");
    let out = Command::new("gs")
        .args(["-q", "-dNODISPLAY", "-dBATCH", "-dNOPAUSE", "-"])
        .output_with_input(src.as_bytes())
        .expect("run Ghostscript correction");
    assert!(
        out.status.success(),
        "{} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "QR-CORRECTION-PASS"
    );
}

#[test]
fn planning_guards_and_executable_values_preserve_frames_and_ownership() {
    let mut it = interpreter();
    for (src, name) in [
        ("1.0 /M qr--block-spec", "qr-version-must-be-integer"),
        ("false /L qr--block-spec", "qr-version-must-be-integer"),
        ("0 /M qr--block-spec", "qr-version-must-be-in-1-40"),
        ("41 /L qr--block-spec", "qr-version-must-be-in-1-40"),
        ("1 /Z qr--block-spec", "qr-level-must-be-L-M-Q-or-H"),
        ("1 (L) qr--block-spec", "qr-level-must-be-L-M-Q-or-H"),
        ("() /m qr--byte-plan", "qr-level-must-be-L-M-Q-or-H"),
        ("() true qr--byte-plan", "qr-level-must-be-L-M-Q-or-H"),
        ("[] /M qr--byte-plan", "qr-data-must-be-a-string"),
        ("{} /M qr--byte-plan", "qr-data-must-be-a-string"),
        ("2954 string /L qr--byte-plan", "qr-data-too-long-for-level"),
        ("2332 string /M qr--byte-plan", "qr-data-too-long-for-level"),
        ("1664 string /Q qr--byte-plan", "qr-data-too-long-for-level"),
        ("1274 string /H qr--byte-plan", "qr-data-too-long-for-level"),
    ] {
        assert_true(
            &mut it,
            &format!(
                "/dictdepth countdictstack def {{ {src} }} stopped \
             $error /command get /{name} eq and \
             countdictstack dictdepth eq and 3 1 roll pop pop"
            ),
            src,
        );
    }
    assert_true(
        &mut it,
        "/dictdepth countdictstack def /source (pop) cvx def /ecc /L cvx def \
         /first /source load /ecc load qr--byte-plan def \
         /second /source load /ecc load qr--byte-plan def \
         first /Data get 0 0 put first /Version 40 put \
         second /Data get 0 get 64 eq second /Version get 1 eq and \
         /source load xcheck /ecc load xcheck and and \
         /source load cvlit (pop) eq and \
         second /Level get xcheck not and \
         countdictstack dictdepth eq and",
        "planner ownership and executable attributes",
    );
}

#[test]
fn ghostscript_matches_capacity_tables_and_boundary_plan_versions() {
    if Command::new("gs")
        .arg("--version")
        .output_deadline()
        .is_err()
    {
        eprintln!("gs unavailable; QR planning portability check not run");
        return;
    }
    let data: serde_json::Value = serde_json::from_str(PLANNING).expect("planning vectors");
    let mut src = String::from(LIB);
    for spec in data["specifications"].as_array().expect("specifications") {
        src.push_str(&format!(
            "\n{} not {{ (FAIL specification) = 1 .quit }} if\n",
            specification_check(spec)
        ));
    }
    for plan in data["plans"].as_array().expect("plans") {
        if plan["version"].is_null() {
            continue;
        }
        let length = plan["length"].as_u64().expect("length");
        let level = plan["level"].as_str().expect("level");
        src.push_str(&format!(
            "\n{} /Version get {} ne {{ (FAIL {length} {level}) = 1 .quit }} if\n",
            plan_source(length, level),
            plan["version"]
        ));
    }
    src.push_str(
        "\n/dictdepth countdictstack def \
         () cvx /H cvx qr--byte-plan /Level get /H eq \
         countdictstack dictdepth eq and \
         not { (FAIL executable values) = 1 .quit } if \
         { (hidden) noaccess /M qr--byte-plan } stopped \
         $error /command get /qr-data-must-be-readable eq and \
         countdictstack dictdepth eq and \
         not { (FAIL unreadable payload/frame) = 1 .quit } if pop pop \
         (QR-PLANNING-PASS) =\n",
    );
    // Check refusal before harness cleanup, for every level's final capacity.
    for (length, level) in [(2954, "L"), (2332, "M"), (1664, "Q"), (1274, "H")] {
        src.push_str(&format!(
            "\n/dictdepth countdictstack def \
             {{ {length} string /{level} qr--byte-plan }} stopped \
             $error /command get /qr-data-too-long-for-level eq and \
             countdictstack dictdepth eq and \
             not {{ (FAIL overflow {level}) = 1 .quit }} if pop pop\n"
        ));
    }
    let out = Command::new("gs")
        .args(["-q", "-dNODISPLAY", "-dBATCH", "-dNOPAUSE", "-"])
        .output_with_input(src.as_bytes())
        .expect("run Ghostscript planner");
    assert!(
        out.status.success(),
        "{} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "QR-PLANNING-PASS"
    );
}

// ---- Matrix construction (QR-3, #970) ----

fn matrix_vectors() -> serde_json::Value {
    serde_json::from_str(MATRIX).expect("matrix vectors")
}

/// One flag bit of every `/Cells` byte, as a 0/1-per-cell row-major grid.
fn plane(cells: &[u8], bit: u8) -> Vec<u8> {
    cells.iter().map(|c| (c >> bit) & 1).collect()
}

fn unpack_hex(hex: &str, cells: usize) -> Vec<u8> {
    (0..cells)
        .map(|i| {
            let byte = u8::from_str_radix(&hex[i / 8 * 2..i / 8 * 2 + 2], 16).expect("fixture hex");
            (byte >> (7 - i % 8)) & 1
        })
        .collect()
}

/// Compare a grid with a packed fixture, naming the first differing cell.
fn assert_plane(grid: &[u8], hex: &str, size: usize, label: &str) {
    assert_eq!(grid.len(), size * size, "{label}: grid length");
    let expected = unpack_hex(hex, size * size);
    if let Some(i) = (0..grid.len()).find(|&i| grid[i] != expected[i]) {
        panic!(
            "{label}: first mismatch at column {}, row {}: got {}, expected {}",
            i % size,
            i / size,
            grid[i],
            expected[i]
        );
    }
}

fn cells_of(it: &mut Interp, src: &str, label: &str) -> Vec<u8> {
    it.run_str(&format!("{src} /Cells get"))
        .unwrap_or_else(|e| panic!("{label}: {}", it.error_report(&e)));
    pop_string(it, label)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn placement_codewords(version: u64, count: usize) -> Vec<u8> {
    (0..count as u64)
        .map(|i| ((i * 149 + i / 251 + version * 31) % 256) as u8)
        .collect()
}

fn level_ordinal(level: &str) -> u64 {
    match level {
        "L" => 0,
        "M" => 1,
        "Q" => 2,
        "H" => 3,
        _ => panic!("fixture level {level}"),
    }
}

fn symbol_data_source(version: u64, level: &str) -> String {
    let ordinal = level_ordinal(level);
    format!(
        "/spec {version} /{level} qr--block-spec def /capacity spec /DataCodewords get def \
         /data capacity string def 0 1 capacity 1 sub {{ /i exch def \
         data i i 149 mul i 251 idiv add {version} 17 mul add {ordinal} 23 mul add 256 mod put \
         }} for"
    )
}

#[test]
fn alignment_centers_match_independent_reference_for_every_version() {
    let data = matrix_vectors();
    let centers = data["alignment"].as_array().expect("alignment");
    assert_eq!(centers.len(), 40);
    let mut it = interpreter();
    for (index, expected) in centers.iter().enumerate() {
        let version = index + 1;
        let expected = expected.as_array().expect("centers");
        let mut src = format!(
            "/c {version} qr--alignment-centers def c length {} eq",
            expected.len()
        );
        for (k, value) in expected.iter().enumerate() {
            src.push_str(&format!(" c {k} get {value} eq and"));
        }
        assert_true(&mut it, &src, &format!("alignment centers v{version}"));
    }
}

#[test]
fn function_patterns_and_reservations_match_reference_for_every_version() {
    let data = matrix_vectors();
    let layouts = data["layouts"].as_array().expect("layouts");
    assert_eq!(layouts.len(), 40);
    let mut it = interpreter();
    for layout in layouts {
        let version = layout["version"].as_u64().expect("version");
        let size = layout["size"].as_u64().expect("size") as usize;
        let label = format!("function matrix v{version}");
        let cells = cells_of(&mut it, &format!("{version} qr--function-matrix"), &label);
        assert_eq!(cells.len(), size * size, "{label}: cell count");
        assert!(
            cells.iter().all(|c| matches!(c, 0 | 2 | 3 | 6)),
            "{label}: unexpected cell flags"
        );
        for (bit, field) in [(1, "function"), (2, "metadata"), (0, "fixed")] {
            let grid = plane(&cells, bit);
            if let Some(full) = layout[field].as_str() {
                assert_plane(&grid, full, size, &format!("{label} {field}"));
            }
            assert_eq!(
                digest_hex(&grid),
                layout[&format!("{field}_sha256")].as_str().expect("digest"),
                "{label}: {field} grid"
            );
        }
        // Reservation invariant: the free cells are exactly the layout's
        // codeword bits plus remainder bits, at every version.
        let free = cells.iter().filter(|c| *c & 2 == 0).count() as u64;
        assert_eq!(free, layout["data_cells"].as_u64().expect("data cells"));
        assert_true(
            &mut it,
            &format!(
                "/spec {version} /M qr--block-spec def \
                 spec /RawCodewords get 8 mul spec /RemainderBits get add {free} eq"
            ),
            &format!("{label}: free cells match block layout"),
        );
    }
}

#[test]
fn zigzag_placement_matches_reference_before_and_after_every_forced_mask() {
    let data = matrix_vectors();
    let placements = data["placements"].as_array().expect("placements");
    let layouts = data["layouts"].as_array().expect("layouts");
    assert!(
        placements
            .iter()
            .any(|p| p["remainder_bits"].as_u64() == Some(7)),
        "placement corpus lost its remainder-bit versions"
    );
    let versions: Vec<u64> = placements
        .iter()
        .map(|p| p["version"].as_u64().expect("version"))
        .collect();
    for required in [1, 2, 7, 10, 27, 40] {
        assert!(versions.contains(&required), "missing version {required}");
    }
    let mut it = interpreter();
    for p in placements {
        let version = p["version"].as_u64().expect("version");
        let layout = &layouts[version as usize - 1];
        let size = layout["size"].as_u64().expect("size") as usize;
        let count = layout["data_cells"].as_u64().expect("data cells") as usize / 8;
        let codewords = placement_codewords(version, count);
        assert_eq!(
            digest_hex(&codewords),
            p["codewords_sha256"].as_str().expect("digest"),
            "v{version}: placement codeword formula drifted from the fixture"
        );
        let label = format!("placement v{version}");
        it.run_str(&format!(
            "/m <{}> {version} qr--matrix def",
            hex(&codewords)
        ))
        .unwrap_or_else(|e| panic!("{label}: {}", it.error_report(&e)));
        let unmasked = cells_of(&mut it, "m", &label);
        assert_plane(
            &plane(&unmasked, 0),
            p["unmasked"].as_str().expect("unmasked"),
            size,
            &format!("{label} unmasked"),
        );
        let shown = p["shown_mask"].as_u64().expect("shown mask");
        for (mask, digest) in p["mask_sha256"]
            .as_array()
            .expect("mask digests")
            .iter()
            .enumerate()
        {
            let masked = cells_of(&mut it, &format!("m {mask} qr--apply-mask"), &label);
            let grid = plane(&masked, 0);
            if mask as u64 == shown {
                assert_plane(
                    &grid,
                    p["masked"].as_str().expect("masked"),
                    size,
                    &format!("{label} mask {mask}"),
                );
            }
            assert_eq!(
                digest_hex(&grid),
                digest.as_str().expect("digest"),
                "{label} mask {mask}"
            );
            assert_eq!(
                plane(&masked, 1),
                plane(&unmasked, 1),
                "{label} mask {mask}: masking changed the function flags"
            );
        }
        assert_eq!(
            cells_of(&mut it, "m", &label),
            unmasked,
            "{label}: masking mutated its input"
        );
        assert_true(&mut it, "m /Mask get null eq", &label);
    }
}

#[test]
fn padded_data_corrects_places_and_masks_like_the_reference_symbol() {
    let data = matrix_vectors();
    let symbols = data["symbols"].as_array().expect("symbols");
    assert!(
        symbols
            .iter()
            .any(|s| s["unequal_blocks"].as_bool() == Some(true)),
        "symbol corpus lost its unequal-block cases"
    );
    let layouts = data["layouts"].as_array().expect("layouts");
    let mut it = interpreter();
    for s in symbols {
        let version = s["version"].as_u64().expect("version");
        let level = s["level"].as_str().expect("level");
        let mask = s["mask"].as_u64().expect("mask");
        let size = layouts[version as usize - 1]["size"]
            .as_u64()
            .expect("size") as usize;
        let label = format!("symbol v{version}-{level} mask {mask}");
        it.run_str(&format!(
            "{} /codewords data {version} /{level} qr--interleave def codewords",
            symbol_data_source(version, level)
        ))
        .unwrap_or_else(|e| panic!("{label}: {}", it.error_report(&e)));
        let codewords = pop_string(&mut it, &label);
        assert_eq!(
            digest_hex(&codewords),
            s["codewords_sha256"].as_str().expect("digest"),
            "{label}: codewords"
        );
        let cells = cells_of(
            &mut it,
            &format!("codewords {version} qr--matrix {mask} qr--apply-mask"),
            &label,
        );
        let grid = plane(&cells, 0);
        if let Some(full) = s["masked"].as_str() {
            assert_plane(&grid, full, size, &label);
        }
        assert_eq!(
            digest_hex(&grid),
            s["masked_sha256"].as_str().expect("digest"),
            "{label}"
        );
    }
}

fn github_matrix_source(f: &serde_json::Value) -> String {
    format!(
        "/plan (https://github.com/PSCatLabs/pscat) /{} qr--byte-plan def \
         plan /Data get plan /Version get plan /Level get qr--interleave \
         plan /Version get qr--matrix {} qr--apply-mask",
        f["level"].as_str().expect("level"),
        f["mask"]
    )
}

#[test]
fn github_url_matrices_match_reference_at_every_level() {
    let data = matrix_vectors();
    let cases = data["github"].as_array().expect("GitHub matrices");
    assert_eq!(cases.len(), 4);
    let mut it = interpreter();
    for f in cases {
        let version = f["version"].as_u64().expect("version") as usize;
        let label = format!("GitHub {} matrix", f["level"]);
        let cells = cells_of(&mut it, &github_matrix_source(f), &label);
        assert_plane(
            &plane(&cells, 0),
            f["masked"].as_str().expect("masked"),
            version * 4 + 17,
            &label,
        );
    }
}

#[test]
fn matrix_guards_preserve_frames_and_outputs_are_fresh() {
    let mut it = interpreter();
    for (src, name) in [
        ("0 qr--alignment-centers", "qr-version-must-be-in-1-40"),
        ("41 qr--function-matrix", "qr-version-must-be-in-1-40"),
        ("(1) qr--function-matrix", "qr-version-must-be-integer"),
        ("26 string 1.0 qr--matrix", "qr-version-must-be-integer"),
        ("26 string 41 qr--matrix", "qr-version-must-be-in-1-40"),
        ("[] 1 qr--matrix", "qr-codewords-must-be-a-string"),
        (
            "25 string 1 qr--matrix",
            "qr-codewords-length-must-match-version",
        ),
        (
            "43 string 2 qr--matrix",
            "qr-codewords-length-must-match-version",
        ),
        (
            "1 qr--function-matrix 8 qr--apply-mask",
            "qr-mask-must-be-in-0-7",
        ),
        (
            "1 qr--function-matrix -1 qr--apply-mask",
            "qr-mask-must-be-in-0-7",
        ),
        (
            "1 qr--function-matrix /a qr--apply-mask",
            "qr-mask-must-be-integer",
        ),
        ("[] 0 qr--apply-mask", "qr-matrix-must-be-a-matrix"),
        (
            "<< /Version 1 /Size 21 /Mask null >> 0 qr--apply-mask",
            "qr-matrix-must-be-a-matrix",
        ),
        (
            "<< /Version 1 /Size 21 /Cells 440 string /Mask null >> 0 qr--apply-mask",
            "qr-matrix-must-be-a-matrix",
        ),
        (
            "<< /Version 2 /Size 21 /Cells 441 string /Mask null >> 0 qr--apply-mask",
            "qr-matrix-must-be-a-matrix",
        ),
        (
            "1 qr--function-matrix 3 qr--apply-mask 3 qr--apply-mask",
            "qr-matrix-already-masked",
        ),
    ] {
        assert_true(
            &mut it,
            &format!(
                "/dictdepth countdictstack def mark {{ {src} }} stopped \
                 $error /command get /{name} eq and \
                 countdictstack dictdepth eq and /guard-ok exch def cleartomark guard-ok"
            ),
            src,
        );
    }
    // Caller bindings with the private frame's names cannot leak in, an
    // executable stream is placed as data, and every result is fresh.
    assert_true(
        &mut it,
        "/dictdepth countdictstack def \
         /cells (caller) def /size 3 def /x 9 def /y 9 def /test { 1 } def /cell 0 def \
         /stream 26 string cvx def /stream load 0 255 put \
         /a /stream load 1 qr--matrix def /b /stream load 1 qr--matrix def \
         a /Cells get 440 0 put b /Cells get 440 get 1 eq \
         /c 1 qr--function-matrix def /d 1 qr--function-matrix def \
         c /Cells get 0 0 put d /Cells get 0 get 3 eq and \
         /e 1 qr--alignment-centers def e length 0 eq and \
         /f 2 qr--alignment-centers def f 0 99 put 2 qr--alignment-centers 0 get 6 eq and \
         /stream load xcheck and /stream load 0 get 255 eq and \
         cells (caller) eq size 3 eq and x 9 eq and y 9 eq and /cell load 0 eq and and \
         countdictstack dictdepth eq and",
        "matrix ownership and caller state",
    );
    assert!(
        it.gfx().pixmap.pixels().iter().all(|p| p.red() == 255),
        "matrix stage drew on the page"
    );
    assert!(!it.gfx().page_shown);
}

#[test]
fn ghostscript_builds_the_same_matrices_and_guards() {
    if Command::new("gs")
        .arg("--version")
        .output_deadline()
        .is_err()
    {
        eprintln!("gs unavailable; QR matrix portability check not run");
        return;
    }
    let data = matrix_vectors();
    let layouts = data["layouts"].as_array().expect("layouts");
    let mut src = String::from(LIB);
    src.push_str("\n/out (%stdout) (w) file def /emit { /Cells get out exch writehexstring out (\\n) writestring } def\n");
    // Each emitted line pairs with its expected packed grid, in order.
    let mut expected: Vec<(String, usize, String)> = Vec::new();
    for p in data["placements"].as_array().expect("placements") {
        let version = p["version"].as_u64().expect("version");
        if ![1, 2, 7, 27].contains(&version) {
            continue;
        }
        let layout = &layouts[version as usize - 1];
        let size = layout["size"].as_u64().expect("size") as usize;
        let count = layout["data_cells"].as_u64().expect("data cells") as usize / 8;
        src.push_str(&format!(
            "\n/m <{}> {version} qr--matrix def m emit m {} qr--apply-mask emit\n",
            hex(&placement_codewords(version, count)),
            p["shown_mask"]
        ));
        let label = format!("gs placement v{version}");
        expected.push((
            label.clone(),
            size,
            p["unmasked"].as_str().expect("u").into(),
        ));
        expected.push((label, size, p["masked"].as_str().expect("m").into()));
    }
    for s in data["symbols"].as_array().expect("symbols") {
        if let Some(full) = s["masked"].as_str() {
            let version = s["version"].as_u64().expect("version");
            let level = s["level"].as_str().expect("level");
            src.push_str(&format!(
                "\n{} data {version} /{level} qr--interleave {version} qr--matrix {} qr--apply-mask emit\n",
                symbol_data_source(version, level),
                s["mask"]
            ));
            expected.push((
                format!("gs symbol v{version}-{level}"),
                version as usize * 4 + 17,
                full.into(),
            ));
        }
    }
    for f in data["github"].as_array().expect("GitHub") {
        src.push_str(&format!("\n{} emit\n", github_matrix_source(f)));
        expected.push((
            format!("gs GitHub {}", f["level"]),
            f["version"].as_u64().expect("version") as usize * 4 + 17,
            f["masked"].as_str().expect("masked").into(),
        ));
    }
    for (code, name) in [
        (
            "26 string noaccess 1 qr--matrix",
            "qr-codewords-must-be-readable",
        ),
        (
            "1 qr--function-matrix 0 qr--apply-mask 0 qr--apply-mask",
            "qr-matrix-already-masked",
        ),
        (
            "1 qr--function-matrix 9 qr--apply-mask",
            "qr-mask-must-be-in-0-7",
        ),
    ] {
        src.push_str(&format!(
            "\n/dictdepth countdictstack def mark {{ {code} }} stopped \
             $error /command get /{name} eq and countdictstack dictdepth eq and \
             not {{ (FAIL guard/frame {name}) = 1 .quit }} if cleartomark\n"
        ));
    }
    src.push_str("\n(QR-MATRIX-PASS) =\n");
    let out = Command::new("gs")
        .args(["-q", "-dNODISPLAY", "-dBATCH", "-dNOPAUSE", "-"])
        .output_with_input(src.as_bytes())
        .expect("run Ghostscript matrix");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout} {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), expected.len() + 1, "{stdout}");
    assert_eq!(lines[expected.len()], "QR-MATRIX-PASS");
    for (line, (label, size, full)) in lines.iter().zip(&expected) {
        let cells: Vec<u8> = (0..line.len() / 2)
            .map(|i| u8::from_str_radix(&line[2 * i..2 * i + 2], 16).expect("gs hex"))
            .collect();
        assert_plane(&plane(&cells, 0), full, *size, label);
    }
}

// ---- Metadata, penalties, mask selection and decoding (QR-4, #971) ----

fn mask_vectors() -> serde_json::Value {
    serde_json::from_str(MASKS).expect("mask vectors")
}

fn penalty_list(f: &serde_json::Value) -> String {
    let scores: Vec<String> = f["penalties"]
        .as_array()
        .expect("penalties")
        .iter()
        .map(|p| p.to_string())
        .collect();
    format!("[{}]", scores.join(" "))
}

/// PostScript comparing an 8-score array on the stack with the fixture's.
fn penalties_equal(f: &serde_json::Value) -> String {
    format!(
        "/got exch def /want {} def true 0 1 7 {{ /i exch def got i get want i get eq and }} for",
        penalty_list(f)
    )
}

/// A bare matrix dictionary over a fixture grid, for penalty scoring.
fn grid_matrix(f: &serde_json::Value) -> String {
    let size = f["size"].as_u64().expect("size") as usize;
    let cells = unpack_hex(f["grid"].as_str().expect("grid"), size * size);
    format!(
        "<< /Version {} /Size {size} /Cells <{}> /Mask null >>",
        (size - 17) / 4,
        hex(&cells)
    )
}

/// Check a finished symbol's fields, its metadata flags and its whole grid.
fn assert_symbol(it: &mut Interp, f: &serde_json::Value, level: &str, label: &str) {
    let version = f["version"].as_u64().expect("version");
    let size = version as usize * 4 + 17;
    assert_true(
        it,
        &format!(
            "sym /Version get {version} eq sym /Size get {size} eq and \
             sym /Level get /{level} eq and sym /Mask get {} eq and",
            f["mask"]
        ),
        &format!("{label}: version/level/mask"),
    );
    let cells = cells_of(it, "sym", label);
    assert!(
        cells.iter().all(|c| matches!(c, 0 | 1 | 2 | 3 | 6 | 7)),
        "{label}: unexpected cell flags"
    );
    assert_plane(
        &plane(&cells, 0),
        f["final"].as_str().expect("final"),
        size,
        label,
    );
}

#[test]
fn format_and_version_words_match_the_reference_bch_codes() {
    let data = mask_vectors();
    let formats = data["formats"].as_array().expect("formats");
    let versions = data["versions"].as_array().expect("versions");
    assert_eq!((formats.len(), versions.len()), (32, 34));
    let mut it = interpreter();
    for f in formats {
        assert_true(
            &mut it,
            &format!(
                "/{} {} qr--format-bits {} eq",
                f["level"].as_str().expect("level"),
                f["mask"],
                f["bits"]
            ),
            &format!("format {f}"),
        );
    }
    for f in versions {
        assert_true(
            &mut it,
            &format!("{} qr--version-bits {} eq", f["version"], f["bits"]),
            &format!("version {f}"),
        );
    }
}

#[test]
fn penalty_rules_match_reference_on_edge_case_grids() {
    let data = mask_vectors();
    let grids = data["penalties"].as_array().expect("penalty grids");
    let names: Vec<&str> = grids
        .iter()
        .map(|g| g["name"].as_str().expect("name"))
        .collect();
    for required in [
        "finder-left-edge",
        "finder-right-edge",
        "finder-four-light-both-sides",
        "finder-three-light-both-sides",
        "finder-vertical-top-edge",
        "runs-five-six-seven",
        "two-by-two-blocks",
        "balance-198-dark",
        "balance-243-dark",
    ] {
        assert!(names.contains(&required), "penalty corpus lost {required}");
    }
    let mut it = interpreter();
    for g in grids {
        it.run_str(&format!("{} qr--penalty", grid_matrix(g)))
            .unwrap_or_else(|e| panic!("{}: {}", g["name"], it.error_report(&e)));
        let got = it.operand_stack()[0].repr();
        it.run_str("pop").expect("pop score");
        assert_eq!(got, g["penalty"].to_string(), "penalty {}", g["name"]);
    }
}

#[test]
fn automatic_masks_match_reference_grids_scores_and_metadata() {
    let data = mask_vectors();
    let symbols = data["symbols"].as_array().expect("symbols");
    assert_eq!(symbols.len(), 24);
    let mut it = interpreter();
    for f in symbols {
        let version = f["version"].as_u64().expect("version");
        let level = f["level"].as_str().expect("level");
        let label = format!("auto v{version}-{level}");
        it.run_str(&format!(
            "{} /m data {version} /{level} qr--interleave {version} qr--matrix def \
             /sym data {version} /{level} qr--codeword-symbol def",
            symbol_data_source(version, level)
        ))
        .unwrap_or_else(|e| panic!("{label}: {}", it.error_report(&e)));
        assert_true(
            &mut it,
            &format!("m /{level} qr--mask-penalties {}", penalties_equal(f)),
            &format!("{label}: per-mask penalties"),
        );
        assert_symbol(&mut it, f, level, &label);
    }
}

#[test]
fn github_url_symbols_and_a_real_tie_match_reference_selection() {
    let data = mask_vectors();
    let mut it = interpreter();
    let github = data["github"].as_array().expect("GitHub symbols");
    assert_eq!(github.len(), 4);
    let tie = &data["tie"];
    let scores: Vec<u64> = tie["penalties"]
        .as_array()
        .expect("tie scores")
        .iter()
        .map(|p| p.as_u64().expect("score"))
        .collect();
    let best = *scores.iter().min().expect("scores");
    assert!(
        scores.iter().filter(|&&s| s == best).count() > 1 && scores[0] != best,
        "tie fixture is not a tie away from mask 0"
    );
    let cases: Vec<(String, &serde_json::Value)> = github
        .iter()
        .map(|f| (hex(GITHUB_URL.as_bytes()), f))
        .chain([(tie["payload"].as_str().expect("payload").to_string(), tie)])
        .collect();
    for (payload, f) in cases {
        let level = f["level"].as_str().expect("level");
        let label = format!("<{payload}> at {level}");
        it.run_str(&format!(
            "/plan <{payload}> /{level} qr--byte-plan def \
             /m plan /Data get plan /Version get /{level} qr--interleave plan /Version get qr--matrix def \
             /sym <{payload}> /{level} qr--symbol def"
        ))
        .unwrap_or_else(|e| panic!("{label}: {}", it.error_report(&e)));
        assert_true(
            &mut it,
            &format!("m /{level} qr--mask-penalties {}", penalties_equal(f)),
            &format!("{label}: per-mask penalties"),
        );
        assert_symbol(&mut it, f, level, &label);
    }
}

#[test]
fn metadata_and_selection_guards_preserve_frames_and_inputs() {
    let mut it = interpreter();
    for (src, name) in [
        ("/M 8 qr--format-bits", "qr-mask-must-be-in-0-7"),
        ("/M -1 qr--format-bits", "qr-mask-must-be-in-0-7"),
        ("/M (0) qr--format-bits", "qr-mask-must-be-integer"),
        ("/m 0 qr--format-bits", "qr-level-must-be-L-M-Q-or-H"),
        ("6 qr--version-bits", "qr-version-bits-need-version-7-40"),
        ("41 qr--version-bits", "qr-version-must-be-in-1-40"),
        ("7.0 qr--version-bits", "qr-version-must-be-integer"),
        (
            "1 qr--function-matrix /M qr--draw-metadata",
            "qr-matrix-must-be-masked",
        ),
        (
            "<< /Version 1 /Size 21 /Cells 441 string /Mask 8 >> /M qr--draw-metadata",
            "qr-matrix-must-be-a-matrix",
        ),
        (
            "1 qr--function-matrix 0 qr--apply-mask /X qr--draw-metadata",
            "qr-level-must-be-L-M-Q-or-H",
        ),
        ("[] qr--penalty", "qr-matrix-must-be-a-matrix"),
        (
            "1 qr--function-matrix 2 qr--apply-mask /M qr--mask-penalties",
            "qr-matrix-already-masked",
        ),
        (
            "[1 2 3] qr--best-mask",
            "qr-penalties-must-be-eight-integers",
        ),
        (
            "[1 2 3 4 5 6 7 (8)] qr--best-mask",
            "qr-penalties-must-be-eight-integers",
        ),
        (
            "1 qr--function-matrix 0 qr--apply-mask /M qr--finish",
            "qr-matrix-already-masked",
        ),
        (
            "19 string 1 /W qr--codeword-symbol",
            "qr-level-must-be-L-M-Q-or-H",
        ),
        (
            "18 string 1 /L qr--codeword-symbol",
            "qr-ecc-data-length-must-match-capacity",
        ),
        ("3000 string /L qr--symbol", "qr-data-too-long-for-level"),
    ] {
        assert_true(
            &mut it,
            &format!(
                "/dictdepth countdictstack def mark {{ {src} }} stopped \
                 $error /command get /{name} eq and \
                 countdictstack dictdepth eq and /guard-ok exch def cleartomark guard-ok"
            ),
            src,
        );
    }
    // Inputs stay unchanged, results are fresh, caller names cannot leak in,
    // and an executable payload is encoded as data.
    assert_true(
        &mut it,
        "/dictdepth countdictstack def \
         /tone 9 def /len 9 def /score 9 def /bits 9 def /level /H def /push { 1 } def \
         /m0 26 string 1 qr--matrix def /before m0 /Cells get dup length string copy def \
         /masked m0 3 qr--apply-mask def /mbefore masked /Cells get dup length string copy def \
         /a masked /L qr--draw-metadata def /b masked /L qr--draw-metadata def \
         a /Cells get 8 0 put b /Cells get 8 get 6 ge \
         masked /Cells get mbefore eq and masked /Level known not and \
         /s1 m0 /M qr--finish def /s2 m0 /M qr--finish def \
         s1 /Cells get s2 /Cells get eq and \
         m0 /Cells get before eq and m0 /Mask get null eq and \
         a /Mask get 3 eq and a /Level get /L eq and \
         /x1 (pop) cvx /M qr--symbol def x1 /Version get 1 eq and \
         tone 9 eq len 9 eq and score 9 eq and bits 9 eq and level /H eq and /push load exec 1 eq and and \
         countdictstack dictdepth eq and",
        "metadata/selection ownership and caller state",
    );
    assert!(
        it.gfx().pixmap.pixels().iter().all(|p| p.red() == 255),
        "selection stage drew on the page"
    );
    assert!(!it.gfx().page_shown);
}

#[test]
fn ghostscript_selects_the_same_masks_scores_and_grids() {
    if Command::new("gs")
        .arg("--version")
        .output_deadline()
        .is_err()
    {
        eprintln!("gs unavailable; QR mask-selection portability check not run");
        return;
    }
    let data = mask_vectors();
    let mut src = String::from(LIB);
    src.push_str("\n/out (%stdout) (w) file def /emit { /Cells get out exch writehexstring out (\\n) writestring } def\n");
    for f in data["formats"].as_array().expect("formats") {
        src.push_str(&format!(
            "\n/{} {} qr--format-bits {} ne {{ (FAIL format) = 1 .quit }} if\n",
            f["level"].as_str().expect("level"),
            f["mask"],
            f["bits"]
        ));
    }
    for f in data["versions"].as_array().expect("versions") {
        src.push_str(&format!(
            "\n{} qr--version-bits {} ne {{ (FAIL version {}) = 1 .quit }} if\n",
            f["version"], f["bits"], f["version"]
        ));
    }
    for g in data["penalties"].as_array().expect("penalties") {
        src.push_str(&format!(
            "\n{} qr--penalty {} ne {{ (FAIL penalty {}) = 1 .quit }} if\n",
            grid_matrix(g),
            g["penalty"],
            g["name"].as_str().expect("name")
        ));
    }
    let mut expected: Vec<(String, usize, String)> = Vec::new();
    for f in data["symbols"].as_array().expect("symbols") {
        let version = f["version"].as_u64().expect("version");
        let level = f["level"].as_str().expect("level");
        if ![1, 7, 27].contains(&version) {
            continue;
        }
        src.push_str(&format!(
            "\n{} data {version} /{level} qr--interleave {version} qr--matrix /{level} qr--mask-penalties \
             {} not {{ (FAIL penalties v{version} {level}) = 1 .quit }} if \
             data {version} /{level} qr--codeword-symbol emit\n",
            symbol_data_source(version, level),
            penalties_equal(f)
        ));
        expected.push((
            format!("gs auto v{version}-{level}"),
            version as usize * 4 + 17,
            f["final"].as_str().expect("final").into(),
        ));
    }
    let tie = &data["tie"];
    let github_hex = hex(GITHUB_URL.as_bytes());
    for (payload, f) in data["github"]
        .as_array()
        .expect("GitHub")
        .iter()
        .map(|f| (github_hex.as_str(), f))
        .chain([(tie["payload"].as_str().expect("payload"), tie)])
    {
        src.push_str(&format!(
            "\n<{payload}> /{} qr--symbol dup /Mask get {} ne {{ (FAIL mask) = 1 .quit }} if emit\n",
            f["level"].as_str().expect("level"),
            f["mask"]
        ));
        expected.push((
            format!("gs <{payload}> {}", f["level"]),
            f["version"].as_u64().expect("version") as usize * 4 + 17,
            f["final"].as_str().expect("final").into(),
        ));
    }
    for (code, name) in [
        (
            "1 qr--function-matrix /M qr--draw-metadata",
            "qr-matrix-must-be-masked",
        ),
        ("(x) noaccess /M qr--symbol", "qr-data-must-be-readable"),
        (
            "[0 0 0 0 0 0 0 0.5] qr--best-mask",
            "qr-penalties-must-be-eight-integers",
        ),
    ] {
        src.push_str(&format!(
            "\n/dictdepth countdictstack def mark {{ {code} }} stopped \
             $error /command get /{name} eq and countdictstack dictdepth eq and \
             not {{ (FAIL guard/frame {name}) = 1 .quit }} if cleartomark\n"
        ));
    }
    src.push_str("\n(QR-MASK-PASS) =\n");
    let out = Command::new("gs")
        .args(["-q", "-dNODISPLAY", "-dBATCH", "-dNOPAUSE", "-"])
        .output_with_input(src.as_bytes())
        .expect("run Ghostscript mask selection");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout} {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), expected.len() + 1, "{stdout}");
    assert_eq!(lines[expected.len()], "QR-MASK-PASS");
    for (line, (label, size, full)) in lines.iter().zip(&expected) {
        let cells: Vec<u8> = (0..line.len() / 2)
            .map(|i| u8::from_str_radix(&line[2 * i..2 * i + 2], 16).expect("gs hex"))
            .collect();
        assert_plane(&plane(&cells, 0), full, *size, label);
    }
}

/// Pixels per module in the decode proof: whole device pixels at 72 dpi, so
/// adjacent modules meet without anti-aliased seams.
const MODULE_PX: usize = 4;

/// Render `payload` at `level` through the pscat CLI's PNG path: an opaque
/// white page, a four-module quiet zone, one filled square per dark module.
fn render_symbol_png(payload: &str, level: &str, size: usize) -> tiny_skia::Pixmap {
    let side = (size + 8) * MODULE_PX;
    let program = format!(
        "(lib/qr.ps) run\n\
         /sym ({payload}) /{level} qr--symbol def\n\
         /n sym /Size get def /c sym /Cells get def /k {MODULE_PX} def\n\
         n {size} ne {{ (unexpected symbol size) = quit }} if\n\
         1 setgray clippath fill 0 setgray\n\
         0 1 n 1 sub {{ /y exch def 0 1 n 1 sub {{ /x exch def\n\
           c y n mul x add get 1 and 1 eq {{ x 4 add k mul n 3 add y sub k mul k k rectfill }} if\n\
         }} for }} for\n"
    );
    let dir = common::scratch_dir(&format!("qr-decode-{level}"));
    let ps = dir.join("symbol.ps");
    let png = dir.join("symbol.png");
    std::fs::write(&ps, program).expect("write symbol program");
    let out = Command::new(env!("CARGO_BIN_EXE_pscat"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "--page",
            &format!("{side}x{side}"),
            "--dpi",
            "72",
            "--png",
            png.to_str().expect("utf-8 path"),
            ps.to_str().expect("utf-8 path"),
        ])
        .output_deadline()
        .expect("run pscat");
    assert!(
        out.status.success(),
        "pscat render failed: {} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let pixmap = tiny_skia::Pixmap::load_png(&png).expect("rendered PNG");
    assert_eq!(
        (pixmap.width() as usize, pixmap.height() as usize),
        (side, side)
    );
    pixmap
}

#[test]
fn rendered_github_symbols_decode_independently_at_every_level() {
    // The decoder is a compiled-in dev-dependency: this test has no skip
    // path, so a missing or failing decoder fails the suite (QR_CODE_TESTING.md).
    let data = mask_vectors();
    for f in data["github"].as_array().expect("GitHub symbols") {
        let level = f["level"].as_str().expect("level");
        let version = f["version"].as_u64().expect("version") as usize;
        let size = version * 4 + 17;
        let pixmap = render_symbol_png(GITHUB_URL, level, size);
        let width = pixmap.width() as usize;
        // Composite onto white: a transparent pixel must read as light.
        let grey: Vec<u8> = pixmap
            .pixels()
            .iter()
            .map(|p| {
                let c = p.demultiply();
                let a = u32::from(c.alpha());
                let luma = (u32::from(c.red()) * 299
                    + u32::from(c.green()) * 587
                    + u32::from(c.blue()) * 114)
                    / 1000;
                ((luma * a + 255 * (255 - a)) / 255) as u8
            })
            .collect();
        // The quiet zone really is light and the finder corner really is dark.
        let q = 4 * MODULE_PX;
        assert!(
            grey[0] > 200 && grey[(q - 1) * width + q - 1] > 200,
            "{level}: quiet zone"
        );
        assert!(grey[(q + 1) * width + q + 1] < 60, "{level}: finder corner");
        let mut image =
            rqrr::PreparedImage::prepare_from_greyscale(width, grey.len() / width, |x, y| {
                grey[y * width + x]
            });
        let grids = image.detect_grids();
        assert_eq!(grids.len(), 1, "{level}: expected exactly one symbol");
        let (meta, content) = grids[0]
            .decode()
            .unwrap_or_else(|e| panic!("{level}: decode failed: {e:?}"));
        assert_eq!(content, GITHUB_URL, "{level}: decoded text");
        assert_eq!(meta.version.0, version, "{level}: decoded version");
        assert_eq!(
            u64::from(meta.mask),
            f["mask"].as_u64().expect("mask"),
            "{level}: decoded mask"
        );
        let format_level = match level {
            "L" => 1,
            "M" => 0,
            "Q" => 3,
            "H" => 2,
            _ => unreachable!(),
        };
        assert_eq!(meta.ecc_level, format_level, "{level}: decoded ECC level");
    }
}
