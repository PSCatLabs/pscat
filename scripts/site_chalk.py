#!/usr/bin/env python3
"""site_chalk.py - the `pscat chalk` showcase on the front page (#849).

    scripts/site_chalk.py render BIN OUTDIR [ID ...]   # draw the boards with BIN
    scripts/site_chalk.py expand SITE_DIR              # fill the page marker
    scripts/site_chalk.py commands                     # print each board's command

site/chalk/boards.json is the one list: each board's id, its LaTeX and its
extra `pscat chalk` flags. `render` runs exactly the command the page shows
(minus the shell), so the picture under a command is what the command makes;
build_site.sh calls it with the release binary, into _site/assets/chalk/.
`expand` replaces <!--@chalk-showcase--> in SITE_DIR/*.html with one figure
per board, each carrying its command, and fails if a board has no alt text or
a page names the marker twice. Run `expand` before site_pages.py, which
rejects any marker left behind.

The README's picture is the quadratic board served from the site
(assets/chalk/quadratic.png), so no PNG is committed for it.
"""
import html
import json
import re
import shlex
import subprocess
import sys
from pathlib import Path

BOARDS = Path(__file__).resolve().parent.parent / "site" / "chalk" / "boards.json"
MARKER = "<!--@chalk-showcase-->"
SIZE = (1600, 800)  # the default board at 144 dpi


def load(path=BOARDS):
    boards = json.loads(Path(path).read_text())
    seen = set()
    for b in boards:
        for key in ("id", "title", "alt", "latex", "args"):
            if key not in b:
                raise ValueError("board %r has no %r" % (b.get("id", "?"), key))
        if not re.fullmatch(r"[a-z0-9-]+", b["id"]) or b["id"] in seen:
            raise ValueError("board id %r is not a unique slug" % b["id"])
        seen.add(b["id"])
    return boards


def argv(b, out):
    """The arguments after `pscat`: what the page shows and what render runs."""
    return ["chalk", b["latex"]] + list(b["args"]) + ["-o", out]


def command(b):
    return "pscat " + " ".join(shlex.quote(a) for a in argv(b, b["id"] + ".png"))


def figure(b):
    dark = ' class="chalk-dark"' if b.get("backdrop") == "dark" else ""
    return (
        '    <figure class="chalk-board">\n'
        '      <div%s><img src="assets/chalk/%s.png" width="%d" height="%d" alt="%s" loading="lazy"></div>\n'
        '      <figcaption><b>%s</b></figcaption>\n'
        '      <pre><code>%s</code></pre>\n'
        '    </figure>'
        % (dark, b["id"], SIZE[0], SIZE[1], html.escape(b["alt"]), html.escape(b["title"]),
           html.escape(command(b), quote=False)))


def expand(text, boards):
    n = text.count(MARKER)
    if n > 1:
        raise ValueError("%s appears %d times on one page" % (MARKER, n))
    if n == 0:
        return text
    grid = '<div class="chalk-grid">\n%s\n  </div>' % "\n".join(figure(b) for b in boards)
    return text.replace(MARKER, grid)


def render(binary, outdir, boards, only=()):
    outdir = Path(outdir)
    outdir.mkdir(parents=True, exist_ok=True)
    unknown = set(only) - {b["id"] for b in boards}
    if unknown:
        raise ValueError("no such board: %s" % ", ".join(sorted(unknown)))
    for b in boards:
        if only and b["id"] not in only:
            continue
        out = outdir / (b["id"] + ".png")
        subprocess.run([binary] + argv(b, str(out)), check=True, stdout=subprocess.DEVNULL)


def main(args):
    if not args or args[0] not in ("render", "expand", "commands"):
        print(__doc__, file=sys.stderr)
        return 2
    boards = load()
    if args[0] == "render" and len(args) >= 3:
        render(args[1], args[2], boards, args[3:])
    elif args[0] == "expand" and len(args) == 2:
        placed = 0
        for page in sorted(Path(args[1]).glob("*.html")):
            text = page.read_text()
            new = expand(text, boards)
            if new != text:
                page.write_text(new)
                placed += 1
        if placed != 1:
            raise ValueError("%s is in %d pages, want 1" % (MARKER, placed))
        print("chalk showcase: %d boards" % len(boards))
    elif args[0] == "commands" and len(args) == 1:
        for b in boards:
            print(command(b))
    else:
        print(__doc__, file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except (ValueError, OSError, subprocess.CalledProcessError) as e:
        print("site_chalk: %s" % e, file=sys.stderr)
        sys.exit(1)
