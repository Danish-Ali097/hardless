import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { deflateRawSync, gzipSync } from "node:zlib";
import { afterEach, describe, expect, it } from "vitest";
import {
  archiveName,
  binaryName,
  cacheDir,
  ensureBinary,
  extractFromTarGz,
  extractFromZip,
  parseChecksum,
  releaseTarget,
  releaseUrl,
  sha256,
} from "../src/index.js";

/** A one-entry-per-file ustar archive, gzipped, plus an optional GNU long-name entry. */
function tarGz(files: Record<string, string>, longName?: string): Uint8Array {
  const blocks: Buffer[] = [];
  const header = (path: string, size: number, type: string) => {
    const h = Buffer.alloc(512);
    h.write(path.slice(0, 100), 0);
    h.write("0000755\0", 100);
    h.write(`${size.toString(8).padStart(11, "0")}\0`, 124);
    h.write(type, 156);
    h.write("ustar\0", 257);
    h.write("        ", 148);
    let sum = 0;
    for (const b of h) sum += b;
    h.write(`${sum.toString(8).padStart(6, "0")}\0 `, 148);
    return h;
  };
  const pad = (data: Buffer) =>
    Buffer.concat([data, Buffer.alloc((512 - (data.length % 512)) % 512)]);
  if (longName !== undefined) {
    const name = Buffer.from(`${longName}\0`);
    blocks.push(header("././@LongLink", name.length, "L"), pad(name));
    blocks.push(header(longName.slice(0, 100), 4, "0"), pad(Buffer.from("long")));
  }
  for (const [path, text] of Object.entries(files)) {
    const data = Buffer.from(text);
    blocks.push(header(path, data.length, "0"), pad(data));
  }
  blocks.push(Buffer.alloc(1024));
  return gzipSync(Buffer.concat(blocks));
}

/** A zip archive with the given files, stored or deflated. */
function zip(files: Record<string, string>, deflate = true): Uint8Array {
  const locals: Buffer[] = [];
  const centrals: Buffer[] = [];
  let offset = 0;
  for (const [path, text] of Object.entries(files)) {
    const name = Buffer.from(path);
    const raw = Buffer.from(text);
    const data = deflate ? deflateRawSync(raw) : raw;
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(deflate ? 8 : 0, 8);
    local.writeUInt32LE(data.length, 18);
    local.writeUInt32LE(raw.length, 22);
    local.writeUInt16LE(name.length, 26);
    const central = Buffer.alloc(46);
    central.writeUInt32LE(0x02014b50, 0);
    central.writeUInt16LE(deflate ? 8 : 0, 10);
    central.writeUInt32LE(data.length, 20);
    central.writeUInt32LE(raw.length, 24);
    central.writeUInt16LE(name.length, 28);
    central.writeUInt32LE(offset, 42);
    locals.push(local, name, data);
    centrals.push(central, name);
    offset += local.length + name.length + data.length;
  }
  const directory = Buffer.concat(centrals);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(Object.keys(files).length, 8);
  end.writeUInt16LE(Object.keys(files).length, 10);
  end.writeUInt32LE(directory.length, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...locals, directory, end]);
}

