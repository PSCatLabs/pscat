#!/usr/bin/env bash
# Push the committed QR checkpoint and create/update its draft review PR.
set -euo pipefail

die() { printf 'publish_qr_pr.sh: %s\n' "$*" >&2; exit 1; }

if [[ "${1:-}" == --help || "${1:-}" == -h ]]; then
  printf '%s\n' 'Usage: bash scripts/publish_qr_pr.sh' \
    'Requires git, gh, network access and GitHub authentication (gh auth login).' \
    'Pushes feature/qr-code-library and creates or updates its draft PR.' \
    'Uses docs/dev/design/QR_CODE_PR.md; does not commit, force-push or merge.'
  exit 0
fi
[[ $# == 0 ]] || die 'unexpected arguments; use --help'
for tool in git gh; do
  command -v "$tool" >/dev/null || die "$tool is not installed"
done

cd "$(dirname "${BASH_SOURCE[0]}")/.."
ROOT=$(git rev-parse --show-toplevel)
cd "$ROOT"
REPO=PSCatLabs/pscat
BRANCH=feature/qr-code-library
BODY=docs/dev/design/QR_CODE_PR.md
TITLE='Native PostScript QR data and correction pipeline'

[[ "$(git branch --show-current)" == "$BRANCH" ]] || die "switch to $BRANCH first"
[[ -z "$(git status --porcelain)" ]] || die 'commit or stash working-tree changes first'
[[ -s "$BODY" ]] || die "missing PR body: $BODY"
# Check the push destination as well as the gh repository before publishing.
case "$(git remote get-url --push --all origin)" in
  https://github.com/PSCatLabs/pscat|https://github.com/PSCatLabs/pscat.git|git@github.com:PSCatLabs/pscat|git@github.com:PSCatLabs/pscat.git|ssh://git@github.com/PSCatLabs/pscat|ssh://git@github.com/PSCatLabs/pscat.git) ;;
  *) die "origin must have a single push URL for github.com/$REPO" ;;
esac

gh auth status --hostname github.com || die 'GitHub auth/network check failed; check connectivity and gh auth status'
# Query before pushing: an API failure must not be mistaken for an absent PR.
PR=$(gh pr list --repo "$REPO" --head "$BRANCH" --base main \
  --state open --limit 100 --json number --jq '.[].number')
if [[ -n "$PR" ]]; then
  [[ "$PR" != *$'\n'* ]] || die 'multiple matching PRs; inspect them with gh pr list'
  [[ "$(gh pr view "$PR" --repo "$REPO" --json isDraft --jq '.isDraft')" == true ]] ||
    die "PR #$PR is already ready for review; inspect it before updating"
fi

git push --set-upstream origin "refs/heads/$BRANCH:refs/heads/$BRANCH"
if [[ -n "$PR" ]]; then
  gh pr edit "$PR" --repo "$REPO" --title "$TITLE" --body-file "$BODY"
else
  gh pr create --repo "$REPO" --draft --base main --head "$BRANCH" \
    --title "$TITLE" --body-file "$BODY"
fi
gh pr view "$BRANCH" --repo "$REPO" --json url,headRefOid,isDraft
printf '%s\n' 'Draft PR published. Opus review handoff: docs/dev/design/QR_CODE_REVIEW.md' \
  "Check CI with: gh pr checks $BRANCH --repo $REPO"
