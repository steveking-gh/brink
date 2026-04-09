// std::sha256 — SHA-256 extension for Brink.
//
// Computes a SHA-256 digest over a caller-specified image region and
// writes the 32-byte result into the output image.
//
// Call-site syntax (section-name form):
//     wr std::sha256(my_section);
//
// Call-site syntax (explicit-range form):
//     wr std::sha256(start_offset, length);
//
// Output: 32 bytes, big-endian digest (standard SHA-256 byte order).

use brink_extension::BrinkRangedExtension;
use ext::ExtensionRegistry;
use sha2::{Digest, Sha256};

pub struct Sha256Ext;

impl BrinkRangedExtension for Sha256Ext {
    fn name(&self) -> &str {
        "std::sha256"
    }

    fn size(&self) -> usize {
        32
    }

    fn execute(
        &self,
        _args: &[u64],
        img_buffer: &[u8],
        out_buffer: &mut [u8],
    ) -> Result<(), String> {
        let digest = Sha256::digest(img_buffer);
        out_buffer.copy_from_slice(&digest);
        Ok(())
    }
}

/// Registers `std::sha256` into the given registry.
/// Call this once during process startup, before compiling any scripts.
pub fn register(registry: &mut ExtensionRegistry) {
    registry.register_ranged(Box::new(Sha256Ext));
}
