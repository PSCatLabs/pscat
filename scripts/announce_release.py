#!/usr/bin/env python3
"""Fetch a pscat release, and send its announcement e-mail to the mailing list.

The /announce-release skill (.claude/skills/announce-release/) drives this;
docs/dev/ANNOUNCING.md has the setup. Standard library only.

  announce_release.py info [--tag vX.Y.Z] [--allow-prerelease]
      Print the release (tag, name, URL, date, notes) as JSON for drafting.
      Without --tag: the latest published release that is not a pre-release.

  announce_release.py send --draft FILE [--to ADDR | --test-to ADDR]
                           [--send --confirm TAG] [--force]
      Send FILE. Its first line is "Subject: ...", then a blank line, then the
      plain-text body; a "Tag: vX.Y.Z" line before the blank line names the
      release. Without --send this is a dry run: it prints the message and
      sends nothing. --send needs --confirm with the tag typed back.
      --test-to sends to a test address and never marks the release announced.

Credentials (never in the repo): PSCAT_SMTP_USER, and the password from
PSCAT_SMTP_PASSWORD or the macOS keychain item
  security add-generic-password -s pscat-announce -a "$PSCAT_SMTP_USER" -w
Optional: PSCAT_SMTP_HOST (smtp.gmail.com), PSCAT_SMTP_PORT (587),
PSCAT_SMTP_FROM (defaults to the user), PSCAT_ANNOUNCE_TO (the default list),
PSCAT_ANNOUNCE_LOG (the sent log).
"""
import argparse
import datetime
import json
import os
import smtplib
import ssl
import subprocess
import sys
from email.message import EmailMessage
from email.utils import formatdate, make_msgid

DEFAULT_TO = "pscat-announcements@googlegroups.com"
KEYCHAIN_SERVICE = "pscat-announce"
REPO = "PSCatLabs/pscat"


def log_path():
    p = os.environ.get("PSCAT_ANNOUNCE_LOG")
    if p:
        return p
    # Under the home directory, not the checkout: the repo is worked from many
    # worktrees, and a per-worktree log would not stop a duplicate.
    if sys.platform == "darwin":
        base = os.path.expanduser("~/Library/Application Support/pscat")
    else:
        base = os.path.join(
            os.environ.get("XDG_STATE_HOME", os.path.expanduser("~/.local/state")), "pscat"
        )
    return os.path.join(base, "announcements.jsonl")


