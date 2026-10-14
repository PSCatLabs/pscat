"""Rebuild the typewriter reference board (issue #210).

    python3 -I docs/typewriter/tools/make_board.py CACHE_DIR

Downloads each source scan into CACHE_DIR (once), cuts the crops listed in
CROPS into docs/typewriter/refs/, and composes docs/typewriter/reference_board.jpg.
Every committed crop's source carries {{PD-USGov}} or {{PD-US-no notice}} on
Wikimedia Commons, or is a JFK Assassination Records release PDF (a federal
record). docs/typewriter/REFERENCES.md has the table. The O'Brien memo (T12)
is measured but not cropped: see REFERENCES.md.
Needs Pillow, Ghostscript (for the PDF) and tesseract (for the letter tiles).
"""
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request

from PIL import Image, ImageDraw, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
TW = os.path.dirname(HERE)
ROOT = os.path.abspath(os.path.join(TW, "..", ".."))
REFS = os.path.join(TW, "refs")
UA = {"User-Agent": "pscat-reference-board/1.0 (github.com/jeromebanks/postscript_interpreter)"}

C = "https://upload.wikimedia.org/wikipedia/commons/"
SOURCES = {
    # name: (url, kind)
    "duncan": (C + "9/99/J.J._Duncan_to_Superintendent_W.C._Kohlenberg%2C_Crow_Creek_Agency%2C_South_Dakota._-_NARA_-_284501.tif", "img"),
    "fdr1": (C + "2/2e/Annotated_Draft_of_Proposed_Message_to_Congress_Requesting_Declaration_of_War_Against_Japan_-_DPLA_-_2eb3ebba93444052d0046022975cbff5_%28page_1%29.jpg", "img"),
    "fdr2": (C + "d/dc/Annotated_Draft_of_Proposed_Message_to_Congress_Requesting_Declaration_of_War_Against_Japan_-_DPLA_-_2eb3ebba93444052d0046022975cbff5_%28page_2%29.jpg", "img"),
    "szilard": (C + "d/d3/Petition_from_Leo_Szilard_and_Other_Scientists_to_President_Harry_S._Truman_01.jpg", "img"),
    "mfr1955": (C + "f/f4/Memorandum_for_the_Record_-_NARA_-_186540.jpg", "img"),
    "nsam55": (C + "8/81/National_Security_Action_Memorandum_No._55_Relations_of_the_JCS_to_the_President_in_Cold_War_Operations_-_NARA_-_193454.jpg", "img"),
    "jfk10106": ("https://www.archives.gov/files/research/jfk/releases/104-10297-10106.pdf", "pdf"),
}

# (file, source, box (x0, y0, x1, y1) in source pixels, scale, caption)
CROPS = [
    ("T01_duncan_page_1914.jpg", "duncan", None, 0.25, "T1 Duncan 1914, manual pica, whole page"),
    ("T02_duncan_line.jpg", "duncan", (560, 1130, 1500, 1215), 1, "T2 light 'e', heavy 'r', ink spread"),
    ("T03_fdr_strikeover.jpg", "fdr1", (370, 440, 640, 505), 2, "T3 strikeover in 'Yesterday'"),
    ("T04_fdr_filled_counters.jpg", "fdr1", (1060, 440, 1440, 505), 2, "T4 mottled ink, filled 'a'"),
    ("T05_fdr2_strikeover.jpg", "fdr2", (960, 1055, 1230, 1149), 2, "T5 'm' struck over 'n'"),
    ("T09_szilard_block.jpg", "szilard", (120, 330, 2000, 900), 0.6, "T9 Szilard 1945, elite, margins"),
    ("T10_szilard_zoom.jpg", "szilard", (250, 570, 1150, 650), 1.5, "T10 worn grey-brown ribbon"),
    ("T11_nsam55_underscores.jpg", "nsam55", (300, 560, 2200, 1450), 0.5, "T11 typed underscores (proportional face)"),
    ("T13_mfr1955_erasure.jpg", "mfr1955", (202, 1265, 1045, 1375), 1.2, "T13 erasure ghost; proportional face"),
    ("T14_nsam55_marks.jpg", "nsam55", (300, 180, 2400, 640), 0.5, "T14 redaction, stamps (not for specimens)"),
    ("T15_jfk_release_copy.jpg", "jfk10106", (452, 1081, 2009, 1539), 0.7, "T15 bitonal release copy"),
]
# letter tiles: (file, source, char, cell half-size, scale, region, caption)
TILES = [
    ("T06_fdr_a_tiles.png", "fdr1", "a", 22, 1.5, None, "T6 every 'a' on FDR p.1: clogged bowl"),
    ("T07_duncan_e_tiles.png", "duncan", "e", 24, 1.5, (0, 630, 2411, 2205), "T7 every 'e', Duncan"),
    ("T08_szilard_e_tiles.png", "szilard", "e", 16, 2, (150, 400, 2000, 2450), "T8 every 'e', Szilard"),
]


