#!/usr/bin/env bash
# clean_install_test.sh — prove an installed pscat needs no checkout
# (issue #598). It installs pscat with `cargo install --path` into a
# scratch root, with HOME and CARGO_HOME pointed at empty scratch
# directories, then renders a mathkit, a drychalk and a typewriter
# example from an empty directory outside the checkout. The resolver
# trace (PSCAT_TRACE_RESOURCES, src/paths.rs) must show every lib/ file
# was served by the embedded copy: a pass that found a checkout, the
# crates.io source tree or a stray lib/ on disk is a failure.
#
#   ./scripts/clean_install_test.sh
#   KEEP=1 ./scripts/clean_install_test.sh     # keep the scratch dir
#
# Needs network (a fresh CARGO_HOME downloads the dependencies) and a
# rustup toolchain. CI runs it as the `clean-install` job.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd -P)
EXAMPLES=(mathkit_specimen.ps drychalk_specimen.ps typewriter_specimen.ps)

fail() { echo "FAIL: $*" >&2; exit 1; }

# Packaging check first, with the caller's real environment: `install
# --path` never runs the packager, so an `include`/`exclude` in
# Cargo.toml that dropped lib/ from the crates.io tarball would go
# unnoticed (the build embeds lib/ from the package's own tree).
# (cargo prints `lib\apparel.ps` on Windows, issue #601: normalise to `/`.)
PACKAGED=$(cargo package --list --allow-dirty --manifest-path "$ROOT/Cargo.toml" 2>/dev/null | tr -d '\r' | tr '\\' '/') ||
  fail "cargo package --list failed"
while IFS= read -r f; do
  rel=${f#"$ROOT"/}
  grep -qxF "$rel" <<<"$PACKAGED" || fail "$rel missing from cargo package --list"
done < <(find "$ROOT/lib" -name '*.ps')

SCRATCH=$(mktemp -d "${TMPDIR:-/tmp}/pscat-clean-install.XXXXXX")
SCRATCH=$(cd "$SCRATCH" && pwd -P)
case "$SCRATCH/" in
  "$ROOT"/*) fail "scratch dir $SCRATCH is inside the checkout" ;;
esac
if [[ -z "${KEEP:-}" ]]; then
  trap 'rm -rf "$SCRATCH"' EXIT
else
  echo "keeping $SCRATCH"
fi
mkdir -p "$SCRATCH/home" "$SCRATCH/cargo-home" "$SCRATCH/work"

# rustup finds its toolchains through RUSTUP_HOME (default ~/.rustup):
# pin it before HOME moves, or the cargo proxy finds no toolchain.
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export HOME="$SCRATCH/home"
export CARGO_HOME="$SCRATCH/cargo-home"
unset PSCAT_HOME PSCAT_ROOT PSCAT_LIB XDG_DATA_HOME CARGO_TARGET_DIR

echo "==> cargo install --path into $SCRATCH/root"
cargo install --quiet --locked --path "$ROOT" --root "$SCRATCH/root" \
  --target-dir "$SCRATCH/target" --bin pscat
# Git Bash on Windows (issue #601): the installed binary is pscat.exe.
EXE=""
case "$(uname -s)" in MINGW* | MSYS* | CYGWIN*) EXE=.exe ;; esac
PSCAT="$SCRATCH/root/bin/pscat$EXE"
[[ -x "$PSCAT" ]] || fail "no installed binary at $PSCAT"

for ex in "${EXAMPLES[@]}"; do
  cp "$ROOT/examples/$ex" "$SCRATCH/work/$ex"
done
cd "$SCRATCH/work"
[[ ! -e lib ]] || fail "work dir has a lib/"

for ex in "${EXAMPLES[@]}"; do
  png="${ex%.ps}.png"
  echo "==> rendering $ex"
  if ! TRACE=$(PSCAT_TRACE_RESOURCES=1 "$PSCAT" --png "$png" "$ex" 2>&1 >/dev/null); then
    echo "$TRACE" >&2
    fail "$ex did not render"
  fi
  [[ -s "$png" ]] || fail "$ex wrote no PNG"
  LIB_LINES=$(grep -c '^pscat: resource lib/' <<<"$TRACE" || true)
  [[ "$LIB_LINES" -ge 1 ]] || { echo "$TRACE" >&2; fail "$ex: no resolver trace for lib/ (tracing broken?)"; }
  if grep '^pscat: resource ' <<<"$TRACE" | grep -v ' <- embedded$'; then
    fail "$ex: a resource came from somewhere other than the embedded copy (above)"
  fi
  echo "    $LIB_LINES lib/ files, all embedded; $(wc -c <"$png" | tr -d ' ') bytes"
done

echo "==> clean install OK"
