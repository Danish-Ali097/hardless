// Publishes the workspace's crates to crates.io in dependency order (publish-crates.yml),
// skipping any version that is already there, so a failed or partial publish can simply be
// run again. When crates.io's rate limit for new crates answers 429 ("try again after ..."),
// it waits until then and retries.
//
//   node scripts/publish-crates.mjs [--dry-run]
//
// Needs CARGO_REGISTRY_TOKEN (unless --dry-run; a dry run stops at the first crate whose
// workspace dependencies are not on crates.io yet, so CI checks with `cargo publish
// --workspace --dry-run` instead). Extra arguments go to `cargo publish`.
import { spawnSync, execFileSync } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";

const dryRun = process.argv.includes("--dry-run");
const USER_AGENT = "hardless-release (https://github.com/hardlessdev/hardless)";

const metadata = JSON.parse(
  execFileSync("cargo", ["metadata", "--no-deps", "--format-version", "1", "--locked"], {
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  }),
);
const members = new Set(metadata.workspace_members);
const packages = metadata.packages.filter(
  (p) => members.has(p.id) && !(Array.isArray(p.publish) && p.publish.length === 0),
);
const names = new Set(packages.map((p) => p.name));

/** Publishable workspace packages, each after the workspace crates it depends on. */
function order() {
  const sorted = [];
  const done = new Set();
  const visit = (pkg) => {
    if (done.has(pkg.name)) return;
    done.add(pkg.name);
    for (const dep of pkg.dependencies) {
      if (dep.kind !== "dev" && names.has(dep.name)) {
        const target = packages.find((p) => p.name === dep.name);
        if (target) visit(target);
      }
    }
    sorted.push(pkg);
  };
  for (const pkg of [...packages].sort((a, b) => a.name.localeCompare(b.name))) visit(pkg);
  return sorted;
}

/** The crate's path in the crates.io sparse index. */
function indexPath(name) {
  const n = name.toLowerCase();
  if (n.length <= 2) return `${String(n.length)}/${n}`;
  if (n.length === 3) return `3/${n[0]}/${n}`;
  return `${n.slice(0, 2)}/${n.slice(2, 4)}/${n}`;
}

/** Whether crates.io already has `name` at `version`. */
async function published(name, version) {
  const response = await fetch(`https://index.crates.io/${indexPath(name)}`, {
    headers: { "User-Agent": USER_AGENT },
  });
  if (response.status === 404 || response.status === 403) return false;
  if (!response.ok) throw new Error(`index lookup for ${name}: HTTP ${String(response.status)}`);
  return (await response.text())
    .split("\n")
    .filter(Boolean)
    .some((line) => JSON.parse(line).vers === version);
}

const extra = process.argv.slice(2).filter((a) => a !== "--dry-run");
for (const pkg of order()) {
  if (await published(pkg.name, pkg.version)) {
    console.log(`${pkg.name} ${pkg.version} is already on crates.io; skipping`);
    continue;
  }
  for (let attempt = 1; ; attempt++) {
    const args = [
      "publish",
      "-p",
      pkg.name,
      "--locked",
      ...(dryRun ? ["--dry-run"] : []),
      ...extra,
    ];
    console.log(`\n$ cargo ${args.join(" ")}`);
    const run = spawnSync("cargo", args, {
      encoding: "utf8",
      stdio: ["ignore", "inherit", "pipe"],
    });
    process.stderr.write(run.stderr);
    if (run.status === 0) break;
    const retry = /try again after ([^.]+? GMT)/.exec(run.stderr);
    if (retry?.[1] === undefined || attempt >= 6) process.exit(run.status ?? 1);
    const wait = Math.max(0, Date.parse(retry[1]) - Date.now()) + 15_000;
    console.log(
      `crates.io rate limit: waiting ${String(Math.ceil(wait / 1000))} s, until ${retry[1]}`,
    );
    await sleep(wait);
  }
}
console.log("\nevery crate is on crates.io");
