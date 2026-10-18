/**
 * The `hardless` npm package: `npx hardless` downloads the `hardless` binary for the current
 * platform from GitHub Releases and runs it.
 *
 * Milestone M0 maps platforms to release targets. Downloading arrives with the first release.
 *
 * @packageDocumentation
 */

/** A Rust target triple the `hardless` binary is released for (NFR-11). */
export type ReleaseTarget =
  | "x86_64-pc-windows-msvc"
  | "aarch64-pc-windows-msvc"
  | "x86_64-apple-darwin"
  | "aarch64-apple-darwin"
  | "x86_64-unknown-linux-musl"
  | "aarch64-unknown-linux-musl";

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
