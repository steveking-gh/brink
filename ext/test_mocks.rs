use std::cell::Cell;
use super::*;

pub struct MockCrc {
    size_call_count: Cell<usize>,
}

impl MockCrc {
    pub fn new() -> Self {
        Self { size_call_count: Cell::new(0) }
    }
}

impl Default for MockCrc {
    fn default() -> Self {
        Self::new()
    }
}

impl BrinkExtension for MockCrc {
    fn name(&self) -> &str {
        "brink::test_crc"
    }

    fn size(&self) -> usize {
        let prev = self.size_call_count.get();
        assert_eq!(prev, 0, "MockCrc::size() called more than once");
        self.size_call_count.set(prev + 1);
        4
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

pub struct MockLogger {
    size_call_count: Cell<usize>,
}

impl MockLogger {
    pub fn new() -> Self {
        Self { size_call_count: Cell::new(0) }
    }
}

impl Default for MockLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl BrinkExtension for MockLogger {
    fn name(&self) -> &str {
        "brink::test_logger"
    }

    fn size(&self) -> usize {
        let prev = self.size_call_count.get();
        assert_eq!(prev, 0, "MockLogger::size() called more than once");
        self.size_call_count.set(prev + 1);
        0
    }

    fn execute(&self, _args: &[u64], _img: &[u8], _out: &mut [u8]) -> Result<(), String> {
        tracing::info!("MockLogger executed successfully via tracing API");
        Err("Intentional mock fallback error".to_string())
    }
}

pub fn register_test_extensions(reg: &mut ExtensionRegistry) {
    reg.register(Box::new(MockCrc::new()));
    reg.register(Box::new(MockLogger::new()));
}
