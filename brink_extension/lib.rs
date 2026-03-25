/// The trait that all Brink extensions must implement.
///
/// An extension writes a fixed number of bytes into the Brink image. Brink
/// calls [`BrinkExtension::size`] once at registration time and caches the
/// result. [`BrinkExtension::execute`] is called during the generate phase to
/// produce the actual bytes.
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
///     fn size(&self) -> usize { 4 }
///     fn execute(&self, args: &[u64], _img: &[u8], out: &mut [u8]) -> Result<(), String> {
///         let val = args.first().copied().unwrap_or(0) as u32;
///         out.copy_from_slice(&val.to_be_bytes());
///         Ok(())
///     }
/// }
/// ```
pub trait BrinkExtension {
    /// Returns the name used to invoke this extension in Brink scripts,
    /// e.g. `"my_org::crc"`. The `brink` and `std` namespaces are reserved.
    fn name(&self) -> &str;

    /// Returns the exact number of bytes this extension writes to `out_buffer`.
    ///
    /// Brink calls this method **once**, at registration time, and caches the
    /// result. Do not rely on being called more than once.
    fn size(&self) -> usize;

    /// Produces the extension's output bytes.
    ///
    /// * `args` — evaluated 64-bit arguments from the Brink script call site.
    /// * `img_buffer` — the image bytes written so far (read-only).
    /// * `out_buffer` — pre-allocated buffer of exactly [`size`](Self::size)
    ///   bytes; write your output here.
    ///
    /// Return `Err(message)` to abort compilation with a diagnostic.
    fn execute(&self, args: &[u64], img_buffer: &[u8], out_buffer: &mut [u8])
        -> Result<(), String>;
}
