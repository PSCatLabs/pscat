<!--
Only when the owner must act on this PR (it touches a .github/CODEOWNERS path, the reviewer agent
failed a visible change twice, or a design choice is the owner's), copy the block below to the very
top of the body, outside this comment, and fill it in. It is the same Decision block the work-issue
skill writes; the full rules (and why each line is there) are in
.claude/skills/work-issue/references/perceptual-review.md, "The Decision block". The owner reads it
cold, often days later, so it must stand alone: link the issues, explain any jargon, give full URLs
and exact values. Pin images to a commit SHA, never a branch. A PR that changes no output says
"no visible change" instead of a table. A PR that needs nothing beyond the merge says "Nothing to do
but merge" and drops the steps.

## Decision

**Source issue:** #<N> (<what it asks for, in a few words>), part of #<M> (<what the parent is>). Related: #<X> (<what>).

**Why this PR exists.** <the problem, in plain words>

**Merge is yours** (tier c: <the CODEOWNERS paths it touches>). CI is <green | state>. Codex: <rounds and verdict | combined wave review (dispatcher)>.

- **Look at:** <the one thing to check, in plain words>
- **Choice:** <merge as is | A or B | ...>
- **Recommendation:** <what you recommend, and why, in one line; whether the steps below are required or optional>

### What to do (omit when merging as is needs nothing)
1. Open <full URL>.
2. Click **<button>**. In <field> type `<exact value>`.
3. **Do not click <button>:** <what it would do>.
4. A good result looks like <...>; a bad one <...>. If bad, comment on <PR or issue link>.

| before | after |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/<sha>/<path>?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/<sha>/<path>?raw=true) |
-->
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
