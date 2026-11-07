//! `pdfmark` (#1032): `/DOCINFO`, `/OBJ` + `/PUT` + `/EMBED`, and the no-op
//! on every other device. Ghostscript 10's pdfwrite is the reference for
//! precedence and errors (`notes/1032-pdfmark.md`); the wasm build must
//! write the same bytes as the CLI (`wasm_pdfmark_document_matches_the_cli`).

use std::process::Command;

mod common;

use common::{CommandDeadline as _, pdf_text};
use pscat::Interp;

/// The PDF `src` produces, with the DSC title/author fed in the way the
/// CLI and the wasm build do it.
fn pdf_of(src: &str) -> Vec<u8> {
    let mut it = Interp::with_page(200, 200).expect("page");
    it.gfx_mut().enable_pdf();
    let (title, author) = pscat::pdf::scan_document_info(src.as_bytes());
    it.gfx_mut().set_pdf_info(title, author);
    it.run_source(src.as_bytes())
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    it.gfx().pdf_document().expect("recording on")
}

/// What `src` leaves printed, run without PDF recording (a PNG run).
fn eval(src: &str, pdf: bool) -> String {
    let mut it = Interp::with_page(50, 50).expect("page");
    if pdf {
        it.gfx_mut().enable_pdf();
    }
    it.capture_stdout();
    it.run_source(src.as_bytes())
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    String::from_utf8_lossy(it.captured_stdout()).into_owned()
}

fn info_dict(pdf: &[u8]) -> String {
    let text = String::from_utf8_lossy(pdf).into_owned();
    let at = text.find("<< /Producer").expect("an /Info dict");
    let end = text[at..].find(">>\nendobj").expect("end of /Info") + at;
    text[at..end + 2].to_string()
}

const DSC: &str = "%!PS-Adobe-3.0\n%%Title: DSC Title\n%%For: DSC Author\n%%EndComments\n";

#[test]
fn docinfo_mark_wins_over_dsc_per_key_and_producer_stays_pscat() {
    let src = format!(
        "{DSC}[ /Subject (X:1 abc) /Title (Mark Title) /Keywords (k) /Producer (Mine) /DOCINFO pdfmark\n\
         [ /Keywords (k2) /CustomKey 42 /DOCINFO pdfmark showpage\n"
    );
    assert_eq!(
        info_dict(&pdf_of(&src)),
        "<< /Producer (pscat) /Title (Mark Title) /Author (DSC Author) \
         /Subject (X:1 abc) /Keywords (k2) /CustomKey 42 >>"
    );
}

#[test]
fn docinfo_text_strings_utf8_becomes_utf16_and_bom_passes_through() {
    let src = "[ /Title (caf\\303\\251) /Author (\\376\\377\\000A) /Subject (\\351t\\351) \
               /Keywords (a\\(b\\)) /DOCINFO pdfmark showpage";
    assert_eq!(
        info_dict(&pdf_of(src)),
        "<< /Producer (pscat) /Title <FEFF00630061006600E9> /Author <FEFF0041> \
         /Subject (\\351t\\351) /Keywords (a\\(b\\)) >>"
    );
}

#[test]
fn a_document_without_pdfmark_is_unchanged() {
    // The pre-#1032 layout: plain Catalog, DSC /Info, nothing after.
    let pdf = pdf_of(&format!("{DSC}0 0 10 10 rectfill showpage"));
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains("1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj"));
    assert_eq!(
        info_dict(&pdf),
        "<< /Producer (pscat) /Title (DSC Title) /Author (DSC Author) >>"
    );
}

