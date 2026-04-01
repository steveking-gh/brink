// std::crc32c — Castagnoli CRC-32 extension for Brink.
//
// Computes the CRC-32C (Castagnoli polynomial, 0x1EDC6F41) over a
// caller-specified image region and writes the 4-byte result
// little-endian into the output image.
//
// Call-site syntax (section-name form):
//     wr std::crc32c(my_section);
//
// Call-site syntax (explicit-range form):
//     wr std::crc32c(start_offset, length);
//
// Output: 4 bytes, little-endian u32.

use brink_extension::BrinkRangedExtension;
use ext::ExtensionRegistry;

pub struct Crc32c;

impl BrinkRangedExtension for Crc32c {
    fn name(&self) -> &str {
        "std::crc32c"
    }

    // Return the fixed byte length of the output buffer.
    fn size(&self) -> usize {
        4
    }

    fn execute(
        &self,
        _args: &[u64],
        img_buffer: &[u8],
        out_buffer: &mut [u8],
    ) -> Result<(), String> {
        let crc = crc32c::crc32c(img_buffer);
        out_buffer.copy_from_slice(&crc.to_le_bytes());
        Ok(())
    }
}

/// Registers `std::crc32c` into the given registry.
/// Call this once during process startup, before compiling any scripts.
pub fn register(registry: &mut ExtensionRegistry) {
    registry.register_ranged(Box::new(Crc32c));
}
