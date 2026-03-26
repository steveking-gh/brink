use std::collections::HashMap;

pub use brink_extension::BrinkExtension;
pub use brink_extension::BrinkRangedExtension;

/// Wraps either a basic or ranged extension behind a unified enum.
///
/// The variant determines which `execute` signature Brink calls and whether
/// the call site must supply an image range specifier.
pub enum RegisteredExtension {
    Basic(Box<dyn BrinkExtension>),
    Ranged(Box<dyn BrinkRangedExtension>),
}

impl RegisteredExtension {
    /// Returns the extension's fully-qualified name.
    pub fn name(&self) -> &str {
        match self {
            Self::Basic(e) => e.name(),
            Self::Ranged(e) => e.name(),
        }
    }

    /// Returns `true` if the extension requires an image range at its call site.
    pub fn is_ranged(&self) -> bool {
        matches!(self, Self::Ranged(_))
    }
}
pub use brink_extension::BrinkRangedExtension;

/// Wraps either a basic or ranged extension behind a unified enum.
///
/// The variant determines which `execute` signature Brink calls and whether
/// the call site must supply an image range specifier.
pub enum RegisteredExtension {
    Basic(Box<dyn BrinkExtension>),
    Ranged(Box<dyn BrinkRangedExtension>),
}

impl RegisteredExtension {
    /// Returns the extension's fully-qualified name.
    pub fn name(&self) -> &str {
        match self {
            Self::Basic(e) => e.name(),
            Self::Ranged(e) => e.name(),
        }
    }

    /// Returns `true` if the extension requires an image range at its call site.
    pub fn is_ranged(&self) -> bool {
        matches!(self, Self::Ranged(_))
    }
}

/// Owns a registered extension alongside its cached size.
///
/// Brink calls `size()` exactly once — at registration time — and stores the
/// result here. All internal size lookups use [`cached_size`] rather than
/// calling the extension again.
/// Brink calls `size()` exactly once — at registration time — and stores the
/// result here. All internal size lookups use [`cached_size`] rather than
/// calling the extension again.
///
/// [`cached_size`]: ExtensionEntry::cached_size
pub struct ExtensionEntry {
    pub extension: RegisteredExtension,
    pub extension: RegisteredExtension,
    pub cached_size: usize,
}

/// A registry that owns and provides lookup for all available Brink extensions.
pub struct ExtensionRegistry {
    extensions: HashMap<String, ExtensionEntry>,
}

impl Default for ExtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtensionRegistry {
    /// Creates a new, empty extension registry.
    pub fn new() -> Self {
        Self {
            extensions: HashMap::new(),
        }
    }

    /// Registers a non-ranged extension. Calls [`BrinkExtension::size`] exactly
    /// once and caches the result. Panics if an extension with the same name is
    /// Registers a non-ranged extension. Calls [`BrinkExtension::size`] exactly
    /// once and caches the result. Panics if an extension with the same name is
    /// already registered.
    pub fn register(&mut self, ext: Box<dyn BrinkExtension>) {
        let name = ext.name().to_string();
        assert!(
            !self.extensions.contains_key(&name),
            "Extension '{}' is already registered",
            name
        );
        let cached_size = ext.size();
        self.extensions.insert(
            name,
            ExtensionEntry {
                extension: RegisteredExtension::Basic(ext),
                cached_size,
            },
        );
    }

