import { defineConfig } from "vite";

/**
 * `dist/hardless-shim.iife.js`: the shim as one classic script that defines
 * `globalThis.HardlessShim`, for `page.addInitScript` in `@hardless/playwright` (FR-40). A
 * classic script has no `import.meta`, so callers of standalone mode pass `wasmUrl`.
 */
export default defineConfig({
  publicDir: false,
  define: { "import.meta.url": "undefined" },
  build: {
    outDir: "dist",
    emptyOutDir: false,
    sourcemap: false,
    lib: {
      entry: "src/index.ts",
      formats: ["iife"],
      name: "HardlessShim",
      fileName: () => "hardless-shim.iife.js",
    },
  },
});
