// NFR-14 for npm: every production dependency of the browser packages must use an allowed
// licence. Runs `pnpm licenses list` and exits non-zero on anything else.
//
// Dev-only build tools are not checked here; see "Licences" in docs/decisions.md.
import { execFileSync } from "node:child_process";

const ALLOWED = new Set([
  "MIT",
  "Apache-2.0",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "ISC",
  "Zlib",
  "Unicode-3.0",
]);

const output = execFileSync("pnpm", ["licenses", "list", "--recursive", "--prod", "--json"], {
  encoding: "utf8",
  shell: process.platform === "win32",
});

/** @type {Record<string, { name: string; versions: string[] }[]>} */
const byLicense = output.trim() === "" ? {} : JSON.parse(output);

/** An SPDX expression is allowed if every alternative of an OR, or every part of an AND, is. */
function allowed(expression) {
  const clean = expression.replace(/[()]/g, " ").trim();
  if (/\sOR\s/.test(clean)) return clean.split(/\s+OR\s+/).some(allowed);
  return clean.split(/\s+AND\s+/).every((id) => ALLOWED.has(id.trim()));
}

let failures = 0;
let total = 0;
for (const [license, packages] of Object.entries(byLicense)) {
  total += packages.length;
  if (!allowed(license)) {
    for (const pkg of packages) {
      console.error(`not allowed: ${pkg.name}@${pkg.versions.join(",")} (${license})`);
      failures += 1;
    }
  }
}

if (failures > 0) {
  console.error(`${failures} production dependencies have licences outside NFR-14`);
  process.exit(1);
}
console.log(`npm licences ok (${total} production dependencies)`);
