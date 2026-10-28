#!/usr/bin/env python3
"""Validate offline Functional PostScript assets, links, metrics and PS examples."""
import argparse
import hashlib
import base64
import gzip
from html.parser import HTMLParser
import json
from pathlib import Path
import re
import struct
import subprocess
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parent.parent
DOCS = ROOT / "docs/functional-postscript"


class Page(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.ids = set()
        self.targets = []
        self.images = []

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if "id" in attrs:
            assert attrs["id"] not in self.ids, f"duplicate id {attrs['id']}"
            self.ids.add(attrs["id"])
        for key in ("href", "src"):
            if key in attrs:
                target = urlsplit(attrs[key])
                if key == "src":
                    assert not target.scheme and not target.netloc, "asset must work offline"
                if not target.scheme and not target.netloc:
                    self.targets.append(target)
        if tag == "img":
            assert attrs.get("alt"), f"missing image alt: {attrs}"
            self.images.append(attrs)


def png_size(path):
    raw = path.read_bytes()
    assert raw[:8] == b"\x89PNG\r\n\x1a\n", f"not PNG: {path}"
    return struct.unpack(">II", raw[16:24])


def examples(pscat):
    files = sorted(DOCS.glob("*.md")) + [ROOT / "plugin/skills/functional-postscript/SKILL.md"]
    count = 0
    for path in files:
        text = path.read_text()
        for index, match in enumerate(re.finditer(r"^```postscript([^\n]*)\n(.*?)^```\s*$", text, re.M | re.S)):
            if "partial" in match.group(1).split():
                continue
            out = subprocess.run([str(pscat), "--page", "100x100", "--headless", "-"],
                                 cwd=ROOT, input=match.group(2), text=True,
                                 capture_output=True, timeout=30)
            assert out.returncode == 0, f"{path.name} example {index + 1}: {out.stderr} {out.stdout}"
            count += 1
    assert count >= 12, "complete PS examples unexpectedly disappeared"
    return count


def check():
    total_targets = total_images = 0
    for name in ("index.html", "live-studio.html"):
        page = Page()
        page.feed((DOCS / name).read_text())
        total_targets += len(page.targets)
        total_images += len(page.images)
        for target in page.targets:
            path = DOCS / unquote(target.path) if target.path else DOCS / name
            assert path.exists(), f"{name}: missing HTML target {target.geturl()}"
            if target.fragment and path.resolve() == (DOCS / name).resolve():
                assert target.fragment in page.ids, f"missing HTML anchor {target.fragment}"
        for img in page.images:
            path = DOCS / img["src"]
            assert png_size(path) == (int(img["width"]), int(img["height"])), f"wrong image dimensions: {path}"
    html = (DOCS / "index.html").read_text()
    manifest = json.loads((DOCS / "assets/render-manifest.json").read_text())
    expected_assets = {"ultraviolet", "sunprint", "blue-hour", "imperative", "ground", "detail", "primitives",
                       "type-nocturne", "type-sunprint", "press-inspected", "press-clean"}
    assert {Path(r["file"]).stem for r in manifest} == expected_assets, "render manifest missing a study"
    for record in manifest:
        path = DOCS / "assets" / record["file"]
        assert hashlib.sha256(path.read_bytes()).hexdigest() == record["sha256"], f"stale hash: {path}"
        assert png_size(path) == tuple(map(int, record["page"].split("x")))
    report = json.loads((DOCS / "MEASUREMENTS.json").read_text())
    for path, expected in report["input_sha256"].items():
        assert hashlib.sha256((ROOT / path).read_bytes()).hexdigest() == expected, f"stale measured source: {path}"
    for case, values in report["results"].items():
        for kind, metrics in values.items():
            expected = f"{metrics['median'] * 1000:.0f} ms"
            match = re.search(fr'<td data-metric="{case}\.{kind}">([^<]+)</td>', html)
            assert match and match.group(1) == expected, f"stale HTML metric {case}.{kind}, expected {expected}"
    runtime = (DOCS / "assets/pscat-studio-runtime.js").read_text().splitlines()[1]
    prefix = "window.PSCAT_STUDIO_BUNDLE = "
    assert runtime.startswith(prefix) and runtime.endswith(";")
    bundle = json.loads(runtime[len(prefix):-1])
    metadata = json.loads((DOCS / "assets/studio-manifest.json").read_text())
    assert bundle["manifest"] == metadata, "studio metadata differs from payload"
    packed = base64.b64decode(bundle["wasmGzipBase64"])
    assert hashlib.sha256(packed).hexdigest() == metadata["gzip_sha256"]
    raw = gzip.decompress(packed)
    assert hashlib.sha256(raw).hexdigest() == metadata["wasm_sha256"]
    assert raw[:8] == b"\x00asm\x01\x00\x00\x00", "invalid WASM header"
    api = (ROOT / "web/pscat.js").read_text()
    assert bundle["apiSource"] == api, "studio API is stale"
    assert hashlib.sha256(api.encode()).hexdigest() == metadata["api_sha256"]
    for path, expected in {**metadata["source_sha256"], **metadata["embedded_library_sha256"], **metadata["engine_source_sha256"]}.items():
        assert hashlib.sha256((ROOT / path).read_bytes()).hexdigest() == expected, f"stale studio input: {path}"
    common = (ROOT / "examples/functional/type-common.ps").read_text()
    for key, name in (("terrain", "type-terrain.ps"), ("press", "observable-press.ps")):
        source = (ROOT / "examples/functional" / name).read_text()
        assert bundle["programs"][key] == source.replace("(examples/functional/type-common.ps) run", common)
    print(f"HTML: {total_targets} local targets, {total_images} images; {len(manifest)} asset hashes and 4 metrics pass")
    print("Studio: actual WASM payload, unchanged JS API and both PostScript sources pass")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pscat", required=True, type=Path)
    args = parser.parse_args()
    skill = (ROOT / "plugin/skills/functional-postscript/SKILL.md").read_text()
    front = skill.split("---", 2)
    assert len(front) == 3 and front[0] == "", "skill frontmatter required"
    fields = dict(line.split(":", 1) for line in front[1].strip().splitlines())
    assert fields["name"].strip() == "functional-postscript"
    assert fields["description"].strip() and not any(x in skill for x in ("TODO", "[insert", "[TODO"))
    check()
    count = examples(args.pscat.resolve())
    print(f"PostScript: {count} complete Markdown/skill examples pass")


if __name__ == "__main__":
    main()
