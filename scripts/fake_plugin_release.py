#!/usr/bin/env python3
"""Write a local stand-in for a GitHub release, for the plugin install checks (#853, #866).

    python3 scripts/fake_plugin_release.py BIN_DIR OUT_DIR TAG

OUT_DIR/TAG/ gets pscat-<version>-<triple>.tar.gz for the four Unix targets
the launcher (plugin/scripts/pscat-mcp.sh) downloads and
pscat-<version>-x86_64-pc-windows-msvc.zip for its Windows twin
(pscat-mcp.ps1), each holding BIN_DIR's pscat and pscat-mcp (pscat.exe and
pscat-mcp.exe on Windows) in a pscat-<version>-<triple>/ directory as
scripts/package_release.sh lays it out, and a SHA256SUMS. An archive whose
binaries BIN_DIR lacks is skipped (a Unix BIN_DIR has no .exe, a Windows one
no extensionless binaries). The version is TAG without its leading v. Point
the launcher at it with PSCAT_RELEASE_BASE_URL=file://OUT_DIR, so no network
is used.
"""
import hashlib
import os
import sys
import tarfile
import zipfile

TRIPLES = ("aarch64-apple-darwin", "x86_64-apple-darwin",
           "aarch64-unknown-linux-musl", "x86_64-unknown-linux-musl")
WINDOWS = "x86_64-pc-windows-msvc"


def main(bin_dir, out_dir, tag):
    version = tag[1:] if tag.startswith("v") else tag
    rel = os.path.join(out_dir, tag)
    os.makedirs(rel, exist_ok=True)
    sums = []
    if all(os.path.isfile(os.path.join(bin_dir, e)) for e in ("pscat", "pscat-mcp")):
        for triple in TRIPLES:
            name = f"pscat-{version}-{triple}"
            archive = os.path.join(rel, name + ".tar.gz")
            with tarfile.open(archive, "w:gz") as tar:
                for exe in ("pscat", "pscat-mcp"):
                    tar.add(os.path.join(bin_dir, exe), f"{name}/{exe}")
            sums.append((archive, name + ".tar.gz"))
    if all(os.path.isfile(os.path.join(bin_dir, e)) for e in ("pscat.exe", "pscat-mcp.exe")):
        name = f"pscat-{version}-{WINDOWS}"
        archive = os.path.join(rel, name + ".zip")
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as z:
            for exe in ("pscat.exe", "pscat-mcp.exe"):
                z.write(os.path.join(bin_dir, exe), f"{name}/{exe}")
        sums.append((archive, name + ".zip"))
    if not sums:
        sys.exit(f"no pscat and pscat-mcp (or .exe) in {bin_dir}")
    with open(os.path.join(rel, "SHA256SUMS"), "w", newline="\n") as f:
        for path, name in sums:
            with open(path, "rb") as a:
                f.write(f"{hashlib.sha256(a.read()).hexdigest()}  {name}\n")


if __name__ == "__main__":
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    main(*sys.argv[1:])
