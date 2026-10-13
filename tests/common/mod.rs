//! Helpers shared by integration tests that inspect PDF output.

/// The document as text with every page content stream inflated in
/// place (content streams are Flate-compressed, issue #244), so the
/// assertions below can read operators directly. Image XObjects (dict
/// has `/Subtype /Image`) are left compressed.
pub fn pdf_text(pdf: &[u8]) -> String {
    use flate2::read::ZlibDecoder;
    use std::io::Read as _;

    let find = |hay: &[u8], needle: &[u8], from: usize| {
        hay[from..]
            .windows(needle.len())
            .position(|w| w == needle)
            .map(|i| i + from)
    };
    let mut out: Vec<u8> = Vec::new();
    let mut at = 0;
    while let Some(s) = find(pdf, b"stream\n", at) {
        let body = s + b"stream\n".len();
        // Skip "endstream" matches: a real stream keyword follows ">>\n".
        let dict_start = pdf[..s].windows(2).rposition(|w| w == b"<<").unwrap_or(0);
        let dict = String::from_utf8_lossy(&pdf[dict_start..s]).into_owned();
        if !dict.ends_with(">>\n") || dict.contains("/Subtype /Image") {
            out.extend_from_slice(&pdf[at..body]);
            at = body;
            continue;
        }
        let len: usize = dict
            .split("/Length ")
            .nth(1)
            .and_then(|r| r.split_whitespace().next())
            .and_then(|n| n.parse().ok())
            .expect("stream /Length");
        out.extend_from_slice(&pdf[at..body]);
        let raw = &pdf[body..body + len];
        if dict.contains("/Filter /FlateDecode") {
            let mut inflated = Vec::new();
            ZlibDecoder::new(raw)
                .read_to_end(&mut inflated)
                .expect("content stream inflates");
            out.extend_from_slice(&inflated);
        } else {
            out.extend_from_slice(raw);
        }
        at = body + len;
    }
    out.extend_from_slice(&pdf[at..]);
    String::from_utf8_lossy(&out).into_owned()
}
