//! The page a file's `%%BoundingBox` asks for (#921).
//!
//! An Encapsulated PostScript file (first line `%!PS-Adobe-3.0 EPSF-3.0`)
//! must declare the box its marks fall in (Adobe TN 5002), and EPS viewers
//! and converters render it cropped to that box. Ghostscript does the same
//! with `-dEPSCrop`, which only acts on an EPSF header. A plain
//! `%!PS-Adobe-3.0` document's box is, by the DSC spec (Adobe TN 5001), the
//! bounding box of its marks, not its media size, so a letter document with
//! 1-inch margins says `72 72 540 720`; it is used only when asked for
//! (`pscat --bbox`).
//!
//! The rules follow Ghostscript's EPSCrop where it has one:
//! `%%HiResBoundingBox` wins over `%%BoundingBox` when it is usable, each
//! side is the box's width or height rounded to the nearest point, and the
//! page origin moves to the box's lower-left corner. `(atend)` is resolved
//! from the trailer (Ghostscript ignores it). A box that is malformed, empty
//! or larger than 8000 points is reported, never a hard error: the caller
//! warns and renders on its default page.

/// The largest page side, in points: `pscat --page`'s own limit.
pub const MAX_SIDE: u32 = 8000;

/// A page taken from a bounding box: its size in whole points and the
/// user-space point that sits at the page's lower-left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageBox {
    /// Page width in points.
    pub width: u32,
    /// Page height in points.
    pub height: u32,
    /// The box's lower-left x: user space is translated by `-llx`.
    pub llx: f32,
    /// The box's lower-left y: user space is translated by `-lly`.
    pub lly: f32,
}

/// Whether `source` starts with an EPSF header line
/// (`%!PS-Adobe-3.0 EPSF-3.0`, any versions).
pub fn is_epsf(source: &[u8]) -> bool {
    let first = lines(source).next().unwrap_or_default();
    first.starts_with(b"%!PS-Adobe-")
        && first
            .split(|b| b.is_ascii_whitespace())
            .skip(1)
            .any(|word| word.starts_with(b"EPSF-"))
}

/// The page `source`'s bounding box asks for. Only an EPSF file's box is
/// read unless `any_header` (`--bbox`) is set. `Ok(None)`: no box to use
/// (not EPSF, or an EPSF file without one). `Err`: a box that cannot be
/// used, as a message for a warning.
pub fn page_box(source: &[u8], any_header: bool) -> Result<Option<PageBox>, String> {
    if !any_header && !is_epsf(source) {
        return Ok(None);
    }
    let header = Header::scan(source);
    // `(atend)` defers a value to the trailer; it is scanned only then.
    let mut trailer: Option<Header> = None;
    let mut resolve = |value: Option<&[u8]>, hires: bool| -> Option<Vec<u8>> {
        let value = trim(value?);
        if value != b"(atend)" {
            return Some(value.to_vec());
        }
        let t = trailer.get_or_insert_with(|| Header::scan_trailer(source));
        Some(trim(if hires { t.hires } else { t.bbox }?).to_vec())
    };
    let hires = resolve(header.hires, true);
    let bbox = resolve(header.bbox, false);
    // A usable HiResBoundingBox wins; a broken one falls back to the
    // integer box, which every EPS writer emits alongside it.
    if let Some(found) = hires.as_deref().and_then(|v| parse(v).ok()) {
        return Ok(Some(found));
    }
    match bbox {
        Some(v) => parse(&v)
            .map(Some)
            .map_err(|why| format!("%%BoundingBox: {} {why}", String::from_utf8_lossy(&v))),
        None if header.bbox.is_some() => {
            Err("%%BoundingBox: (atend) has no box in the trailer".into())
        }
        None if any_header => Err("no %%BoundingBox in the header comments".into()),
        None => Ok(None),
    }
}

/// The header comments' first `%%BoundingBox` and `%%HiResBoundingBox`
/// values (the text after the colon), or a trailer's last ones.
#[derive(Default)]
struct Header<'a> {
    bbox: Option<&'a [u8]>,
    hires: Option<&'a [u8]>,
}

impl<'a> Header<'a> {
    /// DSC header comments: the lines after the first, up to
    /// `%%EndComments` or the first line that is not a comment. The first
    /// occurrence of a comment wins, as the spec says for the header.
    fn scan(source: &'a [u8]) -> Self {
        let mut header = Header::default();
        for line in lines(source).skip(1) {
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            if !line.starts_with(b"%") || line.starts_with(b"%%EndComments") {
                break;
            }
            header.take(line, false);
        }
        header
    }

