/**
 * The `hardless` npm package: `npx hardless` downloads the `hardless` binary for the current
 * platform from GitHub Releases, checks its SHA-256, caches it and runs it.
 *
 * No dependencies: archives are unpacked with Node's own `zlib`, so nothing third-party ships.
 *
 * @packageDocumentation
 */
import { createHash } from "node:crypto";
import { chmod, mkdir, rename, rm, stat, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { gunzipSync, inflateRawSync } from "node:zlib";

/** A Rust target triple the `hardless` binary is released for (NFR-11). */
export type ReleaseTarget =
  | "x86_64-pc-windows-msvc"
  | "aarch64-pc-windows-msvc"
  | "x86_64-apple-darwin"
  | "aarch64-apple-darwin"
  | "x86_64-unknown-linux-musl"
  | "aarch64-unknown-linux-musl";

/** Where releases are published. */
export const RELEASES = "https://github.com/hardlessdev/hardless/releases/download";

/**
 * The release target for a Node.js `process.platform` and `process.arch`, or `undefined` if
 * Hardless has no binary for it. Linux uses the static musl build so it runs on any distro.
 *
 * @example
 * ```ts
 * releaseTarget("darwin", "arm64"); // "aarch64-apple-darwin"
 * ```
 */
export function releaseTarget(platform: string, arch: string): ReleaseTarget | undefined {
  const cpu = arch === "x64" ? "x86_64" : arch === "arm64" ? "aarch64" : undefined;
  if (cpu === undefined) return undefined;
  switch (platform) {
    case "win32":
      return `${cpu}-pc-windows-msvc`;
    case "darwin":
      return `${cpu}-apple-darwin`;
    case "linux":
      return `${cpu}-unknown-linux-musl`;
    default:
      return undefined;
  }
}

/** The file name of the binary inside a release archive. */
export function binaryName(platform: string): string {
  return platform === "win32" ? "hardless.exe" : "hardless";
}

/**
 * The release archive for a target, as cargo-dist names it: a zip on Windows, a gzipped tar
 * elsewhere (`dist-workspace.toml`).
 */
export function archiveName(target: ReleaseTarget): string {
  return `hardless-cli-${target}.${target.endsWith("windows-msvc") ? "zip" : "tar.gz"}`;
}

/** The download URL of a release file for version `version` (without the `v`). */
export function releaseUrl(version: string, file: string): string {
  return `${RELEASES}/v${version}/${file}`;
}

/**
 * The folder the binary for `version` is cached in: `HARDLESS_CACHE_DIR` if set, otherwise
 * the platform's user cache folder.
 */
export function cacheDir(
  version: string,
  platform: string = process.platform,
  env: NodeJS.ProcessEnv = process.env,
  home: string = homedir(),
): string {
  if (env.HARDLESS_CACHE_DIR) return join(env.HARDLESS_CACHE_DIR, version);
  if (platform === "win32") {
    return join(env.LOCALAPPDATA ?? join(home, "AppData", "Local"), "hardless", version);
  }
  if (platform === "darwin") return join(home, "Library", "Caches", "hardless", version);
  return join(env.XDG_CACHE_HOME ?? join(home, ".cache"), "hardless", version);
}

/** The hex digest from a cargo-dist `.sha256` file (`<hex>  <file name>` or just `<hex>`). */
export function parseChecksum(text: string): string {
  const hex = text.trim().split(/\s+/)[0]?.toLowerCase() ?? "";
  if (!/^[0-9a-f]{64}$/.test(hex)) throw new Error("hardless: malformed .sha256 file");
  return hex;
}

/** The SHA-256 of `data` as lowercase hex. */
export function sha256(data: Uint8Array): string {
  return createHash("sha256").update(data).digest("hex");
}

/** The last path segment, for `/` and `\` separators. */
function baseName(path: string): string {
  return path.split(/[/\\]/).pop() ?? path;
}

/**
 * The file called `name` (in any folder) from a gzipped tar archive. Handles ustar, GNU long
 * names and pax headers; only the file's base name is compared.
 */
export function extractFromTarGz(archive: Uint8Array, name: string): Uint8Array {
  const tar = gunzipSync(archive);
  let offset = 0;
  let longName: string | undefined;
  while (offset + 512 <= tar.length) {
    const header = tar.subarray(offset, offset + 512);
    if (header.every((b) => b === 0)) break;
    const field = (start: number, length: number) =>
      header
        .subarray(start, start + length)
        .toString("utf8")
        .replace(/\0.*$/s, "");
    const size = Number.parseInt(field(124, 12).trim() || "0", 8);
    const type = field(156, 1);
    const prefix = field(345, 155);
    const body = tar.subarray(offset + 512, offset + 512 + size);
    const path = longName ?? (prefix ? `${prefix}/${field(0, 100)}` : field(0, 100));
    longName = undefined;
    if (type === "L") {
      longName = body.toString("utf8").replace(/\0.*$/s, "");
    } else if ((type === "0" || type === "") && baseName(path) === name) {
      return new Uint8Array(body);
    }
    offset += 512 + Math.ceil(size / 512) * 512;
  }
  throw new Error(`hardless: ${name} not found in the release archive`);
}

/** The file called `name` (in any folder) from a zip archive (stored or deflated). */
export function extractFromZip(archive: Uint8Array, name: string): Uint8Array {
  const zip = Buffer.from(archive.buffer, archive.byteOffset, archive.byteLength);
  let end = -1;
  for (let i = zip.length - 22; i >= Math.max(0, zip.length - 22 - 0xffff); i--) {
    if (zip.readUInt32LE(i) === 0x06054b50) {
      end = i;
      break;
    }
  }
  if (end < 0) throw new Error("hardless: the release archive is not a zip file");
  const count = zip.readUInt16LE(end + 10);
  let entry = zip.readUInt32LE(end + 16);
  for (let i = 0; i < count; i++) {
    if (zip.readUInt32LE(entry) !== 0x02014b50) break;
    const method = zip.readUInt16LE(entry + 10);
    const compressed = zip.readUInt32LE(entry + 20);
    const nameLength = zip.readUInt16LE(entry + 28);
    const extraLength = zip.readUInt16LE(entry + 30);
    const commentLength = zip.readUInt16LE(entry + 32);
    const local = zip.readUInt32LE(entry + 42);
    const path = zip.toString("utf8", entry + 46, entry + 46 + nameLength);
    if (!path.endsWith("/") && baseName(path) === name) {
      const start = local + 30 + zip.readUInt16LE(local + 26) + zip.readUInt16LE(local + 28);
      const data = zip.subarray(start, start + compressed);
      if (method === 0) return new Uint8Array(data);
      if (method === 8) return new Uint8Array(inflateRawSync(data));
      throw new Error(`hardless: unsupported zip compression method ${String(method)}`);
    }
    entry += 46 + nameLength + extraLength + commentLength;
  }
  throw new Error(`hardless: ${name} not found in the release archive`);
}

/** Downloads `url` and returns its bytes, failing on any HTTP error. */
async function download(url: string): Promise<Uint8Array> {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`hardless: download failed (${String(response.status)}) for ${url}`);
  }
  return new Uint8Array(await response.arrayBuffer());
}

