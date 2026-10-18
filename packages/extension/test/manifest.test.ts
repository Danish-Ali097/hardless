import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { INJECTION } from "../src/index.js";

interface Manifest {
  manifest_version: number;
  host_permissions?: string[];
  content_scripts: unknown[];
}

const manifest = JSON.parse(
  readFileSync(new URL("../manifest.json", import.meta.url), "utf8"),
) as Manifest;

describe("extension manifest", () => {
  it("is Manifest V3", () => {
    expect(manifest.manifest_version).toBe(3);
  });

  it("NFR-03: requests no host access up front; origins are enabled by the user", () => {
    expect(manifest.host_permissions ?? []).toEqual([]);
    expect(manifest.content_scripts).toEqual([]);
  });

  it("FR-17: injects in the MAIN world at document_start", () => {
    expect(INJECTION).toEqual({ world: "MAIN", runAt: "document_start" });
  });
});
