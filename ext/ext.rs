use std::collections::HashMap;

pub use brink_extension::BrinkExtension;

/// Owns a registered extension alongside its cached size.
///
/// Brink calls [`BrinkExtension::size`] exactly once — at registration time —
/// and stores the result here. All internal size lookups use [`cached_size`]
/// rather than calling the extension again.
///
/// [`cached_size`]: ExtensionEntry::cached_size
pub struct ExtensionEntry {
    pub extension: Box<dyn BrinkExtension>,
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

    /// Registers a new extension. Calls [`BrinkExtension::size`] exactly once
    /// and caches the result. Panics if an extension with the same name is
    /// already registered.
    pub fn register(&mut self, extension: Box<dyn BrinkExtension>) {
        let name = extension.name().to_string();
        if self.extensions.contains_key(&name) {
            panic!("Extension '{}' is already registered", name);
        }
        let cached_size = extension.size();
        self.extensions.insert(name, ExtensionEntry { extension, cached_size });
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

    #[test]
    fn test_valid_extension_execution() {
        let mut reg = ExtensionRegistry::new();
        reg.register(Box::new(MockCrc::new()));
        let entry = reg.get("brink::test_crc").unwrap();

        let args = vec![0xDEADBEEF];
        let mut out = vec![0; 4];
        let img = vec![];

        entry.extension.execute(&args, &img, &mut out).unwrap();
        assert_eq!(out, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn test_invalid_argument_count() {
        let mut reg = ExtensionRegistry::new();
        reg.register(Box::new(MockCrc::new()));
        let entry = reg.get("brink::test_crc").unwrap();

        let args = vec![1, 2]; // Pass 2 args, but mock only expects 1
        let mut out = vec![0; 4];
        let res = entry.extension.execute(&args, &[], &mut out);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "Expected exactly 1 argument for CRC");
    }

    #[test]
    fn test_invalid_output_buffer() {
        let mut reg = ExtensionRegistry::new();
        reg.register(Box::new(MockCrc::new()));
        let entry = reg.get("brink::test_crc").unwrap();

        let args = vec![1];
        let mut out = vec![0; 2]; // Mock expects 4
        let res = entry.extension.execute(&args, &[], &mut out);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "Expected 4 bytes of output space");
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

        // Try initializing the global subscriber.
        // It might be initialized by another test running competitively, so we drop the Result.
        let _ = tracing_subscriber::fmt()
            .with_writer(writer)
            .with_max_level(tracing::Level::INFO)
            .try_init();

        let mut reg = ExtensionRegistry::new();
        reg.register(Box::new(MockLogger::new()));

        let entry = reg.get("brink::test_logger").unwrap();
        let res = entry.extension.execute(&[], &[], &mut []);

        // Exercise the error reporting assertion
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "Intentional mock fallback error");

        // Exercise the logging assertion
        let log_output = String::from_utf8(logs.lock().unwrap().clone()).unwrap();
        assert!(log_output.contains("MockLogger executed successfully via tracing API"));
    }
}
