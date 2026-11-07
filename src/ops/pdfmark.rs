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

/// How many objects one mark's operands may hold in all. Limits are only
/// looked at between operators, and an array that holds one array twice,
/// doubled 25 times, is 2^26 objects to copy within depth 32.
const MAX_NODES: usize = 1 << 20;

/// What one mark may still copy: objects ([`MAX_NODES`]) and string and
/// name bytes (the document's room under [`crate::pdf::MAX_MARK_BYTES`],
/// shared with `/PUT` stream data). A shared string is charged every
/// place it appears, as it is copied every time.
struct Budget {
    nodes: usize,
    bytes: usize,
    used: usize,
    /// Bytes this call may copy only for a while (a `{name}` reference
    /// becomes an object number, an ignored key is dropped): checked, but
    /// not charged to the document.
    scratch: usize,
}

impl Budget {
    fn scratch(&mut self, n: usize) -> Result<(), Skip> {
        if n > self.scratch {
            return Err(PsError::Limitcheck.into());
        }
        self.scratch -= n;
        Ok(())
    }

    fn bytes(&mut self, n: usize) -> Result<(), Skip> {
        if n > self.bytes {
            return Err(PsError::Limitcheck.into());
        }
        self.bytes -= n;
        self.used += n;
        Ok(())
    }

    /// A copy of string `s`, charged.
    fn string(&mut self, s: &crate::object::PsString) -> Result<Vec<u8>, Skip> {
        self.bytes(s.len())?;
        Ok(s.borrow_bytes().to_vec())
    }

    /// Name text, charged; a NUL byte has no PDF spelling, even as `#00`
    /// (PDF 32000-1 7.3.5), so it is a `rangecheck`.
    fn name(&mut self, n: &str) -> Result<String, Skip> {
        if n.contains('\0') {
            return Err(PsError::Rangecheck.into());
        }
        self.bytes(n.len())?;
        Ok(n.to_string())
    }
}

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
    let mut budget = Budget {
        nodes: MAX_NODES,
        bytes: pdf.mark_room(),
        used: 0,
        scratch: crate::pdf::MAX_MARK_BYTES,
    };
    apply_kind(pdf, kind, args, &mut budget)?;
    pdf.charge(budget.used);
    Ok(())
}

fn apply_kind(
    pdf: &mut PdfRecorder,
    kind: &str,
    args: &[Object],
    budget: &mut Budget,
) -> Result<(), Skip> {
    match kind {
        "DOCINFO" => {
            let mut entries = Vec::new();
            for (key, value) in pairs(args, budget)? {
                let value = match &value.value {
                    Value::String(s) => PdfValue::Text(budget.string(s)?),
                    _ => convert(value, 0, budget)?,
                };
                budget.bytes(key.len())?;
                entries.push((key, value));
            }
            pdf.docinfo(entries);
        }
        "OBJ" => {
            let (mut name, mut kind) = (None, None);
            for (key, value) in pairs(args, budget)? {
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
                // A mark that fails must leave no trace, so the kind is
                // checked before `named_ref` makes a missing target.
                let kind = pdf.kind_of(&target);
                match &value.value {
                    Value::Dict(_) => {
                        if kind == ObjKind::Array {
                            return Err(PsError::Typecheck.into());
                        }
                        let PdfValue::Dict(entries) = convert(value, 0, budget)? else {
                            unreachable!("a dict converts to a dict");
                        };
                        let obj = pdf.named_ref(&target);
                        pdf.put_dict(obj, entries)?;
                    }
                    Value::String(s) => {
                        if kind != ObjKind::Stream {
                            return Err(PsError::Typecheck.into());
                        }
                        let obj = pdf.named_ref(&target);
                        pdf.put_data(obj, &s.borrow_bytes())?;
                    }
                    Value::File(f) => {
                        if kind != ObjKind::Stream {
                            return Err(PsError::Typecheck.into());
                        }
                        let obj = pdf.named_ref(&target);
                        // Read at most one byte past the room left, so an
                        // endless or expanding source stops at the cap.
                        let room = pdf.mark_room();
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
                if pdf.kind_of(&target) != ObjKind::Array {
                    return Err(PsError::Typecheck.into());
                }
                let value = convert(value, 0, budget)?;
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
            if pdf.kind_of(&target) != ObjKind::Array {
                return Err(PsError::Typecheck.into());
            }
            let value = convert(value, 0, budget)?;
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
            for (key, value) in pairs(args, budget)? {
                match key.as_str() {
                    "Name" => match &value.value {
                        Value::String(s) => name = Some(budget.string(s)?),
                        _ => return Err(PsError::Typecheck.into()),
                    },
                    "FS" => match &value.value {
                        Value::Dict(_) => fs = Some(convert(value, 0, budget)?),
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
fn pairs<'a>(args: &'a [Object], budget: &mut Budget) -> Result<Vec<(String, &'a Object)>, Skip> {
    if !args.len().is_multiple_of(2) {
        return Err(PsError::Rangecheck.into());
    }
    (0..args.len())
        .step_by(2)
        .map(|i| match &args[i].value {
            Value::Name(n) if !n.contains('\0') => {
                budget.scratch(n.len())?;
                Ok((n.to_string(), &args[i + 1]))
            }
            Value::Name(_) => Err(PsError::Rangecheck.into()),
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
/// host (the CLI, the wasm build) may number differently. Every object,
/// string and name is charged to `budget`.
fn convert(obj: &Object, depth: usize, budget: &mut Budget) -> Result<PdfValue, Skip> {
    if depth > MAX_DEPTH || budget.nodes == 0 {
        return Err(PsError::Limitcheck.into());
    }
    budget.nodes -= 1;
    Ok(match &obj.value {
        Value::Null => PdfValue::Null,
        Value::Boolean(b) => PdfValue::Bool(*b),
        Value::Integer(i) => PdfValue::Int(*i),
        Value::Real(r) if r.is_finite() => PdfValue::Real(*r),
        Value::Real(_) => return Err(PsError::Rangecheck.into()),
        Value::Name(n) => PdfValue::Name(budget.name(n)?),
        Value::String(s) => PdfValue::Str(budget.string(s)?),
        Value::Array(_) if obj.executable => {
            let name = reference(obj)?;
            budget.scratch(name.len())?;
            PdfValue::Named(name)
        }
        Value::Array(a) => {
            let mut items = Vec::with_capacity(a.len());
            for i in 0..a.len() {
                let Some(item) = a.get(i) else { break };
                items.push(convert(&item, depth + 1, budget)?);
            }
            PdfValue::Array(items)
        }
        Value::Dict(d) => {
            let pairs = d.borrow().pairs();
            let mut entries = Vec::with_capacity(pairs.len());
            for (k, v) in &pairs {
                let key = match &k.value {
                    Value::Name(n) => budget.name(n)?,
                    Value::String(s) => {
                        let bytes = budget.string(s)?;
                        budget.name(&String::from_utf8_lossy(&bytes))?
                    }
                    _ => return Err(PsError::Typecheck.into()),
                };
                entries.push((key, convert(v, depth + 1, budget)?));
            }
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            PdfValue::Dict(entries)
        }
        _ => return Err(PsError::Typecheck.into()),
    })
}
