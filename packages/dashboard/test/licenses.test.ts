import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { collectThirdParty, moduleSource, noticesText, satisfies } from "../build/licenses.js";

const root = fileURLToPath(new URL("..", import.meta.url));
const repoRoot = fileURLToPath(new URL("../../..", import.meta.url));
const policyFile = fileURLToPath(new URL("../../../license-policy.json", import.meta.url));

describe("bundle licence check", () => {
  it("NFR-14: identifies the package behind every kind of bundled module", () => {
    expect(
      moduleSource(
        "C:\\r\\node_modules\\.pnpm\\react@18.3.1\\node_modules\\react\\index.js",
        "C:\\r",
      ),
    ).toEqual({ name: "react", dir: "C:/r/node_modules/.pnpm/react@18.3.1/node_modules/react" });
    expect(moduleSource("/r/node_modules/@scope/pkg/dist/x.js?v=1", "/r")).toEqual({
      name: "@scope/pkg",
      dir: "/r/node_modules/@scope/pkg",
    });
    expect(moduleSource("\0vite/modulepreload-polyfill.js", "/r")).toEqual({ name: "vite" });
    expect(moduleSource("/r/packages/dashboard/src/App.tsx", "/r")).toBe("own");
    expect(moduleSource("/elsewhere/lib.js", "/r")).toBeUndefined();
  });

  it("NFR-14: evaluates SPDX expressions", () => {
    const allowed = new Set(["MIT", "Apache-2.0"]);
    expect(satisfies("MIT", allowed)).toBe(true);
    expect(satisfies("(MIT OR GPL-3.0)", allowed)).toBe(true);
    expect(satisfies("MIT AND GPL-3.0", allowed)).toBe(false);
    expect(satisfies("MPL-2.0", allowed)).toBe(false);
  });

  it("NFR-14: accepts the real dashboard dependencies and collects their licence texts", () => {
    const { packages, errors } = collectThirdParty(["\0vite/modulepreload-polyfill.js"], {
      root,
      repoRoot,
      policyFile,
      emittedBy: ["react", "tailwindcss"],
    });
    expect(errors).toEqual([]);
    expect(packages.map((p) => p.name)).toEqual(["react", "tailwindcss", "vite"]);
    expect(packages.every((p) => p.license === "MIT" && p.text.length > 0)).toBe(true);
    expect(noticesText(packages)).toContain("react 18.");
  });

  it("NFR-14: rejects a bundled package whose licence is not allowed for shipped code", () => {
    const dir = mkdtempSync(join(tmpdir(), "hardless-licences-"));
    const pkg = join(dir, "node_modules", "copyleft");
    const fake = join(pkg, "index.js");
    // A fake installed package with a licence that may not ship.
    mkdirSync(pkg, { recursive: true });
    writeFileSync(
      join(pkg, "package.json"),
      JSON.stringify({ name: "copyleft", version: "1.0.0", license: "GPL-3.0-only" }),
    );
    writeFileSync(join(pkg, "LICENSE"), "GPL");
    writeFileSync(fake, "");
    const { errors } = collectThirdParty([fake], { root, repoRoot, policyFile });
    expect(errors).toEqual(["copyleft@1.0.0 is bundled but licensed GPL-3.0-only"]);
  });

  it("NFR-14: refuses modules whose source it cannot identify", () => {
    const { errors } = collectThirdParty(["/somewhere/else/lib.js"], {
      root,
      repoRoot,
      policyFile,
    });
    expect(errors).toHaveLength(1);
  });
});
