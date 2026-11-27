#!/usr/bin/env python3
"""Regenerate tests/data/plrm_operators.txt from the PostScript Language
Reference Manual, third edition (#1280).

The data file is the PLRM's own list of operator names. This script is how it
was derived, so a reviewer can reproduce it and check the list against the book.

    curl -sL -o PLRM.pdf https://www.adobe.com/jp/print/postscript/pdfs/PLRM.pdf
    shasum -a 256 PLRM.pdf     # 6b29e79e4ab64aaa61a3fb27a0f36838c01f2530362873ac316bdb493a1bab6b
    python3 -I scripts/plrm_operator_index.py PLRM.pdf tests/data/plrm_operators.txt

It needs PyMuPDF (`pip install pymupdf`) and nothing else. Nothing in the build or
the tests runs it: `tests/plrm_operators.rs` reads the checked-in data file.

Method (all of it keyed on the PDF's fonts, so there is no guessing at layout):

* Section 8.2, "Operator Details" (PDF pages 538-738), has one entry per
  operator. An entry starts with its name set in the bold Myriad face at 9.8 pt,
  in the left margin. Those names are the operator list.
* Section 8.1, "Operator Summary" (PDF pages 522-537), groups the same operators
  under 24 headings (11.3 pt bold). The operator names in each signature are bold
  Myriad at 8.7 pt. This gives each name its summary section. A name in 8.2 that
  is not in 8.1 is marked `details-only`.
* An 8.2 entry whose signature line carries the annotation `(error)` is an error
  name (`typecheck`, `VMerror`, ...), not an operator, and is dropped.
* A parenthesised annotation on the signature line that names a procedure set
  (`(CIDInit procedure set)`, `(Trapping procedure set)`) is kept as the note.

Output: one tab-separated row per name, `name  section  page  note`, sorted by
name. `page` is the book's page number of the 8.2 entry (PDF page minus 14).
"""

import re
import sys
import unicodedata

try:
    import fitz  # PyMuPDF
except ImportError:  # pragma: no cover
    sys.exit("needs PyMuPDF: pip install pymupdf")

PDF_SHA256 = "6b29e79e4ab64aaa61a3fb27a0f36838c01f2530362873ac316bdb493a1bab6b"
SUMMARY_PAGES = range(522, 538)  # 1-based PDF pages of section 8.1
DETAIL_PAGES = range(538, 739)  # section 8.2
BOOK_OFFSET = 14


def norm(text):
    """The PDF sets fi/fl as ligature glyphs: fold them back to letters."""
    return unicodedata.normalize("NFKC", text).strip()


def is_bold_myriad(span):
    return "MyriadMM_565" in span["font"]


def summary_sections(doc):
    """name -> first 8.1 section heading it is listed under."""
    sections = {}
    current = None
    for pn in SUMMARY_PAGES:
        for block in doc[pn - 1].get_text("dict")["blocks"]:
            for line in block.get("lines", []):
                for span in line["spans"]:
                    if not is_bold_myriad(span):
                        continue
                    text = norm(span["text"])
                    if abs(span["size"] - 11.3) < 0.2:
                        current = text
                    elif abs(span["size"] - 8.7) < 0.2 and current and text:
                        if current != "Errors":
                            sections.setdefault(text, current)
    return sections


def detail_entries(doc):
    """(name, book page, note, is_error) for every 8.2 entry, in book order."""
    entries = []
    for pn in DETAIL_PAGES:
        lines = []
        for block in doc[pn - 1].get_text("dict")["blocks"]:
            for line in block.get("lines", []):
                lines.append((line["bbox"][1], line["bbox"][0], line["spans"]))
        lines.sort(key=lambda t: (t[0], t[1]))
        current = None
        for _y, x, spans in lines:
            first = spans[0]
            text = norm("".join(s["text"] for s in spans))
            if abs(first["size"] - 9.8) < 0.15 and is_bold_myriad(first) and x < 165:
                current = {"name": norm(first["text"]), "page": pn - BOOK_OFFSET,
                           "sig": [], "in_body": False}
                entries.append(current)
            elif current is not None and not current["in_body"]:
                if "Minion-Regular" in first["font"] and abs(first["size"] - 9.5) < 0.2 and x > 160:
                    current["in_body"] = True
                else:
                    current["sig"].append(text)
    out = []
    for e in entries:
        sig = " ".join(e["sig"])
        is_error = "(error)" in sig and e["name"] != "handleerror"  # also a systemdict procedure
        found = re.search(r"\(([^()]*(?:procedure set|serverdict))\)", sig)
        note = found.group(1) if found else ""
        out.append((e["name"], e["page"], note, is_error))
    return out


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    doc = fitz.open(sys.argv[1])
    if doc.page_count != 912:
        sys.exit(f"expected the 912-page PLRM, got {doc.page_count} pages")
    sections = summary_sections(doc)
    rows = {}
    errors = []
    for name, page, note, is_error in detail_entries(doc):
        if is_error:
            errors.append(name)
            continue
        rows.setdefault(name, (sections.get(name, "details-only"), page, note))
    with open(sys.argv[2], "w", encoding="utf-8") as fh:
        fh.write(
            "# PLRM operator index, third edition (Adobe, 1999), section 8.2 \"Operator Details\".\n"
            "# One row per operator entry: name, 8.1 summary section, book page of the entry, note.\n"
            "# `details-only` = an 8.2 entry that the 8.1 summary does not list.\n"
            "# Generated by scripts/plrm_operator_index.py from PLRM.pdf\n"
            f"# (sha256 {PDF_SHA256}); do not edit by hand.\n"
            f"# {len(errors)} error names ({', '.join(sorted(errors)[:3])}, ...) are in 8.2 but are not operators: dropped.\n"
        )
        for name in sorted(rows):
            section, page, note = rows[name]
            fh.write(f"{name}\t{section}\t{page}\t{note}\n")
    print(f"{len(rows)} operators, {len(errors)} error names dropped")


if __name__ == "__main__":
    main()
