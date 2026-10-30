#!/usr/bin/env python3
"""Reproduce Functional PostScript PNGs or measure paired CPU costs.

Run from any directory. Python standard library only. Benchmark includes child
startup/loading; no wall-time pass/fail thresholds. See TESTING.md for scope.
"""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import statistics
import subprocess

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "docs/functional-postscript/assets"
STYLES = ("ultraviolet", "sunprint", "blue-hour")
STUDY_RENDERS = (
    ("type-nocturne", "960x1200", "(examples/functional/type-terrain.ps) run"),
    ("type-sunprint", "960x1200", "/TypeStyle 1 def (examples/functional/type-terrain.ps) run"),
    ("type-blueprint", "960x1200", "/TypeStyle 2 def /TypeFont /AvantGarde-Demi def (examples/functional/type-terrain.ps) run"),
    ("type-risograph", "960x1200", "/TypeStyle 3 def /TypeFont /Bookman-Demi def (examples/functional/type-terrain.ps) run"),
    ("type-neon", "960x1200", "/TypeStyle 4 def /TypeFont /Times-BoldItalic def (examples/functional/type-terrain.ps) run"),
    ("type-opart", "960x1200", "/TypeStyle 5 def /TypeFont /Courier-Bold def (examples/functional/type-terrain.ps) run"),
    ("press-inspected", "1280x960", "(examples/functional/observable-press.ps) run"),
    ("press-clean", "1280x960", "/Inspect false def (examples/functional/observable-press.ps) run"),
)


def call(argv):
    return subprocess.run([str(a) for a in argv], cwd=ROOT, check=True,
                          capture_output=True, text=True, timeout=120)


def binary(explicit):
    if explicit:
        return Path(explicit).resolve()
    call(["cargo", "build", "--profile", "ci", "--locked", "--bin", "pscat"])
    meta = json.loads(call(["cargo", "metadata", "--no-deps", "--format-version", "1"]).stdout)
    return Path(meta["target_directory"]) / "ci" / ("pscat.exe" if os.name == "nt" else "pscat")


def render(pscat):
    ASSETS.mkdir(parents=True, exist_ok=True)
    manifest = []
    for style, name in enumerate(STYLES):
        dest = ASSETS / f"{name}.png"
        source = f"/Style {style} def (examples/functional/poster.ps) run"
        argv = [pscat, "--page", "720x960", "--lint-strict", "--png", dest, "-e", source]
        out = call(argv)
        assert out.stdout.split("pscat: wrote ")[0] == "enter garden\nleave garden\nflowers=13\nradius-checksum=622\n", out.stdout
        manifest.append({"file": dest.name, "style": style, "page": "720x960",
                         "sha256": hashlib.sha256(dest.read_bytes()).hexdigest(),
                         "stdout": out.stdout})
        print(f"rendered {name}: 13 flowers, checksum 622")
    for name, page, source in [
        ("primitives", "720x300", "(examples/functional/primitives.ps) run"),
        ("detail", "480x480", "(examples/functional/detail.ps) run"),
        ("ground", "720x960", "/Layer 1 def (examples/functional/poster.ps) run"),
        ("imperative", "720x960", "(examples/functional/imperative.ps) run"),
        *STUDY_RENDERS,
    ]:
        dest = ASSETS / f"{name}.png"
        out = call([pscat, "--page", page, "--lint-strict", "--png", dest, "-e", source])
        if name.startswith(("type-", "press-")):
            output = out.stdout.split("pscat: wrote ")[0]
            expected = ("enter type-terrain\nleave type-terrain\nglyphs=15\n"
                        if name.startswith("type-") else
                        "enter proof\nleave proof\nattempted=7\ncompleted=6\nrejected=1\nfault=obs-font-size-must-be-number\n")
            assert output == expected, output
        manifest.append({"file": dest.name, "page": page,
                         "sha256": hashlib.sha256(dest.read_bytes()).hexdigest()})
    (ASSETS / "render-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")


def benchmark(pscat, samples):
    # resource is Unix-only. Rendering above remains portable.
    import platform
    import resource

    def timed(argv):
        before = resource.getrusage(resource.RUSAGE_CHILDREN)
        out = call([pscat, *argv])
        after = resource.getrusage(resource.RUSAGE_CHILDREN)
        return out, (after.ru_utime + after.ru_stime - before.ru_utime - before.ru_stime)

    report = {"source_revision": call(["git", "rev-parse", "HEAD"]).stdout.strip(),
              "input_sha256": {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest()
                               for p in sorted((ROOT / "examples/functional").glob("*.ps"))
                               + [ROOT / "lib/functional.ps", ROOT / "lib/aspect.ps",
                                  ROOT / "lib/artkit.ps", ROOT / "lib/paintkit.ps"]},
              "platform": platform.platform(), "binary": call([pscat, "--version"]).stdout.strip(),
              "samples": samples, "clock": "child user + system CPU seconds; includes loading/startup",
              "profile": ("ci (optimized with debug assertions and overflow checks)"
                          if pscat.parent.name == "ci" else "external binary; build profile unknown"),
              "results": {}}
    cases = {
        "map_fold": [
            ["--headless", "-e", f"/Functional {v} def (examples/functional/benchmark.ps) run"]
            for v in ("true", "false")],
        "poster": [
            ["--page", "720x960", "--headless", f"examples/functional/{v}.ps"]
            for v in ("poster", "imperative")],
    }
    for case, commands in cases.items():
        timings = [[], []]
        for _ in range(samples):
            for index, argv in enumerate(commands):
                out, duration = timed(argv)
                timings[index].append(duration)
                expected = ("332833500\n" if case == "map_fold" else
                            "enter garden\nleave garden\nflowers=13\nradius-checksum=622\n")
                assert out.stdout == expected, out.stdout
        report["results"][case] = {
            name: {"cpu_seconds": values, "median": statistics.median(values), "minimum": min(values)}
            for name, values in zip(("functional", "imperative"), timings)
        }
    for name in ("poster", "imperative"):
        lines = (ROOT / f"examples/functional/{name}.ps").read_text().splitlines()
        report.setdefault("driver_noncomment_lines", {})[name] = sum(
            bool(line.strip()) and not line.lstrip().startswith("%") for line in lines)
    output = ROOT / "docs/functional-postscript/MEASUREMENTS.json"
    output.write_text(json.dumps(report, indent=2) + "\n")
    html = ROOT / "docs/functional-postscript/index.html"
    if html.exists():
        text = html.read_text()
        for case, values in report["results"].items():
            for kind, metrics in values.items():
                key = re.escape(f"{case}.{kind}")
                value = f"{metrics['median'] * 1000:.0f} ms"
                text = re.sub(fr'(<td data-metric="{key}">)[^<]+(</td>)',
                              lambda m: m[1] + value + m[2], text)
        html.write_text(text)
    print(json.dumps(report, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("render", "benchmark"))
    parser.add_argument("--pscat", help="use an existing binary, without building")
    parser.add_argument("--samples", type=int, default=5)
    args = parser.parse_args()
    if args.samples < 1:
        parser.error("--samples must be positive")
    pscat = binary(args.pscat)
    if args.action == "render":
        render(pscat)
    else:
        benchmark(pscat, args.samples)


if __name__ == "__main__":
    main()
