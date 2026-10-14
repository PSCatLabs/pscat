#!/usr/bin/env python3
"""site_examples.py - put runnable examples into site pages (#252).

    scripts/site_examples.py SITE_DIR [EXAMPLES_DIR]

A page names an example instead of carrying a copy of it:

    <!--@example 01-postfix-->
    <!--@example 10-paths "a stroked triangle beside a filled blue square"-->

EXAMPLES_DIR (default site/learn/) holds NAME.ps, the program, and for an
example that prints, NAME.out, exactly what it prints. Each marker becomes
the program's source, a "Try it" link that opens it in the playground
(playground.html?example=learn/NAME.ps), and the result: the printed output
from NAME.out, or, for a program that draws (it has a %%BoundingBox
comment), its render at assets/learn/NAME.png with the quoted text as the
image's alt text. build_site.sh copies the programs to examples/learn/ and
renders the images before running this, then site_pages.py applies the
shell.

So the page shows exactly the file the playground opens and the tests run:
tests/learn.rs checks every NAME.ps against NAME.out in pscat (and in
Ghostscript when it is installed), and scripts/check_learn_examples.mjs
checks them in the browser build.

The build fails on an unknown example, a drawing example with no alt text
or no render, or an example that no page uses.
"""
import html
import re
import sys
from pathlib import Path

MARKER = re.compile(r'<!--@example\s+([A-Za-z0-9_-]+)(?:\s+"([^"]*)")?\s*-->')
BBOX = re.compile(r"^%%BoundingBox:\s*0\s+0\s+(\d+)\s+(\d+)\s*$", re.M)


def page_size(source):
    """(w, h) from a '%%BoundingBox: 0 0 W H' line, or None for a non-drawing example."""
    m = BBOX.search(source)
    return (int(m.group(1)), int(m.group(2))) if m else None


def render_block(name, alt, examples, site):
    ps = examples / (name + ".ps")
    if not ps.is_file():
        raise ValueError("no example %s" % ps)
    source = ps.read_text()
    code = html.escape(source.rstrip("\n"), quote=False)
    try_link = ('<a class="try" href="playground.html?example=learn/%s.ps">'
                'Try it in the playground</a>' % name)
    size = page_size(source)
    if size:
        if not alt:
            raise ValueError("example %s draws, so its marker needs alt text" % name)
        img = "assets/learn/%s.png" % name
        if not (site / img).is_file():
            raise ValueError("example %s has no render at %s" % (name, img))
        result = ('<div class="result"><span class="label">Result</span>'
                  '<img src="%s" alt="%s" width="%d" height="%d" loading="lazy"></div>'
                  % (img, html.escape(alt), size[0], size[1]))
    else:
        out = examples / (name + ".out")
        if not out.is_file():
            raise ValueError("example %s prints, so it needs %s" % (name, out))
        text = html.escape(out.read_text().rstrip("\n"), quote=False)
        result = ('<div class="result"><span class="label">Output</span>'
                  '<pre class="output"><code>%s</code></pre></div>' % text)
    return ('<figure class="example" id="ex-%s">\n<pre><code>%s</code></pre>\n'
            '<figcaption>%s</figcaption>\n%s\n</figure>' % (name, code, try_link, result))


def expand(text, examples, site, used):
    def repl(m):
        used.add(m.group(1))
        return render_block(m.group(1), m.group(2), examples, site)
    return MARKER.sub(repl, text)


def main(argv):
    if len(argv) not in (2, 3):
        print(__doc__, file=sys.stderr)
        return 2
    site = Path(argv[1])
    default = Path(__file__).resolve().parent.parent / "site" / "learn"
    examples = Path(argv[2]) if len(argv) == 3 else default
    used = set()
    n = 0
    for page in sorted(site.glob("*.html")):
        text = page.read_text()
        new = expand(text, examples, site, used)
        if new != text:
            page.write_text(new)
            n += 1
    unused = sorted(p.stem for p in examples.glob("*.ps") if p.stem not in used)
    if unused:
        raise ValueError("examples no page shows: %s" % ", ".join(unused))
    print("examples placed: %d in %d pages" % (len(used), n))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv))
    except (ValueError, OSError) as e:
        print("site_examples: " + str(e), file=sys.stderr)
        sys.exit(1)
