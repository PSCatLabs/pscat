// Types for the pscat npm package (#566). The same declarations cover the
// browser entry (index.js) and the Node one (node.js).

/** Where to load the WebAssembly module from. */
export type WasmSource =
  | URL
  | string
  | Response
  | PromiseLike<Response>
  | ArrayBuffer
  | ArrayBufferView
  | WebAssembly.Module;

export interface RendererOptions {
  /**
   * The module to load. Default: the package's own pscat.wasm. In Node a
   * string is a file path (or a URL with a scheme), and a file: URL is read
   * from disk.
   */
  wasm?: WasmSource;
}

export interface PageOptions {
  /** Page width in points (1/72 inch). Default 612 (US Letter). */
  width?: number;
  /** Page height in points. Default 792 (US Letter). */
  height?: number;
  /**
   * Device resolution. Default 72, one pixel per point. SVG and PDF are
   * sized in device pixels as on the command line, so keep 72 for a
   * print-size vector page.
   */
  dpi?: number;
  /** Also record the run as SVG, read with svg() / svgPages(). Default false. */
  svg?: boolean;
  /** Also record the run as PDF, read with pdf(). Default false. */
  pdf?: boolean;
  /**
   * With svg: write Helvetica, Times and Courier text as SVG <text>
   * (selectable, much smaller) rather than glyph outlines, as
   * `pscat --svg-text` does. Default false.
   */
  svgText?: boolean;
  /**
   * With pdf: true (the default) writes text as selectable, searchable,
   * extractable PDF text in embedded subset fonts (a face given to addFont
   * is embedded too), as `pscat --pdf` does; false draws every glyph as its
   * outline, as `pscat --text-as-paths` does. Default true.
   */
  pdfText?: boolean;
  /**
   * Cap on the steps the whole program may run, counted from begin(): objects
   * executed, loop passes, and a step per KiB a decode filter produces, as
   * `pscat --max-steps` counts them. Over it the run throws a PostScriptError
   * with errorName 'limitexceeded' and `limitexceeded (steps)` in its report;
   * the page drawn so far and the renderer survive. 0, null or undefined is no
   * limit, the default, and every begin() sets it afresh, so one program's cap
   * never carries into the next. A whole number, 0 to 2^53 - 1; anything else
   * throws RangeError (a non-number throws TypeError).
   *
   * The module has no clock, so a step cap is the only limit that stops an
   * endless loop, or one operator draining a decompression bomb, in a call to
   * render() or step(). It is a bound on work, not a deadline: an ordinary step
   * takes tens of nanoseconds, a bomb's KiB step about 11 microseconds (so a cap
   * that lets a heavy program finish still lets a bomb run for minutes). To bound each
   * frame of a stepped page instead of the whole program, see
   * FrameOptions.frameMaxSteps.
   */
  maxSteps?: number | null;
}

export interface FrameOptions extends PageOptions {
  /** Objects executed per frame. Default 5000. */
  budget?: number;
  /**
   * Cap on the steps each frame (each step(budget) call) may run, re-armed
   * before every frame so the count restarts: a program of any length passes
   * while no one frame is over it. This is what keeps a tab responsive, since
   * the work that freezes it is one call, and it is how the site's playground
   * bounds a frame (500,000 steps). Same values and errors as `maxSteps`,
   * which it replaces: give one or the other (TypeError otherwise).
   * stepsUsed then reads the latest frame's count.
   */
  frameMaxSteps?: number | null;
}

/** A rendered page. `data` is RGBA, row by row from the top, the ImageData layout. */
export interface RenderResult {
  width: number;
  height: number;
  data: Uint8ClampedArray;
  /** What the program printed with =, ==, print, stack or pstack. */
  output: string;
  /**
   * One note per font the program asked for that was replaced: a pack face
   * (Bangers, a Korean face, ...) not given to addFont stands in as Helvetica
   * or Courier. Each names the face, the stand-in and the addFont call that
   * supplies it. Empty when nothing was replaced.
   */
  warnings: string[];
}