/// The issue's own program: an ABC tune attached as `tune.abc`.
const EMBED_DOC: &str = "%!PS
[ /Title (Test) /Subject (X:1 abc) /Keywords (k) /DOCINFO pdfmark
[ /_objdef {f1} /type /stream /OBJ pdfmark
[ {f1} (X:1\\nK:C\\nCDE|) /PUT pdfmark
[ /Name (tune.abc) /FS << /Type /Filespec /F (tune.abc) /EF << /F {f1} >> >> /EMBED pdfmark
/Helvetica findfont 20 scalefont setfont 72 100 moveto (hi) show showpage
";

#[test]
fn embed_attaches_the_stream_under_its_name() {
    let pdf = pdf_of(EMBED_DOC);
    let text = pdf_text(&pdf);
    // Two pages-tree objects, one page + contents, then the named stream (5).
    assert!(
        text.contains(
            "<< /Type /Catalog /Pages 2 0 R /Names << /EmbeddedFiles << /Names \
             [ (tune.abc) << /EF << /F 5 0 R >> /F (tune.abc) /Type /Filespec >> ] >> >> >>"
        ),
        "{text}"
    );
    assert!(
        text.contains("5 0 obj\n<< /Filter /FlateDecode /Length ")
            && text.contains("stream\nX:1\nK:C\nCDE|\nendstream"),
        "{text}"
    );
}

#[test]
fn put_appends_strings_dicts_and_file_data_to_a_stream() {
    // currentfile through a decoding filter: inline data that needs no
    // file system (the wasm build has none). A `/Filter` the program gives
    // means the data is already encoded, so it is written as it is.
    let src = "[ /_objdef {s} /type /stream /OBJ pdfmark
[ {s} (one,) /PUT pdfmark
[ {s} currentfile /ASCIIHexDecode filter /PUT pdfmark
74776F2C>
[ {s} << /Subtype (text/plain) cvn /Length 999 >> /PUT pdfmark
[ {s} (three) /PUT pdfmark
[ /_objdef {raw} /type /stream /OBJ pdfmark
[ {raw} << /Filter /ASCIIHexDecode >> /PUT pdfmark
[ {raw} (414243>) /PUT pdfmark
[ /Name (b.txt) /FS << /F (b.txt) /EF << /F {s} >> >> /EMBED pdfmark
[ /Name (a.txt) /FS << /F (a.txt) /EF << /F {raw} >> >> /EMBED pdfmark
showpage";
    let text = pdf_text(&pdf_of(src));
    assert!(text.contains("stream\none,two,three\nendstream"), "{text}");
    assert!(
        text.contains("<< /Subtype /text#2Fplain /Filter /FlateDecode /Length "),
        "{text}"
    );
    assert!(
        text.contains("<< /Filter /ASCIIHexDecode /Length 7 >>\nstream\n414243>\nendstream"),
        "{text}"
    );
    // Name-tree keys sorted by bytes.
    let a = text.find("(a.txt) <<").expect("a.txt");
    let b = text.find("(b.txt) <<").expect("b.txt");
    assert!(a < b, "{text}");
}

#[test]
fn dict_and_array_objects_and_forward_references() {
    let src = "[ /_objdef {arr} /type /array /OBJ pdfmark
[ {arr} 1 (second) /PUT pdfmark
[ {arr} {later} /APPEND pdfmark
[ {arr} /CLOSE pdfmark
[ /_objdef {d} /type /dict /OBJ pdfmark
[ {d} << /Kind /Thing /List {arr} /Ratio 0.5 /On true /None null >> /PUT pdfmark
[ /Name (x) /FS << /F (x) /Extra {d} >> /EMBED pdfmark
showpage";
    let text = pdf_text(&pdf_of(src));
    // arr = 5, later = 6 (made by its forward reference), d = 7.
    assert!(
        text.contains("5 0 obj\n[null (second) 6 0 R]\nendobj"),
        "{text}"
    );
    assert!(text.contains("6 0 obj\n<< >>\nendobj"), "{text}");
    assert!(
        text.contains("7 0 obj\n<< /Kind /Thing /List 5 0 R /None null /On true /Ratio 0.5 >>"),
        "{text}"
    );
}

#[test]
fn other_devices_and_unsupported_marks_clear_to_the_mark() {
    let src = "1 [ /Title (t) /DOCINFO pdfmark \
               [ /Rect [0 0 1 1] /ANN pdfmark \
               [ {Catalog} << /PageMode /UseOutlines >> /PUT pdfmark \
               [ {ThisPage} << /Rotate 90 >> /PUT pdfmark \
               count =";
    assert_eq!(eval(src, false), "1\n");
    assert_eq!(eval(src, true), "1\n");
    // The unsupported ones left nothing in the document.
    let text = pdf_text(&pdf_of(src));
    assert!(
        !text.contains("UseOutlines") && !text.contains("Rotate"),
        "{text}"
    );
}

#[test]
fn errors_leave_the_operands_on_the_stack() {
    let cases = [
        // No mark at all.
        ("/Title (t) /DOCINFO pdfmark", "unmatchedmark"),
        // An odd number of key/value operands.
        ("[ /Title /DOCINFO pdfmark", "rangecheck"),
        // gs refuses an /EMBED without /Name too.
        ("[ /FS << /F (a) >> /EMBED pdfmark", "rangecheck"),
        // A type that is not a name.
        ("[ /Title (t) (DOCINFO) pdfmark", "typecheck"),
        // String data on a dict object.
        (
            "[ /_objdef {d} /type /dict /OBJ pdfmark [ {d} (x) /PUT pdfmark",
            "typecheck",
        ),
        // Defining a name twice.
        (
            "[ /_objdef {d} /type /dict /OBJ pdfmark [ /_objdef {d} /type /dict /OBJ pdfmark",
            "rangecheck",
        ),
        // A value with no PDF form.
        ("[ /Title /add load /DOCINFO pdfmark", "typecheck"),
    ];
    for (body, want) in cases {
        let src = format!(
            "{{ {body} }} stopped {{ $error /errorname get 20 string cvs print ( ) print count = }} if"
        );
        let out = eval(&src, true);
        let (name, depth) = out.trim().split_once(' ').expect("an error");
        assert_eq!(name, want, "{body}");
        assert!(
            depth.parse::<usize>().unwrap() >= 2,
            "{body}: operands kept"
        );
    }
}

#[test]
fn a_failed_mark_leaves_no_trace_in_the_document() {
    let clean = pdf_of("showpage");
    for body in [
        "[ {nope} (x) /PUT pdfmark",
        "[ {nope} 0 1 /PUT pdfmark",
        "[ {nope} 1 /APPEND pdfmark",
        "[ /FS << /EF << /F {nope} >> >> /EMBED pdfmark",
        "[ /Title {nope} /Subject /add load /DOCINFO pdfmark",
    ] {
        let pdf = pdf_of(&format!("{{ {body} }} stopped pop clear showpage"));
        assert!(pdf == clean, "{body} left something in the PDF");
    }
}

#[test]
fn shared_structure_is_counted_per_object() {
    // 2^26 objects at depth 26: under the depth cap, so the node count is
    // what stops it, at once.
    let src = "/a [ 1 ] def 25 { /a [ a a ] def } repeat \
               { [ /Title a /DOCINFO pdfmark } stopped { $error /errorname get = } if";
    assert_eq!(eval(src, true), "limitcheck\n");
}

#[test]
fn string_bytes_are_counted_per_copy() {
    // A 1 MiB string in 256 places is 256 MiB of copies, over the 64 MiB
    // a document's marks may hold; and many keys naming one string too.
    let src = "/s 1048576 string def /a [ s s ] def 7 { /a [ a a ] def } repeat \
               [ /_objdef {x} /type /array /OBJ pdfmark \
               { [ {x} a /APPEND pdfmark } stopped { $error /errorname get = } if \
               { mark 0 1 99 { 3 string cvs cvn s } for /DOCINFO pdfmark } stopped \
               { $error /errorname get = } if";
    assert_eq!(eval(src, true), "limitcheck\nlimitcheck\n");
}

#[test]
fn reference_names_and_keys_are_counted_per_copy() {
    // A 1 MiB name used 128 times as a `{name}` reference, and 100 times as
    // a key: each copy is counted, though neither is stored.
    let src = "/n 1048576 string dup 0 1 1048575 { 97 put dup } for pop cvn def \
               /r [ n ] cvx def /a [ /r load /r load ] def 6 { /a [ a a ] def } repeat \
               [ /_objdef {x} /type /array /OBJ pdfmark \
               { [ {x} a /APPEND pdfmark } stopped { $error /errorname get = } if \
               { mark 1 1 100 { pop n 0 } for /OBJ pdfmark } stopped \
               { $error /errorname get = } if";
    assert_eq!(eval(src, true), "limitcheck\nlimitcheck\n");
}

#[test]
fn decodeparms_without_filter_is_not_given_pscats_flate() {
    let src = "[ /_objdef {s} /type /stream /OBJ pdfmark \
               [ {s} << /DecodeParms << /Columns 4 >> >> /PUT pdfmark [ {s} (abcd) /PUT pdfmark \
               [ /Name (s) /FS << /F (s) /EF << /F {s} >> >> /EMBED pdfmark showpage";
    let text = pdf_text(&pdf_of(src));
    assert!(
        text.contains("<< /DecodeParms << /Columns 4 >> /Length 4 >>\nstream\nabcd\nendstream"),
        "{text}"
    );
}

#[test]
fn a_nul_in_a_name_is_a_rangecheck() {
    let src = "{ [ /Foo << (a\\000b) 1 >> /DOCINFO pdfmark } stopped { $error /errorname get = } if \
               { [ (a\\000b) cvn 1 /DOCINFO pdfmark } stopped { $error /errorname get = } if";
    assert_eq!(eval(src, true), "rangecheck\nrangecheck\n");
}

#[test]
fn a_self_containing_value_is_a_limitcheck() {
    let src = "{ [ /Title [ 1 ] dup dup 0 exch put /DOCINFO pdfmark } stopped \
               { $error /errorname get = } if";
    assert_eq!(eval(src, true), "limitcheck\n");
}

#[test]
fn gs_reads_the_attachment_document() {
    if !Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        eprintln!("skipping: gs not installed");
        return;
    }
    let dir = common::scratch_dir("pscat-pdfmark-gs");
    let pdf = dir.join("embed.pdf");
    std::fs::write(&pdf, pdf_of(EMBED_DOC)).expect("write pdf");
    let out = Command::new("gs")
        .args([
            "-q",
            "-dNOPAUSE",
            "-dBATCH",
            "-dPDFSTOPONERROR",
            "-sDEVICE=nullpage",
        ])
        .arg(&pdf)
        .output_deadline()
        .expect("run gs");
    assert!(
        out.status.success() && out.stderr.is_empty(),
        "gs: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn the_cli_writes_marks_and_dsc_together() {
    let dir = common::scratch_dir("pscat-pdfmark-cli");
    let doc = dir.join("doc.ps");
    std::fs::write(
        &doc,
        format!("{DSC}[ /Subject (S) /DOCINFO pdfmark showpage"),
    )
    .expect("doc");
    let pdf = dir.join("doc.pdf");
    let png = dir.join("doc.png");
    let out = Command::new(env!("CARGO_BIN_EXE_pscat"))
        .arg("--pdf")
        .arg(&pdf)
        .arg("--png")
        .arg(&png)
        .arg(&doc)
        .output_deadline()
        .expect("run pscat");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        info_dict(&std::fs::read(&pdf).expect("pdf")),
        "<< /Producer (pscat) /Title (DSC Title) /Author (DSC Author) /Subject (S) >>"
    );
}

#[test]
fn a_file_put_reads_through_the_read_roots() {
    // /PUT takes a file the program opened with `file`, so the read roots
    // were already applied: inside a root it embeds, outside it is the
    // `file` operator's invalidfileaccess and nothing is attached.
    let dir = common::scratch_dir("pscat-pdfmark-roots");
    let inside = dir.join("inside");
    let outside = dir.join("outside");
    std::fs::create_dir_all(&inside).expect("inside");
    std::fs::create_dir_all(&outside).expect("outside");
    std::fs::write(inside.join("tune.abc"), b"X:1\nK:G\n").expect("data");
    std::fs::write(outside.join("secret.txt"), b"secret").expect("data");
    let run = |data: &std::path::Path| {
        let doc = dir.join("doc.ps");
        std::fs::write(
            &doc,
            format!(
                "[ /_objdef {{f}} /type /stream /OBJ pdfmark\n\
                 [ {{f}} ({}) (r) file /PUT pdfmark\n\
                 [ /Name (a) /FS << /F (a) /EF << /F {{f}} >> >> /EMBED pdfmark showpage\n",
                common::ps_path(data)
            ),
        )
        .expect("doc");
        let pdf = dir.join("doc.pdf");
        let _ = std::fs::remove_file(&pdf);
        let out = Command::new(env!("CARGO_BIN_EXE_pscat"))
            .arg("--read-root")
            .arg(&inside)
            .arg("--pdf")
            .arg(&pdf)
            .arg(&doc)
            .output_deadline()
            .expect("run pscat");
        (
            String::from_utf8_lossy(&out.stderr).into_owned(),
            std::fs::read(&pdf)
                .map(|p| pdf_text(&p))
                .unwrap_or_default(),
        )
    };
    let (err, text) = run(&inside.join("tune.abc"));
    assert!(
        text.contains("stream\nX:1\nK:G\n\nendstream"),
        "{err}{text}"
    );
    let (err, text) = run(&outside.join("secret.txt"));
    assert!(err.contains("invalidfileaccess"), "{err}");
    assert!(!text.contains("secret"), "{text}");
}

// ---------------------------------------------------------------------
// CLI and wasm write the same bytes.
//
// The `npm` CI workflow runs only `tests/wasm.rs` with the wasm target
// installed; elsewhere this skips, as tests/wasm.rs does.
// ---------------------------------------------------------------------

fn root() -> &'static std::path::Path {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn wasm_ready() -> bool {
    let target = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("wasm32-unknown-unknown"));
    let node = Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !(target && node) {
        eprintln!("skipping wasm parity: wasm32-unknown-unknown target or node missing");
        return false;
    }
    let build = Command::new(root().join("scripts/build_wasm.sh"))
        .output()
        .expect("run build_wasm.sh");
    assert!(
        build.status.success(),
        "wasm build failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    true
}

/// Every supported mark, the issue's attachment, inline `currentfile`
/// data, non-ASCII text and the DSC header, in one document.
const PARITY_DOC: &str = "%!PS-Adobe-3.0
%%Title: Parity
%%For: Someone
%%EndComments
[ /Subject (X:1 abc) /Keywords (k) /Creator (caf\\303\\251) /DOCINFO pdfmark
[ /_objdef {f1} /type /stream /OBJ pdfmark
[ {f1} (X:1\\nK:C\\nCDE|) /PUT pdfmark
[ {f1} currentfile /ASCIIHexDecode filter /PUT pdfmark
0A4445467C>
[ {f1} << /Subtype (text/vnd.abc) cvn /Params << /Size 18 >> >> /PUT pdfmark
[ /_objdef {a} /type /array /OBJ pdfmark
[ {a} (x) /APPEND pdfmark
[ /Name (tune.abc) /FS << /Type /Filespec /F (tune.abc) /UF (tune.abc) /EF << /F {f1} >> /Desc {a} >> /EMBED pdfmark
0.2 0.4 0.9 setrgbcolor 10 10 80 80 rectfill
/Helvetica findfont 20 scalefont setfont 20 120 moveto (hi) show showpage
";

#[test]
fn wasm_pdfmark_document_matches_the_cli() {
    if !wasm_ready() {
        return;
    }
    let dir = common::scratch_dir("pscat-pdfmark-wasm");
    let doc = dir.join("doc.ps");
    std::fs::write(&doc, PARITY_DOC).expect("write doc");
    let native = dir.join("native.pdf");
    let out = Command::new(env!("CARGO_BIN_EXE_pscat"))
        .args(["--page", "200x200", "--pdf"])
        .arg(&native)
        .arg(&doc)
        .output_deadline()
        .expect("run pscat");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let driver = format!(
        r#"
import {{ readFileSync, writeFileSync }} from 'node:fs';
import {{ Pscat }} from 'file://{lib}';
const ps = await Pscat.instantiate(readFileSync('{wasm}'));
if (ps.run(readFileSync('{doc}'), 200, 200, 1, {{ pdf: true }}) !== 0) {{ console.error(ps.error); process.exit(1); }}
writeFileSync('{dir}/wasm.pdf', ps.pdf());
console.log('PDFMARK_OK');
"#,
        lib = root().join("web/pscat.js").display(),
        wasm = root().join("web/pscat.wasm").display(),
        doc = doc.display(),
        dir = dir.display(),
    );
    let script = dir.join("driver.mjs");
    std::fs::write(&script, driver).expect("write driver");
    let out = Command::new("node")
        .arg(&script)
        .output_deadline()
        .expect("run node");
    assert!(
        out.status.success() && String::from_utf8_lossy(&out.stdout).contains("PDFMARK_OK"),
        "node driver failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let native = std::fs::read(&native).expect("native pdf");
    let wasm = std::fs::read(dir.join("wasm.pdf")).expect("wasm pdf");
    let text = pdf_text(&native);
    assert!(
        text.contains("stream\nX:1\nK:C\nCDE|\nDEF|\nendstream")
            && text.contains("/Creator <FEFF00630061006600E9>")
            && text.contains("/EmbeddedFiles"),
        "{text}"
    );
    assert!(
        wasm == native,
        "the wasm build's PDF differs from pscat --pdf"
    );
}
