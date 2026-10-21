// Packs the public npm packages for a release (publish-npm.yml) or a dry run (ci.yml), and
// checks each tarball before anything is published:
//
// - its version matches the release tag (with --tag vX.Y.Z);
// - it has LICENSE, its built files, and, for @hardless/shim-web, the WebAssembly engine and
//   its third-party notices (NFR-14);
// - its package.json has no `workspace:` ranges left and its exports point at dist/.
//
// Writes the tarballs and order.txt (publish order: dependencies first) to --out.
//
//   node scripts/pack-npm.mjs [--tag v0.1.0] --out npm-packs
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");

/** Publish order, dependencies first, with the files each tarball must contain. */
const PACKAGES = [
  { dir: "packages/types", files: ["LICENSE", "dist/index.js", "dist/index.d.ts"] },
  {
    dir: "packages/shim-web",
    files: [
      "LICENSE",
      "THIRD_PARTY_LICENSES.txt",
      "dist/index.js",
      "dist/index.d.ts",
      "dist/hardless-shim.js",
      "dist/hardless-shim.iife.js",
      "wasm/hardless_wasm.js",
      "wasm/hardless_wasm_bg.wasm",
    ],
  },
  { dir: "packages/playwright", files: ["LICENSE", "dist/index.js", "dist/index.d.ts"] },
  { dir: "packages/npm-wrapper", files: ["LICENSE", "README.md", "dist/cli.js", "dist/index.js"] },
];

function option(name) {
  const i = process.argv.indexOf(name);
  return i === -1 ? undefined : process.argv[i + 1];
}

const tag = option("--tag");
const out = resolve(option("--out") ?? "npm-packs");
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });

const problems = [];
const order = [];
for (const { dir, files } of PACKAGES) {
  const manifest = JSON.parse(readFileSync(join(root, dir, "package.json"), "utf8"));
  const label = `${manifest.name}@${manifest.version}`;
  if (tag !== undefined && tag !== `v${manifest.version}`) {
    problems.push(`${label}: version does not match the tag ${tag}`);
  }
  for (const file of files) {
    if (!existsSync(join(root, dir, file)))
      problems.push(`${label}: ${file} is missing (build first?)`);
  }

  const before = new Set(readdirSync(out));
  execFileSync("pnpm", ["pack", "--pack-destination", out], {
    cwd: join(root, dir),
    stdio: ["ignore", "ignore", "inherit"],
    shell: process.platform === "win32",
  });
  const tarball = readdirSync(out).find((name) => !before.has(name) && name.endsWith(".tgz"));
  if (tarball === undefined) {
    problems.push(`${label}: pnpm pack wrote no tarball`);
    continue;
  }
  order.push(tarball);

  // tar is on every CI runner and on Windows 10+.
  const listing = execFileSync("tar", ["-tzf", tarball], { cwd: out, encoding: "utf8" })
    .split(/\r?\n/)
    .map((line) => line.replace(/^package\//, ""));
  for (const file of files) {
    if (!listing.includes(file)) problems.push(`${label}: ${file} is not in ${tarball}`);
  }
  const packed = JSON.parse(
    execFileSync("tar", ["-xzOf", tarball, "package/package.json"], { cwd: out, encoding: "utf8" }),
  );
  const ranges = {
    ...packed.dependencies,
    ...packed.peerDependencies,
    ...packed.optionalDependencies,
  };
  for (const [name, range] of Object.entries(ranges)) {
    if (String(range).startsWith("workspace:"))
      problems.push(`${label}: ${name} is still ${range}`);
  }
  if (JSON.stringify(packed.exports ?? {}).includes("./src/")) {
    problems.push(`${label}: exports still point at src/ (publishConfig not applied)`);
  }
  console.log(`packed ${label} -> ${tarball} (${String(listing.filter(Boolean).length)} files)`);
}

writeFileSync(join(out, "order.txt"), `${order.join("\n")}\n`);
if (problems.length > 0) {
  console.error(`\n${problems.join("\n")}`);
  process.exit(1);
}
console.log(`\nall ${String(order.length)} packages are ready in ${out}`);
