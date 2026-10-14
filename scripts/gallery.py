#!/usr/bin/env python3
"""gallery.py - the gallery manifest: check it, and generate from it (#251).

    scripts/gallery.py check            validate gallery/manifest.json against the repo
    scripts/gallery.py sync             rewrite the generated parts of README.md and
                                        gallery/README.md from the manifest
    scripts/gallery.py build OUT        write gallery.html and one gallery-<slug>.html per
                                        piece into OUT (scripts/build_site.sh calls this)
    scripts/gallery.py stage OUT        copy the renders, reference photos and playground
                                        sources the pages use into OUT
    scripts/gallery.py renders          list the specimens build_site.sh must render
                                        ("source WxH id", one per line)
    scripts/gallery.py list             list the pieces for gallery/show.sh
                                        ("id<TAB>page<TAB>speed", in slideshow order)

gallery/manifest.json is the one list of what the gallery shows. A new piece adds
an entry there (and its .ps and render); nobody edits gallery.html by hand, and
there is no gallery.html in site/ any more. `check` fails when:

  * a tracked gallery/*.ps has no entry, or an entry's source, render or reference
    is missing, or a render in gallery/renders/ belongs to no entry;
  * an entry's page size disagrees with its %%BoundingBox, or its `libraries`
    differ from the (lib/x.ps) files the source loads;
  * an entry marked `playground` is not self-contained (the browser build has no
    filesystem), or two playground sources share a file name;
  * the text uses stage-era or issue-tracker wording a newcomer cannot follow;
  * the generated parts of README.md or gallery/README.md are out of date
    (run `scripts/gallery.py sync`).

Standard library only, so CI needs nothing installed. The generated pages carry
the <!--@head-->, <!--@header gallery--> and <!--@footer--> markers, so
scripts/site_pages.py (the last step of build_site.sh) gives them the shared shell.
"""
import html
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "gallery" / "manifest.json"
REPO_URL = "https://github.com/jeromebanks/postscript_interpreter"
SITE_URL = "https://jeromebanks.github.io/postscript_interpreter"
SOURCE_LINES = 40          # how much of the source a piece page shows inline

HIGHLIGHTS = ("README.md", "<!-- gallery:highlights:start -->", "<!-- gallery:highlights:end -->")
TABLES = ("gallery/README.md", "<!-- gallery:table:start -->", "<!-- gallery:table:end -->")

PIECE_FIELDS = ("id", "title", "category", "tags", "libraries", "featured", "playground",
                "added", "source", "render", "page", "speed", "alt", "blurb", "technique")
SPECIMEN_FIELDS = ("id", "group", "title", "alt", "blurb")
# wording that means nothing to a newcomer: the roadmap's stage numbers and issue refs
JARGON = re.compile(r"\bStages? \d+\b|\bGallery II\b|\bissues? #\d+|\(#\d+\)")


def slug(entry_id):
    return entry_id.replace("_", "-")


def piece_page(entry_id):
    return "gallery-%s.html" % slug(entry_id)


def load(path=MANIFEST):
    return json.loads(Path(path).read_text(encoding="utf-8"))


# ---------------------------------------------------------------- text helpers

def inline(text):
    """Escape text, then turn `code` and [label](https://url) into HTML."""
    t = html.escape(text, quote=False)
    t = re.sub(r"`([^`]+)`", r"<code>\1</code>", t)
    return re.sub(r"\[([^\]]+)\]\((https?://[^)\s]+)\)", r'<a href="\2">\1</a>', t)


def attr(text):
    return html.escape(text, quote=True)


def blob(path):
    kind = "tree" if (ROOT / path).is_dir() else "blob"
    return "%s/%s/main/%s" % (REPO_URL, kind, path)


def size(page):
    w, h = page.split("x")
    return int(w), int(h)


def categories(m):
    """[(category dict, [pieces])] in manifest order, skipping empty categories."""
    out = []
    for c in m["categories"]:
        ps = [p for p in m["pieces"] if p["category"] == c["id"]]
        if ps:
            out.append((c, ps))
    return out


# ---------------------------------------------------------------- the check

def tracked(pattern):
    """Tracked files matching a git pathspec (untracked work in progress is none of our business)."""
    try:
        out = subprocess.run(["git", "-C", str(ROOT), "ls-files", "--", pattern],
                             capture_output=True, text=True, check=True).stdout
        return sorted(out.splitlines())
    except (OSError, subprocess.CalledProcessError):
        base, _, glob = pattern.rpartition("/")
        return sorted("%s/%s" % (base, f.name) for f in (ROOT / base).glob(glob))


