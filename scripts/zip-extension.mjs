// Builds the Chrome MV3 extension and zips it for a GitHub Release (publish-extension.yml)
// or a dry run (ci.yml): packages/extension/hardless-extension-<version>.zip, with the
// release version in manifest.json and Hardless's LICENSE inside. The extension bundles only
// Hardless's own code (server mode, no WebAssembly), so it needs no third-party notices.
//
//   node scripts/zip-extension.mjs [--version 0.1.0] [--no-build]
import { execFileSync } from "node:child_process";
import { copyFileSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { crc32, deflateRawSync } from "node:zlib";

const root = resolve(import.meta.dirname, "..");
const extension = join(root, "packages/extension");
const dist = join(extension, "dist");

function option(name) {
  const i = process.argv.indexOf(name);
  return i === -1 ? undefined : process.argv[i + 1];
}

const version =
  option("--version") ?? JSON.parse(readFileSync(join(extension, "package.json"), "utf8")).version;
if (!/^\d+\.\d+\.\d+$/.test(version)) {
  // Chrome accepts only dot-separated integers as an extension version.
  throw new Error(`zip-extension: ${version} is not a valid extension version`);
}

if (!process.argv.includes("--no-build")) {
  execFileSync("pnpm", ["--filter", "@hardless/extension", "build"], {
    cwd: root,
    stdio: "inherit",
    shell: process.platform === "win32",
  });
}
const manifestPath = join(dist, "manifest.json");
const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
manifest.version = version;
writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
copyFileSync(join(root, "LICENSE"), join(dist, "LICENSE"));

/** Every file under `dir`, as paths relative to `dist` with `/` separators. */
function files(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory()
      ? files(path)
      : [relative(dist, path).replaceAll("\\", "/")];
  });
}

// A plain zip (deflate, no dependencies): local headers, then the central directory.
const locals = [];
const centrals = [];
let offset = 0;
for (const name of files(dist).sort()) {
  const raw = readFileSync(join(dist, name));
  const data = deflateRawSync(raw);
  const path = Buffer.from(name);
  const header = (signature, size) => {
    const h = Buffer.alloc(size);
    h.writeUInt32LE(signature, 0);
    return h;
  };
  const local = header(0x04034b50, 30);
  local.writeUInt16LE(20, 4);
  local.writeUInt16LE(8, 8);
  local.writeUInt32LE(crc32(raw), 14);
  local.writeUInt32LE(data.length, 18);
  local.writeUInt32LE(raw.length, 22);
  local.writeUInt16LE(path.length, 26);
  const central = header(0x02014b50, 46);
  central.writeUInt16LE(20, 4);
  central.writeUInt16LE(20, 6);
  central.writeUInt16LE(8, 10);
  central.writeUInt32LE(crc32(raw), 16);
  central.writeUInt32LE(data.length, 20);
  central.writeUInt32LE(raw.length, 24);
  central.writeUInt16LE(path.length, 28);
  central.writeUInt32LE(offset, 42);
  locals.push(local, path, data);
  centrals.push(central, path);
  offset += local.length + path.length + data.length;
}
const directory = Buffer.concat(centrals);
const end = Buffer.alloc(22);
end.writeUInt32LE(0x06054b50, 0);
end.writeUInt16LE(centrals.length / 2, 8);
end.writeUInt16LE(centrals.length / 2, 10);
end.writeUInt32LE(directory.length, 12);
end.writeUInt32LE(offset, 16);

for (const old of readdirSync(extension).filter((n) => /^hardless-extension-.*\.zip$/.test(n))) {
  rmSync(join(extension, old));
}
const zip = join(extension, `hardless-extension-${version}.zip`);
writeFileSync(zip, Buffer.concat([...locals, directory, end]));
console.log(`wrote ${relative(root, zip)} (${String(centrals.length / 2)} files)`);
