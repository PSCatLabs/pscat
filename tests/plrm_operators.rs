//! Level 3 coverage (#1280): every operator in the PLRM's operator index must be
//! defined in `systemdict`, or be listed in `tests/data/plrm_operator_gaps.txt`
//! with the issue that tracks it (or the reason it is left out).
//!
//! `tests/data/plrm_operators.txt` is the PLRM third edition's section 8.2
//! "Operator Details" entry list (how it was derived: `scripts/plrm_operator_index.py`).
//! A newly missing operator fails here, and so does a listed operator that now
//! exists, so the list cannot rot. Ghostscript is the cross-check the data
//! files record (their headers say where the two disagree); it is not run here.

use pscat::Interp;
use std::collections::{BTreeMap, BTreeSet};

fn data(name: &str) -> String {
    let path = format!("{}/tests/data/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

/// `(name, section)` for every operator in the PLRM index, in file order.
fn plrm_index() -> Vec<(String, String)> {
    data("plrm_operators.txt")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let mut cols = l.split('\t');
            let name = cols.next().expect("name column").to_string();
            let section = cols
                .next()
                .unwrap_or_else(|| panic!("plrm_operators.txt line needs a section column: {l:?}"))
                .to_string();
            (name, section)
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
enum Gap {
    /// Tracked by `#N`, or `untracked`.
    Missing(String),
    Omitted,
}

/// name -> gap, with the note kept for the diff.
fn gap_list() -> BTreeMap<String, (Gap, String)> {
    let mut out = BTreeMap::new();
    for line in data("plrm_operator_gaps.txt").lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        assert!(
            cols.len() == 4,
            "gap line needs 4 tab-separated columns: {line:?}"
        );
        let (kind, name, issue, note) = (cols[0], cols[1], cols[2], cols[3]);
        assert!(!note.trim().is_empty(), "{name}: a gap needs a note");
        let gap = match kind {
            "missing" => {
                let numbered = issue
                    .strip_prefix('#')
                    .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
                assert!(
                    numbered || issue == "untracked",
                    "{name}: the issue column is `#N` or `untracked`, got {issue:?}"
                );
                Gap::Missing(issue.to_string())
            }
            "omitted" => {
                assert_eq!(issue, "-", "{name}: an omission has no issue");
                Gap::Omitted
            }
            other => panic!("{name}: unknown gap kind {other:?}"),
        };
        let prior = out.insert(name.to_string(), (gap, note.to_string()));
        assert!(prior.is_none(), "{name} is listed twice in the gap list");
    }
    out
}

/// Names the PLRM defines inside another dictionary of systemdict, not in systemdict itself.
const HOME_DICT: &[(&str, &str)] = &[("exitserver", "serverdict")];

/// The PostScript that answers "is this PLRM name defined?" for `name`.
fn probe(name: &str) -> String {
    match HOME_DICT.iter().find(|(n, _)| *n == name) {
        Some((_, home)) => format!(
            "systemdict /{home} known {{ systemdict /{home} get ({name}) cvn known }} {{ false }} ifelse"
        ),
        None => format!("systemdict ({name}) cvn known"),
    }
}

/// The PLRM names `systemdict` defines, asked of the interpreter itself.
fn defined_in_systemdict(names: &[String]) -> BTreeSet<String> {
    let mut it = Interp::with_page(10, 10).expect("test page");
    let mut defined = BTreeSet::new();
    for name in names {
        // (name) cvn, never a literal: `[`, `<<` and `>>` are tokens, not names.
        it.run_str(&probe(name))
            .unwrap_or_else(|e| panic!("probe {name}: {}", it.error_report(&e)));
        if it.pop().expect("bool").repr() == "true" {
            defined.insert(name.clone());
        }
    }
    defined
}

#[test]
fn systemdict_matches_the_plrm_operator_index() {
    let index = plrm_index();
    let gaps = gap_list();
    let names: Vec<String> = index.iter().map(|(n, _)| n.clone()).collect();
    let section: BTreeMap<&str, &str> = index
        .iter()
        .map(|(n, s)| (n.as_str(), s.as_str()))
        .collect();
    assert_eq!(
        names.iter().collect::<BTreeSet<_>>().len(),
        names.len(),
        "the PLRM index lists a name twice"
    );

    // A gap-list name the PLRM index does not have is a typo or a stale line.
    let unknown: Vec<&String> = gaps
        .keys()
        .filter(|n| !section.contains_key(n.as_str()))
        .collect();
    assert!(
        unknown.is_empty(),
        "tests/data/plrm_operator_gaps.txt names operators that are not in the PLRM index: {unknown:?}"
    );

    let defined = defined_in_systemdict(&names);

    let mut report = String::new();
    for name in &names {
        let is_defined = defined.contains(name);
        match (is_defined, gaps.get(name)) {
            (false, None) => report.push_str(&format!(
                "  + {name}: newly missing from systemdict ({}); define it, or add a `missing` line to tests/data/plrm_operator_gaps.txt with its issue\n",
                section[name.as_str()]
            )),
            (true, Some((gap, _))) => report.push_str(&format!(
                "  - {name}: systemdict now defines it; delete its `{}` line from tests/data/plrm_operator_gaps.txt\n",
                match gap {
                    Gap::Missing(issue) => format!("missing {issue}"),
                    Gap::Omitted => "omitted".to_string(),
                }
            )),
            _ => {}
        }
    }
    let missing = gaps
        .values()
        .filter(|(g, _)| matches!(g, Gap::Missing(_)))
        .count();
    let omitted = gaps.len() - missing;
    assert!(
        report.is_empty(),
        "systemdict and tests/data/plrm_operators.txt disagree \
         (PLRM {} operators, systemdict defines {}, {missing} missing and {omitted} omitted are listed):\n{report}",
        names.len(),
        defined.len(),
    );
}

#[test]
fn the_check_sees_a_missing_operator() {
    // Control: the probe must be able to say "not defined", or the test above
    // could pass on an empty index.
    let names = vec![
        "add".to_string(),
        "no_such_operator_1280".to_string(),
        "[".to_string(),
    ];
    let defined = defined_in_systemdict(&names);
    assert!(defined.contains("add") && defined.contains("["));
    assert!(!defined.contains("no_such_operator_1280"));
}
