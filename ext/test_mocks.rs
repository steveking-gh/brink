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

    fn execute(&self, args: &[u64], out: &mut [u8]) -> Result<(), String> {
        if args.len() != 1 {
            return Err("Expected exactly 1 argument for CRC".to_string());
        }
        if out.len() != 4 {
            return Err("Expected 4 bytes of output space".to_string());
        }
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

    fn execute(&self, _args: &[u64], _out: &mut [u8]) -> Result<(), String> {
        tracing::info!("MockLogger executed successfully via tracing API");
        Err("Intentional mock fallback error".to_string())
    }
}

/// Reads the caller-specified image slice and writes each byte + 1 to the
/// output buffer.  Verifies that `img_buffer` is correctly sliced before the
/// extension executes.
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

impl BrinkRangedExtension for MockIncrement {
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
        assert!(
            img_buffer.len() >= out_buffer.len(),
            "MockIncrement: img_buffer must be at least as large as out_buffer (got {} vs {})",
            img_buffer.len(),
            out_buffer.len()
        );
        for (out, src) in out_buffer.iter_mut().zip(img_buffer.iter()) {
            *out = src.wrapping_add(1);
        }
        Ok(())
    }
}

/// Sums every byte in the caller-specified image slice and writes the result
/// as a little-endian u64.  Used to verify ranged and section-name call forms.
pub struct MockRangedSum {
    size_call_count: Cell<usize>,
}

impl MockRangedSum {
    pub fn new() -> Self {
        Self { size_call_count: Cell::new(0) }
    }
}

impl Default for MockRangedSum {
    fn default() -> Self {
        Self::new()
    }
}

impl BrinkRangedExtension for MockRangedSum {
    fn name(&self) -> &str {
        "brink::test_ranged_sum"
    }

    fn size(&self) -> usize {
        let prev = self.size_call_count.get();
        assert_eq!(prev, 0, "MockRangedSum::size() called more than once");
        self.size_call_count.set(prev + 1);
        8
    }

    fn execute(&self, _args: &[u64], img_buffer: &[u8], out_buffer: &mut [u8]) -> Result<(), String> {
        assert_eq!(out_buffer.len(), 8, "MockRangedSum: out_buffer must be exactly 8 bytes");
        let sum: u64 = img_buffer.iter().map(|&b| b as u64).sum();
        out_buffer.copy_from_slice(&sum.to_le_bytes());
        Ok(())
    }
}

/// Rejects a zero-length input slice with an error.
/// Used to verify that extensions can reject empty ranges.
pub struct MockRejectEmpty {
    size_call_count: Cell<usize>,
}

impl MockRejectEmpty {
    pub fn new() -> Self {
        Self { size_call_count: Cell::new(0) }
    }
}

impl Default for MockRejectEmpty {
    fn default() -> Self {
        Self::new()
    }
}

impl BrinkRangedExtension for MockRejectEmpty {
    fn name(&self) -> &str {
        "brink::test_reject_empty"
    }

    fn size(&self) -> usize {
        let prev = self.size_call_count.get();
        assert_eq!(prev, 0, "MockRejectEmpty::size() called more than once");
        self.size_call_count.set(prev + 1);
        4
    }

    fn execute(&self, _args: &[u64], img_buffer: &[u8], out_buffer: &mut [u8]) -> Result<(), String> {
        if img_buffer.is_empty() {
            return Err("brink::test_reject_empty: input range must not be empty".to_string());
        }
        out_buffer.fill(0xAB);
        Ok(())
    }
}

pub fn register_test_extensions(reg: &mut ExtensionRegistry) {
    reg.register(Box::new(MockCrc::new()));
    reg.register(Box::new(MockLogger::new()));
    reg.register_ranged(Box::new(MockIncrement::new()));
    reg.register_ranged(Box::new(MockRangedSum::new()));
    reg.register_ranged(Box::new(MockRejectEmpty::new()));
}
