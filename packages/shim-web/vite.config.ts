import { defineConfig } from "vite";

/**
 * One-file ESM bundle of the shim (`dist/hardless-shim.js`) for script tags, the Chrome
 * extension and the browser tests. The WebAssembly engine stays a separate, lazily loaded
 * file in `wasm/` (NFR-08).
 */
export default defineConfig({
  build: {
    outDir: "dist",
    emptyOutDir: false,
    sourcemap: true,
    lib: {
      entry: "src/index.ts",
      formats: ["es"],
      fileName: () => "hardless-shim.js",
    },
  },
});
