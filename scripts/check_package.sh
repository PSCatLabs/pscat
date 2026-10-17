#!/usr/bin/env bash
# check_package.sh — the crates.io acceptance checks for issue #565, run
# against the crate exactly as it would be uploaded, not the checkout:
#
#   1. `cargo package` builds pscat-<ver>.crate; it must be under crates.io's
#      10 MB limit.
#   2. The .crate is unpacked into a scratch directory outside the checkout
#      (no git, no sibling files to lean on) and `cargo publish --dry-run`
#      runs there.
#   3. A scratch downstream project depends on that tree with
#      `default-features = false`:
#      - `cargo tree` must not find winit or softbuffer in its graph (and must
#        find winit once `pscat/window` is turned back on, so a broken check
#        cannot pass);
#      - it renders a PostScript program to PNG with only the documented API
#        (Interp, Page) and the embedded lib/, and checks the result.
#   4. `cargo install --path <tree>` with the default features, then the
#      installed pscat renders a lib/-using program (SKIP_INSTALL=1 skips it).
#
# Nothing is published: the only publish here is `--dry-run`. The first real
# publish is a manual owner step (issue #602).
#
#   ./scripts/check_package.sh
#   KEEP=1 ./scripts/check_package.sh          # keep the scratch dir
#   SKIP_INSTALL=1 ./scripts/check_package.sh  # skip step 4
#
# Works under Git Bash on Windows: every path cargo reads from a Cargo.toml
# is relative, since cargo.exe cannot resolve an MSYS `/tmp/...` path. CI runs
# it as the `crate` job on Linux, macOS and Windows.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd -P)
LIMIT=$((10 * 1024 * 1024))

fail() { echo "FAIL: $*" >&2; exit 1; }

VERSION=$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' "$ROOT/Cargo.toml" | head -1)
[[ -n "$VERSION" ]] || fail "no version in Cargo.toml"
NAME="pscat-$VERSION"

echo "==> cargo package ($NAME)"
# --no-verify: the dry run below verifies, from the unpacked tree.
cargo package --quiet --locked --no-verify --allow-dirty --manifest-path "$ROOT/Cargo.toml"
CRATE="$ROOT/target/package/$NAME.crate"
[[ -f "$CRATE" ]] || fail "no $CRATE"
SIZE=$(wc -c <"$CRATE" | tr -d ' ')
echo "    $NAME.crate is $SIZE bytes ($((SIZE / 1024)) KiB; limit $((LIMIT / 1024)) KiB)"
[[ "$SIZE" -lt "$LIMIT" ]] || fail "$NAME.crate is over crates.io's 10 MB limit"

BASE="${TMPDIR:-/tmp}"
SCRATCH=$(mktemp -d "$BASE/pscat-package.XXXXXX")
SCRATCH=$(cd "$SCRATCH" && pwd -P)
case "$SCRATCH/" in
  "$ROOT"/*) fail "scratch dir $SCRATCH is inside the checkout" ;;
esac
if [[ -z "${KEEP:-}" ]]; then
  trap 'rm -rf "$SCRATCH"' EXIT
else
  echo "keeping $SCRATCH"
fi
mkdir -p "$SCRATCH/pkg" "$SCRATCH/down/src"
cp "$CRATE" "$SCRATCH/pkg/"
(cd "$SCRATCH/pkg" && tar -xzf "$NAME.crate")
[[ -f "$SCRATCH/pkg/$NAME/Cargo.toml" ]] || fail "the .crate did not unpack to $NAME/"

echo "==> cargo publish --dry-run in the unpacked tree"
(cd "$SCRATCH/pkg/$NAME" && cargo publish --dry-run --locked --allow-dirty)

echo "==> downstream project, default-features = false"
cd "$SCRATCH/down"
cat >Cargo.toml <<EOF
[package]
name = "pscat-downstream-check"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
pscat = { path = "../pkg/$NAME", default-features = false }

[workspace]
EOF
# Start from pscat's own lock file, so the check resolves the versions the
# crate was tested with rather than whatever is newest today.
cp "../pkg/$NAME/Cargo.lock" Cargo.lock
cat >src/main.rs <<'EOF'
//! Renders PostScript to PNG with only pscat's documented API.
use pscat::{Interp, Page};

fn main() {
    let mut interp = Interp::with_page(200, 120).expect("valid page size");
    let program = "
        (lib/artkit.ps) run
        0.85 0.2 0.1 setrgbcolor 10 10 180 50 rectfill
        0 setgray /Helvetica findfont 28 scalefont setfont
        20 80 moveto (pscat) show
        showpage";
    if let Err(e) = interp.run_str(program) {
        panic!("{}", interp.error_report(&e));
    }
    let pages: Vec<Page> = interp.pages();
    assert_eq!(pages.len(), 1, "one showpage, one page");
    let page = &pages[0];
    assert_eq!((page.width(), page.height()), (200, 120));
    let rgba = page.rgba8();
    let inked = rgba
        .chunks(4)
        .filter(|p| p[0] < 128 && p[1] < 128 && p[2] < 128)
        .count();
    let red = rgba
        .chunks(4)
        .filter(|p| p[0] > 180 && p[1] < 100 && p[2] < 80)
        .count();
    assert!(inked > 50, "the text drew ({inked} dark pixels)");
    assert!(red > 8000, "the rectangle drew ({red} red pixels)");
    let png = page.to_png().expect("PNG encoding");
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    page.save_png("out.png").expect("write out.png");
    println!("rendered {}x{} page, {} PNG bytes", page.width(), page.height(), png.len());
}
EOF

for dep in winit softbuffer; do
  if cargo tree -e normal --target all -i "$dep" >/dev/null 2>&1; then
    cargo tree -e normal --target all -i "$dep" >&2 || true
    fail "$dep is in a default-features = false dependency graph"
  fi
done
cargo tree -e normal --target all --features pscat/window -i winit >/dev/null 2>&1 ||
  fail "winit is missing with pscat/window on: the cargo tree check above proves nothing"
echo "    no winit/softbuffer without the window feature (and winit present with it)"

cargo run --quiet --target-dir ../down-target
[[ -s out.png ]] || fail "downstream render wrote no out.png"

if [[ -n "${SKIP_INSTALL:-}" ]]; then
  echo "==> skipping cargo install (SKIP_INSTALL)"
else
  echo "==> cargo install --path <unpacked tree>, default features"
  cd "$SCRATCH"
  cargo install --quiet --locked --path "pkg/$NAME" --root inst --target-dir install-target
  EXE=""
  case "$(uname -s)" in MINGW* | MSYS* | CYGWIN*) EXE=.exe ;; esac
  for bin in pscat pscat-mcp; do
    [[ -x "inst/bin/$bin$EXE" ]] || fail "no installed $bin$EXE"
  done
  "inst/bin/pscat$EXE" --png installed.png \
    -e '(lib/artkit.ps) run 0 0 1 setrgbcolor 72 72 200 100 rectfill showpage'
  [[ -s installed.png ]] || fail "installed pscat wrote no PNG"
fi

echo "OK: $NAME packages ($SIZE bytes), dry-runs from its own tree, and builds headless downstream"
