use super::*;

pub struct MockCrc;

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

pub struct MockLogger;

impl BrinkExtension for MockLogger {
    fn name(&self) -> &str {
        "brink::test_logger"
    }
    fn execute(&self, _args: &[u64], _img: &[u8], _out: &mut [u8]) -> Result<(), String> {
        tracing::info!("MockLogger executed successfully via tracing API");
        Err("Intentional mock fallback error".to_string())
    }
}

pub fn register_test_extensions(reg: &mut ExtensionRegistry) {
    reg.register(Box::new(MockCrc));
    reg.register(Box::new(MockLogger));
}
