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

Next: publish tracking/child issues when authenticated; implement QR-2A standards
version/capacity selection, then QR-2B GF(256)/Reed–Solomon correction/interleaving,
verified against independent codewords. Then fixed-mask matrices and first
decoded URL, before card art. See `docs/dev/design/QR_CODE.md` for the complete
dependency sequence, provisional API, risks and physical-print acceptance.
