## paint docs: which presets take /Seed, and what a stroke costs (issue #1414, 2026-11-26)

Documentation only, plus one stale comment in `lib/paintkit.ps`. The first
author to paint over time in psanim (Heron, 2026-11-24) asked which paint
presets pin themselves with `/Seed`, which draw from `rand`, and what a
stroke costs. Their finding was half right; this records what was run.

**What was added.** A "Seeding" rule and a cost table in the psart skill's
`references/paint.md`, and the same two sections in `docs/guides/painting.md`.
The `pkdry` header comment in `lib/paintkit.ps` said a 700-unit stroke took
~1.5 s (and ~8 s at `/Bristles 100`); measured now, 76 ms and 420 ms, so it
was corrected. The paint chapter itself had no cost warning to correct (a
quoted-glob search of `plugin/`, `docs/` and `lib/` found only that header and
`dry-media.md`'s hatching note); the "tens of ms per stroke" text the agent
read is not in this repository's paint docs.

**Findings (all measured, none from reading the source).**

- `N srand` immediately before a call pins every preset. The same program under
  `1 srand` twice gave identical PNGs for all of them, also after 1000 junk
  `rand` draws before the `srand`, and `2 srand` changed all of them except
  `pkribbon` and `pknib` (which draw nothing) and `pkwash`/`pkpaper` with `/Seed`.
- The issue's claim is half right. `pkwash`/`pkpaper` leave `rand` alone only
  with `/Seed`; without it they take exactly one draw (`pkwash 1`, `pkpaper 1`).
  `pkcanvas` takes none (its `/Seed` defaults to the constant 1). `pkribbon` and
  `pknib` take none unless `/Jitter` is set (177 draws at `/Jitter 1`).
- `/Seed` on any other preset is silently ignored: `pkdry << /Width 14 /Seed 9 >>`
  renders byte-identical to `pkdry << /Width 14 >>`, with no error and no warning.
  That is why the Heron author did both everywhere.
- Draw counts depend on the path: `pkdry` 2322 draws on a 100 pt line, 10962 on
  a 540 pt curve, 22176 on a 1060 pt line. So without a `srand` per stroke, editing
  one stroke re-rolls the rest.
- Cost follows stroke length and bristle count, not pixels: at 144 dpi a 600 pt
  `pkdry` was 66 ms against 64 ms at 72 dpi. A narrow clip does not make a
  `pkwash` cheaper (14 to 21 ms clipped, 18 to 24 ms not). `pktrowelplane` is
  the one area-bound preset (1.5 to 1.7 s over a full frame).

**Environment.** `pscat 0.7.1+90a130d` (`cargo build --release`), Apple M4,
72 dpi, `--page 1280x720`, CPU time (user plus system of the child), best of 7
runs minus the same page with no strokes. The machine was shared: load average
was 15 to 34 during the runs, so figures under about 5 ms are noise-limited. The
whole table was run twice; the rows over 10 ms agreed within 15%, and the
published numbers are rounded to two figures. Ghostscript was not timed.

**Deferred / proposed follow-ups.** An unknown or inapplicable `/Seed` should
raise an error or warning on the rand-driven presets (an author believes a stroke
is pinned when it is not).

### Rand draws per preset (`draws.ps`)

Run: `./target/release/pscat --headless draws.ps`. The probe seeds with
`5 srand`, runs the call, reads one `rand`, reseeds, and counts how many draws
reach that value.

```postscript
(lib/artkit.ps) run
(lib/paintkit.ps) run
/curve { newpath 40 100 moveto 200 140 300 60 560 100 curveto } def
/box { 40 40 400 120 screct } def
/dcount {
  /dproc exch def
  5 srand dproc rand /dafter exch def
  5 srand /di -1 def
  0 1 300000 { rand dafter eq { /di exch def exit } { pop } ifelse } for
  di
} def
/try { % (name) proc
  exch dup print ( ) print exch dcount =
} def
(pkribbon)  { curve << /Width 12 >> pkribbon } try
(pknib)     { curve << /Width 12 >> pknib } try
(pkdry)     { curve << /Width 12 >> pkdry } try
(pkspray)   { curve << /Nozzle 12 /Density 40 >> pkspray } try
(pkoil)     { curve << /Width 12 >> pkoil } try
(pktrowel)  { curve << /Width 12 >> pktrowel } try
(pktrowelplane) { 40 40 moveto 400 40 lineto 400 160 lineto 40 160 lineto closepath << /Width 12 >> pktrowelplane } try
(pkfan)     { curve << /Width 12 >> pkfan } try
(pkbroad)   { curve << /Width 12 >> pkbroad } try
(pkliner)   { curve << /Width 3 >> pkliner } try
(pkdab)     { curve << /Size 12 >> pkdab } try
(pkwash)    { 40 40 moveto 400 40 lineto 400 160 lineto 40 160 lineto closepath << >> pkwash } try
(pkwash /Seed 3) { 40 40 moveto 400 40 lineto 400 160 lineto 40 160 lineto closepath << /Seed 3 >> pkwash } try
(pkpaper)   { 0 0 200 200 << >> pkpaper } try
(pkpaper /Seed 3) { 0 0 200 200 << /Seed 3 >> pkpaper } try
(pkcanvas)  { << /Weave 2.5 >> pkcanvas } try
(pkcanvas /Seed 3) { << /Seed 3 >> pkcanvas } try
(pkwet(pkdry)) { { curve << /Width 12 >> pkdry } << /Soft 0.8 /Under [0.6 0.7 0.9] >> pkwet } try
(pkcrisscross) { box << /Count 20 /Length 30 >> pkcrisscross } try
(pkscumblein) { box << /Count 20 /Radius 5 >> pkscumblein } try
(pkscumble) { curve << /Radius 8 >> pkscumble } try
(pktap)     { curve << /Length 6 >> pktap } try
(pkpull)    { curve << /Length 20 /Direction 270 >> pkpull } try
(pkzigzag)  { curve << /Width 12 >> pkzigzag } try
(pkfeather) { 40 40 moveto 400 40 lineto 400 160 lineto 40 160 lineto closepath << >> pkfeather } try
(pkbrushedge) { 40 40 moveto 400 40 lineto 400 160 lineto 40 160 lineto closepath << >> pkbrushedge } try
```

The first file's last row, `pkbrushedge`, stops on `pkbroad-deposit-count-exceeds-safety-limit`. That is the probe, not the preset: the row has no `newpath`, so it appended its rectangle to the huge path `pkfeather` had just left behind (`pkfeather` replaces the current path). Run alone on the same rectangle it renders (exit 0, on the default page and at `--page 1280x720`). `draws2.ps` adds the `/Jitter` and path-length rows (its two `pkbrushedge` rows also lack a `newpath`, so read them as 'draws', not as a count); `draws3.ps` counts the edge presets from a clean path:

```postscript
(lib/artkit.ps) run
(lib/paintkit.ps) run
/curve { newpath 40 100 moveto 200 140 300 60 560 100 curveto } def
/dcount {
  /dproc exch def
  5 srand dproc rand /dafter exch def
  5 srand /di -1 def
  0 1 300000 { rand dafter eq { /di exch def exit } { pop } ifelse } for
  di
} def
/try { exch dup print ( ) print exch dcount = } def
(pkribbon /Jitter 1)  { curve << /Width 12 /Jitter 1 >> pkribbon } try
(pknib /Jitter 1)     { curve << /Width 12 /Jitter 1 >> pknib } try
(pkbrushedge small) { 100 100 moveto 160 100 lineto 160 140 lineto 100 140 lineto closepath << >> pkbrushedge } try
(pkbrushedge small /Under) { 100 100 moveto 160 100 lineto 160 140 lineto 100 140 lineto closepath << /Under [0.5 0.6 0.9] >> pkbrushedge } try
(pkribbon x2 length) { newpath 40 100 moveto 1100 140 lineto << /Width 12 >> pkribbon } try
(pkdry long) { newpath 40 100 moveto 1100 140 lineto << /Width 12 >> pkdry } try
(pkdry short) { newpath 40 100 moveto 140 140 lineto << /Width 12 >> pkdry } try
```

```postscript
(lib/artkit.ps) run
(lib/paintkit.ps) run
/dcount {
  /dproc exch def
  5 srand dproc rand /dafter exch def
  5 srand /di -1 def
  0 1 300000 { rand dafter eq { /di exch def exit } { pop } ifelse } for
  di
} def
/try { exch dup print ( ) print exch dcount = } def
% each row starts with newpath: pkfeather and the motions leave a path behind
(pkfeather, clean path) { newpath 100 100 moveto 160 100 lineto 160 140 lineto 100 140 lineto closepath << >> pkfeather } try
(pkbrushedge, clean path) { newpath 100 100 moveto 160 100 lineto 160 140 lineto 100 140 lineto closepath << >> pkbrushedge } try
(pkbrushedge /Under, clean path) { newpath 100 100 moveto 160 100 lineto 160 140 lineto 100 140 lineto closepath << /Under [0.5 0.6 0.9] >> pkbrushedge } try
```

Output: `pkfeather, clean path 269`, `pkbrushedge, clean path 2196`, `pkbrushedge /Under, clean path 6023`.

### Same output? (`det.py`)

Run: `python3 -I det.py` (renders each case with `1 srand`, `1 srand` again, `2 srand`, and `1 srand` after 1000 junk draws; compares the PNG hashes).

```python
import subprocess, hashlib, os
WT = "/Users/jeromebanks/dev/postscript_interpreter-issue-1414"
BIN = WT + "/target/release/pscat"
HEAD = "(lib/artkit.ps) run (lib/paintkit.ps) run 0.97 0.95 0.9 setrgbcolor 0 0 400 200 rectfill 0.2 0.3 0.6 setrgbcolor\n"
CURVE = "newpath 20 100 moveto 120 150 220 50 380 100 curveto "
BOX = "newpath 20 20 moveto 380 20 lineto 380 180 lineto 20 180 lineto closepath "
cases = {
 "pkdry": CURVE + "<< /Width 14 >> pkdry",
 "pkdry /Seed 9": CURVE + "<< /Width 14 /Seed 9 >> pkdry",
 "pkspray": CURVE + "<< /Nozzle 14 /Density 80 >> pkspray",
 "pkoil": CURVE + "<< /Width 14 >> pkoil",
 "pktrowel": CURVE + "<< /Width 14 >> pktrowel",
 "pkfan": CURVE + "<< /Width 14 >> pkfan",
 "pkbroad": CURVE + "<< /Width 14 >> pkbroad",
 "pkliner": CURVE + "<< /Width 3 >> pkliner",
 "pkdab": CURVE + "<< /Size 12 >> pkdab",
 "pkribbon": CURVE + "<< /Width 14 >> pkribbon",
 "pkribbon /Jitter 2": CURVE + "<< /Width 14 /Jitter 2 >> pkribbon",
 "pknib": CURVE + "<< /Width 14 >> pknib",
 "pkwash": BOX + "<< /Grain 0.5 >> pkwash",
 "pkwash /Seed 4": BOX + "<< /Grain 0.5 /Seed 4 >> pkwash",
 "pkpaper": "0 0 400 200 << /Grain 0.6 >> pkpaper",
 "pkpaper /Seed 4": "0 0 400 200 << /Grain 0.6 /Seed 4 >> pkpaper",
 "pkwet(pkspray)": "{ " + CURVE + "<< /Nozzle 14 /Density 80 >> pkspray } << /Soft 0.8 /Under [0.6 0.7 0.9] >> pkwet",
}
def render(body, pre):
    prog = HEAD + pre + body + "\n"
    p = "/private/tmp/claude-501/-Users-jeromebanks-dev-postscript-interpreter/6fe1a128-4e28-4b07-9599-c019bab1e6e3/scratchpad/w/d.ps"
    open(p, "w").write(prog)
    r = subprocess.run([BIN, "--page", "400x200", "--png", p + ".png", p], cwd=WT, capture_output=True, text=True)
    if r.returncode: return "ERR " + r.stderr[:80]
    return hashlib.md5(open(p + ".png", "rb").read()).hexdigest()[:8]
print("%-18s %-9s %-9s %-9s %-14s" % ("case", "1srand", "1srand-again", "2srand", "1srand,junk rand first"))
for name, body in cases.items():
    a = render(body, "1 srand ")
    b = render(body, "1 srand ")
    c = render(body, "2 srand ")
    d = render(body, "1 srand 1000 { rand pop } repeat 1 srand ")
    print("%-18s %-9s %-12s %-9s %s" % (name, a, "same" if a == b else "DIFF", "same" if a == c else "differs", "same" if a == d else "DIFF"))
```

### Per-stroke cost (`cost.py`)

Run: `python3 -I cost.py`. Strokes are 100 and 600 pt curves, 10 per run, `i srand` before each.

```python
import subprocess, resource, os, sys, json, statistics
WT = "/Users/jeromebanks/dev/postscript_interpreter-issue-1414"
BIN = WT + "/target/release/pscat"
W = os.path.dirname(os.path.abspath(__file__))
REPS = 7
def cpu(prog_path, png):
    r0 = resource.getrusage(resource.RUSAGE_CHILDREN)
    subprocess.run([BIN, "--page", "1280x720", "--png", png, prog_path], cwd=WT, check=True, capture_output=True)
    r1 = resource.getrusage(resource.RUSAGE_CHILDREN)
    return (r1.ru_utime - r0.ru_utime) + (r1.ru_stime - r0.ru_stime)
def best(prog):
    p = os.path.join(W, "p.ps"); open(p, "w").write(prog)
    ts = [cpu(p, os.path.join(W, "o.png")) for _ in range(REPS)]
    return min(ts), statistics.median(ts)
HEAD = "(lib/artkit.ps) run (lib/paintkit.ps) run\n0.97 0.95 0.9 setrgbcolor 0 0 1280 720 rectfill 0.2 0.3 0.6 setrgbcolor\n"
def stroke_prog(call, K, L):
    # K strokes of length L, spread over 10 rows
    s = HEAD + "/mk { /i exch def i srand newpath\n 100 i 10 mod 60 mul 60 add moveto\n %g 3 div 0 add 100 add i 10 mod 60 mul 100 add %g 3 div 2 mul 100 add i 10 mod 60 mul 20 add %g 100 add i 10 mod 60 mul 60 add curveto\n } def\n" % (L, L, L)
    s += "0 1 %d { mk %s } for\n" % (K - 1, call) if K > 0 else ""
    return s
presets = [
 ("pkribbon", "<< /Width 14 >> pkribbon"),
 ("pknib", "<< /Width 14 >> pknib"),
 ("pkdry", "<< /Width 14 >> pkdry"),
 ("pkspray", "<< /Nozzle 14 /Density 80 >> pkspray"),
 ("pkoil", "<< /Width 14 >> pkoil"),
 ("pktrowel", "<< /Width 14 >> pktrowel"),
 ("pkfan", "<< /Width 14 >> pkfan"),
 ("pkbroad", "<< /Width 14 >> pkbroad"),
 ("pkliner", "<< /Width 3 >> pkliner"),
 ("pkdab", "<< /Size 12 >> pkdab"),
 ("pkwet(pkspray)", "{ << /Nozzle 14 /Density 80 >> pkspray } << /Soft 0.8 /Under [0.6 0.7 0.9] >> pkwet"),
]
only = sys.argv[1:] 
K = 10
base_min, base_med = best(HEAD)
print("baseline (libs + 1280x720 fill + PNG): min %.3f med %.3f s CPU" % (base_min, base_med), flush=True)
res = {}
for name, call in presets:
    if only and name not in only: continue
    row = {}
    for L in (100, 600):
        t0 = best(stroke_prog(call, 0, L))[0]
        t, tm = best(stroke_prog(call, K, L))
        row[L] = (t - base_min) / K * 1000
        row[str(L)+"_med"] = (tm - base_med) / K * 1000
    res[name] = row
    print("%-16s L=100: %7.1f ms   L=600: %7.1f ms   (median-based %7.1f / %7.1f)" % (name, row[100], row[600], row["100_med"], row["600_med"]), flush=True)
```

### Areas and combinations (`cost2.py`, `cost3.py`, `cost4.py`)

`cost2.py` ran each once; `cost3.py` repeats the small items 10 times in one program to dilute the noise; `cost4.py` times `pkcanvas` (a `pkcanvas set-up` row that was in `cost3.py` crashed on a stray `pop` and is left out here).
Both import `cost.py`'s helpers by splitting its source at `only = sys.argv[1:]`.

```python
import importlib.util, sys
sys.argv = ["x", "NONE"]
spec = importlib.util.spec_from_file_location("cost", "cost.py")
# reuse helpers without running presets loop
src = open("cost.py").read().split("only = sys.argv[1:]")[0]
exec(src)
base_min, base_med = best(HEAD)
print("baseline min %.3f" % base_min)
def rect(x, y, w, h): return "newpath %g %g moveto %g %g lineto %g %g lineto %g %g lineto closepath" % (x, y, x+w, y, x+w, y+h, x, y+h)
def row(name, body, K=1):
    t, tm = best(HEAD + body)
    print("%-44s %8.1f ms (median %8.1f)" % (name, (t - base_min) * 1000, (tm - base_med) * 1000), flush=True)
for (w, h) in ((200, 150), (640, 360), (1280, 720)):
    row("pkwash %dx%d" % (w, h), "1 srand %s << >> pkwash" % rect(10, 10, w-20, h-20))
    row("pkwash %dx%d /Seed 1" % (w, h), "%s << /Seed 1 >> pkwash" % rect(10, 10, w-20, h-20))
for (w, h) in ((200, 150), (1280, 720)):
    row("pkpaper %dx%d" % (w, h), "1 srand 0 0 %d %d << >> pkpaper" % (w, h))
    row("pktrowelplane %dx%d" % (w, h), "1 srand %s << /Width 14 >> pktrowelplane" % rect(10, 10, w-20, h-20))
# wash full frame under a narrow clip
row("pkwash full frame, clip 1280x60 strip", "gsave 0 330 1280 60 rectclip 1 srand %s << >> pkwash grestore" % rect(10, 10, 1260, 700))
row("pkwash full frame, clip 200x150", "gsave 100 100 200 150 rectclip 1 srand %s << >> pkwash grestore" % rect(10, 10, 1260, 700))
row("pkwash full frame, no clip", "1 srand %s << >> pkwash" % rect(10, 10, 1260, 700))
row("pkwash 200x150 /Layers 4", "1 srand %s << /Layers 4 >> pkwash" % rect(10, 10, 200, 150))
row("pkcanvas (setup, 1280x720 weave 2.5)", "<< /Seed 7 /Weave 2.5 >> pkcanvas")
row("pkdry 600pt, no touch", "(x) pop 1 srand newpath 100 300 moveto 700 340 lineto << /Width 14 >> pkdry")
row("pkdry 600pt, /Touch 0.7 after pkcanvas", "<< /Seed 7 >> pkcanvas 1 srand newpath 100 300 moveto 700 340 lineto << /Width 14 /Touch 0.7 >> pkdry")
row("pkbrushedge 300x200 closed mass", "0.3 0.5 0.3 setrgbcolor 1 srand %s << /Under [0.6 0.7 0.9] >> pkbrushedge" % rect(100, 100, 300, 200))
row("pkbrushedge 100x60 closed mass", "0.3 0.5 0.3 setrgbcolor 1 srand %s << /Under [0.6 0.7 0.9] >> pkbrushedge" % rect(100, 100, 100, 60))
row("pkcrisscross sky 1280x300 + pkbroad", "1 srand 0 420 1280 300 screct << /Count 90 /Length 50 >> pkcrisscross << /Width 30 >> pkbroad")
row("pkscumblein 400x200 + pkoil", "1 srand 100 100 400 200 screct << /Count 60 /Radius 20 >> pkscumblein << /Width 12 >> pkoil")
```

```python
# small items repeated 10x inside one program so the baseline noise is divided by 10
src = open("cost.py").read().split("only = sys.argv[1:]")[0]
exec(src)
bm = best(HEAD)[0]
def rect(x, y, w, h): return "newpath %g %g moveto %g %g lineto %g %g lineto %g %g lineto closepath" % (x, y, x+w, y, x+w, y+h, x, y+h)
def row(name, body, K=10):
    t = best(HEAD + "0 1 %d { srand %s } for\n" % (K - 1, body))[0]
    print("%-36s %6.1f ms per call" % (name, (t - bm) / K * 1000), flush=True)
for (w, h) in ((200, 150), (640, 360), (1280, 720)):
    row("pkwash %dx%d" % (w, h), rect(10, 10, w-20, h-20) + " << >> pkwash")
row("pkwash 200x150 /Layers 4", rect(10, 10, 190, 140) + " << /Layers 4 >> pkwash")
row("pkwash 200x150 /Seed 1", rect(10, 10, 190, 140) + " << /Seed 1 >> pkwash")
for (w, h) in ((200, 150), (1280, 720)):
    row("pkpaper %dx%d" % (w, h), "0 0 %d %d << >> pkpaper" % (w, h))
```

```python
src = open("cost.py").read().split("only = sys.argv[1:]")[0]
exec(src)
bm = best(HEAD)[0]
t = best(HEAD + "0 1 9 { pop << /Seed 7 /Weave 2.5 >> pkcanvas } for\n")[0]
print("pkcanvas set-up %.1f ms per call" % ((t - bm) / 10 * 1000))
```

### Dpi and the 700 pt `pkdry` (`dpi.py`, `dry700.py`)

```python
src = open("cost.py").read().split("only = sys.argv[1:]")[0]
exec(src)
def cpu2(prog_path, extra):
    r0 = resource.getrusage(resource.RUSAGE_CHILDREN)
    subprocess.run([BIN, "--page", "1280x720", "--png", os.path.join(W,"o.png"), prog_path] + extra, cwd=WT, check=True, capture_output=True)
    r1 = resource.getrusage(resource.RUSAGE_CHILDREN)
    return (r1.ru_utime - r0.ru_utime) + (r1.ru_stime - r0.ru_stime)
def b2(prog, extra):
    p = os.path.join(W, "p.ps"); open(p, "w").write(prog)
    return min(cpu2(p, extra) for _ in range(REPS))
for name, call in [("pkdry","<< /Width 14 >> pkdry"),("pkoil","<< /Width 14 >> pkoil"),("pkbroad","<< /Width 14 >> pkbroad"),("pkspray","<< /Nozzle 14 /Density 80 >> pkspray")]:
    out=[]
    for extra in ([], ["--dpi","144"]):
        bm = b2(HEAD, extra); t = b2(stroke_prog(call, 10, 600), extra)
        out.append((t-bm)/10*1000)
    print("%-10s 72dpi %6.1f ms   144dpi %6.1f ms" % (name, out[0], out[1]))
```

```python
src = open("cost.py").read().split("only = sys.argv[1:]")[0]
exec(src)
bm = best(HEAD)[0]
for opts in ("<< /Width 24 >>", "<< /Width 24 /Bristles 100 >>"):
    t = best(stroke_prog(opts + " pkdry", 10, 700))[0]
    print("pkdry 700pt", opts, "%.1f ms/stroke" % ((t - bm) / 10 * 1000))
```

