// The shared implementation behind index.js (browsers, bundlers) and
// node.js (Node): Renderer, PostScriptError and the module loaders. Not
// an entry point of its own (package.json's exports map leaves it out).
// core.js is a copy of the repository's web/pscat.js, the low-level
// wrapper over the module's C ABI (scripts/build_npm.sh).

import { Pscat } from './core.js';

const DEFAULT_PAGE = { width: 612, height: 792, dpi: 72 };

/**
 * A PostScript error (`undefined`, `typecheck`, `stackunderflow`, ...),
 * raised by render() and step(). The renderer stays usable, and the
 * page drawn before the error is still readable.
 */
export class PostScriptError extends Error {
  constructor(report, output = '') {
    const m = /Error:\s*([A-Za-z]+)/.exec(report);
    // One unambiguous run of characters, trimmed after: no backtracking
    // over the report's spaces.
    const cmd = /OffendingCommand: ([^;\]]*)/.exec(report);
    const line = /Line:\s*(\d+)/.exec(report);
    super(report.split('\n')[0] || 'PostScript error');
    this.name = 'PostScriptError';
    /** The PostScript error name, e.g. 'undefined'. */
    this.errorName = m ? m[1] : 'unknown';
    /** The offending command, as the interpreter reports it. */
    this.command = cmd ? cmd[1].trim() : undefined;
    /** The source line the error happened on, when known. */
    this.line = line ? Number(line[1]) : undefined;
    /** The full report, `%%[ Error: ...; OffendingCommand: ... ]%%` plus any hint. */
    this.report = report;
    /** What the program printed before the error. */
    this.output = output;
  }
}

function pageOf(options = {}) {
  const width = options.width ?? DEFAULT_PAGE.width;
  const height = options.height ?? DEFAULT_PAGE.height;
  const dpi = options.dpi ?? DEFAULT_PAGE.dpi;
  for (const [k, v] of [['width', width], ['height', height]]) {
    // The ABI takes a u32: anything larger would wrap at the boundary
    // instead of being refused (the 8000-pixel ceiling is checked inside).
    if (!Number.isInteger(v) || v <= 0 || v > 0xffffffff) {
      throw new RangeError(`pscat: ${k} must be a positive integer (points), got ${v}`);
    }
  }
  if (!Number.isFinite(dpi) || dpi <= 0) throw new RangeError(`pscat: dpi must be a positive number, got ${dpi}`);
  return { width, height, scale: dpi / 72 };
}

/** One interpreter in its own WebAssembly instance. Make one with createRenderer(). */
export class Renderer {
  #ps;

  /** @internal Use createRenderer(). */
  constructor(core) {
    if (!(core instanceof Pscat)) throw new TypeError('pscat: use createRenderer() to make a Renderer');
    this.#ps = core;
  }

  /**
   * Run a program to completion on a fresh page and return the page.
   * Throws PostScriptError on a PostScript error.
   */
  render(source, options) {
    this.begin(source, options);
    while (this.step(1 << 16));
    return this.image();
  }

  /** Queue a program on a fresh page; drive it with step() or frames(). */
  begin(source, options) {
    if (typeof source !== 'string' && !(source instanceof Uint8Array)) {
      throw new TypeError('pscat: the program must be a string or a Uint8Array');
    }
    const { width, height, scale } = pageOf(options);
    this.#ps.begin(source, width, height, scale);
  }