def loaded_libraries(source):
    return sorted(set(re.findall(r"\(lib/([a-z0-9_]+)\.ps\)", source)))


def self_contained(source):
    """True if the program reads no file: the browser build has no filesystem."""
    return not re.search(r"\([^)]*\.ps\)\s*run\b|\(lib/|\(fonts/", source)


def check(m):
    errs = []
    add = errs.append
    cat_ids = [c["id"] for c in m["categories"]]
    lib_ids = set(m["libraries"])
    group_ids = [g["id"] for g in m["groups"]]

    for lid, lib in m["libraries"].items():
        if not (ROOT / lib["guide"]).is_file():
            add("library %s: guide %s does not exist" % (lid, lib["guide"]))
    for g in m["groups"]:
        if not (ROOT / g["guide"]).is_file():
            add("group %s: guide %s does not exist" % (g["id"], g["guide"]))

    ids = [p["id"] for p in m["pieces"]] + [s["id"] for s in m["specimens"]]
    for i in sorted({i for i in ids if ids.count(i) > 1}):
        add("id %r is used twice (ids name the render and the page)" % i)

    # art: every tracked gallery/*.ps has exactly one entry
    sources = {p["source"] for p in m["pieces"]}
    for f in tracked("gallery/*.ps"):
        if f.count("/") == 1 and f not in sources:
            add("%s has no entry in gallery/manifest.json (add one: see gallery/README.md)" % f)
    renders = {p["render"] for p in m["pieces"]}
    for f in tracked("gallery/renders/*.png"):
        if f not in renders:
            add("%s is a render that no manifest entry uses" % f)

    flagged = {}
    for p in m["pieces"]:
        who = "piece %s" % p["id"]
        for k in PIECE_FIELDS:
            if k not in p or p[k] in ("", None):
                if k not in ("tags", "libraries", "featured", "playground"):
                    add("%s: missing %s" % (who, k))
        if p.get("category") not in cat_ids:
            add("%s: unknown category %r (have: %s)" % (who, p.get("category"), ", ".join(cat_ids)))
        for lib in p.get("libraries", []):
            if lib not in lib_ids:
                add("%s: library %r is not in the manifest's libraries" % (who, lib))
        for k in ("source", "render"):
            if p.get(k) and not (ROOT / p[k]).is_file():
                add("%s: %s %s does not exist" % (who, k, p[k]))
        ref = p.get("reference")
        if ref:
            for k in ("file", "alt", "caption", "render_caption", "text"):
                if not ref.get(k):
                    add("%s: reference is missing %s" % (who, k))
            if ref.get("file") and not (ROOT / ref["file"]).is_file():
                add("%s: reference %s does not exist" % (who, ref["file"]))
        if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", str(p.get("added", ""))):
            add("%s: added must be a YYYY-MM-DD date" % who)
        if not re.fullmatch(r"\d+x\d+", str(p.get("page", ""))):
            add("%s: page must look like 700x700" % who)
        if not isinstance(p.get("speed"), int) or p["speed"] < 1:
            add("%s: speed must be a positive integer (interpreter steps per frame)" % who)
        src = ROOT / p.get("source", "")
        if src.is_file():
            text = src.read_text(encoding="utf-8", errors="replace")
            bb = re.search(r"%%BoundingBox:\s*0\s+0\s+(\d+)\s+(\d+)", text)
            if bb and p.get("page") and p["page"] != "%sx%s" % bb.groups():
                add("%s: page %s disagrees with the source's %%%%BoundingBox %sx%s"
                    % (who, p["page"], *bb.groups()))
            want = loaded_libraries(text)
            if sorted(p.get("libraries", [])) != want:
                add("%s: libraries should be %s (what the source loads), not %s"
                    % (who, want, sorted(p.get("libraries", []))))
            if p.get("playground"):
                if not self_contained(text):
                    add("%s: marked playground but the source reads a file, and the "
                        "browser build has no filesystem" % who)
                flagged.setdefault(Path(p["source"]).name, []).append(p["id"])
        for k in ("alt", "blurb", "technique"):
            if JARGON.search(p.get(k, "")):
                add("%s: %s uses stage or issue wording a newcomer cannot follow: %r"
                    % (who, k, JARGON.search(p[k]).group(0)))

    for s in m["specimens"]:
        who = "specimen %s" % s["id"]
        for k in SPECIMEN_FIELDS:
            if not s.get(k):
                add("%s: missing %s" % (who, k))
        if s.get("group") not in group_ids:
            add("%s: unknown group %r" % (who, s.get("group")))
        has_render = bool(s.get("render"))
        if not has_render and not (s.get("source") and re.fullmatch(r"\d+x\d+", s.get("page", ""))):
            add("%s: needs a committed render, or a source and a page size to render at build time" % who)
        for k in ("render", "source"):
            if s.get(k) and not (ROOT / s[k]).exists():
                add("%s: %s %s does not exist" % (who, k, s[k]))
        if s.get("playground") and s.get("source"):
            f = ROOT / s["source"]
            if f.is_file() and not self_contained(f.read_text(encoding="utf-8", errors="replace")):
                add("%s: marked playground but the source reads a file" % who)
            flagged.setdefault(Path(s["source"]).name, []).append(s["id"])
        for k in ("alt", "blurb"):
            if JARGON.search(s.get(k, "")):
                add("%s: %s uses stage or issue wording: %r" % (who, k, JARGON.search(s[k]).group(0)))
    for name, owners in flagged.items():
        if len(owners) > 1:
            add("playground sources named %s collide in _site/examples/: %s" % (name, ", ".join(owners)))

    for c in m["categories"]:
        if JARGON.search(c["blurb"]):
            add("category %s: stage or issue wording" % c["id"])
    for g in m["groups"]:
        if JARGON.search(g["blurb"]):
            add("group %s: stage or issue wording" % g["id"])

    for path, new in generated_files(m).items():
        if not (ROOT / path).is_file() or (ROOT / path).read_text(encoding="utf-8") != new:
            add("%s is out of date with the manifest: run scripts/gallery.py sync" % path)
    return errs


