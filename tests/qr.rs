//! Data-stage QR regression tests. Frozen fixtures come from an independent,
//! pinned encoder, not a second implementation of our packing algorithm.

mod common;
use common::CommandDeadline as _;
use pscat::Interp;
use std::process::Command;

const LIB: &str = include_str!("../lib/qr.ps");
const VECTORS: &str = include_str!("data/qr_byte_vectors.json");

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