  /**
   * Execute up to `budget` objects. Returns true while there is more
   * work, false when the program has finished. Throws PostScriptError.
   */
  step(budget = 5000) {
    // An i32 in the ABI: a larger budget would wrap (to 0 or negative,
    // which runs nothing and never finishes).
    if (!Number.isInteger(budget) || budget <= 0 || budget > 0x7fffffff) {
      throw new RangeError(`pscat: budget must be a positive integer, got ${budget}`);
    }
    const status = this.#ps.step(budget);
    if (status === 1) return true;
    if (status === 0) return false;
    if (status === -1) throw new PostScriptError(this.#ps.error, this.#ps.output);
    throw new Error('pscat: step() called before begin()');
  }

  /**
   * begin(), then step(budget) once per iteration: yields the frame
   * number after each step, so a loop can paint the page as it draws.
   * The last frame is the finished page.
   */
  *frames(source, options = {}) {
    const budget = options.budget ?? 5000;
    this.begin(source, options);
    for (let n = 0; ; n++) {
      const more = this.step(budget);
      yield n;
      if (!more) return;
    }
  }

  /** Page width in device pixels. */
  get width() {
    return this.#ps.width;
  }

  /** Page height in device pixels. */
  get height() {
    return this.#ps.height;
  }

  /** What the program has printed with =, ==, print, stack or pstack since begin(). */
  get output() {
    return this.#ps.output;
  }

  /**
   * Notes about fonts the program asked for that were replaced since
   * begin(): a face from a font pack (display, Korean, Japanese, Thai,
   * typewriter) that was not given to addFont stands in as Helvetica or
   * Courier. The same one-line note the command-line tool prints on
   * stderr, once per face, saying what to pass to addFont; [] when
   * nothing was replaced. Readable after a PostScriptError too.
   */
  get warnings() {
    return this.#ps.warnings;
  }

  /** A copy of the page as RGBA bytes, row by row from the top. */
  pixels() {
    return new Uint8ClampedArray(this.#ps.pixels().buffer);
  }

  /** The page as { width, height, data, output, warnings }: data is RGBA, ImageData-shaped. */
  image() {
    return {
      width: this.width,
      height: this.height,
      data: this.pixels(),
      output: this.output,
      warnings: this.warnings,
    };
  }

  /** The page as an ImageData (browsers, workers; needs a global ImageData). */
  imageData() {
    if (typeof ImageData === 'undefined') throw new Error('pscat: ImageData is not available here; use image()');
    return new ImageData(this.pixels(), this.width, this.height);
  }

  /** Draw the page into a canvas (resized to the page), via its 2D context. */
  paintTo(canvas) {
    this.#ps.paintTo(canvas);
  }

  /**
   * Make a TrueType/OpenType font available to findfont. `name` is the
   * font pack's file stem, e.g. 'EBGaramond-Regular' (which /Garamond
   * then finds too); the face's own PostScript name also finds it.
   * The PostScript standard 35 are built in; any other face needs this,
   * and without it findfont substitutes (see `warnings`).
   */
  addFont(name, bytes) {
    if (typeof name !== 'string' || name === '') throw new TypeError('pscat: addFont needs a font name');
    const data = bytes instanceof ArrayBuffer ? new Uint8Array(bytes)
      : ArrayBuffer.isView(bytes) ? new Uint8Array(bytes.buffer, bytes.byteOffset, bytes.byteLength)
        : null;
    if (!data) throw new TypeError('pscat: addFont needs the font bytes (Uint8Array or ArrayBuffer)');
    this.#ps.addFont(name, data);
  }
}

async function compileResponse(res) {
  if (!res.ok) throw new Error(`pscat: fetching ${res.url || 'the wasm module'} failed: HTTP ${res.status}`);
  // Streaming compilation needs the application/wasm type, which many
  // servers do not send; fall back to compiling the bytes.
  const type = res.headers.get('content-type') || '';
  if (WebAssembly.compileStreaming && type.startsWith('application/wasm')) {
    return WebAssembly.compileStreaming(res);
  }
  return WebAssembly.compile(await res.arrayBuffer());
}

export async function toModule(src) {
  if (src instanceof WebAssembly.Module) return src;
  if (src instanceof ArrayBuffer || ArrayBuffer.isView(src)) return WebAssembly.compile(src);
  if (typeof Response !== 'undefined' && src instanceof Response) return compileResponse(src);
  if (src && typeof src.then === 'function') return toModule(await src);
  if (typeof src === 'string' || src instanceof URL) return compileResponse(await fetch(src));
  throw new TypeError('pscat: options.wasm must be a URL, a Response, bytes or a WebAssembly.Module');
}

export async function instantiate(module) {
  const instance = await WebAssembly.instantiate(module, {});
  return new Renderer(new Pscat(instance));
}

/**
 * A memoized loader: the first call starts loading, later calls share
 * it, and a failure is not cached (the next call retries).
 */
export function once(load) {
  let pending;
  return () => {
    pending ??= load().catch((e) => {
      pending = undefined;
      throw e;
    });
    return pending;
  };
}
