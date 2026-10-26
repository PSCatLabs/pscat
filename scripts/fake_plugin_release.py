#!/usr/bin/env python3
"""Write a local stand-in for a GitHub release, for the plugin install checks (#853).

    python3 scripts/fake_plugin_release.py BIN_DIR OUT_DIR TAG

OUT_DIR/TAG/ gets pscat-<version>-<triple>.tar.gz for the four Unix targets
the launcher (plugin/scripts/pscat-mcp.sh) downloads, each holding BIN_DIR's
pscat and pscat-mcp in a pscat-<version>-<triple>/ directory as
scripts/package_release.sh lays it out, and a SHA256SUMS. The version is TAG
without its leading v. Point the launcher at it with
PSCAT_RELEASE_BASE_URL=file://OUT_DIR, so no network is used.
"""
import hashlib
import os
import sys
import tarfile

TRIPLES = ("aarch64-apple-darwin", "x86_64-apple-darwin",
           "aarch64-unknown-linux-musl", "x86_64-unknown-linux-musl")


def main(bin_dir, out_dir, tag):
    version = tag[1:] if tag.startswith("v") else tag
    rel = os.path.join(out_dir, tag)
    os.makedirs(rel, exist_ok=True)
    sums = []
    for triple in TRIPLES:
        name = f"pscat-{version}-{triple}"
        archive = os.path.join(rel, name + ".tar.gz")
        with tarfile.open(archive, "w:gz") as tar:
            for exe in ("pscat", "pscat-mcp"):
                tar.add(os.path.join(bin_dir, exe), f"{name}/{exe}")
        with open(archive, "rb") as f:
            sums.append(f"{hashlib.sha256(f.read()).hexdigest()}  {name}.tar.gz\n")
    with open(os.path.join(rel, "SHA256SUMS"), "w") as f:
        f.writelines(sums)


if __name__ == "__main__":
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    main(*sys.argv[1:])
