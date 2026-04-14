// std::crc32c -- Castagnoli CRC-32 extension for Brink.
//
// Computes the CRC-32C (Castagnoli polynomial, 0x1EDC6F41) over a
// caller-specified image region and writes the 4-byte result
// little-endian into the output image.
//
// Call-site syntax (section-name form):
//     wr std::crc32c(my_section);
//
// Output: 4 bytes, little-endian u32.

use brink_extension::{BrinkExtension, ExtArg};
use ext::ExtensionRegistry;

pub struct Crc32c;

impl BrinkExtension for Crc32c {
    fn name(&self) -> &str {
        "std::crc32c"
    }

    fn size(&self) -> usize {
        4
    }

    fn execute<'a>(&self, args: &[ExtArg<'a>], out_buffer: &mut [u8]) -> Result<(), String> {
        let ExtArg::Section { data: img_buffer, .. } = args.first().ok_or(
            "std::crc32c: expected ExtArg::Section as args[0]".to_string(),
        )? else {
            return Err("std::crc32c: args[0] must be ExtArg::Section (use section-name form)".to_string());
        };
        let crc = crc32c::crc32c(img_buffer);
        out_buffer.copy_from_slice(&crc.to_le_bytes());
        Ok(())
    }
}

/// Registers `std::crc32c` into the given registry.
/// Call this once during process startup, before compiling any scripts.
pub fn register(registry: &mut ExtensionRegistry) {
    registry.register(Box::new(Crc32c));
}
