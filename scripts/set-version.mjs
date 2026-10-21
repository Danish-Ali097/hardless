// Sets one version everywhere a release needs it (RELEASING.md): the Rust workspace
// (`[workspace.package]` and the internal `[workspace.dependencies]`), every package.json
// under packages/ and examples/, and the extension's manifest.json. Run `pnpm changeset
// version` first so the npm changelogs are written, then this, then `cargo update -w`.
//
//   node scripts/set-version.mjs 0.1.0
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const version = process.argv[2] ?? "";
if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error("usage: node scripts/set-version.mjs <version>, for example 0.1.0");
  process.exit(1);
}

/** Rewrites a file with `edit` and reports whether it changed. */
function update(file, edit) {
  const path = join(root, file);
  const before = readFileSync(path, "utf8");
  const after = edit(before);
  if (after !== before) writeFileSync(path, after);
  console.log(`${after === before ? "unchanged" : "updated  "} ${file}`);
}

update("Cargo.toml", (text) =>
  text
    .replace(/(\[workspace\.package\][^[]*?\nversion = )"[^"]*"/, `$1"${version}"`)
    .replace(
      /^(hardless-[a-z-]+ = \{ path = "crates\/[^"]+", version = )"[^"]*"/gm,
      `$1"${version}"`,
    ),
);

for (const parent of ["packages", "examples"]) {
  for (const name of readdirSync(join(root, parent))) {
    update(`${parent}/${name}/package.json`, (text) =>
      text.replace(/^( {2}"version": )"[^"]*"/m, `$1"${version}"`),
    );
  }
}

// Chrome takes only dot-separated integers here, so a pre-release keeps its base version.
update("packages/extension/public/manifest.json", (text) =>
  text.replace(/("version": )"[^"]*"/, `$1"${version.split("-")[0] ?? version}"`),
);
