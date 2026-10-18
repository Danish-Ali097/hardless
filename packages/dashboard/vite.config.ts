import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  // hardless-server serves these files at `/` (FR-21).
  base: "./",
  build: { outDir: "dist", emptyOutDir: true },
});
