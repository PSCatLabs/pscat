## Summary

<!-- What changed and why, in a few bullets. -->

Closes #<!-- issue number -->

## Test plan

- [ ] `PSCAT_PROFILE=ci ./scripts/selftest.sh && cargo test --profile ci && cargo clippy --all-targets -- -D warnings && cargo fmt --all -- --check`
<!-- Anything else you ran or checked. For a change that alters how something looks, put the before and after images here. -->

## Checklist

- [ ] The docs match the code after this change (the matching page of `docs/guides/`, and a note in `notes/`)
- [ ] I license this contribution as `CONTRIBUTING.md` says (MIT OR Apache-2.0 for code, CC0-1.0 for original creative resources; fonts keep their upstream licence)
- [ ] If this touches a path in `.github/CODEOWNERS`, I know the owner merges it
