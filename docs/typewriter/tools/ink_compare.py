"""Compare the /Typewriter ribbon ink with the reference crops (issue #212).

    python3 -I docs/typewriter/tools/ink_compare.py [PSCAT] [OUT.png]

Run from the repo root. For each reference crop it types the same characters
at the source scan's resolution (300 dpi; 240 for Szilard), with the pitch,
preset and ink that match the source, in pscat and in Ghostscript. It then
scales each render by the crop's own factor (make_board.py's CROPS), so the
three rows of a panel are at one scale: the scan, pscat, gs. The default
output is docs/typewriter/ink_vs_real.png.

Needs Pillow, gs on PATH and a release pscat (default target/release/pscat).
The machines are picked for the quirk a crop shows: machine 4 has clogged
`a` and `e` counters, like the FDR draft's typewriter (T4, T6).
"""
import os
import subprocess
import sys
import tempfile

from PIL import Image, ImageDraw, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
TW = os.path.dirname(HERE)
ROOT = os.path.abspath(os.path.join(TW, "..", ".."))
REFS = os.path.join(TW, "refs")

PICA = "/Pitch 10 /Size 12.5"
ELITE = "/Pitch 12 /Size 11.5"
INK = "/Ink [0.13 0.13 0.15]"
GREYBROWN = "/Ink [0.36 0.32 0.29]"

# (reference crop, caption, text, options, dpi, scale, paper rgb, grey?)
PANELS = [
    ("T02_duncan_line.jpg", "T2 Duncan 1914, pica, 300 dpi", "In reply to your letter of June",
     f"/Preset /manual {PICA} /Machine 1 /Seed 2 {INK}", 300, 1, (0.93, 0.93, 0.92), True),
    ("T04_fdr_filled_counters.jpg", "T4 FDR 1941, elite, 300 dpi x2", "a date which w",
     f"/Preset /manual {ELITE} /Machine 4 /Seed 4 {INK}", 300, 2, (0.93, 0.93, 0.92), True),
    ("T10_szilard_zoom.jpg", "T10 Szilard 1945, elite, worn grey-brown ribbon, 240 dpi x1.5",
     "present phase of the war against Japan.",
     f"/Preset /worn {ELITE} /Machine 3 /Seed 10 {GREYBROWN}", 240, 1.5, (0.92, 0.85, 0.72), False),
    ("T06_fdr_a_tiles.png", "T6 FDR: every 'a' (scan tiles) / machine 4's clogged 'a', 300 dpi x1.5",
     "a a a a a a a a a a a a a a", f"/Preset /manual {ELITE} /Machine 4 /Seed 6 {INK}",
     300, 1.5, (0.93, 0.93, 0.92), True),
    ("T07_duncan_e_tiles.png", "T7 Duncan: every 'e' (scan tiles) / repeated 'e', 300 dpi x1.5",
     "e e e e e e e e e e e e e e e", f"/Preset /manual {PICA} /Machine 1 /Seed 7 {INK}",
     300, 1.5, (0.93, 0.93, 0.92), True),
]


def ps_source(text, opts, paper, width, height):
    esc = text.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")
    return f"""%!PS
(lib/typewriter_outlines.ps) run (lib/typewriter.ps) run
{paper[0]} {paper[1]} {paper[2]} setrgbcolor clippath fill
4 6 moveto << {opts} >> tw-setup ({esc}) show
showpage
"""


def render(pscat, text, opts, dpi, paper, tmp, tag):
    adv = 7.2 if "/Pitch 10" in opts else 6.0
    w, h = round(len(text) * adv + 8), 18
    src = os.path.join(tmp, tag + ".ps")
    with open(src, "w") as f:
        f.write(ps_source(text, opts, paper, w, h))
    a = os.path.join(tmp, tag + "_pscat.png")
    b = os.path.join(tmp, tag + "_gs.png")
    subprocess.run([pscat, "--page", f"{w}x{h}", "--dpi", str(dpi), "--png", a, src],
                   check=True, cwd=ROOT, stdout=subprocess.DEVNULL)
    px = (round(w * dpi / 72), round(h * dpi / 72))
    subprocess.run(["gs", "-q", "-dNOSAFER", "-dBATCH", "-dNOPAUSE", "-sDEVICE=png16m",
                    f"-r{dpi}", f"-g{px[0]}x{px[1]}", "-dGraphicsAlphaBits=4",
                    f"-sOutputFile={b}", src],
                   check=True, cwd=ROOT, stdin=subprocess.DEVNULL)
    return Image.open(a).convert("RGB"), Image.open(b).convert("RGB")


def main():
    pscat = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "target", "release", "pscat")
    out = sys.argv[2] if len(sys.argv) > 2 else os.path.join(TW, "ink_vs_real.png")
    font = ImageFont.load_default()
    rows = []
    with tempfile.TemporaryDirectory() as tmp:
        for i, (ref, cap, text, opts, dpi, sc, paper, grey) in enumerate(PANELS):
            r = Image.open(os.path.join(REFS, ref)).convert("RGB")
            p, g = render(pscat, text, opts, dpi, paper, tmp, f"p{i}")
            p, g = [im.resize((round(im.width * sc), round(im.height * sc)), Image.LANCZOS)
                    for im in (p, g)]
            if grey:
                p, g = [im.convert("L").convert("RGB") for im in (p, g)]
            rows.append((cap, [("scan", r), ("pscat", p), ("gs", g)]))
    width = max(im.width for _, ims in rows for _, im in ims) + 70
    height = sum(18 + sum(im.height + 6 for _, im in ims) + 14 for _, ims in rows)
    board = Image.new("RGB", (width, height), "white")
    d = ImageDraw.Draw(board)
    y = 0
    for cap, ims in rows:
        d.text((6, y + 3), cap, fill=(0, 0, 0), font=font)
        y += 18
        for label, im in ims:
            d.text((6, y + im.height // 2 - 5), label, fill=(90, 90, 90), font=font)
            board.paste(im, (64, y))
            y += im.height + 6
        y += 14
    board.save(out, optimize=True)
    print("wrote", out)


if __name__ == "__main__":
    main()
