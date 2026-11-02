# Announcing a release

After a stable release is published, the owner e-mails an announcement to
the **PSCat-announcements** Google Group
(`pscat-announcements@googlegroups.com`). It is manual for now: run
`/announce-release` in Claude Code, which drafts the e-mail from the release
notes, shows it for approval, and sends it with
`scripts/announce_release.py`. Nothing is sent without approval, and a
release is announced to a given address only once.

## One-time setup

1. **Posting rights.** The sending account must be allowed to post to the
   group (Group settings → *Who can post*: managers, or members with the
   sender a member). Otherwise Google holds or bounces the message with no
   SMTP error.
2. **Gmail app password.** SMTP needs 2-Step Verification on the account
   and an app password (Google Account → Security → App passwords). The
   normal account password will not work.
3. **Store it in the keychain** (macOS), so it never sits in a file or in
   shell history:

   ```sh
   export PSCAT_SMTP_USER=you@gmail.com        # e.g. in ~/.zshrc
   security add-generic-password -s pscat-announce -a "$PSCAT_SMTP_USER" -w
   # prompts for the app password
   ```

   Elsewhere (or in CI later), set `PSCAT_SMTP_PASSWORD` instead.

Never commit credentials. The sender address lives only in your
environment.

### Settings

| Variable | Default | Meaning |
| --- | --- | --- |
| `PSCAT_SMTP_USER` | — (required to send) | SMTP login, and the From address |
| `PSCAT_SMTP_PASSWORD` | keychain `pscat-announce` | App password |
| `PSCAT_SMTP_FROM` | `PSCAT_SMTP_USER` | A different From address |
| `PSCAT_SMTP_HOST` / `PSCAT_SMTP_PORT` | `smtp.gmail.com` / `587` | Server (STARTTLS) |
| `PSCAT_ANNOUNCE_TO` | `pscat-announcements@googlegroups.com` | Default recipient |
| `PSCAT_ANNOUNCE_LOG` | `~/Library/Application Support/pscat/announcements.jsonl` (macOS), `~/.local/state/pscat/announcements.jsonl` | Sent log |

## Usage

In Claude Code: `/announce-release` (or "announce v0.7.0"). By hand:

```sh
python3 scripts/announce_release.py info [--tag v0.7.0] [--allow-prerelease]
python3 scripts/announce_release.py send --draft draft.txt                  # dry run
python3 scripts/announce_release.py send --draft draft.txt --test-to me@example.com --send --confirm v0.7.0
python3 scripts/announce_release.py send --draft draft.txt --send --confirm v0.7.0
```

A draft file is `Subject:` and `Tag:` lines, a blank line, and a plain-text
body. `--confirm` must repeat the draft's tag. `info` defaults to the
latest stable release and refuses pre-releases and drafts unless told
otherwise.

## The log

Each real send appends one JSON line (tag, recipient, subject, UTC date,
Message-ID; never credentials) to the log. A second send of the same tag to
the same address is refused unless `--force` is passed. Test sends
(`--test-to`) and dry runs are not logged. The log lives under the home
directory rather than the checkout so that every worktree sees it.

## Tests

`python3 scripts/test_announce_release.py` (no network; SMTP and `gh` are
mocked).
