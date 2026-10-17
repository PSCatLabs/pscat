#!/usr/bin/env python3
"""Build pscat's downloadable font packs (issue #599).

    python3 scripts/font_packs.py check
    python3 scripts/font_packs.py build --tag fonts-2026.10 --out dist/fonts
    python3 scripts/font_packs.py pin --index dist/fonts/font-index.json

`check` verifies fonts/packs.json against fonts/catalog/ and
licenses/assets.json. `build` writes one `<pack>-<tag>.tar.zst` per
pack in fonts/packs.json, a `font-index.json` listing them, and
`SHA256SUMS`. `pin` writes the index's tag and SHA-256 into
src/fontpacks/pinned.rs, which is how a pscat release fixes the packs
its plain `pscat fonts install` downloads.

A pack holds, for each of its families, the font files fonts/packs.json
lists and the licence files licenses/assets.json lists for that
family, copied verbatim (so URW-symbols ships LICENSE.md, LICENSE.OFL
and PROVENANCE.md, the OFL route chosen in #563, and not the AGPL/LPPL
texts beside them), plus `manifest.json`: for every file its SPDX id,
copyright line and SHA-256, taken from assets.json. Nothing is
relicensed here.

The tar is deterministic (sorted, mtime 0, uid/gid 0, mode 0644,
ustar), then compressed with the `zstd` CLI at level 19 (no long
mode, so the window stays at 8 MiB). The same zstd version gives the
same bytes; released tags are immutable anyway (docs/dev/RELEASING-FONTS.md).
"""

import argparse
import hashlib
import io
import json
import os
import re
import subprocess
import sys
import tarfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MANIFEST_SCHEMA = 1
INDEX_SCHEMA = 1
TAG_RE = re.compile(r"^fonts-\d{4}\.\d{2}(\.\d+)?$")
FONT_EXTS = (".ttf", ".otf", ".ttc")


def load_json(rel):
    with open(os.path.join(ROOT, rel), encoding="utf-8") as f:
        return json.load(f)


def glob_match(pattern, path):
    """assets.json's globs: '**' spans directories, '*' stays in one."""
    rx = ""
    i = 0
    while i < len(pattern):
        if pattern.startswith("**", i):
            rx += ".*"
            i += 2
        elif pattern[i] == "*":
            rx += "[^/]*"
            i += 1
        else:
            rx += re.escape(pattern[i])
            i += 1
    return re.fullmatch(rx, path) is not None


def asset_entry(assets, path):
    for e in assets["entries"]:
        if any(glob_match(p, path) for p in e["paths"]):
            return e
    return None


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def pack_files(packs, assets):
    """{pack name: [(archive path, repo path, kind, entry)]}, and errors."""
    out = {}
    errors = []
    for pack in packs["packs"]:
        files = []
        for family, fonts in sorted(pack["families"].items()):
            fam_dir = f"fonts/catalog/{family}"
            entry = asset_entry(assets, f"{fam_dir}/{fonts[0]}") if fonts else None
            if entry is None or "font-pack" not in entry.get("ships_in", []):
                errors.append(f"{family}: no font-pack entry in licenses/assets.json")
                continue
            for font in fonts:
                files.append((f"{family}/{font}", f"{fam_dir}/{font}", "font", entry))
            lic = entry.get("license_files", [])
            if not lic:
                errors.append(f"{family}: assets.json lists no licence files")
            for lf in lic:
                if not lf.startswith(fam_dir + "/"):
                    errors.append(f"{family}: licence file {lf} is outside {fam_dir}")
                    continue
                files.append((lf[len("fonts/catalog/"):], lf, "license", entry))
        for _, repo, _, _ in files:
            if not os.path.isfile(os.path.join(ROOT, repo)):
                errors.append(f"{pack['name']}: {repo} does not exist")
        out[pack["name"]] = sorted(files)
    return out, errors


def check(packs, assets):
    errors = []
    catalog = os.path.join(ROOT, "fonts/catalog")
    on_disk = {}
    for fam in sorted(os.listdir(catalog)):
        d = os.path.join(catalog, fam)
        if os.path.isdir(d):
            on_disk[fam] = sorted(f for f in os.listdir(d) if f.lower().endswith(FONT_EXTS))
    listed = {}
    names = set()
    for pack in packs["packs"]:
        if pack["name"] in names:
            errors.append(f"pack {pack['name']} listed twice")
        names.add(pack["name"])
        for fam, fonts in pack["families"].items():
            if fam in listed:
                errors.append(f"{fam} is in two packs")
            listed[fam] = sorted(fonts)
    if listed != on_disk:
        for fam in sorted(set(listed) | set(on_disk)):
            if listed.get(fam) != on_disk.get(fam):
                errors.append(
                    f"{fam}: fonts/packs.json lists {listed.get(fam)}, fonts/catalog has {on_disk.get(fam)}"
                )
    _, errs = pack_files(packs, assets)
    return errors + errs


def deterministic_tar(members):
    """members: [(archive path, bytes)] -> uncompressed ustar bytes."""
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w", format=tarfile.USTAR_FORMAT) as tar:
        for name, data in sorted(members):
            info = tarfile.TarInfo(name)
            info.size = len(data)
            info.mtime = 0
            info.mode = 0o644
            info.uid = info.gid = 0
            info.uname = info.gname = ""
            tar.addfile(info, io.BytesIO(data))
    return buf.getvalue()


