# 669: CodeQL cache-poisoning alerts on release.yml

CodeQL's `actions/cache-poisoning/poisonable-step` raised 13 alerts (1-12 and
16; the issue title says 11, two more arrived later) on `release.yml`.

## Why it fired

The query (`CachePoisoningViaPoisonableStep.ql`) does not look for a cache
write. It fires when a job that can write the default branch's cache scope
(`push`, `workflow_dispatch`, ...) runs a "poisonable" step (a `run:` that
runs a repo script, `cargo`, ...) after a checkout it believes is untrusted.
It decides that by name: a `ref:` expression whose field is called `ref`,
`head`, `branch`, `sha` or `commit`, unless it is a `github.*` expression.

- Alerts 1-7, 12, 16 (job `verify`): `ref: ${{ inputs.ref }}` from the
  `workflow_dispatch` input.
- Alerts 8-11 (jobs `binaries`, `assemble`): `ref: ${{ needs.verify.outputs.commit }}`;
  the field name `commit` is enough.
- `draft` and `publish` are skipped by the query because they are privileged.

## Was it real?

Not exploitable today: no job in `release.yml` writes a cache (no
`actions/cache`, no `rust-cache`, `setup-node` has no `cache:` input and the
repo root has no `package.json` for its automatic caching). A dispatcher also
needs write access. But the input did let a dispatcher run a branch's scripts
inside the default branch's cache scope, so the first cache anyone added would
have been poisonable. The fix removes that shape instead of dismissing.

## Fix

- Drop the `ref` input and `verify`'s `ref:`: a manual dry run rehearses the
  branch chosen under "Use workflow from".
- `binaries`, `assemble`, `draft` and `publish` check out `${{ github.sha }}`
  instead of `needs.verify.outputs.commit`. `verify` checks out `github.sha`
  (no `ref:`), and now fails if `HEAD` differs from `github.sha^{commit}`, so
  "exactly the commit `verify` checked" still holds. The `commit` output stays
  for the manifest.
- A comment in the header says the file writes no cache and must not gain one.

## Cost

A manual dry run can no longer name a bare commit or tag; push a branch at it.
In exchange the workflow file and the scripts come from the same branch, which
removes the "ref older than the workflow's script options fails" caveat from
RELEASING.md.