# ---------------------------------------------------------------- README parts

def highlights_md(m):
    feat = [p for p in m["pieces"] if p["featured"]]
    rows = [feat[i:i + 3] for i in range(0, len(feat), 3)]
    lines = []
    for row in rows:
        lines.append("<p>")
        for p in row:
            lines.append('  <a href="%s/%s"><img src="%s" width="32%%" alt="%s"/></a>'
                         % (SITE_URL, piece_page(p["id"]), p["render"], attr(p["alt"])))
        lines.append("</p>")
    return "\n".join(lines)


def table_md(m):
    out = []
    for c, ps in categories(m):
        out.append("### %s\n\n%s\n" % (c["title"], c["blurb"]))
        out.append("| Piece | File | How it is made |\n|---|---|---|")
        for p in ps:
            f = Path(p["source"]).name
            out.append("| [%s](%s/%s) | [`%s`](%s) | %s |" % (
                p["title"], SITE_URL, piece_page(p["id"]), f, f, p["technique"].replace("|", "\\|")))
        out.append("")
    return "\n".join(out).rstrip("\n")


def splice(text, start, end, body, name):
    a, b = text.find(start), text.find(end)
    if a < 0 or b < a:
        raise ValueError("%s: missing the %s ... %s markers" % (name, start, end))
    return text[:a + len(start)] + "\n" + body + "\n" + text[b:]


def generated_files(m):
    """{repo path: the text it should have}, for every file with a generated region."""
    out = {}
    for path, start, end in (HIGHLIGHTS, TABLES):
        p = ROOT / path
        if not p.is_file():
            continue
        body = highlights_md(m) if path == HIGHLIGHTS[0] else table_md(m)
        try:
            out[path] = splice(p.read_text(encoding="utf-8"), start, end, body, path)
        except ValueError:
            out[path] = ""  # reported as out of date; sync explains the markers
    return out


def sync(m):
    for path, new in generated_files(m).items():
        if not new:
            raise ValueError("%s has no generated-region markers" % path)
        (ROOT / path).write_text(new, encoding="utf-8")
        print("updated " + path)


# ---------------------------------------------------------------- pages

def jump_bar(label, items):
    links = " ".join('<a href="#%s">%s <span>%s</span></a>' % (i, html.escape(t), n) for i, t, n in items)
    return '<p class="jump-label">%s</p>\n  <nav class="jump" aria-label="%s">%s</nav>' % (
        html.escape(label), attr(label), links)


