import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import { licenses } from "./build/licenses.js";

const root = fileURLToPath(new URL(".", import.meta.url));
const repoRoot = fileURLToPath(new URL("../..", import.meta.url));

export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
    // NFR-14: check bundled licences and ship their notices.
    licenses({
      root,
      repoRoot,
      policyFile: fileURLToPath(new URL("../../license-policy.json", import.meta.url)),
      emittedBy: ["tailwindcss"],
    }),
  ],
  // hardless-server serves these files at `/` (FR-21).
  base: "./",
  build: { outDir: "dist", emptyOutDir: true },
});
