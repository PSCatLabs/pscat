//! `pdfmark` (#1032): the Distiller/Ghostscript hook a program uses to put
//! things in the PDF it is printed to.
//!
//! `mark … /TYPE pdfmark`. With `--pdf` (or the wasm build's `pdf: true`)
//! these types are applied to the document:
//!
//! - `/DOCINFO` — `key value …` into the `/Info` dict. A marked key wins
//!   over the DSC `%%Title:`/`%%For:` one; `/Producer` is ignored (gs's
//!   pdfwrite does both).
//! - `/OBJ` — `/_objdef {name} /type /stream|/dict|/array` makes a named
//!   object; `{name}` anywhere in a later value refers to it.
//! - `/PUT` — `{stream} string|file|dict`, `{dict} dict`, `{array} index
//!   value`. A file is read to its end (it was opened by `file` or
//!   `currentfile`, so the read roots already applied).
//! - `/APPEND` — `{array} value`; `/CLOSE` — `{name}`, accepted.
//! - `/EMBED` — `/Name string /FS filespec-dict`: an attachment in the
//!   Catalog's `/EmbeddedFiles` name tree.
//!
//! Every other type (`/ANN`, `/OUT`, `/DEST`, …), and a mark that names one
//! of the reserved objects (`{Catalog}`, `{ThisPage}`, `{Page3}` …), is
//! consumed and has no effect, and so is every pdfmark on a device that is
//! not a PDF (PNG, SVG, the window), as in Ghostscript.

use crate::error::PsError;
use crate::interp::Interp;
use crate::object::{Dict, Object, Value};
use crate::pdf::{MarkError, ObjKind, PdfRecorder, PdfValue};

pub fn install(dict: &mut Dict) {
    super::op(dict, "pdfmark", pdfmark);
}

/// How deep an operand may nest (arrays in dicts in arrays …). A deeper
/// one, or one that contains itself, is a `limitcheck`.
const MAX_DEPTH: usize = 32;

/// Why a mark was not applied.
enum Skip {
    /// A PostScript error: the operands stay on the stack.
    Error(PsError),
    /// A mark pscat does not support: consumed, no effect.
    Ignore,
}

impl From<PsError> for Skip {
    fn from(e: PsError) -> Self {
        Skip::Error(e)
    }
}

impl From<MarkError> for Skip {
    fn from(e: MarkError) -> Self {
        Skip::Error(match e {
            MarkError::Range => PsError::Rangecheck,
            MarkError::Type => PsError::Typecheck,
            MarkError::Limit => PsError::Limitcheck,
        })
    }
}

fn pdfmark(it: &mut Interp) -> Result<(), PsError> {
    let depth = it
        .ostack
        .iter()
        .rev()
        .position(|o| matches!(o.value, Value::Mark))
        .ok_or(PsError::UnmatchedMark)?;
    let len = it.ostack.len();
    if let Some(pdf) = it.gfx.pdf_mut() {
        let operands: Vec<Object> = it.ostack[len - depth..].to_vec();
        match apply(pdf, &operands) {
            Ok(()) | Err(Skip::Ignore) => {}
            Err(Skip::Error(e)) => return Err(e),
        }
    }
    it.ostack.truncate(len - depth - 1);
    Ok(())
}

fn apply(pdf: &mut PdfRecorder, operands: &[Object]) -> Result<(), Skip> {
    let Some((kind, args)) = operands.split_last() else {
        return Err(PsError::Rangecheck.into());
    };
    let Value::Name(kind) = &kind.value else {
        return Err(PsError::Typecheck.into());
    };
    match &**kind {
        "DOCINFO" => {
            let mut entries = Vec::new();
            for (key, value) in pairs(args)? {
                let value = match &value.value {
                    Value::String(s) => PdfValue::Text(s.borrow_bytes().to_vec()),
                    _ => convert(value, 0)?,
                };
                entries.push((key, value));
            }
            pdf.docinfo(entries);
        }
        "OBJ" => {
            let (mut name, mut kind) = (None, None);
            for (key, value) in pairs(args)? {
                match key.as_str() {
                    "_objdef" => name = Some(reference(value)?),
                    "type" => {
                        kind = Some(match &value.value {
                            Value::Name(n) => match &**n {
                                "dict" => ObjKind::Dict,
                                "array" => ObjKind::Array,
                                "stream" => ObjKind::Stream,
                                _ => return Err(PsError::Rangecheck.into()),
                            },
                            _ => return Err(PsError::Typecheck.into()),
                        })
                    }
                    _ => {}
                }
            }
            let (Some(name), Some(kind)) = (name, kind) else {
                return Err(PsError::Rangecheck.into());
            };
            pdf.define(&name, kind)?;
        }
        "PUT" => match args {
            [target, value] => {
                let target = reference(target)?;
                match &value.value {
                    Value::Dict(_) => {
                        let PdfValue::Dict(entries) = convert(value, 0)? else {
                            unreachable!("a dict converts to a dict");
                        };
                        let obj = pdf.named_ref(&target);
                        pdf.put_dict(obj, entries)?;
                    }
                    Value::String(s) => {
                        let obj = pdf.named_ref(&target);
                        pdf.put_data(obj, &s.borrow_bytes())?;
                    }
                    Value::File(f) => {
                        let obj = pdf.named_ref(&target);
                        if pdf.kind(obj) != ObjKind::Stream {
                            return Err(PsError::Typecheck.into());
                        }
                        // Read at most one byte past the room left, so an
                        // endless or expanding source stops at the cap.
                        let room = pdf.stream_room();
                        let mut data = Vec::new();
                        let mut file = f.borrow_mut();
                        while data.len() <= room {
                            let want = (room + 1 - data.len()).min(64 * 1024);
                            if file.read_into(&mut data, want)? == 0 {
                                break;
                            }
                        }
                        drop(file);
                        pdf.put_data(obj, &data)?;
                    }
                    _ => return Err(PsError::Typecheck.into()),
                }
            }
            [target, index, value] => {
                let target = reference(target)?;
                let Value::Integer(i) = index.value else {
                    return Err(PsError::Typecheck.into());
                };
                // An index this far past the end would only pad with nulls.
                let i = usize::try_from(i)
                    .ok()
                    .filter(|&i| i < 1 << 16)
                    .ok_or(PsError::Rangecheck)?;
                let value = convert(value, 0)?;
                let obj = pdf.named_ref(&target);
                pdf.put_index(obj, i, value)?;
            }
            _ => return Err(PsError::Rangecheck.into()),
        },
        "APPEND" => {
            let [target, value] = args else {
                return Err(PsError::Rangecheck.into());
            };
            let target = reference(target)?;
            let value = convert(value, 0)?;
            let obj = pdf.named_ref(&target);
            pdf.append(obj, value)?;
        }
        "CLOSE" => {
            let [target] = args else {
                return Err(PsError::Rangecheck.into());
            };
            reference(target)?;
        }
        "EMBED" => {
            let (mut name, mut fs) = (None, None);
            for (key, value) in pairs(args)? {
                match key.as_str() {
                    "Name" => match &value.value {
                        Value::String(s) => name = Some(s.borrow_bytes().to_vec()),
                        _ => return Err(PsError::Typecheck.into()),
                    },
                    "FS" => match &value.value {
                        Value::Dict(_) => fs = Some(convert(value, 0)?),
                        _ => return Err(PsError::Typecheck.into()),
                    },
                    _ => {}
                }
            }
            // gs's pdfwrite refuses an /EMBED without /Name the same way.
            let (Some(name), Some(fs)) = (name, fs) else {
                return Err(PsError::Rangecheck.into());
            };
            pdf.embed(name, fs);
        }
        _ => return Err(Skip::Ignore),
    }
    Ok(())
}