const text = (data: Uint8Array) => new TextDecoder().decode(data);

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

  it("names archives and URLs the way cargo-dist publishes them", () => {
    expect(archiveName("x86_64-pc-windows-msvc")).toBe("hardless-cli-x86_64-pc-windows-msvc.zip");
    expect(archiveName("aarch64-apple-darwin")).toBe("hardless-cli-aarch64-apple-darwin.tar.gz");
    expect(releaseUrl("0.1.0", "a.zip")).toBe(
      "https://github.com/hardlessdev/hardless/releases/download/v0.1.0/a.zip",
    );
  });

  it("caches per version in the platform's cache folder", () => {
    expect(cacheDir("0.1.0", "linux", {}, "/home/a")).toBe(
      join("/home/a", ".cache", "hardless", "0.1.0"),
    );
    expect(cacheDir("0.1.0", "linux", { XDG_CACHE_HOME: "/x" }, "/home/a")).toBe(
      join("/x", "hardless", "0.1.0"),
    );
    expect(cacheDir("0.1.0", "darwin", {}, "/Users/a")).toBe(
      join("/Users/a", "Library", "Caches", "hardless", "0.1.0"),
    );
    expect(cacheDir("0.1.0", "win32", { LOCALAPPDATA: "C:\\L" }, "C:\\Users\\a")).toBe(
      join("C:\\L", "hardless", "0.1.0"),
    );
    expect(cacheDir("0.1.0", "linux", { HARDLESS_CACHE_DIR: "/c" }, "/home/a")).toBe(
      join("/c", "0.1.0"),
    );
  });

  it("reads cargo-dist checksum files", () => {
    const hex = "a".repeat(64);
    expect(parseChecksum(`${hex}  hardless-cli-x.tar.gz\n`)).toBe(hex);
    expect(parseChecksum(`${hex.toUpperCase()} *x.zip`)).toBe(hex);
    expect(() => parseChecksum("not a hash")).toThrow(/malformed/);
  });

  it("extracts the binary from a gzipped tar, in any folder", () => {
    const archive = tarGz({
      "hardless-cli-x/README.md": "readme",
      "hardless-cli-x/hardless": "ELF",
    });
    expect(text(extractFromTarGz(archive, "hardless"))).toBe("ELF");
    expect(() => extractFromTarGz(archive, "hardless.exe")).toThrow(/not found/);
  });

  it("follows GNU long names in tar archives", () => {
    const deep = `${"d".repeat(120)}/hardless`;
    expect(text(extractFromTarGz(tarGz({}, deep), "hardless"))).toBe("long");
  });

  it("extracts the binary from a zip, deflated or stored", () => {
    const files = {
      "hardless-cli-x/LICENSE": "Apache",
      "hardless-cli-x/hardless.exe": "MZ binary",
    };
    expect(text(extractFromZip(zip(files), "hardless.exe"))).toBe("MZ binary");
    expect(text(extractFromZip(zip(files, false), "hardless.exe"))).toBe("MZ binary");
    expect(() => extractFromZip(zip(files), "hardless")).toThrow(/not found/);
    expect(() => extractFromZip(new Uint8Array(10), "x")).toThrow(/not a zip/);
  });

  describe("ensureBinary", () => {
    let dir = "";
    afterEach(async () => {
      if (dir) await rm(dir, { recursive: true, force: true });
    });

    /** A fake release server for one archive. */
    function server(archive: Uint8Array, checksum = sha256(archive)) {
      const requested: string[] = [];
      const download = (url: string) => {
        requested.push(url);
        const body = url.endsWith(".sha256") ? `${checksum}  file` : archive;
        return Promise.resolve(typeof body === "string" ? new TextEncoder().encode(body) : body);
      };
      return { requested, download };
    }

    it("downloads, verifies, caches and then reuses the binary", async () => {
      dir = await mkdtemp(join(tmpdir(), "hardless-wrapper-"));
      const env = { HARDLESS_CACHE_DIR: dir };
      const fake = server(tarGz({ "hardless-cli-x86_64-unknown-linux-musl/hardless": "ELF" }));
      const options = { platform: "linux", arch: "x64", env, download: fake.download };

      const path = await ensureBinary("0.1.0", options);
      expect(path).toBe(join(dir, "0.1.0", "hardless"));
      expect(await readFile(path, "utf8")).toBe("ELF");
      expect(fake.requested).toEqual([
        releaseUrl("0.1.0", "hardless-cli-x86_64-unknown-linux-musl.tar.gz"),
        releaseUrl("0.1.0", "hardless-cli-x86_64-unknown-linux-musl.tar.gz.sha256"),
      ]);

      await ensureBinary("0.1.0", options);
      expect(fake.requested).toHaveLength(2);
    });

    it("unpacks the Windows zip", async () => {
      dir = await mkdtemp(join(tmpdir(), "hardless-wrapper-"));
      const fake = server(zip({ "hardless-cli-x86_64-pc-windows-msvc/hardless.exe": "MZ" }));
      const path = await ensureBinary("0.1.0", {
        platform: "win32",
        arch: "x64",
        env: { HARDLESS_CACHE_DIR: dir },
        download: fake.download,
      });
      expect(await readFile(path, "utf8")).toBe("MZ");
    });

    it("refuses an archive whose checksum does not match", async () => {
      dir = await mkdtemp(join(tmpdir(), "hardless-wrapper-"));
      const fake = server(tarGz({ "x/hardless": "tampered" }), "0".repeat(64));
      await expect(
        ensureBinary("0.1.0", {
          platform: "darwin",
          arch: "arm64",
          env: { HARDLESS_CACHE_DIR: dir },
          download: fake.download,
        }),
      ).rejects.toThrow(/checksum mismatch/);
    });

    it("uses HARDLESS_BINARY without downloading", async () => {
      const path = await ensureBinary("0.1.0", { env: { HARDLESS_BINARY: "/opt/hardless" } });
      expect(path).toBe("/opt/hardless");
    });

    it("explains unsupported platforms", async () => {
      await expect(
        ensureBinary("0.1.0", { platform: "freebsd", arch: "x64", env: {} }),
      ).rejects.toThrow(/cargo install hardless-cli/);
    });
  });
});
