#!/usr/bin/env python3
"""Licence and packaging checks that need only python3 and git (issue #601).

    python3 scripts/check_licences.py assets      # licenses/assets.json vs tracked files
    python3 scripts/check_licences.py fonts       # every fonts/catalog/*/ has a licence file
    python3 scripts/check_licences.py package     # `cargo package --list` vs the allowlist
    python3 scripts/check_licences.py package FILE  # same, reading the listing from FILE
    python3 scripts/check_licences.py all         # assets + fonts

Exit status 0 when every check passes, 1 when one fails (each problem is
printed), 2 on a usage or environment error.

`assets` guards licenses/assets.json (issue #563), the path -> licence map
behind THIRD_PARTY_NOTICES.md. Its last entry is a `**` catch-all (the
project's own code), so "every tracked file matches an entry" is true by
construction and proves nothing. What can go wrong is a file landing in the
catch-all that should not: a new font, image or PDF has no entry of its own
and is silently called MIT OR Apache-2.0. Renders and specimens under docs/,
notes/, lib/, site/ and tests/ are the project's own and are exempt (fonts and
anything under a refs/ directory are not), so in practice this catches a font
anywhere, or an image or PDF in gallery/, art/, refs/ or the repo root, that
nobody wrote an entry for. So the check is
  * every tracked binary asset (font, raster image, PDF) matches an entry
    other than the catch-all, unless it is in SELF_AUTHORED below;
  * every entry other than the catch-all matches at least one tracked file
    (a stale entry hides a rename that dropped a file's real licence);
  * every ship-in-the-crate entry names licence files that exist.

`fonts` is the unlicensed-font-directory check: a directory under
fonts/catalog/ with no licence text (OFL.txt, LICENSE*, COPYING*, ...) fails.

`package` is the "no unlisted assets" check: `cargo package --list` must
contain the licence files, and every file in it must match an assets.json
entry whose `ships_in` includes "crate" (so a font-pack or site asset that
sneaks into the include allowlist fails).
"""
import fnmatch
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ASSETS = "licenses/assets.json"
FONT_CATALOG = "fonts/catalog"

# Files that must be in the published crate.
REQUIRED_IN_CRATE = [
    "LICENSE-MIT",
    "LICENSE-APACHE",
    "LICENSE-OFL",
    "THIRD_PARTY_NOTICES.md",
    "licenses/assets.json",
    "fonts/LICENSE",
]
# Never in the crate, whatever assets.json says: the trees with other terms or
# no business in a library package.
FORBIDDEN_IN_CRATE = ("fonts/catalog/", "gallery/", "examples/", "site/",
                      "docs/", "art/", "tests/", "web/", "branding")
# cargo adds these itself.
CARGO_GENERATED = {".cargo_vcs_info.json", "Cargo.toml.orig"}

BINARY_EXT = {".ttf", ".otf", ".ttc", ".woff", ".woff2", ".png", ".jpg",
              ".jpeg", ".gif", ".webp", ".pdf", ".ico"}
# Images and PDFs under these trees are the project's own renders, specimen
# sheets and test fixtures, and rightly fall under the code catch-all (the
# comparison panels that embed third-party crops have entries of their own,
# listed in assets.json). A font file, or anything in a refs/ directory (the
# collected third-party reference crops), never gets this exemption.
FONT_EXT = {".ttf", ".otf", ".ttc", ".woff", ".woff2"}
SELF_AUTHORED = ("tests/", "selftest/", "benches/", "web/", "site/", "notes/",
                 "lib/", "docs/")

# A directory under fonts/catalog/ is licensed if it holds a file named like one.
LICENCE_NAME = re.compile(
    r"^(OFL|LICEN[CS]E|COPYING|UFL|GUST-FONT-LICEN[CS]E|COPYRIGHT)([-_. ].*)?$", re.I)


def glob_to_regex(pattern):
    """assets.json globs: `**` matches any number of directories, `*` stays
    within one."""
    out, i = [], 0
    while i < len(pattern):
        if pattern.startswith("**/", i):
            out.append("(?:.*/)?")
            i += 3
        elif pattern.startswith("**", i):
            out.append(".*")
            i += 2
        elif pattern[i] == "*":
            out.append("[^/]*")
            i += 1
        elif pattern[i] == "?":
            out.append("[^/]")
            i += 1
        else:
            out.append(re.escape(pattern[i]))
            i += 1
    return re.compile("^" + "".join(out) + "$")


