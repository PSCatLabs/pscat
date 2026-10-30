## Native QR library, initial slice (issue publication blocked, 2026-10-28)

Owner requested a long-running native PostScript QR feature and procedural
business card. Branch `feature/qr-code-library` starts at main `07f054c`.
No other feature work was merged or copied. Rename this note with the eventual
tracking issue number after publishing the ready-to-file issue queue.

Discovery covered README/AGENTS/contribution/SDLC, architecture/handoff,
library embedding and capability tags, selftests/Rust integration, exporters,
gallery manifest and the separate functional PostScript experiment. Read that
branch's code/docs/example organization and saved offline-WASM browser results;
its independent review remains outstanding. Main's typography/artkit features
are sufficient for an initial card, so no functional-branch dependency is needed.

Implemented `lib/qr.ps`'s internal `qr--byte-data`: explicit version and capacity,
raw readable byte string -> fresh padded data codewords. Mode/count bits,
nibble-aligned payload, four-bit terminator and EC/11 padding use standard
PostScript only. Validations run before a private dictionary opens. There is
no public symbol API, matrix, correction, mask, renderer or working QR yet.

Independent fixtures: 98 cases from pinned Nayuki revision
`3c6d0b3cefb4e049dc337e82237c9644399716a8`, including every ECC at selected
versions 1/2/3/4/9/10/26/27/40, empty/raw binary/capacity-limit/URL inputs.
The generator captures the real encoder's constructor input with explicit
byte mode, fixed version/mask and ECC boosting disabled. Normal tests use
checked-in JSON, no network/packages. No upstream implementation is vendored.

Plan and implementation received a separate same-family second look. It found
two plan gaps (direct frame-depth checks and 255/256 count-width coverage),
implemented before code review. The implementation review found no correctness
blocker and one comment wording nit, fixed. This is not the final independent
cross-family PR review required before merge; no PR or merge was attempted.

Checks already passed: locked offline CI-profile build; all five new QR tests,
including real Ghostscript reference/access parity and embedded loading away
from checkout; two inline selftest blocks; reference regeneration equality;
Clippy all targets with warnings denied; formatting; Markdown links, gallery
manifest and license checks. Full locked offline CI-profile repository suite
passed (exit 0), including all five QR tests and all 38 `tests/selftest.rs`
checks: manifested libraries, discovery consistency and strict rendering drivers.
After the comment-only review correction, the five-test QR suite passed again.

The exact shell selftest wrapper could not acquire Cargo's external target
lock inside the sandbox. Its manifested-library/strict-driver checks also live
in `tests/selftest.rs` and are included in the full suite; record those outcomes
separately rather than claiming the shell wrapper passed.

Infrastructure: the initial full clone ran out of internal disk space, then a
Mac reboot interrupted the session. Recovered the original root checkout and
created the requested branch. Ignored local `target` points to
`/Volumes/YOTUO/pscat-qr-code/target`; build/check/test output (about 1.9 GiB at
inspection) and reference clones are external. No home caches were modified:
the sandbox denies those writes. Largest observed candidate was `~/.cache/uv`
(about 18 GiB). GitHub connector issue creation returned HTTP 403 and CLI auth
reported an invalid token. No issues were created; complete drafts are in
`docs/dev/design/QR_CODE_ISSUES.md`.

Checkpoints: `17be393` (plan/issue queue), `0cbaef1` (tested byte-data slice).
The working tree was clean after both. `git push -u origin
feature/qr-code-library` then failed because the sandbox could not resolve
`github.com`; these commits remain local. No PR was opened. Publish the existing
branch when Git/network authentication is available; do not recreate the work
on main. This final note is a documentation-only follow-up checkpoint.

Next: publish tracking/child issues when authenticated; implement QR-2A standards
version/capacity selection, then QR-2B GF(256)/Reed–Solomon correction/interleaving,
verified against independent codewords. Then fixed-mask matrices and first
decoded URL, before card art. See `docs/dev/design/QR_CODE.md` for the complete
dependency sequence, provisional API, risks and physical-print acceptance.

## Follow-up: executable byte strings

The QR-2A plan review found that the initial packer stored executable strings
under a bare `data` name, so accessing the payload executed it. Real Ghostscript
confirmed an error and leaked dictionary frame for `() cvx`. Store a literal
view with `cvlit` instead; input bytes and the caller's executable attribute
remain unchanged. Added native and Ghostscript regressions for executable
empty and `pop` payloads, including direct dictionary-depth preservation.

## QR-2A: all-version capacity planning

Added readonly per-block ECC/count tables and a raw-module geometry calculation
for all versions 1–40 at L/M/Q/H. `qr--block-spec` describes the layout;
`qr--byte-plan` selects the smallest fitting version and returns layout plus
fresh padded data, keeping requested ECC exact. Guards run before private
frames open; executable level names are canonicalized as literal values.