def lib_chips(m):
    counts = {}
    plain = 0
    for p in m["pieces"]:
        for lib in p["libraries"]:
            counts[lib] = counts.get(lib, 0) + 1
        plain += not p["libraries"]
    # a one-piece library is no filter: its page lists it already
    chips = sorted(((k, n) for k, n in counts.items() if n >= 2), key=lambda kv: (-kv[1], kv[0]))
    return chips, plain


def art_card(p):
    href = piece_page(p["id"])
    w, h = size(p["page"])
    libs = " ".join(p["libraries"]) or "plain"
    return ('    <div class="card" data-libs="%s"><a href="%s"><img src="assets/renders/%s.png" '
            'width="%d" height="%d" alt="%s" loading="lazy"></a>\n'
            '      <div class="body"><b><a href="%s">%s</a></b><span>%s</span></div></div>'
            % (libs, href, p["id"], w, h, attr(p["alt"]), href, html.escape(p["title"]), inline(p["blurb"])))


def specimen_card(s):
    links = []
    if s.get("source"):
        links.append('<a href="%s">source</a>' % blob(s["source"]))
    if s.get("playground"):
        links.append('<a href="playground.html?example=%s">open in playground</a>'
                     % Path(s["source"]).name)
    meta = '\n      <div class="meta">%s</div>' % " &middot; ".join(links) if links else ""
    img = 'assets/renders/%s.png' % s["id"]
    return ('    <div class="card"><a href="%s"><img src="%s" alt="%s" loading="lazy"></a>\n'
            '      <div class="body"><b>%s</b><span>%s</span></div>%s</div>'
            % (img, img, attr(s["alt"]), html.escape(s["title"]), inline(s["blurb"]), meta))


FILTER_JS = """<script>
// Filter the art by library. Without JavaScript the filter bar stays hidden
// and every piece shows.
(function () {
  var bar = document.getElementById('filter');
  if (!bar) return;
  bar.hidden = false;
  var buttons = bar.querySelectorAll('button');
  function apply(lib) {
    buttons.forEach(function (b) { b.setAttribute('aria-pressed', String(b.dataset.lib === lib)); });
    document.querySelectorAll('#art .card').forEach(function (c) {
      var libs = c.dataset.libs.split(' ');
      c.hidden = !(lib === '' || libs.indexOf(lib) >= 0);
    });
    document.querySelectorAll('#art section').forEach(function (s) {
      s.hidden = !s.querySelector('.card:not([hidden])');
    });
  }
  buttons.forEach(function (b) {
    b.addEventListener('click', function () { apply(b.dataset.lib); });
  });
  apply('');
})();
</script>"""


def gallery_html(m):
    cats = categories(m)
    chips, plain = lib_chips(m)
    n_art = len(m["pieces"])
    n_spec = len(m["specimens"])
    out = ['<!DOCTYPE html>\n<html lang="en">\n<head>\n<!--@head-->\n<title>pscat — gallery</title>\n'
           '<meta name="description" content="%d finished PostScript pieces and %d library '
           'specimens, all drawn by pscat, each with its source.">\n</head>\n<body>\n'
           '<!--@header gallery-->\n<main class="wide">' % (n_art, n_spec)]
    out.append('  <h1>Gallery</h1>\n'
               '  <p class="sub">Everything here is plain PostScript, drawn by pscat. The art comes first: '
               '%d finished pieces, each with its own page showing how it is made and the source. '
               'Below it are specimens and examples, the demo sheets for each library. Every picture is '
               'deterministic: run the same file and you get the same pixels. Many run live in the '
               '<a href="playground.html">playground</a>.</p>' % n_art)
    out.append("  " + jump_bar("Art", [(c["id"], c["title"], len(ps)) for c, ps in cats]
                              + [("specimens", "Specimens and examples", n_spec)]))
    out.append('  <details class="filter" id="filter" hidden>\n    <summary>Filter the art by library</summary>\n'
               '    <div class="chips">\n      <button type="button" data-lib="" aria-pressed="true">everything</button>')
    for lib, n in chips:
        out.append('      <button type="button" data-lib="%s" aria-pressed="false">%s <span>%d</span></button>'
                   % (attr(lib), html.escape(m["libraries"][lib]["title"]), n))
    if plain:
        out.append('      <button type="button" data-lib="plain" aria-pressed="false">no library <span>%d</span></button>' % plain)
    out.append("    </div>\n  </details>")
    out.append('  <div id="art">')
    for c, ps in cats:
        out.append('  <section id="%s">\n  <h2>%s</h2>\n  <p>%s</p>\n  <div class="cards">'
                   % (c["id"], html.escape(c["title"]), inline(c["blurb"])))
        out.extend(art_card(p) for p in ps)
        out.append("  </div>\n  </section>")
    out.append("  </div>")
    out.append('  <h2 id="specimens">Specimens and examples</h2>\n'
               '  <p>These are the library demo sheets and sample programs. Each group links to the guide '
               'that explains the library behind it.</p>')
    out.append("  " + jump_bar("Groups", [("group-" + g["id"], g["title"],
                                          sum(s["group"] == g["id"] for s in m["specimens"]))
                                         for g in m["groups"]]))
    for g in m["groups"]:
        ss = [s for s in m["specimens"] if s["group"] == g["id"]]
        out.append('  <section id="group-%s">\n  <h3>%s</h3>\n  <p>%s <a href="%s">Read the guide</a>.</p>\n  <div class="cards">'
                   % (g["id"], html.escape(g["title"]), inline(g["blurb"]), blob(g["guide"])))
        out.extend(specimen_card(s) for s in ss)
        out.append("  </div>\n  </section>")
    out.append("</main>\n<!--@footer-->\n" + FILTER_JS + "\n</body>\n</html>\n")
    return "\n".join(out)


