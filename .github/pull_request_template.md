<!--
Decision: keep this section, at the very top, only when the owner must act on this PR (it touches a
.github/CODEOWNERS path, the reviewer agent failed a visible change twice, or a design choice is the
owner's). Delete it otherwise. Put images inline, pinned to a commit SHA, never a branch:
https://github.com/PSCatLabs/pscat/blob/<sha>/<path>?raw=true
-->
## Decision

- **Look at:** <!-- the one thing to check, in plain words -->
- **Choice:** <!-- merge as is | A or B | ... -->
- **Recommendation:** <!-- what you recommend, and why, in one line -->

## Summary

<!-- What changed and why, in a few bullets. -->

Closes #<!-- issue number -->

<!--
`Closes #N` on its own line, only when this PR completes the issue. For partial or related work
write `Refs #N` instead. GitHub closes the issue on any "fixes #N" or "closes #N", even inside
"does not fix #N" or "partially fixes #N"; the pr-body check (scripts/pr_body_lint.py) fails on those.
-->

## Test plan

- [ ] `PSCAT_PROFILE=ci ./scripts/selftest.sh && cargo test --profile ci && cargo clippy --all-targets -- -D warnings && cargo fmt --all -- --check`
<!-- Anything else you ran or checked. For a change that alters how something looks, put the before and after images here, with the issue's perceptual acceptance check and how the images meet it. -->

## Checklist

- [ ] The docs match the code after this change (the matching page of `docs/guides/`, and a note in `notes/`)
- [ ] I license this contribution as `CONTRIBUTING.md` says (MIT OR Apache-2.0 for code, CC0-1.0 for original creative resources; fonts keep their upstream licence)
- [ ] If this touches a path in `.github/CODEOWNERS`, I know the owner merges it