    /// Registers a ranged extension. Calls [`BrinkRangedExtension::size`] exactly
    /// once and caches the result. Panics if an extension with the same name is
    /// already registered.
    pub fn register_ranged(&mut self, ext: Box<dyn BrinkRangedExtension>) {
        let name = ext.name().to_string();
        assert!(
            !self.extensions.contains_key(&name),
            "Extension '{}' is already registered",
            name
        );
        let cached_size = ext.size();
        self.extensions.insert(
            name,
            ExtensionEntry {
                extension: RegisteredExtension::Ranged(ext),
                cached_size,
            },
        );
    pub fn register(&mut self, ext: Box<dyn BrinkExtension>) {
        let name = ext.name().to_string();
        assert!(
            !self.extensions.contains_key(&name),
            "Extension '{}' is already registered",
            name
        );
        let cached_size = ext.size();
        self.extensions.insert(
            name,
            ExtensionEntry {
                extension: RegisteredExtension::Basic(ext),
                cached_size,
            },
        );
    }

    /// Registers a ranged extension. Calls [`BrinkRangedExtension::size`] exactly
    /// once and caches the result. Panics if an extension with the same name is
    /// already registered.
    pub fn register_ranged(&mut self, ext: Box<dyn BrinkRangedExtension>) {
        let name = ext.name().to_string();
        assert!(
            !self.extensions.contains_key(&name),
            "Extension '{}' is already registered",
            name
        );
        let cached_size = ext.size();
        self.extensions.insert(
            name,
            ExtensionEntry {
                extension: RegisteredExtension::Ranged(ext),
                cached_size,
            },
        );
    }

    /// Retrieves a registered extension entry by its fully-qualified name.
    pub fn get(&self, name: &str) -> Option<&ExtensionEntry> {
        self.extensions.get(name)
    }
}

pub mod test_mocks;

#[cfg(test)]
mod tests {
    use super::*;
    use test_mocks::*;

    #[test]
    fn test_registry_registration_and_lookup() {
        let mut reg = ExtensionRegistry::new();
        reg.register(Box::new(MockCrc::new()));

        assert!(
            reg.get("brink::test_crc").is_some(),
            "Should find registered extension"
        );
        assert!(
            reg.get("missing::func").is_none(),
            "Should reject unregistered extensions"
        );
    }

    #[test]
    #[should_panic(expected = "Extension 'brink::test_crc' is already registered")]
    fn test_duplicate_registration_panics() {
        let mut reg = ExtensionRegistry::new();
        reg.register(Box::new(MockCrc::new()));
        reg.register(Box::new(MockCrc::new()));
    }

    /// size() must be called exactly once — at registration — and never again.
    #[test]
    fn test_size_called_once_on_register() {
        let mut reg = ExtensionRegistry::new();
        reg.register(Box::new(MockCrc::new()));
        // Retrieve the entry multiple times; the Cell counter must stay at 1.
        let _ = reg.get("brink::test_crc");
        let _ = reg.get("brink::test_crc");
        // MockCrc::size() would panic on a second call, so reaching here is the assertion.
    }

    /// cached_size must equal the value size() returned.
    #[test]
    fn test_cached_size_matches_extension() {
        let mut reg = ExtensionRegistry::new();
        reg.register(Box::new(MockCrc::new()));
        let entry = reg.get("brink::test_crc").unwrap();
        assert_eq!(entry.cached_size, 4);
    }

    /// is_ranged() must reflect the registration method used.
    #[test]
    fn test_is_ranged_reflects_variant() {
        let mut reg = ExtensionRegistry::new();
        reg.register(Box::new(MockCrc::new()));
        reg.register_ranged(Box::new(MockIncrement::new()));

        assert!(
            !reg.get("brink::test_crc").unwrap().extension.is_ranged(),
            "Basic extension must not be ranged"
        );
        assert!(
            reg.get("brink::test_increment").unwrap().extension.is_ranged(),
            "Ranged extension must be ranged"
        );
    }

    /// is_ranged() must reflect the registration method used.
    #[test]
    fn test_is_ranged_reflects_variant() {
        let mut reg = ExtensionRegistry::new();
        reg.register(Box::new(MockCrc::new()));
        reg.register_ranged(Box::new(MockIncrement::new()));

        assert!(
            !reg.get("brink::test_crc").unwrap().extension.is_ranged(),
            "Basic extension must not be ranged"
        );
        assert!(
            reg.get("brink::test_increment").unwrap().extension.is_ranged(),
            "Ranged extension must be ranged"
        );
    }

