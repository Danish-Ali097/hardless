import { defineConfig } from "vite";

/**
 * The popup (`popup.html` and its script) plus `public/manifest.json`, into `dist/`. The
 * injected shim is a second build (`vite.inject.config.ts`) because content scripts cannot be
 * ES modules. `dist/` is the unpacked extension.
 */
export default defineConfig({
  base: "./",
  build: {
    outDir: "dist",
    emptyOutDir: true,
    rollupOptions: { input: "popup.html" },
  },
});
