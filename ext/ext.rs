use std::collections::HashMap;

pub use brink_extension::BrinkExtension;

/// A registry that owns and provides lookup for all available Brink extensions.
pub struct ExtensionRegistry {
    extensions: HashMap<String, Box<dyn BrinkExtension>>,
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

    /// Registers a new extension instance. Panics if an extension with the same name already exists.
    pub fn register(&mut self, extension: Box<dyn BrinkExtension>) {
        let name = extension.name().to_string();
        if self.extensions.contains_key(&name) {
            panic!("Extension '{}' is already registered", name);
        }
        self.extensions.insert(name, extension);
    }

    /// Retrieves a registered extension by its fully-qualified name.
    pub fn get(&self, name: &str) -> Option<&dyn BrinkExtension> {
        self.extensions.get(name).map(|b| b.as_ref())
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
        reg.register(Box::new(MockCrc));

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
        reg.register(Box::new(MockCrc));
        reg.register(Box::new(MockCrc));
    }

    #[test]
    fn test_valid_extension_execution() {
        let mut reg = ExtensionRegistry::new();
        reg.register(Box::new(MockCrc));
        let ext = reg.get("brink::test_crc").unwrap();

        let args = vec![0xDEADBEEF];
        let mut out = vec![0; 4];
        let img = vec![];

        ext.execute(&args, &img, &mut out).unwrap();
        assert_eq!(out, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn test_invalid_argument_count() {
        let mut reg = ExtensionRegistry::new();
        reg.register(Box::new(MockCrc));
        let ext = reg.get("brink::test_crc").unwrap();

        let args = vec![1, 2]; // Pass 2 args, but mock only expects 1
        let mut out = vec![0; 4];
        let res = ext.execute(&args, &[], &mut out);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "Expected exactly 1 argument for CRC");
    }

    #[test]
    fn test_invalid_output_buffer() {
        let mut reg = ExtensionRegistry::new();
        reg.register(Box::new(MockCrc));
        let ext = reg.get("brink::test_crc").unwrap();

        let args = vec![1];
        let mut out = vec![0; 2]; // Mock expects 4
        let res = ext.execute(&args, &[], &mut out);
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
        reg.register(Box::new(MockLogger));

        let ext = reg.get("brink::test_logger").unwrap();
        let res = ext.execute(&[], &[], &mut []);

        // Exercise the error reporting assertion
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "Intentional mock fallback error");

        // Exercise the logging assertion
        let log_output = String::from_utf8(logs.lock().unwrap().clone()).unwrap();
        assert!(log_output.contains("MockLogger executed successfully via tracing API"));
    }
}