/// `key value …` with name keys: the key text and the value. An odd
/// count is a `rangecheck`, a key that is not a name a `typecheck`.
fn pairs(args: &[Object]) -> Result<Vec<(String, &Object)>, Skip> {
    if args.len() % 2 != 0 {
        return Err(PsError::Rangecheck.into());
    }
    args.chunks_exact(2)
        .map(|kv| match &kv[0].value {
            Value::Name(n) => Ok((n.to_string(), &kv[1])),
            _ => Err(PsError::Typecheck.into()),
        })
        .collect()
}

/// The name in a `{name}` named-object reference. A reserved name
/// (`{Catalog}`, `{ThisPage}`, `{Page2}` …) is a pdfmark pscat does not
/// support, so the whole mark is skipped.
fn reference(obj: &Object) -> Result<String, Skip> {
    let name = match &obj.value {
        Value::Array(a) if obj.executable && a.len() == 1 => match a.get(0).map(|o| o.value) {
            Some(Value::Name(n)) => n.to_string(),
            _ => return Err(PsError::Typecheck.into()),
        },
        _ => return Err(PsError::Typecheck.into()),
    };
    let page = name
        .strip_prefix("Page")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
    let reserved = matches!(
        name.as_str(),
        "Catalog" | "DocInfo" | "ThisPage" | "PrevPage" | "NextPage" | "DocOpen"
    );
    if page || reserved {
        return Err(Skip::Ignore);
    }
    Ok(name)
}

/// A PostScript operand as a PDF value. Dict keys are written in sorted
/// order: a dict's own order follows interned name ids, which a different
/// host (the CLI, the wasm build) may number differently.
fn convert(obj: &Object, depth: usize) -> Result<PdfValue, Skip> {
    if depth > MAX_DEPTH {
        return Err(PsError::Limitcheck.into());
    }
    Ok(match &obj.value {
        Value::Null => PdfValue::Null,
        Value::Boolean(b) => PdfValue::Bool(*b),
        Value::Integer(i) => PdfValue::Int(*i),
        Value::Real(r) if r.is_finite() => PdfValue::Real(*r),
        Value::Real(_) => return Err(PsError::Rangecheck.into()),
        Value::Name(n) => PdfValue::Name(n.to_string()),
        Value::String(s) => PdfValue::Str(s.borrow_bytes().to_vec()),
        Value::Array(_) if obj.executable => PdfValue::Named(reference(obj)?),
        Value::Array(a) => {
            let mut items = Vec::with_capacity(a.len());
            for i in 0..a.len() {
                let Some(item) = a.get(i) else { break };
                items.push(convert(&item, depth + 1)?);
            }
            PdfValue::Array(items)
        }
        Value::Dict(d) => {
            let pairs = d.borrow().pairs();
            let mut entries = Vec::with_capacity(pairs.len());
            for (k, v) in &pairs {
                let key = match &k.value {
                    Value::Name(n) => n.to_string(),
                    Value::String(s) => String::from_utf8_lossy(&s.borrow_bytes()).into_owned(),
                    _ => return Err(PsError::Typecheck.into()),
                };
                entries.push((key, convert(v, depth + 1)?));
            }
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            PdfValue::Dict(entries)
        }
        _ => return Err(PsError::Typecheck.into()),
    })
}