def fetch(cache, name):
    url, kind = SOURCES[name]
    raw = os.path.join(cache, name + (".pdf" if kind == "pdf" else ".src"))
    png = os.path.join(cache, name + ".png")
    if os.path.exists(png):
        return png
    if not os.path.exists(raw):
        for attempt in range(5):   # Commons answers 429 to bursts: back off and retry
            try:
                with urllib.request.urlopen(urllib.request.Request(url, headers=UA)) as r:
                    data = r.read()
                break
            except urllib.error.HTTPError as e:
                if e.code != 429 or attempt == 4:
                    raise
                time.sleep(10 * (attempt + 1))
        with open(raw, "wb") as f:
            f.write(data)
    if kind == "pdf":
        subprocess.run(["gs", "-q", "-dSAFER", "-dBATCH", "-dNOPAUSE", "-sDEVICE=pnggray", "-r300",
                        "-dFirstPage=1", "-dLastPage=1", "-sOutputFile=" + png, raw], check=True)
    else:
        Image.open(raw).save(png)
    return png


def tiles(path, ch, pad, region):
    im = Image.open(path).convert("L")
    if region:
        im = im.crop(region)
    tmp = path + ".tilecrop.png"
    im.save(tmp)
    out = subprocess.run(["tesseract", tmp, "stdout", "--psm", "6", "makebox"],
                         capture_output=True, text=True).stdout
    H, cells = im.height, []
    for ln in out.splitlines():
        q = ln.split(" ")
        if len(q) < 6 or q[0] != ch:
            continue
        a, b, c, d = map(int, q[1:5])
        cx, cy = (a + c) // 2, H - (b + d) // 2
        cells.append(im.crop((cx - pad, cy - pad, cx + pad, cy + pad)))
    cells = cells[:14]
    sheet = Image.new("L", (len(cells) * (2 * pad + 4), 2 * pad), 255)
    for i, cl in enumerate(cells):
        sheet.paste(cl, (i * (2 * pad + 4), 0))
    return sheet


def main():
    cache = sys.argv[1]
    os.makedirs(cache, exist_ok=True)
    os.makedirs(REFS, exist_ok=True)
    panels = []
    for fn, src, box, sc, cap in CROPS:
        im = Image.open(fetch(cache, src))
        im = im.convert("RGB" if src == "szilard" else "L")
        if box:
            im = im.crop(box)
        if sc != 1:
            im = im.resize((int(im.width * sc), int(im.height * sc)), Image.LANCZOS)
        im.save(os.path.join(REFS, fn), quality=88)
        panels.append((im, cap))
    for fn, src, ch, pad, sc, region, cap in TILES:
        im = tiles(fetch(cache, src), ch, pad, region)
        im = im.resize((int(im.width * sc), int(im.height * sc)), Image.LANCZOS)
        im.save(os.path.join(REFS, fn))
        panels.append((im, cap))
    # contact sheet: two columns, captions above each panel
    font = ImageFont.truetype(os.path.join(ROOT, "fonts", "LiberationSans-Bold.ttf"), 18)
    W = 1100
    rows = []
    for im, cap in sorted(panels, key=lambda p: int(p[1].split()[0][1:])):
        if im.width > W:
            im = im.resize((W, int(im.height * W / im.width)), Image.LANCZOS)
        rows.append((im.convert("RGB"), cap))
    H = sum(im.height + 34 for im, _ in rows) + 20
    sheet = Image.new("RGB", (W + 40, H), "white")
    d = ImageDraw.Draw(sheet)
    y = 10
    for im, cap in rows:
        d.text((20, y), cap, fill=(30, 30, 30), font=font)
        sheet.paste(im, (20, y + 24))
        y += im.height + 34
    sheet.save(os.path.join(TW, "reference_board.jpg"), quality=85)
    print("wrote", len(rows), "panels")


if __name__ == "__main__":
    main()
