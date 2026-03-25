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

/// Reads the first 16 bytes of the image buffer and writes each byte + 1
/// to the output buffer.  Used to verify that `img_buffer` is correctly
/// populated before an extension executes.
pub struct MockIncrement {
    size_call_count: Cell<usize>,
}

impl MockIncrement {
    pub fn new() -> Self {
        Self { size_call_count: Cell::new(0) }
    }
}

impl Default for MockIncrement {
    fn default() -> Self {
        Self::new()
    }
}

impl BrinkExtension for MockIncrement {
    fn name(&self) -> &str {
        "brink::test_increment"
    }

    fn size(&self) -> usize {
        let prev = self.size_call_count.get();
        assert_eq!(prev, 0, "MockIncrement::size() called more than once");
        self.size_call_count.set(prev + 1);
        16
    }

    fn execute(&self, _args: &[u64], img_buffer: &[u8], out_buffer: &mut [u8]) -> Result<(), String> {
        if img_buffer.len() < 16 {
            return Err(format!(
                "brink::test_increment expected at least 16 image bytes, got {}",
                img_buffer.len()
            ));
        }
        for (out, src) in out_buffer.iter_mut().zip(img_buffer.iter()) {
            *out = src.wrapping_add(1);
        }
        Ok(())
    }
}

pub fn register_test_extensions(reg: &mut ExtensionRegistry) {
    reg.register(Box::new(MockCrc::new()));
    reg.register(Box::new(MockLogger::new()));
    reg.register(Box::new(MockIncrement::new()));
}
