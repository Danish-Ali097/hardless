/**
 * NFR-14 for the dashboard bundle: checks the licence of every third-party package whose code
 * ends up in the built files, and writes the notices those licences require
 * (`THIRD_PARTY_LICENSES.txt`) next to the bundle.
 *
 * This complements `scripts/check-npm-licenses.mjs`, which checks declared dependencies: Vite
 * bundles whatever is imported, so this checks what is actually shipped.
 *
 * @packageDocumentation
 */
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { join } from "node:path";
import type { Plugin } from "vite";

/** A third-party package found in the bundle. */
export interface ThirdPartyPackage {
  name: string;
  version: string;
  /** SPDX licence expression from its `package.json`. */
  license: string;
  /** Its licence file, or an empty string if it has none. */
  text: string;
}

/** Options for {@link licenses}. */
export interface LicensesOptions {
  /** Directory of the package being built; dependencies are resolved from here. */
  root: string;
  /** The repository root. Modules inside it (outside `node_modules`) are our own code. */
  repoRoot: string;
  /** Path to `license-policy.json`. */
  policyFile: string;
  /**
   * Packages whose code a build tool writes into the output without an import, e.g.
   * `tailwindcss`, whose base styles are generated into the CSS.
   */
  emittedBy?: readonly string[];
}

interface Policy {
  shipped: string[];
}

/**
 * The package a bundled module belongs to: `{ name }` for a module in `node_modules` or a
 * virtual module such as `\0vite/modulepreload-polyfill.js`, `"own"` for our own code, or
 * `undefined` if the source cannot be identified.
 *
 * @example
 * ```ts
 * moduleSource("/repo/node_modules/.pnpm/react@18.3.1/node_modules/react/index.js", "/repo");
 * // { name: "react", dir: "/repo/node_modules/.pnpm/react@18.3.1/node_modules/react" }
 * ```
 */
export function moduleSource(
  id: string,
  repoRoot: string,
): { name: string; dir?: string } | "own" | undefined {
  if (id.startsWith("\0")) {
    const match = /^(@[^/]+\/[^/]+|[^/]+)\//.exec(id.slice(1));
    return match?.[1] === undefined ? undefined : { name: match[1] };
  }
  const path = (id.split("?")[0] ?? id).replace(/\\/g, "/");
  const marker = "/node_modules/";
  const at = path.lastIndexOf(marker);
  if (at >= 0) {
    const parts = path.slice(at + marker.length).split("/");
    const name = parts[0]?.startsWith("@") ? parts.slice(0, 2).join("/") : parts[0];
    if (name === undefined || name === "") return undefined;
    return { name, dir: path.slice(0, at + marker.length) + name };
  }
  const root = repoRoot.replace(/\\/g, "/").replace(/\/$/, "");
  return path.startsWith(`${root}/`) ? "own" : undefined;
}

/**
 * Whether an SPDX expression is satisfied by the allowed licences: one side of an `OR`, or
 * every part of an `AND`.
 */
export function satisfies(expression: string, allowed: ReadonlySet<string>): boolean {
  const clean = expression.replace(/[()]/g, " ").trim();
  if (/\sOR\s/.test(clean)) return clean.split(/\s+OR\s+/).some((e) => satisfies(e, allowed));
  return clean.split(/\s+AND\s+/).every((id) => allowed.has(id.trim()));
}

function readPackage(dir: string): ThirdPartyPackage {
  const manifest = JSON.parse(readFileSync(join(dir, "package.json"), "utf8")) as {
    name?: string;
    version?: string;
    license?: string | { type?: string };
  };
  const license =
    typeof manifest.license === "string" ? manifest.license : (manifest.license?.type ?? "");
  const file = readdirSync(dir).find((f) => /^(licen[cs]e|copying)(\.|$)/i.test(f));
  return {
    name: manifest.name ?? dir,
    version: manifest.version ?? "",
    license,
    text: file === undefined ? "" : readFileSync(join(dir, file), "utf8").trim(),
  };
}

