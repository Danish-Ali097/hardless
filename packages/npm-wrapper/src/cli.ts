#!/usr/bin/env node
import { releaseTarget } from "./index.js";

const target = releaseTarget(process.platform, process.arch);
if (target === undefined) {
  console.error(`hardless: no binary for ${process.platform}/${process.arch}`);
} else {
  console.error(
    `hardless: no release published yet (would download ${target}). ` +
      "Build from source with `cargo build -p hardless-cli` for now.",
  );
}
process.exitCode = 1;
