use std::collections::HashMap;

/// A trait representing Brink extension.
pub trait BrinkExtension {
    /// Returns the name of the extension as it will be invoked in Brink scripts.
    /// E.g., "brink::test_crc"
    fn name(&self) -> &str;

    /// Executes the extension with the given arguments and image buffers.
    ///
    /// * `args` - An array of 64-bit integer values corresponding to the evaluated arguments.
    /// * `img_buffer` - A slice of the generated Brink image bytes up to this point.
    /// * `out_buffer` - A mutable slice where the extension should write its output.
    ///                  The size typically corresponds to the WR statement width (e.g., 4 bytes for wr32).
    fn execute(&self, args: &[u64], img_buffer: &[u8], out_buffer: &mut [u8])
    -> Result<(), String>;
}

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

    /// Registers a new extension instance.
    pub fn register(&mut self, extension: Box<dyn BrinkExtension>) {
        self.extensions
            .insert(extension.name().to_string(), extension);
    }

    /// Retrieves a registered extension by its fully-qualified name.
    pub fn get(&self, name: &str) -> Option<&dyn BrinkExtension> {
        self.extensions.get(name).map(|b| b.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockCrc;

    impl BrinkExtension for MockCrc {
        fn name(&self) -> &str {
            "brink::test_crc"
        }

        fn execute(&self, args: &[u64], _img: &[u8], out: &mut [u8]) -> Result<(), String> {
            if args.len() != 1 {
                return Err("Expected exactly 1 argument for CRC".to_string());
            }
            if out.len() != 4 {
                return Err("Expected 4 bytes of output space".to_string());
            }
            // Mock behavior: write the isolated argument as a big-endian u32.
            let val = args[0] as u32;
            out.copy_from_slice(&val.to_be_bytes());
            Ok(())
        }
    }

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
        use tracing_subscriber::fmt::MakeWriter;
        use std::sync::{Arc, Mutex};

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

        struct MockLogger;
        impl BrinkExtension for MockLogger {
            fn name(&self) -> &str {
                "brink::test_logger"
            }
            fn execute(&self, _args: &[u64], _img: &[u8], _out: &mut [u8]) -> Result<(), String> {
                tracing::info!("MockLogger executed successfully via tracing API");
                Err("Intentional mock fallback error".to_string())
            }
        }

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
