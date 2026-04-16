// Extension API for the Brink binary image compiler.
//
// This crate defines the public API for Brink extension authors.
//
// BrinkExtension -- the single trait all extensions implement.
//
// ExtArg -- typed argument passed to execute().  Each parameter passed
// to an extension in Brink code maps to one ExtArg passed to the extension
// by the compiler.
//
// Argument types:
//   Int(u64)                     -- numeric expression (u64, i64, or integer const)
//   Str(&str)                    -- quoted string const
//   Section { start, len, data } -- section name; Brink resolves to file offset,
//                                   byte length, and a zero-copy slice of the
//                                   image at that range.
//
// Args may appear in any order.  Brink resolves the section name to its
// location and provides the image bytes without extension authors having to
// compute offsets.
//
// Brink registers extensions at startup via the ExtensionRegistry (defined in
// the ext crate).  Brink calls size() exactly once during registration and caches
// the result.  The `brink` and `std` namespaces are reserved.
//
// Named parameters:
//   An extension declares named parameters via params(), returning a slice of
//   ParamDesc values.  Each ParamDesc pairs a name with a ParamKind.
//   ParamKind::ByteArray declares that a parameter accepts a sequence of bytes
//   (supplied at call sites as a section name).  ParamKind::Int and
//   ParamKind::Str declare numeric and string parameters respectively.
//   An empty params() slice opts out of named-arg enforcement; positional-only
//   rules apply with legacy heuristics.

/// The kind of a declared extension parameter.
///
/// Used in [`ParamDesc`] to specify what type of argument a parameter accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    /// A numeric argument: u64, i64, or an untyped integer constant.
    Int,
    /// A quoted string constant.
    Str,
    /// A sequence of bytes.  At call sites a section name supplies the bytes.
    /// The kind names the data type; the mechanism (section name) is separate.
    ByteArray,
}

/// Describes one declared parameter of an extension.
///
/// An extension returns a slice of `ParamDesc` from [`BrinkExtension::params`]
/// to opt into named-argument call sites.
#[derive(Debug, Clone, Copy)]
pub struct ParamDesc {
    /// The parameter name used at call sites: `foo::bar(name=value)`.
    pub name: &'static str,
    /// The kind of value the parameter accepts.
    pub kind: ParamKind,
}

/// The argument type passed to an extension at execution time.
///
/// Each Brink call-site argument produces exactly one `ExtArg`.
#[derive(Debug)]
pub enum ExtArg<'a> {
    /// A numeric argument: u64, i64, or an untyped integer constant.
    Int(u64),
    /// A quoted string constant.
    Str(&'a str),
    /// A section name argument, resolved to a zero-copy image slice.
    Section {
        /// Byte offset of the section from the start of the output file.
        start: u64,
        /// Byte length of the section.
        len: u64,
        /// Read-only view of the output image at `start..start+len`.
        data: &'a [u8],
    },
}

/// Implement this trait to create a Brink extension.
///
/// An extension writes a fixed number of bytes into the output image.
/// Arguments arrive as typed [`ExtArg`] values corresponding 1:1 to the
/// Brink call-site arguments.  A section name argument delivers the image
/// bytes at that section directly in the [`ExtArg::Section`] variant.
///
/// # Example: numeric argument
///
/// ```rust
/// use brink_extension::{BrinkExtension, ExtArg};
///
/// pub struct MyCrc;
///
/// impl BrinkExtension for MyCrc {
///     fn name(&self) -> &str { "my_org::crc" }
///     fn size(&self) -> usize { 4 }
///     fn execute<'a>(&self, args: &[ExtArg<'a>], out: &mut [u8]) -> Result<(), String> {
///         let val = match args.first() {
///             Some(ExtArg::Int(v)) => *v as u32,
///             _ => return Err("Expected one numeric argument".to_string()),
///         };
///         out.copy_from_slice(&val.to_be_bytes());
///         Ok(())
///     }
/// }
/// ```
///
/// # Example: section argument
///
/// ```rust
/// use brink_extension::{BrinkExtension, ExtArg};
///
/// pub struct MyChecksum;
///
/// impl BrinkExtension for MyChecksum {
///     fn name(&self) -> &str { "my_org::checksum" }
///     fn size(&self) -> usize { 8 }
///     fn execute<'a>(&self, args: &[ExtArg<'a>], out: &mut [u8]) -> Result<(), String> {
///         let data = match args.first() {
///             Some(ExtArg::Section { data, .. }) => *data,
///             _ => return Err("Expected a section argument".to_string()),
///         };
///         let sum: u64 = data.iter().map(|&b| b as u64).sum();
///         out.copy_from_slice(&sum.to_le_bytes());
///         Ok(())
///     }
/// }
/// ```
pub trait BrinkExtension {
    /// Returns the namespace-qualified name used to invoke the extension.
    /// For example, `"my_org::crc"`. The `brink` and `std` namespaces are
    /// reserved for internal use.
    fn name(&self) -> &str;

    /// Returns the exact number of bytes the extension writes to `out`.
    ///
    /// Brink calls this method once at registration and caches the result.
    /// All layout calculations use the cached value.
    fn size(&self) -> usize;

    /// Returns the declared parameters for this extension.
    ///
    /// Each entry in the slice declares one parameter: its name and kind.
    /// Return a non-empty slice to enable named-argument call sites
    /// (`foo::bar(data=my_section, seed=42)`).  Brink validates argument names
    /// against this slice and reorders call-site args to declaration order
    /// before passing them to [`execute`](Self::execute).
    ///
    /// Return an empty slice (the default) to opt out.  Positional-only rules
    /// apply and legacy section-detection heuristics remain active.
    fn params(&self) -> &[ParamDesc] {
        &[]
    }

    /// Produces the extension output bytes.
    ///
    /// * `args` -- one [`ExtArg`] per Brink call-site argument, in declaration order.
    /// * `out`  -- pre-allocated buffer of exactly [`size`](Self::size) bytes.
    ///
    /// Return `Err(message)` to abort compilation with a diagnostic.
    fn execute<'a>(&self, args: &[ExtArg<'a>], out: &mut [u8]) -> Result<(), String>;
}
