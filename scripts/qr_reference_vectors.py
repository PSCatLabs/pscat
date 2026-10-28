#!/usr/bin/env python3
"""Regenerate offline QR data fixtures with a separately checked-out Nayuki oracle.

Not used by normal tests. No network access or third-party package installation.
Usage: python3 scripts/qr_reference_vectors.py /path/to/QR-Code-generator
"""

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
from unittest.mock import patch

PIN = "3c6d0b3cefb4e049dc337e82237c9644399716a8"
ROOT = Path(__file__).resolve().parents[1]


def load_reference(reference):
    revision = subprocess.check_output(
        ["git", "-C", str(reference), "rev-parse", "HEAD"], text=True, timeout=10
    ).strip()
    if revision != PIN:
        raise ValueError(f"oracle revision {revision}, expected {PIN}")
    source = reference / "python/qrcodegen.py"
    pristine = subprocess.check_output(
        ["git", "-C", str(reference), "show", f"{PIN}:python/qrcodegen.py"], timeout=10
    )
    if source.read_bytes() != pristine:
        raise ValueError("oracle source differs from pinned commit")
    spec = importlib.util.spec_from_file_location("qr_reference", source)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module, hashlib.sha256(pristine).hexdigest()


def generate(reference):
    module, source_hash = load_reference(reference)
    qr, segment = module.QrCode, module.QrSegment
    levels = {"L": qr.Ecc.LOW, "M": qr.Ecc.MEDIUM,
              "Q": qr.Ecc.QUARTILE, "H": qr.Ecc.HIGH}
    fixtures = []
    for version in (1, 2, 3, 4, 9, 10, 26, 27, 40):
        for name, level in levels.items():
            capacity = qr._get_num_data_codewords(version, level)
            maximum = capacity - (2 if version < 10 else 3)
            samples = [("empty", b""), ("maximum", bytes(i % 256 for i in range(maximum)))]
            url = b"https://github.com/PSCatLabs/pscat"
            if len(url) <= maximum:
                samples.append(("github", url))
            for label, data in samples:
                captured = []

                def capture(_self, ver, ecl, codewords, mask):
                    assert ver == version and ecl is level and mask == 0
                    captured.append(bytes(codewords))

                # Capture the real encoder's padded data at its matrix boundary.
                # Even empty input is explicitly one BYTE segment, never no segments.
                with patch.object(qr, "__init__", capture):
                    qr.encode_segments([segment.make_bytes(data)], level,
                                       version, version, 0, False)
                assert len(captured) == 1 and len(captured[0]) == capacity
                fixtures.append({"name": f"v{version}-{name}-{label}",
                                 "version": version, "level": name,
                                 "capacity": capacity, "payload": data.hex(),
                                 "codewords": captured[0].hex()})
    return {"oracle": "https://github.com/nayuki/QR-Code-generator",
            "revision": PIN, "source_sha256": source_hash,
            "description": "One explicit BYTE segment; fixed version, mask 0, ECC boost disabled; captured padded data before correction.",
            "fixtures": fixtures}


def generate_planning(reference):
    module, source_hash = load_reference(reference)
    qr, segment = module.QrCode, module.QrSegment
    levels = {"L": qr.Ecc.LOW, "M": qr.Ecc.MEDIUM,
              "Q": qr.Ecc.QUARTILE, "H": qr.Ecc.HIGH}
    specifications, plans = [], []
    for name, level in levels.items():
        lengths = {0, 1, 31, 255, 256, 2953, 2954}
        for version in range(1, 41):
            raw_modules = qr._get_num_raw_data_modules(version)
            capacity = qr._get_num_data_codewords(version, level)
            maximum = capacity - (2 if version < 10 else 3)
            specifications.append({
                "Version": version, "Level": name, "Size": version * 4 + 17,
                "RawCodewords": raw_modules // 8, "RemainderBits": raw_modules % 8,
                "DataCodewords": capacity,
                "ECCodewordsPerBlock": qr._ECC_CODEWORDS_PER_BLOCK[level.ordinal][version],
                "Blocks": qr._NUM_ERROR_CORRECTION_BLOCKS[level.ordinal][version],
            })
            lengths.update((maximum - 1, maximum, maximum + 1))
        for length in sorted(lengths):
            captured = []

            def capture(_self, version, ecl, data, mask):
                assert ecl is level and mask == 0
                captured.append((version, bytes(data)))

            # Actual automatic version selection, not a local capacity search.
            with patch.object(qr, "__init__", capture):
                try:
                    qr.encode_segments(
                        [segment.make_bytes(bytes(i % 256 for i in range(length)))],
                        level, 1, 40, 0, False)
                except module.DataTooLongError:
                    assert not captured
                    plans.append({"length": length, "level": name, "version": None})
                    continue
            assert len(captured) == 1
            version, data = captured[0]
            plans.append({"length": length, "level": name, "version": version,
                          "capacity": len(data), "data_sha256": hashlib.sha256(data).hexdigest()})
    return {"oracle": "https://github.com/nayuki/QR-Code-generator", "revision": PIN,
            "source_sha256": source_hash,
            "description": "All 160 block specifications; automatic byte-mode plans at maximum-1/maximum/maximum+1 for every version/level. Payload byte i is i mod 256; ECC boost disabled.",
            "specifications": specifications, "plans": plans}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path)
    parser.add_argument("--check", action="store_true", help="fail if fixtures differ")
    args = parser.parse_args()
    for filename, generator in [("qr_byte_vectors.json", generate),
                                ("qr_planning_vectors.json", generate_planning)]:
        content = json.dumps(generator(args.reference), indent=2) + "\n"
        target = ROOT / "tests/data" / filename
        if args.check:
            if target.read_text() != content:
                raise SystemExit(f"{filename} differs from pinned oracle")
        else:
            target.write_text(content)
        print(f"QR fixtures {'match' if args.check else 'written'}: {target}")


if __name__ == "__main__":
    main()