/** Options for {@link ensureBinary}; the defaults describe the running machine. */
export interface EnsureOptions {
  platform?: string;
  arch?: string;
  env?: NodeJS.ProcessEnv;
  /** Replaces `fetch` for tests. */
  download?: (url: string) => Promise<Uint8Array>;
}

/**
 * The path to the `hardless` binary for `version`, downloading and caching it on first use.
 * `HARDLESS_BINARY` skips the download and points at a binary you already have.
 */
export async function ensureBinary(version: string, options: EnsureOptions = {}): Promise<string> {
  const platform = options.platform ?? process.platform;
  const arch = options.arch ?? process.arch;
  const env = options.env ?? process.env;
  if (env.HARDLESS_BINARY) return env.HARDLESS_BINARY;

  const target = releaseTarget(platform, arch);
  if (target === undefined) {
    throw new Error(
      `hardless: no prebuilt binary for ${platform}/${arch}. ` +
        "Install from source with `cargo install hardless-cli`.",
    );
  }
  const binary = join(cacheDir(version, platform, env), binaryName(platform));
  if (
    await stat(binary).then(
      (s) => s.isFile(),
      () => false,
    )
  )
    return binary;

  const get = options.download ?? download;
  const archive = archiveName(target);
  const [data, checksum] = await Promise.all([
    get(releaseUrl(version, archive)),
    get(releaseUrl(version, `${archive}.sha256`)),
  ]);
  const expected = parseChecksum(new TextDecoder().decode(checksum));
  if (sha256(data) !== expected) {
    throw new Error(`hardless: checksum mismatch for ${archive}; not running it`);
  }
  const executable = archive.endsWith(".zip")
    ? extractFromZip(data, binaryName(platform))
    : extractFromTarGz(data, binaryName(platform));

  // Write next to the final path and rename, so an interrupted download never leaves a
  // half-written binary in the cache.
  await mkdir(dirname(binary), { recursive: true });
  const partial = `${binary}.${String(process.pid)}.partial`;
  try {
    await writeFile(partial, executable);
    await chmod(partial, 0o755);
    await rename(partial, binary);
  } finally {
    await rm(partial, { force: true });
  }
  return binary;
}
