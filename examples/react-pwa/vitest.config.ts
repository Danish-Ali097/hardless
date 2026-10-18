import { defineConfig } from "vitest/config";

// Unit tests only; e2e/ holds the Playwright acceptance tests.
export default defineConfig({
  test: { include: ["test/**/*.test.ts"] },
});
