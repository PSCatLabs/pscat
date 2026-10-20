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
  /** Device resolution. Default 72, one pixel per point. */
  dpi?: number;
}

export interface FrameOptions extends PageOptions {
  /** Objects executed per frame. Default 5000. */
  budget?: number;
}

/** A rendered page. `data` is RGBA, row by row from the top, the ImageData layout. */
export interface RenderResult {
  width: number;
  height: number;
  data: Uint8ClampedArray;
  /** What the program printed with =, ==, print, stack or pstack. */
  output: string;
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
  /** The full report: `%%[ Error: ...; OffendingCommand: ... ]%%`, plus any hint. */
  readonly report: string;
  /** What the program printed before the error. */
  readonly output: string;
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
  /** A copy of the page as RGBA bytes. */
  pixels(): Uint8ClampedArray;
  /** The page as width, height, RGBA data and output. */
  image(): RenderResult;
  /** The page as an ImageData (needs a global ImageData: browsers, workers). */
  imageData(): ImageData;
  /** Draw the page into a canvas, resizing it to the page. */
  paintTo(canvas: CanvasLike): void;
  /**
   * Make a TrueType/OpenType font available to findfont, under `name`: the
   * font pack's file stem, e.g. 'EBGaramond-Regular' (/Garamond then finds it
   * too). Only the Liberation faces behind the base fonts are built in.
   */
  addFont(name: string, bytes: ArrayBuffer | ArrayBufferView): void;
}

/** Load the interpreter and return a Renderer. */
export function createRenderer(options?: RendererOptions): Promise<Renderer>;