/** A PostScript error raised by render() or step(). The renderer stays usable. */
export class PostScriptError extends Error {
  readonly name: 'PostScriptError';
  /** The PostScript error name: 'undefined', 'typecheck', 'syntaxerror', ... */
  readonly errorName: string;
  /** The offending command, as the interpreter reports it. */
  readonly command: string | undefined;
  /** The source line the error happened on, when known. */
  readonly line: number | undefined;
  /**
   * The `run` file the error happened in, as the program named it; `line` is
   * then that file's line. Undefined for an error in the main program.
   */
  readonly file: string | undefined;
  /**
   * The open `run` frames, innermost first (`inner.ps:5 <- outer.ps:2 <- line 1`),
   * when the error was raised inside a run file. Undefined otherwise.
   */
  readonly runChain: string | undefined;
  /** The full report: `%%[ Error: ...; OffendingCommand: ... ]%%`, plus any hint. */
  readonly report: string;
  /** What the program printed before the error. */
  readonly output: string;
  /**
   * Font substitution notes up to the error, as in RenderResult.warnings: a
   * face that fell back to Helvetica often explains the error that followed.
   * A snapshot; empty when nothing was replaced.
   */
  readonly warnings: string[];
}

/** Anything with a 2D context to draw into: HTMLCanvasElement, OffscreenCanvas. */
export interface CanvasLike {
  width: number;
  height: number;
  getContext(kind: '2d'): { putImageData(data: ImageData, x: number, y: number): void } | null;
}

/** One interpreter in its own WebAssembly instance. */
export class Renderer {
  private constructor();
  /** Run a program to completion on a fresh page. Throws PostScriptError. */
  render(source: string | Uint8Array, options?: PageOptions): RenderResult;
  /** Queue a program on a fresh page, to drive with step(). */
  begin(source: string | Uint8Array, options?: PageOptions): void;
  /** Execute up to `budget` objects (default 5000). True while there is more work. Throws PostScriptError. */
  step(budget?: number): boolean;
  /** begin(), then one step(budget) per iteration, yielding the frame number. The last frame is the finished page. */
  frames(source: string | Uint8Array, options?: FrameOptions): Generator<number, void, undefined>;
  /** Page width in device pixels. */
  readonly width: number;
  /** Page height in device pixels. */
  readonly height: number;
  /** What the program has printed since begin(). */
  readonly output: string;
  /** Font substitution notes since begin(), as in RenderResult.warnings. */
  readonly warnings: string[];
  /**
   * Steps run since the cap was last armed: begin() under `maxSteps`, or the
   * start of the latest frame under `frameMaxSteps`. Counted only while a cap
   * is set (0 without one) and 0 before begin(); it trails the true figure by
   * up to a slice of a few thousand. Readable after a PostScriptError, a
   * `limitexceeded` one included, where it is at least the cap.
   */
  readonly stepsUsed: number;
  /** A copy of the page as RGBA bytes. */
  pixels(): Uint8ClampedArray;
  /** The page as width, height, RGBA data, output and warnings. */
  image(): RenderResult;
  /**
   * Pages the run holds now: those showpage/copypage emitted, plus the page
   * being drawn when it holds art (or is the only one). Works between step()
   * calls and after the run, whatever is recorded (pixels, SVG, PDF); equals
   * svgPages().length with { svg: true } and the PDF's page count with
   * { pdf: true }. 0 before begin().
   */
  pageCount(): number;
  /**
   * SVG page `page` (from 0): the bytes `pscat --svg` writes for the same
   * program, page size and dpi. Needs a program begun with { svg: true };
   * throws RangeError for a page the document does not have.
   */
  svg(page?: number): string;
  /** Every SVG page in order. Needs { svg: true }. */
  svgPages(): string[];
  /**
   * The whole document as one PDF: the bytes `pscat --pdf` writes for the
   * same program, page size and dpi. Needs { pdf: true }.
   */
  pdf(): Uint8Array;
  /** The page as an ImageData (needs a global ImageData: browsers, workers). */
  imageData(): ImageData;
  /** Draw the page into a canvas, resizing it to the page. */
  paintTo(canvas: CanvasLike): void;
  /**
   * Make a TrueType/OpenType font available to findfont, under `name`: the
   * font pack's file stem, e.g. 'EBGaramond-Regular' (/Garamond then finds it
   * too). The PostScript standard 35 are built in; other faces stand in until
   * added, and say so in `warnings`.
   */
  addFont(name: string, bytes: ArrayBuffer | ArrayBufferView): void;
}

/** Load the interpreter and return a Renderer. */
export function createRenderer(options?: RendererOptions): Promise<Renderer>;
