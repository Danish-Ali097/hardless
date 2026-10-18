// NFR-14 licence check for npm dependencies, using the policy in license-policy.json:
//
// - Shipped (production `dependencies` of any package): only licences in `shipped`.
// - Build and dev tools (everything else): `shipped` plus `buildToolsAlsoAllow`.
// - `neverAllow` (GPL family, AGPL, SSPL, ...) fails anywhere, and so does any licence not
//   listed at all, so new licences need an explicit decision.
//
// What actually ends up in the dashboard bundle is checked separately at build time by
// packages/dashboard/build/licenses.ts. See "Licences" in docs/decisions.md.
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

const policy = JSON.parse(readFileSync(new URL("../license-policy.json", import.meta.url), "utf8"));
const SHIPPED = new Set(policy.shipped);
const BUILD_TOOLS = new Set([...policy.shipped, ...policy.buildToolsAlsoAllow]);
const NEVER = new Set(policy.neverAllow);

/** `pnpm licenses list` grouped by licence expression. */
function licenses(prodOnly) {
  const args = ["licenses", "list", "--recursive", "--json", ...(prodOnly ? ["--prod"] : [])];
  const output = execFileSync("pnpm", args, {
    encoding: "utf8",
    shell: process.platform === "win32",
  });
  /** @type {Record<string, { name: string; versions: string[] }[]>} */
  const byLicense = output.trim() === "" ? {} : JSON.parse(output);
  return byLicense;
}

/** Whether an SPDX expression is satisfied by `allowed`: one side of an OR, every part of an AND. */
function satisfies(expression, allowed) {
  const clean = expression.replace(/[()]/g, " ").trim();
  if (/\sOR\s/.test(clean)) return clean.split(/\s+OR\s+/).some((e) => satisfies(e, allowed));
  return clean.split(/\s+AND\s+/).every((id) => allowed.has(id.trim()));
}

/** Whether an SPDX expression can only be met through a never-allowed licence. */
function forbidden(expression) {
  const clean = expression.replace(/[()]/g, " ").trim();
  if (/\sOR\s/.test(clean)) return clean.split(/\s+OR\s+/).every(forbidden);
  return clean.split(/\s+AND\s+/).some((id) => NEVER.has(id.trim()));
}

const errors = [];
const prod = licenses(true);
const all = licenses(false);

const shippedNames = new Set();
let shippedCount = 0;
for (const [license, packages] of Object.entries(prod)) {
  for (const pkg of packages) {
    shippedNames.add(pkg.name);
    shippedCount += 1;
    if (!satisfies(license, SHIPPED)) {
      errors.push(`shipped dependency ${pkg.name}@${pkg.versions.join(",")} is ${license}`);
    }
  }
}

let toolCount = 0;
const toolExceptions = [];
for (const [license, packages] of Object.entries(all)) {
  for (const pkg of packages) {
    if (shippedNames.has(pkg.name)) continue;
    toolCount += 1;
    const id = `${pkg.name}@${pkg.versions.join(",")}`;
    if (forbidden(license)) {
      errors.push(`build tool ${id} is ${license}, which is never allowed`);
    } else if (!satisfies(license, BUILD_TOOLS)) {
      errors.push(`build tool ${id} is ${license}, which is not in license-policy.json`);
    } else if (!satisfies(license, SHIPPED)) {
      toolExceptions.push(`${id} (${license})`);
    }
  }
}

if (errors.length > 0) {
  for (const error of errors) console.error(`not allowed: ${error}`);
  console.error(
    `${errors.length} licence problem(s); see license-policy.json and docs/decisions.md`,
  );
  process.exit(1);
}

console.log(`npm licences ok: ${shippedCount} shipped, ${toolCount} build/dev tools`);
if (toolExceptions.length > 0) {
  console.log(`build-tool-only licences in use: ${toolExceptions.join(", ")}`);
}
