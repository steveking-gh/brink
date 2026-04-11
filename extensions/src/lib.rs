// Extension registration for Brink.
//
// This crate is the single place where all compiled-in extensions are
// registered with Brink's extension registry.  To add a new extension:
//
//   1. Create a new crate implementing `BrinkExtension` or
//      `BrinkRangedExtension` from the `brink_extension` crate.
//   2. Add it as a dependency in this file's Cargo.toml.
//   3. Call its `register` function inside `register_all` below.
//
// `process.rs` calls `register_all` once at startup and does not need
// to know about individual extensions.

use ext::ExtensionRegistry;

/// Registers all compiled-in extensions into `registry`.
/// Call once before compiling any Brink scripts.
pub fn register_all(registry: &mut ExtensionRegistry) {
    #[cfg(feature = "std-crc32c")]
    std_crc32c::register(registry);
    #[cfg(feature = "std-sha256")]
    std_sha256::register(registry);
    #[cfg(feature = "std-md5")]
    std_md5::register(registry);
}
