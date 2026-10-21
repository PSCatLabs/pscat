// pscat.js — PostScript in the browser (Stage 16).
//
// A dependency-free ES module wrapping web/pscat.wasm (built by
// scripts/build_wasm.sh). Two ways to drive it:
//
//   const ps = await Pscat.load();
//   ps.run('0 0 300 300 rectfill showpage');   // to completion
//   ps.paintTo(canvas);
//
// or step-driven — the watch-it-draw window, in a tab:
//
//   ps.begin(source, 612, 792);
//   function frame() {
//     const more = ps.step(300);               // objects per frame
//     ps.paintTo(canvas);
//     if (more === 1) requestAnimationFrame(frame);
//     else if (more === -1) console.error(ps.error);
//   }
//   requestAnimationFrame(frame);
//
// Works in node too (no canvas there — use pixels()/width/height):
//   const ps = await Pscat.instantiate(fs.readFileSync('pscat.wasm'));
//
// The npm package (npm/, #566) ships this file as its core.js and wraps
// it in the documented createRenderer() API.

export class Pscat {
  /** Fetch and instantiate the wasm module (browser path). */
  static async load(url = new URL('./pscat.wasm', import.meta.url)) {
    const { instance } = await WebAssembly.instantiateStreaming(fetch(url), {});
    return new Pscat(instance);
  }

  /** Instantiate from bytes already in hand (node path). */
  static async instantiate(bytes) {
    const { instance } = await WebAssembly.instantiate(bytes, {});
    return new Pscat(instance);
  }

  constructor(instance) {
    this.ex = instance.exports;
  }

  /**
   * Queue a program (a string, or the bytes of one) on a fresh
   * width x height point page, at `scale` device pixels per point
   * (dpi / 72). Throws on unusable page sizes.
   */
  begin(source, width = 612, height = 792, scale = 1) {
    const bytes = typeof source === 'string' ? new TextEncoder().encode(source) : source;
    const ptr = this.#copyIn(bytes);
    const rc = scale === 1
      ? this.ex.ps_begin(ptr, bytes.length, width, height)
      : this.ex.ps_begin_scaled(ptr, bytes.length, width, height, scale);
    this.ex.ps_dealloc(ptr, bytes.length);
    if (rc !== 0) throw new RangeError(`pscat: unusable page size ${width}x${height} at scale ${scale}`);
  }

  /**
   * Register a TrueType/OpenType font file's bytes for findfont, under
   * `name`: a font pack's file stem such as 'EBGaramond-Regular' (which
   * /Garamond then finds too, as with the installed pack). The face's
   * own PostScript name finds it as well. Throws if the bytes are not a
   * font. Fonts stay registered for this module instance.
   */
  addFont(name, bytes) {
    const n = new TextEncoder().encode(name);
    const data = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    const np = this.#copyIn(n);
    const dp = this.#copyIn(data);
    const rc = this.ex.ps_add_font(np, n.length, dp, data.length);
    this.ex.ps_dealloc(np, n.length);
    this.ex.ps_dealloc(dp, data.length);
    if (rc !== 0) throw new Error(`pscat: addFont(${name}): ${this.error || 'invalid font name'}`);
  }

  #copyIn(bytes) {
    const ptr = this.ex.ps_alloc(bytes.length);
    new Uint8Array(this.ex.memory.buffer, ptr, bytes.length).set(bytes);
    return ptr;
  }

  /**
   * Execute up to n objects. Returns 1 (more work), 0 (done), or -1
   * (a PostScript error — see .error; the page and stacks survive).
   */
  step(n = 5000) {
    return this.ex.ps_step(n);
  }

  /** begin + run to completion. Returns 0, or -1 with .error set. */
  run(source, width = 612, height = 792, scale = 1) {
    this.begin(source, width, height, scale);
    let status;
    do {
      status = this.step(1 << 16);
    } while (status === 1);
    return status;
  }

  get width() {
    return this.ex.ps_width();
  }

  get height() {
    return this.ex.ps_height();
  }

  /** The live page as a copied RGBA byte array (node-friendly). */
  pixels() {
    const ptr = this.ex.ps_pixels();
    const len = this.ex.ps_pixels_len();
    return new Uint8Array(this.ex.memory.buffer, ptr, len).slice();
  }

  /** The last error report ('' when the last run was clean). */
  get error() {
    const len = this.ex.ps_error_len();
    if (len === 0) return '';
    const ptr = this.ex.ps_error_ptr();
    return new TextDecoder().decode(new Uint8Array(this.ex.memory.buffer, ptr, len));
  }

  /**
   * What the program has printed with =, ==, stack, pstack or print
   * since begin() ('' when nothing). Read it during a stepped run to
   * show output as it appears.
   */
  get output() {
    const len = this.ex.ps_output_len();
    if (len === 0) return '';
    const ptr = this.ex.ps_output_ptr();
    return new TextDecoder().decode(new Uint8Array(this.ex.memory.buffer, ptr, len));
  }

  /** Blit the live page into a <canvas> (resizes it to match). */
  paintTo(canvas) {
    const w = this.width;
    const h = this.height;
    if (w === 0 || h === 0) return;
    if (canvas.width !== w) canvas.width = w;
    if (canvas.height !== h) canvas.height = h;
    const data = new Uint8ClampedArray(this.ex.memory.buffer, this.ex.ps_pixels(), w * h * 4).slice();
    canvas.getContext('2d').putImageData(new ImageData(data, w, h), 0, 0);
  }
}
