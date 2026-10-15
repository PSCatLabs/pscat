# 481: a ref must end at a boundary in repo_url.match

`repo_url.match(url, ref="main")` used an optional `/` after the ref, so
`blob/mainline/x.md` matched ref `main` with tail `line/x.md`. The md checker
then resolved it to a wrong repo path (`line/x.md`).

The ref now has to be followed by `/`, `?`, `#` or the end of the URL. Bare
`blob/main` and `tree/main` still match (tail empty), and so do `?query` and
`#fragment` after the ref. Only a longer ref that starts with the given ref is
rejected now.

Site rule (`ref=None`) is unchanged: it takes any ref followed by `/`, so
`blob/mainline/x.md` still gives `x.md` there. That is the same behaviour as
before and is out of scope.

Pinned in `SharedRepoRule` (the mainline row now resolves to external) and in
`SharedRepoRule.test_match_returns_scheme_ref_and_tail`.