Pinned reference generator now also emits 160 block specifications and 504
actual automatic plans, including every capacity boundary, zero/binary/count
width cases and per-level oversize failures. Tests compare planned bytes by
SHA-256, not a second version-selection implementation. Original 98 byte
fixtures are unchanged. All ten QR behavioral/reference/Ghostscript tests and
five inline selftest blocks passed. All ten QR tests passed again with the
final four GS overflow assertions. Same-family code review found no blocker;
its optional GS overflow strengthening is included. All-target Clippy and
format/link/gallery/license checks passed at this checkpoint.

CLI publication retry: `gh api` and `gh issue list` cannot connect to GitHub,
and `git push` cannot resolve the host within the sandbox. `git fetch` works
through its preapproved command rule. The attempt to run `gh` outside the
sandbox was rejected because sandbox approvals are disabled. Thus the earlier
`gh auth status` invalid-token message is inconclusive, not evidence requiring
the owner to replace credentials. Issue drafts/publication remain pending.

## QR-2B: Reed–Solomon and block interleaving

Added PostScript-generated GF(256) lookup tables (polynomial 0x11D), generator
polynomials, parity remainders, block splitting and full interleaving. Public
symbol/matrix/drawing API is still absent. Internal correction inputs are
validated before private frames open and stored through literal views; all
outputs are fresh. Existing block-layout facts are reused without another
capacity table. Library data and correction remain ordinary PostScript.

The independent oracle supplies 65,536 field products by hash, all 30 generator
degrees, 91 remainder vectors and full-stream hashes at all 160 version/ECC
layouts. Input pattern changes beyond byte 255, catching truncated indexes;
selected complete hex streams cover single/equal/mixed groups and version 40
under real Ghostscript. Fifteen QR tests and eight inline selftest blocks pass,
including access/executable-attribute/error-frame and output-ownership checks.
Original byte and planning fixtures regenerate unchanged. Same-family code
review found no correctness blocker; its suggested maximum-length remainder
success case is included. Final cross-family PR review remains outstanding and
no merge is authorized.

The exact GitHub URL additionally has independent complete-stream fixtures at
L/M/Q/H, checking automatic planning -> byte packing -> correction/interleaving
under both PSCat and Ghostscript. A byte stream is still not a scannable QR;
matrix construction and masking remain the next stages.

Final validation for this continuation: locked offline full `cargo test
--profile ci` passed (exit 0), including all 38 selftest/manifest/strict-driver
checks. The final targeted QR rerun passed all 15 tests after the extra boundary
and GitHub pipeline cases. All-target Clippy with warnings denied, formatting,
all three pinned fixture regenerations, eight inline blocks, links, gallery
and licenses passed. Build/test/cache output remains on the external drive
(about 2.3 GiB at inspection). No `src/` or Cargo dependency changes occurred.
Checkpoints preceding this correction commit: `8e73992` executable-string fix,
`ac638e2` capacity/version planning. Next: QR-3 function/reservation patterns and
fixed-mask data placement, followed by metadata/mask selection and decoding.

## Opus review preparation

Owner explicitly requested git/gh publication and a PR suitable for Opus.
Prepared `docs/dev/design/QR_CODE_REVIEW.md` as the scoped review entry point,
`QR_CODE_API.md` for implemented contracts/guards/effects, `QR_CODE_TESTING.md`
for reproducible checks/oracle provenance/coverage and limitations, and
`QR_CODE_PR.md` as the exact draft PR body. Linked them from the design index,
contributor index and handoff. No library, Rust, fixture or dependency change
is part of this preparation. No Opus review is claimed as already completed.

The PR explicitly covers QR-1/QR-2A/QR-2B and stays draft: matrix, metadata,
masking, rendering, actual decoding and card artwork remain pending. Base is
`07f054c`; implementation head is `deba699`. Publishing commands use direct
`git push` and `gh pr create --body-file`; no fake issue numbers or PR URL are
inserted. Initial CLI retry still cannot reach GitHub from this sandbox.

## Owner-run publication script

Added `scripts/publish_qr_pr.sh` at the owner's request to finish publication
from their network-enabled terminal. It checks the clean feature branch,
expected origin and GitHub authentication, then pushes and creates or updates
the draft PR using the prepared body. It refuses an existing non-draft PR,
never force-pushes or merges, and can resume after a partial failure. Updated
the review handoff with the single command. No QR implementation changes.
Validation: Bash syntax/help, six mocked CLI paths (create, update, dirty tree,
API failure, push failure and existing non-draft), Markdown links, license
checks and diff whitespace passed. No live publication was attempted by this
script in the restricted session.
