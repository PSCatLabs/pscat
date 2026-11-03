#!/usr/bin/env python3
"""Regenerate offline QR data and matrix fixtures with a separately checked-out Nayuki oracle.

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


def generate_correction(reference):
    module, source_hash = load_reference(reference)
    qr = module.QrCode
    levels = {"L": qr.Ecc.LOW, "M": qr.Ecc.MEDIUM,
              "Q": qr.Ecc.QUARTILE, "H": qr.Ecc.HIGH}
    products = bytes(qr._reed_solomon_multiply(a, b) for a in range(256) for b in range(256))
    divisors, remainders, interleaved, github = [], [], [], []
    for degree in range(1, 31):
        divisor = qr._reed_solomon_compute_divisor(degree)
        divisors.append({"degree": degree, "hex": divisor.hex()})
        for length in ((0, 53, 513, 2956) if degree == 30 else (0, 53, 513)):
            data = bytes((i * 149 + i // 251) % 256 for i in range(length))
            remainder = qr._reed_solomon_compute_remainder(data, divisor)
            remainders.append({"degree": degree, "data": data.hex(), "hex": remainder.hex()})
    for version in range(1, 41):
        for name, level in levels.items():
            capacity = qr._get_num_data_codewords(version, level)
            # Non-periodic at 256 bytes: detects accidental byte-sized indexing.
            data = bytes((i * 149 + i // 251 + version * 17 + level.ordinal * 23) % 256
                         for i in range(capacity))
            symbol = object.__new__(qr)
            symbol._version, symbol._errcorlvl = version, level
            codewords = symbol._add_ecc_and_interleave(bytearray(data))
            fixture = {"version": version, "level": name, "capacity": capacity,
                       "length": len(codewords),
                       "sha256": hashlib.sha256(codewords).hexdigest()}
            if version == 1 or (version, name) in {(5, "Q"), (7, "H"), (10, "M"), (27, "Q"), (40, "H")}:
                fixture.update({"data": data.hex(), "hex": codewords.hex()})
            interleaved.append(fixture)
    for name, level in levels.items():
        captured = []

        def capture(_self, version, ecl, data, mask):
            assert ecl is level and mask == 0
            captured.append((version, bytes(data)))

        with patch.object(qr, "__init__", capture):
            qr.encode_segments([module.QrSegment.make_bytes(b"https://github.com/PSCatLabs/pscat")],
                               level, 1, 40, 0, False)
        assert len(captured) == 1
        version, data = captured[0]
        symbol = object.__new__(qr)
        symbol._version, symbol._errcorlvl = version, level
        github.append({"version": version, "level": name,
                       "hex": symbol._add_ecc_and_interleave(bytearray(data)).hex()})
    return {"oracle": "https://github.com/nayuki/QR-Code-generator", "revision": PIN,
            "source_sha256": source_hash,
            "description": "GF products in a-major order; actual RS divisor/remainder/interleaving methods. Interleave data byte i = (149*i + floor(i/251) + 17*version + 23*levelOrdinal) mod 256.",
            "gf_products_sha256": hashlib.sha256(products).hexdigest(),
            "divisors": divisors, "remainders": remainders,
            "interleaved": interleaved, "github": github}


def grid_hex(grid):
    """Row-major cells, one bit each, most significant first, zero-padded."""
    bits = [cell for row in grid for cell in row]
    bits += [False] * (-len(bits) % 8)
    return bytes(sum(bit << (7 - k) for k, bit in enumerate(bits[i:i + 8]))
                 for i in range(0, len(bits), 8)).hex()


def grid_sha256(grid):
    """SHA-256 of one 0/1 byte per cell, row-major."""
    return hashlib.sha256(bytes(int(cell) for row in grid for cell in row)).hexdigest()


def placement_codewords(version, count):
    # Non-periodic at 256 bytes, so a truncated bit index is observable.
    return bytes((i * 149 + i // 251 + version * 31) % 256 for i in range(count))


def generate_matrix(reference):
    module, source_hash = load_reference(reference)
    qr = module.QrCode
    levels = {"L": qr.Ecc.LOW, "M": qr.Ecc.MEDIUM,
              "Q": qr.Ecc.QUARTILE, "H": qr.Ecc.HIGH}
    originals = {name: getattr(qr, name) for name in (
        "_draw_codewords", "_apply_mask", "_draw_format_bits", "_draw_version",
        "_set_function_module", "_add_ecc_and_interleave")}

    def clear_metadata(grid, metadata):
        return [[cell and not metadata[y][x] for x, cell in enumerate(row)]
                for y, row in enumerate(grid)]

    def construct(version, level, data, mask, *, place=True, codewords=None):
        """Run the oracle's real constructor, recording its intermediate grids."""
        record = {"metadata": set(), "format": set(), "inside": 0}

        def set_module(self, x, y, isdark):
            if record["inside"]:
                record["metadata"].add((x, y))
                if record["inside"] == "format":
                    record["format"].add((x, y))
            originals["_set_function_module"](self, x, y, isdark)

        def scoped(name, tag):
            def wrapped(self, *args):
                record["inside"] = tag
                try:
                    originals[name](self, *args)
                finally:
                    record["inside"] = 0
            return wrapped

        def draw_codewords(self, allcodewords):
            record["codewords"] = bytes(allcodewords)
            record["function"] = [row[:] for row in self._isfunction]
            record["fixed"] = [row[:] for row in self._modules]
            if place:
                originals["_draw_codewords"](self, allcodewords)

        def apply_mask(self, msk):
            record["unmasked"] = [row[:] for row in self._modules]
            if place:
                originals["_apply_mask"](self, msk)
            record["masked"] = [row[:] for row in self._modules]

        patches = [patch.object(qr, "_set_function_module", set_module),
                   patch.object(qr, "_draw_format_bits", scoped("_draw_format_bits", "format")),
                   patch.object(qr, "_draw_version", scoped("_draw_version", "version")),
                   patch.object(qr, "_draw_codewords", draw_codewords),
                   patch.object(qr, "_apply_mask", apply_mask)]
        if codewords is not None:
            patches.append(patch.object(qr, "_add_ecc_and_interleave",
                                        lambda _self, _data: codewords))
        for p in patches:
            p.start()
        try:
            symbol = qr(version, level, data, mask)
        finally:
            for p in reversed(patches):
                p.stop()
        record["final"] = [row[:] for row in symbol._modules]
        return record

    alignment, layouts, metadata_by_version = [], [], {}
    for version in range(1, 41):
        size = version * 4 + 17
        runs = [construct(version, level, bytes(qr._get_num_data_codewords(version, level)),
                          mask, place=False)
                for level in levels.values() for mask in range(8)]
        format_cells = runs[0]["format"]
        assert all(run["format"] == format_cells for run in runs)
        # The oracle's format routine also sets the always-dark module; it is a
        # fixed pattern, not mask/level metadata, so separate it empirically.
        always_dark = {(x, y) for x, y in format_cells
                       if all(run["final"][y][x] for run in runs)}
        assert len(always_dark) == 1
        cells = runs[0]["metadata"] - always_dark
        metadata = [[(x, y) in cells for x in range(size)] for y in range(size)]
        metadata_by_version[version] = metadata
        base = runs[0]
        fixed = clear_metadata(base["fixed"], metadata)
        symbol = object.__new__(qr)
        symbol._version, symbol._size = version, size
        alignment.append(symbol._get_alignment_pattern_positions())
        layout = {"version": version, "size": size,
                  "data_cells": sum(not c for row in base["function"] for c in row),
                  "function_sha256": grid_sha256(base["function"]),
                  "metadata_sha256": grid_sha256(metadata),
                  "fixed_sha256": grid_sha256(fixed)}
        if version in (1, 2, 7, 10, 27, 40):
            layout.update({"function": grid_hex(base["function"]),
                           "metadata": grid_hex(metadata), "fixed": grid_hex(fixed)})
        layouts.append(layout)

    placements = []
    for version in (1, 2, 5, 7, 10, 14, 27, 40):
        raw = qr._get_num_raw_data_modules(version)
        codewords = placement_codewords(version, raw // 8)
        metadata = metadata_by_version[version]
        masked = []
        for mask in range(8):
            run = construct(version, qr.Ecc.LOW,
                            bytes(qr._get_num_data_codewords(version, qr.Ecc.LOW)),
                            mask, codewords=codewords)
            assert run["codewords"] == codewords
            masked.append(clear_metadata(run["masked"], metadata))
            unmasked = clear_metadata(run["unmasked"], metadata)
        placements.append({"version": version, "remainder_bits": raw % 8,
                           "codewords_sha256": hashlib.sha256(codewords).hexdigest(),
                           "unmasked": grid_hex(unmasked),
                           "shown_mask": version % 8,
                           "masked": grid_hex(masked[version % 8]),
                           "mask_sha256": [grid_sha256(g) for g in masked]})

    symbols = []
    cases = [(1, "L", 0), (2, "M", 1), (5, "Q", 2), (5, "H", 3), (7, "H", 4),
             (10, "M", 5), (14, "Q", 6), (27, "Q", 7), (40, "H", 0)]
    for version, name, mask in cases:
        level = levels[name]
        capacity = qr._get_num_data_codewords(version, level)
        data = bytes((i * 149 + i // 251 + version * 17 + level.ordinal * 23) % 256
                     for i in range(capacity))
        run = construct(version, level, data, mask)
        raw = qr._get_num_raw_data_modules(version) // 8
        blocks = qr._NUM_ERROR_CORRECTION_BLOCKS[level.ordinal][version]
        masked = clear_metadata(run["masked"], metadata_by_version[version])
        fixture = {"version": version, "level": name, "mask": mask,
                   "unequal_blocks": raw % blocks != 0,
                   "codewords_sha256": hashlib.sha256(run["codewords"]).hexdigest(),
                   "masked_sha256": grid_sha256(masked)}
        if (version, name) == (5, "Q"):
            fixture["masked"] = grid_hex(masked)
        symbols.append(fixture)

    github = []
    for ordinal, (name, level) in enumerate(levels.items()):
        captured = []
        mask = ordinal * 2 + 1

        def capture(_self, version, ecl, data, msk):
            captured.append((version, ecl, bytes(data), msk))

        with patch.object(qr, "__init__", capture):
            module.QrCode.encode_segments(
                [module.QrSegment.make_bytes(b"https://github.com/PSCatLabs/pscat")],
                level, 1, 40, mask, False)
        assert len(captured) == 1
        version, ecl, data, msk = captured[0]
        run = construct(version, ecl, data, msk)
        github.append({"version": version, "level": name, "mask": mask,
                       "masked": grid_hex(clear_metadata(run["masked"],
                                                         metadata_by_version[version]))})

    return {"oracle": "https://github.com/nayuki/QR-Code-generator", "revision": PIN,
            "source_sha256": source_hash,
            "description": ("Grids from the oracle's real constructor at a fixed mask, captured at its "
                            "_draw_codewords and _apply_mask calls. Metadata = cells written by "
                            "_draw_format_bits/_draw_version except the one cell dark for every "
                            "level and mask (the dark module); metadata cells are cleared to light in "
                            "fixed/unmasked/masked grids. Grid hex: row-major, one bit per cell, MSB "
                            "first, zero-padded; grid SHA-256: one 0/1 byte per cell. Placement "
                            "codewords byte i = (149*i + floor(i/251) + 31*version) mod 256, injected "
                            "in place of the oracle's correction output; symbol data as in the "
                            "correction vectors."),
            "alignment": alignment, "layouts": layouts, "placements": placements,
            "symbols": symbols, "github": github}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path)
    parser.add_argument("--check", action="store_true", help="fail if fixtures differ")
    args = parser.parse_args()
    for filename, generator in [("qr_byte_vectors.json", generate),
                                ("qr_planning_vectors.json", generate_planning),
                                ("qr_correction_vectors.json", generate_correction),
                                ("qr_matrix_vectors.json", generate_matrix)]:
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
