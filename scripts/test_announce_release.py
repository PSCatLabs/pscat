#!/usr/bin/env python3
"""Tests for announce_release.py: python3 scripts/test_announce_release.py

No network: smtplib and gh are mocked throughout.
"""
import io
import json
import os
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from unittest import mock

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import announce_release as ar  # noqa: E402

DRAFT = "Subject: pscat 0.7.0 is out\nTag: v0.7.0\n\nHello list.\n\nhttps://example.invalid/r\n"


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.log = os.path.join(self.tmp.name, "sub", "log.jsonl")
        self.draft = os.path.join(self.tmp.name, "draft.txt")
        with open(self.draft, "w") as f:
            f.write(DRAFT)
        env = {
            "PSCAT_ANNOUNCE_LOG": self.log,
            "PSCAT_SMTP_USER": "sender@example.invalid",
            "PSCAT_SMTP_PASSWORD": "pw",
        }
        self.env = mock.patch.dict(os.environ, env)
        self.env.start()
        os.environ.pop("PSCAT_ANNOUNCE_TO", None)
        self.smtp = mock.patch.object(ar, "smtp_send")
        self.sent = self.smtp.start()
        self.gh = mock.patch.object(ar, "release_info", return_value={})
        self.rel = self.gh.start()

    def tearDown(self):
        self.gh.stop()
        self.smtp.stop()
        self.env.stop()
        self.tmp.cleanup()

    def run_main(self, *args):
        out = io.StringIO()
        with redirect_stdout(out):
            rc = ar.main(list(args))
        return rc, out.getvalue()


class Draft(unittest.TestCase):
    def test_parse(self):
        h, body = ar.parse_draft(DRAFT)
        self.assertEqual(h["subject"], "pscat 0.7.0 is out")
        self.assertEqual(h["tag"], "v0.7.0")
        self.assertTrue(body.startswith("Hello list."))

    def test_rejects(self):
        for bad in ["Subject: x\nTag: v1\n", "Tag: v1\n\nbody", "Subject: x\n\nbody", "Subject: x\nTag: v1\n\n  \n", "nonsense\n\nbody"]:
            with self.assertRaises(ValueError, msg=bad):
                ar.parse_draft(bad)


class Send(Base):
    def test_dry_run_is_default_and_sends_nothing(self):
        rc, out = self.run_main("send", "--draft", self.draft)
        self.assertEqual(rc, 0)
        self.assertIn("DRY RUN", out)
        self.assertIn("To: pscat-announcements@googlegroups.com", out)
        self.sent.assert_not_called()
        self.assertFalse(os.path.exists(self.log))

    def test_send_needs_confirm(self):
        with self.assertRaises(SystemExit):
            self.run_main("send", "--draft", self.draft, "--send")
        with self.assertRaises(SystemExit):
            self.run_main("send", "--draft", self.draft, "--send", "--confirm", "v0.6.3")
        self.sent.assert_not_called()

    def test_send_logs_and_blocks_duplicate(self):
        rc, _ = self.run_main("send", "--draft", self.draft, "--send", "--confirm", "v0.7.0")
        self.assertEqual(rc, 0)
        self.assertEqual(self.sent.call_count, 1)
        msg, user, pw, host, port = self.sent.call_args.args
        self.assertEqual(msg["To"], ar.DEFAULT_TO)
        self.assertEqual((user, pw, host, port), ("sender@example.invalid", "pw", "smtp.gmail.com", 587))
        entries = ar.read_log(self.log)
        self.assertEqual(len(entries), 1)
        self.assertEqual(entries[0]["tag"], "v0.7.0")
        self.assertNotIn("pw", json.dumps(entries))
        with self.assertRaises(SystemExit):
            self.run_main("send", "--draft", self.draft, "--send", "--confirm", "v0.7.0")
        self.assertEqual(self.sent.call_count, 1)
        self.run_main("send", "--draft", self.draft, "--send", "--confirm", "v0.7.0", "--force")
        self.assertEqual(self.sent.call_count, 2)

    def test_test_send_is_not_logged(self):
        self.run_main("send", "--draft", self.draft, "--test-to", "me@example.invalid", "--send", "--confirm", "v0.7.0")
        self.assertEqual(self.sent.call_args.args[0]["To"], "me@example.invalid")
        self.assertFalse(os.path.exists(self.log))
        # The rehearsal does not block the real send.
        self.run_main("send", "--draft", self.draft, "--send", "--confirm", "v0.7.0")
        self.assertEqual(self.sent.call_count, 2)

    def test_send_checks_tag_is_published(self):
        self.rel.side_effect = SystemExit("no such release")
        with self.assertRaises(SystemExit):
            self.run_main("send", "--draft", self.draft, "--send", "--confirm", "v0.7.0")
        self.sent.assert_not_called()

    def test_recipient_override(self):
        os.environ["PSCAT_ANNOUNCE_TO"] = "env@example.invalid"
        _, out = self.run_main("send", "--draft", self.draft)
        self.assertIn("To: env@example.invalid", out)
        _, out = self.run_main("send", "--draft", self.draft, "--to", "cli@example.invalid")
        self.assertIn("To: cli@example.invalid", out)

    def test_duplicate_is_per_recipient(self):
        ar.append_log(self.log, {"tag": "v0.7.0", "to": "other@example.invalid", "date": "d"})
        self.run_main("send", "--draft", self.draft, "--send", "--confirm", "v0.7.0")
        self.assertEqual(self.sent.call_count, 1)

    def test_missing_password(self):
        os.environ.pop("PSCAT_SMTP_PASSWORD")
        with mock.patch.object(ar.sys, "platform", "linux"):
            with self.assertRaises(SystemExit):
                self.run_main("send", "--draft", self.draft, "--send", "--confirm", "v0.7.0")


class Info(unittest.TestCase):
    def rel(self, **kw):
        r = {"tagName": "v0.7.0-rc.2", "name": "n", "url": "u", "body": "b", "isPrerelease": False, "isDraft": False, "publishedAt": "t"}
        r.update(kw)
        return r

    def test_prerelease_refused_without_flag(self):
        with mock.patch.object(ar, "gh_json", return_value=self.rel(isPrerelease=True)):
            with self.assertRaises(SystemExit):
                ar.release_info("v0.7.0-rc.2")
            self.assertEqual(ar.release_info("v0.7.0-rc.2", allow_prerelease=True)["tagName"], "v0.7.0-rc.2")

    def test_draft_refused(self):
        with mock.patch.object(ar, "gh_json", return_value=self.rel(isDraft=True)):
            with self.assertRaises(SystemExit):
                ar.release_info("v0.7.0", allow_prerelease=True)

    def test_latest_has_no_tag_arg(self):
        with mock.patch.object(ar, "gh_json", return_value=self.rel()) as g:
            ar.release_info()
        self.assertEqual(g.call_args.args[0][:2], ["release", "view"])
        self.assertEqual(g.call_args.args[0][2], "-R")


if __name__ == "__main__":
    unittest.main()