def load_entries(root=ROOT):
    data = json.loads((Path(root) / ASSETS).read_text(encoding="utf-8"))
    entries = []
    for e in data["entries"]:
        entries.append((e, [glob_to_regex(p) for p in e["paths"]]))
    return entries


def first_match(entries, path):
    for entry, regexes in entries:
        if any(r.match(path) for r in regexes):
            return entry
    return None


def tracked_files(root=ROOT):
    out = subprocess.run(["git", "-C", str(root), "ls-files", "-z"],
                         capture_output=True, check=True).stdout
    return [p for p in out.decode("utf-8").split("\0") if p]


def is_catch_all(entry):
    return entry["paths"] == ["**"]


def check_assets(files, entries, root=ROOT):
    problems = []
    tracked = set(files)
    for path in files:
        if Path(path).suffix.lower() not in BINARY_EXT:
            continue
        if (path.startswith(SELF_AUTHORED) and "/refs/" not in path
                and Path(path).suffix.lower() not in FONT_EXT):
            continue
        entry = first_match(entries, path)
        if entry is None or is_catch_all(entry):
            problems.append(
                f"{path}: a binary asset with no licenses/assets.json entry of "
                f"its own (it would default to the project's MIT OR Apache-2.0)")
    for entry, regexes in entries:
        if is_catch_all(entry):
            continue
        if not any(r.match(p) for r in regexes for p in files):
            problems.append(f"entry '{entry['id']}' matches no tracked file (stale?)")
        for lf in entry.get("license_files", []):
            if lf not in tracked:
                problems.append(f"entry '{entry['id']}': licence file {lf} is not tracked")
    if not any(is_catch_all(e) for e, _ in entries):
        problems.append("assets.json has no '**' catch-all entry")
    return problems


def check_fonts(files):
    dirs = {}
    prefix = FONT_CATALOG + "/"
    for path in files:
        if not path.startswith(prefix):
            continue
        parts = path[len(prefix):].split("/")
        if len(parts) < 2:  # a loose file in the catalog root, e.g. a specimen png
            continue
        dirs.setdefault(parts[0], []).append("/".join(parts[1:]))
    problems = []
    if not dirs:
        problems.append(f"no font directories found under {FONT_CATALOG}/")
    for name, names in sorted(dirs.items()):
        top = [n for n in names if "/" not in n]
        if not any(LICENCE_NAME.match(n) for n in top):
            problems.append(
                f"{FONT_CATALOG}/{name}/: no licence file (OFL.txt, LICENSE*, "
                f"COPYING*, ...) next to the font")
    return problems


def check_package(listing, entries, files):
    listed = [l.strip().replace("\\", "/") for l in listing if l.strip()]
    problems = []
    for need in REQUIRED_IN_CRATE:
        if need not in listed:
            problems.append(f"{need} is missing from `cargo package --list`")
    for path in listed:
        if path in CARGO_GENERATED or path in ("Cargo.toml", "Cargo.lock"):
            continue
        if path.startswith(FORBIDDEN_IN_CRATE):
            problems.append(f"{path}: must not be in the published crate")
            continue
        entry = first_match(entries, path)
        if entry is None or "crate" not in entry.get("ships_in", []):
            problems.append(
                f"{path}: in the package but its assets.json entry "
                f"('{entry['id'] if entry else 'none'}') does not ship in the crate")
    return problems


def package_listing():
    try:
        out = subprocess.run(
            ["cargo", "package", "--list", "--allow-dirty", "--locked"],
            cwd=ROOT, capture_output=True, text=True, check=True)
    except (OSError, subprocess.CalledProcessError) as e:
        sys.exit(f"check_licences: cargo package --list failed: {e}")
    return out.stdout.splitlines()


def report(title, problems):
    if problems:
        print(f"FAIL {title}: {len(problems)} problem(s)")
        for p in problems:
            print(f"  - {p}")
    else:
        print(f"ok   {title}")
    return not problems


def main(argv):
    if not argv or argv[0] not in ("assets", "fonts", "package", "all"):
        print(__doc__, file=sys.stderr)
        return 2
    cmd = argv[0]
    files = tracked_files()
    entries = load_entries()
    ok = True
    if cmd in ("assets", "all"):
        ok &= report("assets.json covers the tracked binary assets", check_assets(files, entries))
    if cmd in ("fonts", "all"):
        ok &= report("every fonts/catalog/*/ has a licence file", check_fonts(files))
    if cmd == "package":
        listing = (Path(argv[1]).read_text(encoding="utf-8").splitlines()
                   if len(argv) > 1 else package_listing())
        ok &= report("cargo package --list matches the licence allowlist",
                     check_package(listing, entries, files))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
