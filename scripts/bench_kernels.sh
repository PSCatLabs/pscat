#!/usr/bin/env bash
# Benchmark the #742 kernel benches: wall time, median of N runs, for each
# mode. Usage: scripts/bench_kernels.sh [runs] [modes...]
# Modes: plain (hand colorimage loop, 1 core), bands8 (same, --bands 8),
#        kernel (kernelimage), kernel1 (kernelimage, PSCAT_KERNEL_THREADS=1),
#        gpu (kernelimage --gpu-preview).
set -euo pipefail
cd "$(dirname "$0")/.."
RUNS=${1:-5}; shift || true
MODES=${*:-plain bands8}
PSCAT=${PSCAT:-./target/release/pscat}
OUT=$(mktemp -d "$PWD/target/benchk.XXXX")
trap 'rm -rf "$OUT"' EXIT
printf "%-10s" bench; for m in $MODES; do printf "%12s" "$m"; done; echo
for b in benches/kernels/[a-z]*.ps; do
  name=$(basename "$b" .ps); [ "$name" = driver ] && continue
  printf "%-10s" "$name"
  for m in $MODES; do
    args=(--page 480x320 --png "$OUT/$name-$m.png"); env=(); src=$b
    printf '/BenchMode /colorimage def (%s) run\n' "$b" > "$OUT/ci-$name.ps"
    case $m in
      plain)   src=$OUT/ci-$name.ps ;;
      bands8)  args+=(--bands 8); src=$OUT/ci-$name.ps ;;
      kernel)  ;;
      kernel1) env=(PSCAT_KERNEL_THREADS=1) ;;
      gpu)     args+=(--gpu-preview) ;;
    esac
    ts=()
    for _ in $(seq "$RUNS"); do
      s=$(python3 -c 'import time;print(time.time())')
      env "${env[@]+"${env[@]}"}" "$PSCAT" "${args[@]}" "$src" >/dev/null 2>&1 || { echo " FAIL($m)"; continue 2; }
      e=$(python3 -c 'import time;print(time.time())')
      ts+=("$(python3 -c "print($e-$s)")")
    done
    med=$(printf "%s\n" "${ts[@]}" | sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}')
    printf "%11.3fs" "$med"
  done
  echo
done
