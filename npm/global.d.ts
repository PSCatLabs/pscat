// Types for the classic-script build, pscat.global.js (#1108): the one global
// it defines, `pscat`, holds the same three values the ES module exports
// (npm/test/global.test.mjs keeps the two lists equal). Opt in where the
// script tag is, with
//
//   /// <reference types="pscat/global" />
//
// `import 'pscat'` alone does not declare a global. The other types (RenderResult, RendererOptions, ...)
// are index.d.ts's: `import type { RenderResult } from 'pscat'`.
import type { createRenderer, PostScriptError, Renderer } from './index.js';

declare global {
  /** The classic-script build's global; see pscat.global.js. */
  var pscat: {
    createRenderer: typeof createRenderer;
    PostScriptError: typeof PostScriptError;
    Renderer: typeof Renderer;
  };
}

export {};