    #[test]
    fn test_valid_extension_execution() {
        let mut reg = ExtensionRegistry::new();
        reg.register(Box::new(MockCrc::new()));
        let entry = reg.get("brink::test_crc").unwrap();

        let args = vec![0xDEADBEEF_u64];
        let mut out = vec![0u8; 4];
        let args = vec![0xDEADBEEF_u64];
        let mut out = vec![0u8; 4];

        let RegisteredExtension::Basic(ref ext) = entry.extension else {
            panic!("Expected Basic extension");
        };
        ext.execute(&args, &mut out).unwrap();
        let RegisteredExtension::Basic(ref ext) = entry.extension else {
            panic!("Expected Basic extension");
        };
        ext.execute(&args, &mut out).unwrap();
        assert_eq!(out, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn test_invalid_argument_count() {
        let mut reg = ExtensionRegistry::new();
        reg.register(Box::new(MockCrc::new()));
        let entry = reg.get("brink::test_crc").unwrap();

        let RegisteredExtension::Basic(ref ext) = entry.extension else {
            panic!("Expected Basic extension");
        };
        let args = vec![1u64, 2]; // mock expects exactly 1
        let mut out = vec![0u8; 4];
        let res = ext.execute(&args, &mut out);
        let RegisteredExtension::Basic(ref ext) = entry.extension else {
            panic!("Expected Basic extension");
        };
        let args = vec![1u64, 2]; // mock expects exactly 1
        let mut out = vec![0u8; 4];
        let res = ext.execute(&args, &mut out);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "Expected exactly 1 argument for CRC");
    }

    #[test]
    fn test_invalid_output_buffer() {
        let mut reg = ExtensionRegistry::new();
        reg.register(Box::new(MockCrc::new()));
        let entry = reg.get("brink::test_crc").unwrap();

        let RegisteredExtension::Basic(ref ext) = entry.extension else {
            panic!("Expected Basic extension");
        };
        let args = vec![1u64];
        let mut out = vec![0u8; 2]; // mock expects 4
        let res = ext.execute(&args, &mut out);
        let RegisteredExtension::Basic(ref ext) = entry.extension else {
            panic!("Expected Basic extension");
        };
        let args = vec![1u64];
        let mut out = vec![0u8; 2]; // mock expects 4
        let res = ext.execute(&args, &mut out);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "Expected 4 bytes of output space");
    }

    #[test]
    fn test_ranged_sum_execution() {
        let mut reg = ExtensionRegistry::new();
        reg.register_ranged(Box::new(MockRangedSum::new()));
        let entry = reg.get("brink::test_ranged_sum").unwrap();
        assert_eq!(entry.cached_size, 8);

        let RegisteredExtension::Ranged(ref ext) = entry.extension else {
            panic!("Expected Ranged extension");
        };
        let img = vec![0x01u8, 0x02, 0x03, 0x04];
        let mut out = vec![0u8; 8];
        ext.execute(&[], &img, &mut out).unwrap();
        let sum = u64::from_le_bytes(out.try_into().unwrap());
        assert_eq!(sum, 10, "Sum of 1+2+3+4 must be 10");
    }

    #[test]
    fn test_logging_and_error_reporting() {
        use std::sync::{Arc, Mutex};
        use tracing_subscriber::fmt::MakeWriter;

        #[derive(Clone)]
        struct MockWriter {
            logs: Arc<Mutex<Vec<u8>>>,
        }

        impl std::io::Write for MockWriter {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.logs.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        impl<'a> MakeWriter<'a> for MockWriter {
            type Writer = Self;
            fn make_writer(&'a self) -> Self::Writer {
                self.clone()
            }
        }

        let logs = Arc::new(Mutex::new(Vec::new()));
        let writer = MockWriter { logs: logs.clone() };

        // try_init may fail if another test already initialized the subscriber.
        // try_init may fail if another test already initialized the subscriber.
        let _ = tracing_subscriber::fmt()
            .with_writer(writer)
            .with_max_level(tracing::Level::INFO)
            .try_init();

        let mut reg = ExtensionRegistry::new();
        reg.register(Box::new(MockLogger::new()));

        let entry = reg.get("brink::test_logger").unwrap();
        let RegisteredExtension::Basic(ref ext) = entry.extension else {
            panic!("Expected Basic extension");
        };
        let res = ext.execute(&[], &mut []);
        let RegisteredExtension::Basic(ref ext) = entry.extension else {
            panic!("Expected Basic extension");
        };
        let res = ext.execute(&[], &mut []);

        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "Intentional mock fallback error");

        let log_output = String::from_utf8(logs.lock().unwrap().clone()).unwrap();
        assert!(log_output.contains("MockLogger executed successfully via tracing API"));
    }
}