def zstd(data):
    r = subprocess.run(
        ["zstd", "-19", "-q", "-c", "--no-progress"], input=data, capture_output=True
    )
    if r.returncode != 0:
        sys.exit(f"zstd failed: {r.stderr.decode(errors='replace')}")
    return r.stdout


def build(packs, assets, tag, out_dir, only=None):
    if not TAG_RE.match(tag):
        sys.exit(f"tag {tag!r} is not fonts-YYYY.MM or fonts-YYYY.MM.N")
    errors = check(packs, assets)
    if errors:
        sys.exit("fonts/packs.json check failed:\n  " + "\n  ".join(errors))
    os.makedirs(out_dir, exist_ok=True)
    files, _ = pack_files(packs, assets)
    index_packs = []
    sums = []
    for pack in packs["packs"]:
        name = pack["name"]
        if only and name not in only:
            continue
        members = []
        manifest_files = []
        for arc, repo, kind, entry in files[name]:
            with open(os.path.join(ROOT, repo), "rb") as f:
                data = f.read()
            members.append((arc, data))
            rec = {
                "path": arc,
                "kind": kind,
                "license": entry["license"],
                "copyright": entry["copyright"],
                "sha256": sha256(data),
                "size": len(data),
            }
            if kind == "font":
                rec["faces"] = [os.path.splitext(os.path.basename(arc))[0]]
            manifest_files.append(rec)
        manifest = {
            "manifest_schema": MANIFEST_SCHEMA,
            "pack": name,
            "tag": tag,
            "description": pack["description"],
            "files": manifest_files,
        }
        members.append(
            ("manifest.json", (json.dumps(manifest, indent=2, ensure_ascii=False) + "\n").encode())
        )
        blob = zstd(deterministic_tar(members))
        fname = f"{name}-{tag}.tar.zst"
        with open(os.path.join(out_dir, fname), "wb") as f:
            f.write(blob)
        digest = sha256(blob)
        sums.append(f"{digest}  {fname}")
        index_packs.append(
            {
                "name": name,
                "file": fname,
                "size": len(blob),
                "sha256": digest,
                "manifest_schema": MANIFEST_SCHEMA,
                "files": len(manifest_files),
                "description": pack["description"],
            }
        )
        print(f"{fname}: {len(blob)} bytes, {len(manifest_files)} files")
    index = {"index_schema": INDEX_SCHEMA, "tag": tag, "packs": index_packs}
    index_bytes = (json.dumps(index, indent=2) + "\n").encode()
    with open(os.path.join(out_dir, "font-index.json"), "wb") as f:
        f.write(index_bytes)
    sums.append(f"{sha256(index_bytes)}  font-index.json")
    with open(os.path.join(out_dir, "SHA256SUMS"), "w") as f:
        f.write("\n".join(sums) + "\n")
    print(f"font-index.json: sha256 {sha256(index_bytes)}")


PINNED_RS = "src/fontpacks/pinned.rs"
PIN_RE = re.compile(r"pub const PINNED_INDEX: Option<Pinned> = [^;]*;", re.S)


def pin(index_path, pinned_rs):
    with open(index_path, "rb") as f:
        data = f.read()
    index = json.loads(data)
    tag = index["tag"]
    if not TAG_RE.match(tag):
        sys.exit(f"index tag {tag!r} is not a fonts-* tag")
    with open(pinned_rs, encoding="utf-8") as f:
        src = f.read()
    new = (
        "pub const PINNED_INDEX: Option<Pinned> = Some(Pinned {\n"
        f'    tag: "{tag}",\n'
        f'    sha256: "{sha256(data)}",\n'
        "});"
    )
    if not PIN_RE.search(src):
        sys.exit(f"{pinned_rs}: no PINNED_INDEX line to rewrite")
    with open(pinned_rs, "w", encoding="utf-8") as f:
        f.write(PIN_RE.sub(lambda _: new, src, count=1))
    print(f"pinned {tag} sha256 {sha256(data)} in {pinned_rs}")


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("check")
    b = sub.add_parser("build")
    b.add_argument("--tag", required=True)
    b.add_argument("--out", required=True)
    b.add_argument("--pack", action="append", help="build only this pack (repeatable)")
    p = sub.add_parser("pin")
    p.add_argument("--index", required=True)
    p.add_argument("--file", default=os.path.join(ROOT, PINNED_RS))
    args = ap.parse_args(argv)
    packs = load_json("fonts/packs.json")
    assets = load_json("licenses/assets.json")
    if args.cmd == "check":
        errors = check(packs, assets)
        if errors:
            print("\n".join(errors), file=sys.stderr)
            return 1
        print("fonts/packs.json OK")
        return 0
    if args.cmd == "build":
        if args.pack:
            unknown = set(args.pack) - {p["name"] for p in packs["packs"]}
            if unknown:
                sys.exit(f"no such pack: {', '.join(sorted(unknown))}")
        build(packs, assets, args.tag, args.out, args.pack)
        return 0
    pin(args.index, args.file)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