/** Finds an installed package directory, trying each `from` package in turn. */
function resolvePackageDir(name: string, from: readonly string[]): string | undefined {
  for (const base of from) {
    try {
      const manifest = createRequire(join(base, "package.json")).resolve(`${name}/package.json`);
      return manifest.slice(0, -"/package.json".length);
    } catch {
      // Not resolvable from here; try the next base.
    }
  }
  return undefined;
}

/**
 * Collects the third-party packages behind `moduleIds` (plus `emittedBy`) and checks them
 * against the shipped-licence policy.
 */
export function collectThirdParty(
  moduleIds: Iterable<string>,
  options: LicensesOptions,
): { packages: ThirdPartyPackage[]; errors: string[] } {
  const policy = JSON.parse(readFileSync(options.policyFile, "utf8")) as Policy;
  const allowed = new Set(policy.shipped);
  const errors: string[] = [];
  const dirs = new Map<string, string>();

  // Virtual modules come from build tools, which may be dependencies of Vite, not of us.
  const vite = resolvePackageDir("vite", [options.root]);
  const bases = vite === undefined ? [options.root] : [options.root, vite];
  const want = (name: string, dir?: string) => {
    const found = dir ?? resolvePackageDir(name, bases);
    if (found === undefined || !existsSync(join(found, "package.json"))) {
      errors.push(`cannot find package ${name} to check its licence`);
    } else {
      dirs.set(name, found);
    }
  };

  for (const id of moduleIds) {
    const source = moduleSource(id, options.repoRoot);
    if (source === "own") continue;
    if (source === undefined) {
      errors.push(`cannot tell which package bundled module ${JSON.stringify(id)} comes from`);
      continue;
    }
    if (!dirs.has(source.name)) want(source.name, source.dir);
  }
  for (const name of options.emittedBy ?? []) if (!dirs.has(name)) want(name);

  const packages = [...dirs.values()].map(readPackage).sort((a, b) => a.name.localeCompare(b.name));
  for (const pkg of packages) {
    if (!satisfies(pkg.license, allowed)) {
      errors.push(`${pkg.name}@${pkg.version} is bundled but licensed ${pkg.license || "(none)"}`);
    }
    if (pkg.text === "") errors.push(`${pkg.name}@${pkg.version} has no licence file to ship`);
  }
  return { packages, errors };
}

/** Renders the notices file shipped with the bundle. */
export function noticesText(packages: readonly ThirdPartyPackage[]): string {
  const header =
    "Third-party software included in the Hardless dashboard.\n" +
    "Generated at build time by packages/dashboard/build/licenses.ts.\n";
  const sections = packages.map(
    (p) =>
      `${"=".repeat(78)}\n${p.name} ${p.version} (${p.license})\n${"=".repeat(78)}\n\n${p.text}\n`,
  );
  return [header, ...sections].join("\n");
}

/**
 * Vite plugin: fails the build if a bundled package's licence is not allowed for shipped code
 * (NFR-14), and emits `THIRD_PARTY_LICENSES.txt` and `third-party-licenses.json`.
 */
export function licenses(options: LicensesOptions): Plugin {
  return {
    name: "hardless-licenses",
    apply: "build",
    generateBundle(_output, bundle) {
      const ids = new Set<string>();
      for (const file of Object.values(bundle)) {
        if (file.type === "chunk") for (const id of file.moduleIds) ids.add(id);
      }
      const { packages, errors } = collectThirdParty(ids, options);
      if (errors.length > 0) {
        this.error(`NFR-14 licence check failed:\n  ${errors.join("\n  ")}`);
      }
      this.emitFile({
        type: "asset",
        fileName: "THIRD_PARTY_LICENSES.txt",
        source: noticesText(packages),
      });
      this.emitFile({
        type: "asset",
        fileName: "third-party-licenses.json",
        source: `${JSON.stringify(
          packages.map(({ name, version, license }) => ({ name, version, license })),
          null,
          2,
        )}\n`,
      });
    },
  };
}
