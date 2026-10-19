#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

#[cfg(feature = "drawer")]
pub mod drawer;
#[cfg(feature = "printer")]
pub mod printer;
#[cfg(feature = "scanner")]
pub mod scanner;

use hardless_core::{Registry, RegistryError};

/// Registers every device kind enabled by Cargo features.
///
/// ```
/// let mut registry = hardless_core::Registry::new(hardless_core::FakeClock::new());
/// hardless_devices::register_all(&mut registry).unwrap();
/// assert!(registry.plugin("printer-escpos").is_some());
/// assert!(registry.plugin("cash-drawer").is_some());
/// assert!(registry.plugin("scanner-wedge").is_some());
/// ```
pub fn register_all(registry: &mut Registry) -> Result<(), RegistryError> {
    #[cfg(feature = "printer")]
    registry.register(printer::PrinterPlugin)?;
    #[cfg(feature = "drawer")]
    registry.register(drawer::DrawerPlugin)?;
    #[cfg(feature = "scanner")]
    registry.register(scanner::ScannerPlugin)?;
    let _ = &registry;
    Ok(())
}
