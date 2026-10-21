import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { bridgeUrlProblem, INJECTION, originPattern, parseConfig, scriptId } from "../src/index.js";

interface Manifest {
  manifest_version: number;
  host_permissions?: string[];
  optional_host_permissions?: string[];
  permissions: string[];
  content_scripts: unknown[];
  action?: { default_popup?: string };
}

const manifest = JSON.parse(
  readFileSync(new URL("../public/manifest.json", import.meta.url), "utf8"),
) as Manifest;

describe("extension manifest", () => {
  it("is Manifest V3 with a popup", () => {
    expect(manifest.manifest_version).toBe(3);
    expect(manifest.action?.default_popup).toBe("popup.html");
  });

  it("NFR-03: requests no host access up front; origins are enabled by the user", () => {
    expect(manifest.host_permissions ?? []).toEqual([]);
    expect(manifest.content_scripts).toEqual([]);
    expect(manifest.optional_host_permissions).toEqual(["http://*/*", "https://*/*"]);
    expect(manifest.permissions.sort()).toEqual(["scripting", "storage"]);
  });

  it("FR-17: injects in the MAIN world at document_start", () => {
    expect(INJECTION).toEqual({ world: "MAIN", runAt: "document_start" });
  });
});

describe("extension helpers", () => {
  it("NFR-03: runs only on http and https pages, per host", () => {
    expect(originPattern("http://localhost:5173/pos?x=1")).toBe("http://localhost/*");
    expect(originPattern("https://pos.example.test/")).toBe("https://pos.example.test/*");
    expect(originPattern("chrome://extensions")).toBeNull();
    expect(originPattern("file:///C:/app.html")).toBeNull();
    expect(originPattern("not a url")).toBeNull();
    expect(scriptId("http://localhost:5173")).toBe("hardless:http://localhost:5173");
  });

  it("checks bridge URLs and stored settings", () => {
    expect(bridgeUrlProblem("ws://localhost:8080/bridge")).toBeNull();
    expect(bridgeUrlProblem("wss://192.168.1.5:8443/bridge")).toBeNull();
    expect(bridgeUrlProblem("http://localhost:8080")).toMatch(/ws:\/\//);
    expect(bridgeUrlProblem("localhost")).toMatch(/full URL/);
    expect(parseConfig('{"url":"ws://localhost:8080/bridge","token":"t"}')).toEqual({
      url: "ws://localhost:8080/bridge",
      token: "t",
    });
    expect(parseConfig('{"url":"ws://x/bridge","token":""}')).toEqual({ url: "ws://x/bridge" });
    for (const bad of [null, "", "{", "null", '{"url":"http://x"}', '{"url":3}']) {
      expect(parseConfig(bad)).toBeNull();
    }
  });
});