def read_log(path):
    entries = []
    try:
        with open(path, encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if line:
                    entries.append(json.loads(line))
    except FileNotFoundError:
        pass
    return entries


def already_sent(path, tag, to):
    return [e for e in read_log(path) if e.get("tag") == tag and e.get("to", "").lower() == to.lower()]


def append_log(path, entry):
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    with open(path, "a", encoding="utf-8") as f:
        f.write(json.dumps(entry, sort_keys=True) + "\n")


def gh_json(args):
    out = subprocess.run(["gh", *args], check=True, capture_output=True, text=True).stdout
    return json.loads(out)


def release_info(tag=None, allow_prerelease=False, repo=REPO):
    fields = "tagName,name,url,body,isPrerelease,isDraft,publishedAt"
    if tag is None:
        # `gh release view` without a tag is GitHub's "Latest", which is never
        # a pre-release or a draft.
        rel = gh_json(["release", "view", "-R", repo, "--json", fields])
    else:
        rel = gh_json(["release", "view", tag, "-R", repo, "--json", fields])
    if rel.get("isDraft"):
        raise SystemExit(f"{rel['tagName']} is a draft release; publish it first.")
    if rel.get("isPrerelease") and not allow_prerelease:
        raise SystemExit(
            f"{rel['tagName']} is a pre-release; pass --allow-prerelease to announce it."
        )
    return rel


def parse_draft(text):
    """Return (headers, body) from 'Key: value' lines, a blank line, then the body."""
    head, sep, body = text.partition("\n\n")
    if not sep:
        raise ValueError("draft needs header lines, a blank line, then the body")
    headers = {}
    for line in head.splitlines():
        key, colon, value = line.partition(":")
        if not colon or not key.strip():
            raise ValueError(f"bad header line in draft: {line!r}")
        headers[key.strip().lower()] = value.strip()
    if not headers.get("subject"):
        raise ValueError("draft has no Subject: line")
    if not headers.get("tag"):
        raise ValueError("draft has no Tag: line (the release tag, e.g. v0.7.0)")
    if not body.strip():
        raise ValueError("draft body is empty")
    return headers, body.strip() + "\n"


def build_message(sender, to, subject, body):
    msg = EmailMessage()
    msg["From"] = sender
    msg["To"] = to
    msg["Subject"] = subject
    msg["Date"] = formatdate(localtime=True)
    msg["Message-ID"] = make_msgid(domain=sender.rpartition("@")[2] or None)
    msg.set_content(body)
    return msg


def smtp_password(user):
    pw = os.environ.get("PSCAT_SMTP_PASSWORD")
    if pw:
        return pw
    if sys.platform == "darwin":
        r = subprocess.run(
            ["security", "find-generic-password", "-s", KEYCHAIN_SERVICE, "-a", user, "-w"],
            capture_output=True,
            text=True,
        )
        if r.returncode == 0 and r.stdout.strip():
            return r.stdout.strip()
    raise SystemExit(
        "No SMTP password: set PSCAT_SMTP_PASSWORD, or add the keychain item "
        f"(security add-generic-password -s {KEYCHAIN_SERVICE} -a \"$PSCAT_SMTP_USER\" -w)."
    )


def smtp_send(msg, user, password, host, port):
    ctx = ssl.create_default_context()
    with smtplib.SMTP(host, port, timeout=30) as s:
        s.starttls(context=ctx)
        s.login(user, password)
        s.send_message(msg)


def cmd_info(a):
    rel = release_info(a.tag, a.allow_prerelease)
    json.dump(rel, sys.stdout, indent=2)
    print()
    path = log_path()
    sent = [e for e in read_log(path) if e.get("tag") == rel["tagName"]]
    for e in sent:
        print(f"note: {rel['tagName']} was already announced to {e['to']} on {e['date']}", file=sys.stderr)
    return 0


def cmd_send(a):
    with open(a.draft, encoding="utf-8") as f:
        headers, body = parse_draft(f.read())
    tag = headers["tag"]
    test = a.test_to is not None
    to = a.test_to or a.to or os.environ.get("PSCAT_ANNOUNCE_TO") or DEFAULT_TO
    path = log_path()

    if not test:
        prior = already_sent(path, tag, to)
        if prior and not a.force:
            raise SystemExit(
                f"{tag} was already announced to {to} on {prior[-1]['date']} "
                f"(log: {path}); pass --force to send it again."
            )

    user = os.environ.get("PSCAT_SMTP_USER", "")
    sender = os.environ.get("PSCAT_SMTP_FROM") or user or "unset@example.invalid"
    msg = build_message(sender, to, headers["subject"], body)

    if not a.send:
        print(f"DRY RUN: nothing sent. Log: {path}\n")
        for k in ("From", "To", "Subject", "Message-ID"):
            print(f"{k}: {msg[k]}")
        print()
        print(body, end="")
        return 0

    if a.confirm != tag:
        raise SystemExit(f"--send needs --confirm {tag} (the draft's tag typed back).")
    if not user:
        raise SystemExit("Set PSCAT_SMTP_USER to the sending account.")
    # The tag must name a published release: catches a typo that would dodge
    # the duplicate check, and a draft written before the release is out.
    release_info(tag, allow_prerelease=True)
    host = os.environ.get("PSCAT_SMTP_HOST", "smtp.gmail.com")
    port = int(os.environ.get("PSCAT_SMTP_PORT", "587"))
    smtp_send(msg, user, smtp_password(user), host, port)
    print(f"Sent {tag} announcement to {to}.")
    if test:
        print("Test send: not recorded in the log.")
    else:
        append_log(
            path,
            {
                "tag": tag,
                "to": to,
                "subject": headers["subject"],
                "date": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
                "message_id": msg["Message-ID"],
            },
        )
        print(f"Recorded in {path}.")
    return 0


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    i = sub.add_parser("info", help="print the release to announce, as JSON")
    i.add_argument("--tag")
    i.add_argument("--allow-prerelease", action="store_true")
    i.set_defaults(func=cmd_info)
    s = sub.add_parser("send", help="dry-run (default) or send a drafted announcement")
    s.add_argument("--draft", required=True)
    dest = s.add_mutually_exclusive_group()
    dest.add_argument("--to", help=f"recipient (default $PSCAT_ANNOUNCE_TO or {DEFAULT_TO})")
    dest.add_argument("--test-to", help="send to a test address; never logged as announced")
    s.add_argument("--send", action="store_true", help="really send (otherwise a dry run)")
    s.add_argument("--confirm", help="the draft's tag, typed back, required with --send")
    s.add_argument("--force", action="store_true", help="send even if the log says it was sent")
    s.set_defaults(func=cmd_send)
    a = p.parse_args(argv)
    return a.func(a)


if __name__ == "__main__":
    sys.exit(main())
