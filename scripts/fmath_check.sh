#!/usr/bin/env bash
# fmath_check.sh -- bit-for-bit check of src/fmath against musl's libm (#738).
#
#   scripts/fmath_check.sh [N]            # N random inputs per function (default 4000000)
#   PLATFORM=linux/amd64 scripts/fmath_check.sh
#
# Builds scripts/fmath_check in an Alpine container (Rust's std there links
# musl's libm) and prints, per function, how many inputs differ from musl.
# Expected: 0 everywhere except the `libm cosh/cbrt/log2/exp2` rows, which
# fmath does not provide, and on aarch64 about 6 in 100,000 for `port ln` (musl's
# arm64 build fuses a multiply-add; the x86_64 form is what fmath implements).
# Needs Docker. Also: `cargo run --release --bin vectors` inside the container
# prints the (input, musl result) vectors that tests/fmath.rs embeds.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
n="${1:-4000000}"
mkdir -p "$root/scripts/fmath_check/src/fmath"
cp "$root/src/fmath/musl.rs" "$root/src/fmath/tables.rs" "$root/scripts/fmath_check/src/fmath/"
docker run --rm ${PLATFORM:+--platform "$PLATFORM"} -v "$root/scripts/fmath_check:/h" rust:alpine sh -c \
  "apk add --no-cache musl-dev >/dev/null && cd /h && cargo build --release --target-dir /tmp/t 2>&1 | grep -E '^error' -A8; /tmp/t/release/fmath_check $n"
