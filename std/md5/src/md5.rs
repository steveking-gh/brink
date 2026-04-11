// std::md5 — MD5 extension for Brink.
//
// Computes an MD5 digest over a caller-specified image region and
// writes the 16-byte result into the output image.
//
// Call-site syntax (section-name form):
//     wr std::md5(my_section);
//
// Call-site syntax (explicit-range form):
//     wr std::md5(start_offset, length);
//
// Output: 16 bytes, standard MD5 byte order.

use brink_extension::BrinkRangedExtension;
use ext::ExtensionRegistry;
use md5::{Digest, Md5};

pub struct Md5Ext;

impl BrinkRangedExtension for Md5Ext {
    fn name(&self) -> &str {
        "std::md5"
    }

    fn size(&self) -> usize {
        16
    }

    fn execute(
        &self,
        _args: &[u64],
        img_buffer: &[u8],
        out_buffer: &mut [u8],
    ) -> Result<(), String> {
        let digest = Md5::digest(img_buffer);
        out_buffer.copy_from_slice(&digest);
        Ok(())
    }
}

/// Registers `std::md5` into the given registry.
/// Call once during process startup, before compiling any scripts.
pub fn register(registry: &mut ExtensionRegistry) {
    registry.register_ranged(Box::new(Md5Ext));
}
