# Review log: visible changes merged without the owner (#674)

A pull request that changes how something renders is not held for the owner mid-cycle (`SDLC.md`,
"Who merges", tier (b)). An independent reviewer agent with no implementation context compares the
before and after images against the issue's perceptual acceptance check and
[`TASTE.md`](../TASTE.md). On a pass the agent merges and adds an entry here. The owner reads the
whole log once, before the next release.

## Layout

- `pending/` holds the entries merged since the last release, one file per issue:
  `pending/<issue>-<slug>.md`. One file per issue means two agents never edit the same file, so
  parallel PRs never conflict here (the `notes/` rule, #273).
- Images that are not already committed assets sit beside the entry in
  `pending/<issue>-<slug>/` (`<name>-before.png`, `<name>-after.png`: small crops).
- `v<X.Y.Z>/` holds the entries the owner read before that release.

## Entry template

```markdown
## <PR title> (PR #<PR>, issue #<issue>, YYYY-MM-DD)

- **What changed / what to look at:** <one line>
- **Perceptual check:** <the issue's "Perceptual acceptance check", quoted>
- **Reviewer:** PASS, round <R> (<link to the PR's "Perceptual review" comment>)

| before | after |
|---|---|
| ![before](https://github.com/PSCatLabs/pscat/blob/<sha>/<path>?raw=true) | ![after](https://github.com/PSCatLabs/pscat/blob/<sha>/<path>?raw=true) |
```

Pin every image URL to a commit SHA, never to a branch: the PR branch is deleted on merge, and a
squash-merged PR's commits stay reachable by SHA. The full procedure is
`.claude/skills/work-issue/references/perceptual-review.md`.

## At release time

1. The owner reads every entry in `pending/` in one sitting (GitHub renders the images inline).
2. A rejected entry becomes a revert PR or a fix ticket. The owner says which; the dispatcher files
   it. Nothing is filed that asks the owner to do physical work.
3. Whoever opens the release PR (`scripts/release.sh`) adds a commit that moves `pending/*` to
   `v<X.Y.Z>/` with `git mv`, leaving `pending/` empty except for its `.gitkeep`. The release
   checklist ([`RELEASING.md`](../RELEASING.md)) has the step.
