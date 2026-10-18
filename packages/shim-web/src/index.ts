/**
 * `@hardless/shim-web` replaces `navigator.serial`, `navigator.usb` and `NDEFReader` with
 * shims backed by Hardless emulated devices, so an app's production code runs unchanged.
 *
 * Milestone M0 ships the package skeleton only. `install()` and `uninstall()` (FR-14 to
 * FR-17) and the Web Serial shim (PRD section 8.1) arrive in milestone M1.
 *
 * @packageDocumentation
 */

/** The browser APIs the shim can replace. */
export type ShimApi = "serial" | "usb" | "nfc" | "wedge";

/** Every API the shim knows about, in the order the bridge `hello` message lists them. */
export const SHIM_APIS: readonly ShimApi[] = ["serial", "usb", "nfc", "wedge"];

/** The version of this package, sent as `shimVersion` in the bridge `hello` message. */
export const SHIM_VERSION = "0.0.0";
