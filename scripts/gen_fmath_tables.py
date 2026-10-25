#!/usr/bin/env python3
"""Regenerate src/fmath/tables.rs from musl 1.2.5's exp, log and pow data (#738).

    curl -sLO https://musl.libc.org/releases/musl-1.2.5.tar.gz && tar xzf musl-1.2.5.tar.gz
    python3 scripts/gen_fmath_tables.py musl-1.2.5/src/math src/fmath/tables.rs
    cargo fmt --all

The C sources write their constants as hexadecimal float literals (some times
an exact small integer). This converts each one with Python's exact
`float.fromhex` and writes the IEEE-754 bit pattern, so no digit is retyped.
The output is committed (after `cargo fmt`); musl's licence (MIT, Arm Limited
2018) is in the file header. Re-running this and formatting must change nothing.
"""
import re
import struct
import sys


def bits(x):
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def hexf(s):
    s = s.strip()
    m = re.match(r"^(-?0x[0-9a-fA-F.]+p[+-]?\d+)(?:\s*\*\s*(-?\d+))?$", s)
    assert m, s
    v = float.fromhex(m.group(1))
    if m.group(2):
        v *= int(m.group(2))
    return v


def section(text, start, end_marker="\n},"):
    i = text.index(start) + len(start)
    j = text.index(end_marker, i)
    return text[i:j]


def coeffs(body):
    return [hexf(l.rstrip(",")) for l in body.split("\n") if l.strip() and not l.startswith("//")]


def words(name, vals, doc):
    return f"{doc}\npub static {name}: [u64; {len(vals)}] = [\n" + "".join(f"    {bits(v):#018x},\n" for v in vals) + "];\n\n"


def main(musl_math, dest):
    out = [
        "//! Data tables for the exp, log and pow in `musl.rs`, copied bit for bit\n"
        "//! from musl 1.2.5 (`src/math/exp_data.c`, `log_data.c`, `pow_data.c`; MIT\n"
        "//! licence, Copyright (c) 2018, Arm Limited, from ARM's optimized-routines;\n"
        "//! the full text is under \"Math: musl libm port\" in THIRD_PARTY_NOTICES.md).\n"
        "//! Every double is stored as its IEEE-754 bit pattern, converted from the\n"
        "//! C source's hexadecimal float literals by scripts/gen_fmath_tables.py, so\n"
        "//! no digit is retyped. Generated; do not edit by hand.\n\n"
    ]
    t = open(musl_math + "/exp_data.c").read()
    body = section(t, ".tab = {\n")
    vals = [int(x, 16) for x in re.findall(r"0x[0-9a-fA-F]+", body)]
    assert len(vals) == 256
    out.append("/// `__exp_data.tab`: pairs of `asuint64(T[k])` and `asuint64(H[k]) - (k << 52) / N`.\npub static EXP_TAB: [u64; 256] = [\n")
    for i in range(0, 256, 2):
        out.append(f"    {vals[i]:#018x}, {vals[i + 1]:#018x},\n")
    out.append("];\n\n")
    pv = coeffs(section(t, ".poly = {\n"))
    assert len(pv) == 4
    out.append(words("EXP_POLY", pv, "/// `__exp_data.poly`: the polynomial coefficients C2..C5."))
    for name, expr in [
        ("EXP_INVLN2N", "0x1.71547652b82fep0 * 128"),
        ("EXP_NEGLN2HIN", "-0x1.62e42fefa0000p-8"),
        ("EXP_NEGLN2LON", "-0x1.cf79abc9e3b3ap-47"),
        ("EXP_SHIFT", "0x1.8p52"),
    ]:
        out.append(f"pub const {name}: u64 = {bits(hexf(expr)):#018x};\n")
    out.append("\n")

    t = open(musl_math + "/log_data.c").read()
    for nm, start, cols in [("LOG_TAB", ".tab = {\n", "invc, logc"), ("LOG_TAB2", ".tab2 = {\n", "chi, clo")]:
        pairs = re.findall(r"\{([^,]+),\s*([^}]+)\}", section(t, start))
        assert len(pairs) == 128, nm
        out.append(f"/// `__log_data.{nm[4:].lower()}`: `{{{cols}}}` as bit patterns.\npub static {nm}: [[u64; 2]; 128] = [\n")
        out.extend(f"    [{bits(hexf(a)):#018x}, {bits(hexf(b)):#018x}],\n" for a, b in pairs)
        out.append("];\n\n")
    lp = coeffs(section(t, ".poly = {\n"))
    lp1 = coeffs(section(t, ".poly1 = {\n"))
    assert (len(lp), len(lp1)) == (5, 11)
    out.append(words("LOG_POLY", lp, "/// `__log_data.poly`."))
    out.append(words("LOG_POLY1", lp1, "/// `__log_data.poly1`."))
    for name, expr in [("LOG_LN2HI", "0x1.62e42fefa3800p-1"), ("LOG_LN2LO", "0x1.ef35793c76730p-45")]:
        out.append(f"pub const {name}: u64 = {bits(hexf(expr)):#018x};\n")
    out.append("\n")

    t = open(musl_math + "/pow_data.c").read()
    rows = re.findall(r"^A\(([^,]+),\s*([^,]+),\s*([^)]+)\)", t, re.M)
    assert len(rows) == 128
    out.append("/// `__pow_log_data.tab`: `{invc, logc, logctail}` (the unused pad word is dropped).\npub static POW_LOG_TAB: [[u64; 3]; 128] = [\n")
    out.extend(f"    [{bits(hexf(a)):#018x}, {bits(hexf(b)):#018x}, {bits(hexf(c)):#018x}],\n" for a, b, c in rows)
    out.append("];\n\n")
    pp = coeffs(section(t, ".poly = {\n"))
    assert len(pp) == 7
    out.append(words("POW_LOG_POLY", pp, "/// `__pow_log_data.poly`, with the C source's scaling products applied."))
    for name, expr in [("POW_LN2HI", "0x1.62e42fefa3800p-1"), ("POW_LN2LO", "0x1.ef35793c76730p-45")]:
        out.append(f"pub const {name}: u64 = {bits(hexf(expr)):#018x};\n")
    open(dest, "w").write("".join(out))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