def source_head(p):
    text = (ROOT / p["source"]).read_text(encoding="utf-8", errors="replace")
    lines = text.splitlines()
    return "\n".join(lines[:SOURCE_LINES]), len(lines)


def piece_html(m, p, prev, nxt):
    cat = next(c for c in m["categories"] if c["id"] == p["category"])
    w, h = size(p["page"])
    render = "assets/renders/%s.png" % p["id"]
    head, total = source_head(p)
    o = []
    o.append('<!DOCTYPE html>\n<html lang="en">\n<head>\n<!--@head-->\n<title>%s — pscat gallery</title>\n'
             '<meta name="description" content="%s">\n</head>\n<body>\n<!--@header gallery-->\n<main>'
             % (html.escape(p["title"]), attr(p["blurb"])))
    o.append('  <p class="crumbs"><a href="gallery.html">Gallery</a> &rsaquo; '
             '<a href="gallery.html#%s">%s</a></p>' % (cat["id"], html.escape(cat["title"])))
    o.append("  <h1>%s</h1>\n  <p class=\"sub\">%s</p>" % (html.escape(p["title"]), inline(p["blurb"])))
    o.append('  <figure class="piece"><a href="%s"><img src="%s" width="%d" height="%d" alt="%s"></a>\n'
             '    <figcaption>Click the picture for the full-size render.</figcaption></figure>'
             % (render, render, w, h, attr(p["alt"])))
    o.append("  <h2>How it is made</h2>\n  <p>%s</p>" % inline(p["technique"]))
    ref = p.get("reference")
    if ref:
        o.append('  <h2>The reference</h2>\n  <p>%s</p>\n  <div class="reference-comparison">\n'
                 '    <figure>\n      <img src="assets/references/%s" alt="%s" loading="lazy">\n'
                 '      <figcaption><b>Reference photograph</b><span>%s</span></figcaption>\n    </figure>\n'
                 '    <figure>\n      <img src="%s" alt="%s" loading="lazy">\n'
                 '      <figcaption><b>The PostScript version</b><span>%s</span></figcaption>\n    </figure>\n  </div>'
                 % (inline(ref["text"]), Path(ref["file"]).name, attr(ref["alt"]), inline(ref["caption"]),
                    render, attr(p["alt"]), inline(ref["render_caption"])))
    facts = [("Category", '<a href="gallery.html#%s">%s</a>' % (cat["id"], html.escape(cat["title"])))]
    if p["tags"]:
        facts.append(("Tags", " ".join('<span class="pill">%s</span>' % html.escape(t) for t in p["tags"])))
    if p["libraries"]:
        facts.append(("Libraries", ", ".join('<a href="%s">%s</a>' % (blob(m["libraries"][l]["guide"]),
                                                                    html.escape(m["libraries"][l]["title"]))
                                              for l in p["libraries"])))
    else:
        facts.append(("Libraries", "none: this piece is plain PostScript"))
    facts.append(("Added", p["added"]))
    if p.get("issue"):
        facts.append(("Built in", '<a href="%s/issues/%d">issue #%d</a>' % (REPO_URL, p["issue"], p["issue"])))
    facts.append(("Page size", "%d × %d points" % (w, h)))
    o.append("  <h2>Details</h2>\n  <dl class=\"facts\">\n" +
             "\n".join("    <dt>%s</dt><dd>%s</dd>" % (k, v) for k, v in facts) + "\n  </dl>")
    o.append('  <h2>Source</h2>\n  <p>The whole program is <a href="%s">%s</a> on GitHub. '
             'To draw it yourself, from a checkout of the repository:</p>\n'
             '  <pre><code>pscat --page %s %s</code></pre>'
             % (blob(p["source"]), html.escape(p["source"]), p["page"], p["source"]))
    if p["playground"]:
        o.append('  <p><a class="open-pg" href="playground.html?example=%s">Open it in the playground</a> '
                 'and watch it draw in your browser: this piece reads no other file, so it runs there '
                 'unchanged.</p>' % Path(p["source"]).name)
    o.append('  <details>\n    <summary>The first %d of %d lines of the source</summary>\n'
             '    <pre><code>%s</code></pre>\n  </details>'
             % (min(SOURCE_LINES, total), total, html.escape(head)))
    pager = []
    if prev:
        pager.append('<a rel="prev" href="%s">&larr; %s</a>' % (piece_page(prev["id"]), html.escape(prev["title"])))
    pager.append('<a href="gallery.html">All the art</a>')
    if nxt:
        pager.append('<a rel="next" href="%s">%s &rarr;</a>' % (piece_page(nxt["id"]), html.escape(nxt["title"])))
    o.append('  <nav class="pager" aria-label="Neighbouring pieces">%s</nav>' % "".join(pager))
    o.append("</main>\n<!--@footer-->\n</body>\n</html>\n")
    return "\n".join(o)