    /// The comments after the last `%%Trailer` line, where `(atend)` values
    /// are given. The last occurrence wins: a document embedded before the
    /// trailer has its own trailer earlier on.
    fn scan_trailer(source: &'a [u8]) -> Self {
        let mut header = Header::default();
        let mut in_trailer = false;
        for line in lines(source) {
            if line.starts_with(b"%%Trailer") {
                header = Header::default();
                in_trailer = true;
            } else if in_trailer {
                header.take(line, true);
            }
        }
        header
    }

    fn take(&mut self, line: &'a [u8], last_wins: bool) {
        let slot = if let Some(v) = line.strip_prefix(b"%%BoundingBox:") {
            (&mut self.bbox, v)
        } else if let Some(v) = line.strip_prefix(b"%%HiResBoundingBox:") {
            (&mut self.hires, v)
        } else {
            return;
        };
        if last_wins || slot.0.is_none() {
            *slot.0 = Some(slot.1);
        }
    }
}

/// A box value `llx lly urx ury` as a page, or why it is not one.
fn parse(value: &[u8]) -> Result<PageBox, &'static str> {
    let text = std::str::from_utf8(value).map_err(|_| "is not four numbers")?;
    let nums: Vec<f64> = text
        .split_ascii_whitespace()
        .map(str::parse::<f64>)
        .collect::<Result<_, _>>()
        .map_err(|_| "is not four numbers")?;
    let [llx, lly, urx, ury] = nums[..] else {
        return Err("is not four numbers");
    };
    if !nums.iter().all(|n| n.is_finite() && n.abs() < 1e6) {
        return Err("is not four numbers");
    }
    // Rounded to the nearest point, as Ghostscript's EPSCrop sizes the page.
    let (w, h) = ((urx - llx).round(), (ury - lly).round());
    if w < 1.0 || h < 1.0 {
        return Err("is empty or inverted");
    }
    if w > f64::from(MAX_SIDE) || h > f64::from(MAX_SIDE) {
        return Err("is larger than 8000 points a side");
    }
    Ok(PageBox {
        width: w as u32,
        height: h as u32,
        llx: llx as f32,
        lly: lly as f32,
    })
}

/// `source`'s lines, ending at CR, LF or CRLF (classic Mac EPS files use
/// CR alone). CRLF yields an empty line between, which every caller skips.
fn lines(source: &[u8]) -> impl Iterator<Item = &[u8]> {
    source.split(|&b| b == b'\n' || b == b'\r')
}

