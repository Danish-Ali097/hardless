import { describe, expect, it } from "vitest";
import { binaryName, releaseTarget } from "../src/index.js";

describe("hardless npm wrapper", () => {
  it("NFR-11: maps every supported OS and CPU to a release target", () => {
    expect(releaseTarget("win32", "x64")).toBe("x86_64-pc-windows-msvc");
    expect(releaseTarget("win32", "arm64")).toBe("aarch64-pc-windows-msvc");
    expect(releaseTarget("darwin", "x64")).toBe("x86_64-apple-darwin");
    expect(releaseTarget("darwin", "arm64")).toBe("aarch64-apple-darwin");
    expect(releaseTarget("linux", "x64")).toBe("x86_64-unknown-linux-musl");
    expect(releaseTarget("linux", "arm64")).toBe("aarch64-unknown-linux-musl");
  });

  it("returns undefined for unsupported platforms", () => {
    expect(releaseTarget("freebsd", "x64")).toBeUndefined();
    expect(releaseTarget("linux", "ia32")).toBeUndefined();
  });

  it("adds .exe on Windows", () => {
    expect(binaryName("win32")).toBe("hardless.exe");
    expect(binaryName("linux")).toBe("hardless");
  });
});