def build(m, out):
    out = Path(out)
    (out / "gallery.html").write_text(gallery_html(m), encoding="utf-8")
    # neighbours follow the page order a reader sees: category by category
    ordered = [p for _, ps in categories(m) for p in ps]
    for i, p in enumerate(ordered):
        prev = ordered[i - 1] if i else None
        nxt = ordered[i + 1] if i + 1 < len(ordered) else None
        (out / piece_page(p["id"])).write_text(piece_html(m, p, prev, nxt), encoding="utf-8")
    print("gallery: gallery.html and %d piece pages" % len(ordered))


def stage(m, out):
    out = Path(out)
    for d in ("assets/renders", "assets/references", "examples"):
        (out / d).mkdir(parents=True, exist_ok=True)
    for e in m["pieces"] + m["specimens"]:
        if e.get("render"):
            shutil.copyfile(ROOT / e["render"], out / "assets/renders" / (e["id"] + ".png"))
        if e.get("reference"):
            f = e["reference"]["file"]
            shutil.copyfile(ROOT / f, out / "assets/references" / Path(f).name)
        if e.get("playground") and e.get("source"):
            shutil.copyfile(ROOT / e["source"], out / "examples" / Path(e["source"]).name)


def main(argv):
    if len(argv) < 2 or argv[1] not in ("check", "sync", "build", "stage", "renders", "list"):
        print(__doc__, file=sys.stderr)
        return 2
    cmd = argv[1]
    m = load()
    if cmd == "check":
        errs = check(m)
        for e in errs:
            print("FAIL " + e)
        print("gallery manifest: %d pieces, %d specimens: %s" % (
            len(m["pieces"]), len(m["specimens"]), "%d problems" % len(errs) if errs else "ok"))
        return 1 if errs else 0
    if cmd == "sync":
        sync(m)
    elif cmd in ("build", "stage"):
        if len(argv) != 3:
            print(__doc__, file=sys.stderr)
            return 2
        (build if cmd == "build" else stage)(m, argv[2])
    elif cmd == "renders":
        for s in m["specimens"]:
            if not s.get("render"):
                print("%s %s %s" % (s["source"], s["page"], s["id"]))
    elif cmd == "list":
        for p in m["pieces"]:
            print("%s\t%s\t%d" % (p["id"], p["page"], p["speed"]))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv))
    except (ValueError, OSError, KeyError) as e:
        print("gallery: %s: %s" % (type(e).__name__, e), file=sys.stderr)
        sys.exit(1)
