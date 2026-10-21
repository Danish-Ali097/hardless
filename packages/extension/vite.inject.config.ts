import { defineConfig } from "vite";

/**
 * `dist/inject.js`: the shim and its bootstrap as one classic script, for the MAIN-world
 * content script (FR-17).
 */
export default defineConfig({
  publicDir: false,
  // The extension only uses server mode; the standalone engine's `import.meta.url` lookup is
  // never reached, and a classic script has no `import.meta`.
  define: { "import.meta.url": "undefined" },
  build: {
    outDir: "dist",
    emptyOutDir: false,
    lib: {
      entry: "src/inject.ts",
      formats: ["iife"],
      name: "hardlessExtension",
      fileName: () => "inject.js",
    },
  },
});
