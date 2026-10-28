//! Data-stage QR regression tests. Frozen fixtures come from an independent,
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
