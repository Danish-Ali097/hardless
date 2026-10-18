//! WebAssembly build of the Hardless engine, loaded lazily by `@hardless/shim-web` in
//! standalone mode (FR-15) so device behaviour is identical with and without a server.
//!
//! Milestone M0 exports only [`version`]. The device bindings arrive in milestone M1.

use wasm_bindgen::prelude::wasm_bindgen;

/// The engine version, so the shim can check it loaded a matching build.
///
/// ```
/// assert_eq!(hardless_wasm::version(), env!("CARGO_PKG_VERSION"));
/// ```
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_matches_crate() {
        assert_eq!(super::version(), env!("CARGO_PKG_VERSION"));
    }
}