fn trim(value: &[u8]) -> &[u8] {
    value.trim_ascii()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pb(width: u32, height: u32, llx: f32, lly: f32) -> Option<PageBox> {
        Some(PageBox {
            width,
            height,
            llx,
            lly,
        })
    }

    const EPSF: &str = "%!PS-Adobe-3.0 EPSF-3.0\n";

    #[test]
    fn only_an_epsf_header_is_read_by_default() {
        let plain = "%!PS-Adobe-3.0\n%%BoundingBox: 0 0 400 300\nshowpage\n";
        assert_eq!(page_box(plain.as_bytes(), false), Ok(None));
        assert_eq!(page_box(plain.as_bytes(), true), Ok(pb(400, 300, 0.0, 0.0)));
        let eps = format!("{EPSF}%%BoundingBox: 0 0 400 300\n");
        assert_eq!(page_box(eps.as_bytes(), false), Ok(pb(400, 300, 0.0, 0.0)));
        assert!(is_epsf(b"%!PS-Adobe-2.0 EPSF-1.2\r"));
        assert!(!is_epsf(b"%!PS-Adobe-3.0\n% EPSF-3.0\n"));
        assert!(!is_epsf(b"%!PS\n"));
    }

    #[test]
    fn origin_hires_and_rounding_follow_ghostscript() {
        let eps = format!("{EPSF}%%BoundingBox: 100 50 500 350\n");
        assert_eq!(
            page_box(eps.as_bytes(), false),
            Ok(pb(400, 300, 100.0, 50.0))
        );
        // gs -dEPSCrop: 199.75 x 100.25 -> 200 x 100, 100.6 x 50.4 -> 101 x 50.
        let eps = format!(
            "{EPSF}%%BoundingBox: 10 10 211 111\n%%HiResBoundingBox: 10.5 10.5 210.25 110.75\n"
        );
        assert_eq!(
            page_box(eps.as_bytes(), false),
            Ok(pb(200, 100, 10.5, 10.5))
        );
        let eps = format!("{EPSF}%%HiResBoundingBox: 0 0 100.6 50.4\n%%BoundingBox: 0 0 101 51\n");
        assert_eq!(page_box(eps.as_bytes(), false), Ok(pb(101, 50, 0.0, 0.0)));
        // A broken HiRes box falls back to the integer one.
        let eps = format!("{EPSF}%%BoundingBox:0 0 30 20\n%%HiResBoundingBox: oops\n");
        assert_eq!(page_box(eps.as_bytes(), false), Ok(pb(30, 20, 0.0, 0.0)));
        // Negative origins and real numbers in the integer box are fine.
        let eps = format!("{EPSF}%%BoundingBox: -20 -10.5 80 89.5\n");
        assert_eq!(
            page_box(eps.as_bytes(), false),
            Ok(pb(100, 100, -20.0, -10.5))
        );
    }

    #[test]
    fn atend_is_read_from_the_last_trailer() {
        let eps = format!(
            "{EPSF}%%BoundingBox: (atend)\n%%EndComments\n\
             %%BeginDocument: inner.eps\n%%BoundingBox: 0 0 9 9\n%%Trailer\n%%EndDocument\n\
             showpage\n%%Trailer\n%%BoundingBox: 0 0 123 45\n%%EOF\n"
        );
        assert_eq!(page_box(eps.as_bytes(), false), Ok(pb(123, 45, 0.0, 0.0)));
        // No trailer: an embedded document's box is not the page.
        let eps = format!(
            "{EPSF}%%BoundingBox: (atend)\n%%BeginDocument: inner.eps\n\
             %%BoundingBox: 0 0 9 9\n%%EndDocument\nshowpage\n"
        );
        let err = page_box(eps.as_bytes(), false).unwrap_err();
        assert!(err.contains("has no box in the trailer"), "{err}");
    }

    #[test]
    fn the_header_ends_at_code_or_endcomments() {
        let eps = format!("{EPSF}%%Title: x\n\n%%BoundingBox: 0 0 50 60\n%%BoundingBox: 0 0 1 1\n");
        assert_eq!(page_box(eps.as_bytes(), false), Ok(pb(50, 60, 0.0, 0.0)));
        let eps = format!("{EPSF}%%EndComments\n%%BoundingBox: 0 0 50 60\n");
        assert_eq!(page_box(eps.as_bytes(), false), Ok(None));
        let eps = format!("{EPSF}newpath\n%%BoundingBox: 0 0 50 60\n");
        assert_eq!(page_box(eps.as_bytes(), false), Ok(None));
        let plain = "%!PS\nnewpath\n";
        assert!(
            page_box(plain.as_bytes(), true)
                .unwrap_err()
                .contains("no %%BoundingBox")
        );
    }

    #[test]
    fn line_endings_cr_and_crlf() {
        let cr = "%!PS-Adobe-3.0 EPSF-3.0\r%%BoundingBox: 0 0 123 45\r0 0 moveto\r";
        assert_eq!(page_box(cr.as_bytes(), false), Ok(pb(123, 45, 0.0, 0.0)));
        let crlf = "%!PS-Adobe-3.0 EPSF-3.0\r\n%%BoundingBox: 0 0 123 45\r\n0 0 moveto\r\n";
        assert_eq!(page_box(crlf.as_bytes(), false), Ok(pb(123, 45, 0.0, 0.0)));
    }

    #[test]
    fn unusable_boxes_are_reported() {
        for (value, why) in [
            ("0 0 abc 300", "four numbers"),
            ("0 0 400", "four numbers"),
            ("0 0 400 300 7", "four numbers"),
            ("50 50 50 300", "empty"),
            ("400 300 0 0", "empty"),
            ("0 0 9000 300", "8000"),
            ("0 0 nan 300", "four numbers"),
        ] {
            let eps = format!("{EPSF}%%BoundingBox: {value}\n");
            let err = page_box(eps.as_bytes(), false).unwrap_err();
            assert!(err.contains(why), "{value}: {err}");
        }
    }
}
