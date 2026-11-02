---
name: announce-release
description: Draft and (only after explicit approval) e-mail a pscat release announcement to the PSCat-announcements Google Group. Use when the user says "/announce-release", "announce the release", "send the release announcement", or "tell the mailing list about vX.Y.Z". Never sends without the user approving the exact subject and body in this conversation.
---

# announce-release — e-mail a release to the mailing list

`scripts/announce_release.py` does the mechanics (fetch the release, dry
run, send over SMTP/TLS, refuse duplicates, log). Your job is the writing
and the approval gate. Setup and credentials: `docs/dev/ANNOUNCING.md`.

**Hard rule: no e-mail leaves without the user's explicit "send it" in this
conversation for this exact draft.** Not even to a test address. A changed
draft needs a fresh approval.

## 1. Fetch the release

```sh
python3 scripts/announce_release.py info                  # latest stable release
python3 scripts/announce_release.py info --tag v0.7.0     # a specific one
```

It refuses pre-releases (`-rc.N`) unless `--allow-prerelease` is passed;
ask the user before announcing a release candidate. A `note:` on stderr
means the tag was already announced: say so and stop unless the user wants
a resend.

The JSON `body` is the CHANGELOG.md section for that version (terse PR
titles). Also read that section in `CHANGELOG.md`, and, where a headline
entry is unclear, `gh pr view <N>` for the PR it cites.

## 2. Draft

Write the draft to a file in the session scratchpad (not the repo):

```
Subject: pscat 0.7.0: <the one or two headline changes>
Tag: v0.7.0

<body>
```

Body, plain text (Google Groups shows it as-is; no Markdown tables or HTML):

- One opening paragraph: what pscat is, in the README's voice — "PostScript
  for precision and imagination", practical documents and creative art —
  and what this release is about.
- **Highlights**: 3–6 user-visible features, each a sentence of plain
  English saying what someone can now do. Translate PR titles; don't paste
  them.
- **Improvements and fixes**: a short list, merged and summarised.
- Leave out CI, dependency bumps and SDLC/process entries unless they
  change what users install.
- Upgrade/install: one line (`cargo install pscat`, the plugin, npm — only
  the channels this release actually ships, per the release notes) and the
  getting-started guide link.
- The release URL from `info`, then a one-line sign-off.

Keep it under ~300 words. No hype words, no emoji. Don't invent features:
every claim must trace to a CHANGELOG entry or PR.

## 3. Review

Show the user the complete subject, recipient and body (print the draft
file verbatim), plus the dry run:

```sh
python3 scripts/announce_release.py send --draft <file>
```

Then ask with AskUserQuestion: send to the list / send to a test address
first / revise. Apply revisions and show the full draft again.

## 4. Send (only after approval)

```sh
# test address (not logged; does not block the real send)
python3 scripts/announce_release.py send --draft <file> --test-to <addr> --send --confirm v0.7.0
# the list
python3 scripts/announce_release.py send --draft <file> --send --confirm v0.7.0
```

If credentials are missing, the script says what to set — point the user
to `docs/dev/ANNOUNCING.md` and have them run any keychain command
themselves (`! security add-generic-password ...`); never ask them to paste
a password into the chat. If the send fails, show the error; don't retry
in a loop. Report the result and the log line it wrote.
