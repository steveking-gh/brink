/// The trait that all Brink extensions must implement.
///
/// An extension writes a fixed number of bytes into the Brink image.  An extension
/// may optionally take arguments from the call site and access a slice of all or
/// part of the image.
///
/// # Example
///
/// ```rust
/// use brink_extension::BrinkExtension;
///
/// pub struct MyCrc;
///
/// impl BrinkExtension for MyCrc {
///     fn name(&self) -> &str { "my_org::crc" }
///
///     fn size(&self) -> usize { 4 }
///     fn execute(&self, args: &[u64], out: &mut [u8]) -> Result<(), String> {
///         let val = args.first().copied().unwrap_or(0) as u32;
///         out.copy_from_slice(&val.to_be_bytes());
///         Ok(())
///     }
/// }
/// ```

/// Implement for extensions that do not need a caller-specified slice of the image.
pub trait BrinkExtension {
    /// Returns the namespace and name used to invoke this extension.
    /// For example, `"my_org::crc"`. The `brink` and `std` namespaces are reserved.
    fn name(&self) -> &str;

    /// Returns the exact number of bytes this extension writes to `out_buffer`.
    ///
    /// Brink calls this method **once** during extension registration and caches the
    /// result for use during image layout calculations.
    fn size(&self) -> usize;

    /// Produces the extension's output bytes.
    ///
    /// * `args` — evaluated 64-bit arguments from the Brink script call site.
    /// * `out_buffer` — pre-allocated buffer of exactly [`size`](Self::size)
    ///   bytes.  Write your output here.
    ///
    /// Return `Err(message)` to abort with a diagnostic.
    fn execute(&self, args: &[u64], out_buffer: &mut [u8]) -> Result<(), String>;
}

/// Implement for extensions that read a caller-specified slice of the image.
///
/// The call site must supply either explicit bounds `(start_offset, length, ...)`
/// or a section identifier as the first argument. Brink slices the image to
/// exactly those bytes before calling `execute`.
///
/// # Example
///
/// ```rust
/// use brink_extension::BrinkRangedExtension;
///
/// pub struct MyChecksum;
///
/// impl BrinkRangedExtension for MyChecksum {
///     fn name(&self) -> &str { "my_org::checksum" }
///     fn size(&self) -> usize { 8 }
///     fn execute(&self, _args: &[u64], img_buffer: &[u8], out: &mut [u8]) -> Result<(), String> {
///         let sum: u64 = img_buffer.iter().map(|&b| b as u64).sum();
///         out.copy_from_slice(&sum.to_be_bytes());
///         Ok(())
///     }
/// }
/// ```
pub trait BrinkRangedExtension {
    /// Returns the namespace and name used to invoke this extension.
    /// For example, `"my_org::crc"`. The `brink` and `std` namespaces are reserved.
    fn name(&self) -> &str;

    /// Returns the exact number of bytes this extension writes to `out_buffer`.
    ///
    /// Brink calls this method **once** during extension registration and caches the
    /// result for use during image layout calculations.
    fn size(&self) -> usize;

    /// Produces the extension's output bytes.
    ///
    /// * `args` — evaluated 64-bit arguments from the Brink script call site,
    ///   excluding the range specifier (start/length or section name).
    /// * `img_buffer` — the image slice specified by the call site (read-only).
    ///   Exactly `(length)` bytes for explicit-range calls, or the full section
    ///   for section-name calls.
    /// * `out_buffer` — pre-allocated buffer of exactly [`size`](Self::size)
    ///   bytes.  Write your output here.
    ///
    /// Return `Err(message)` to abort compilation with a diagnostic.
    fn execute(&self, args: &[u64], img_buffer: &[u8], out_buffer: &mut [u8])
        -> Result<(), String>;
}
