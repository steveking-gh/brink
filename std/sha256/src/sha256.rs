// std::sha256 -- SHA-256 extension for Brink.
//
// Computes a SHA-256 digest over a caller-specified image region and
// writes the 32-byte result into the output image.
//
// Call-site syntax (section-name form):
//     wr std::sha256(my_section);
//
// Output: 32 bytes, big-endian digest (standard SHA-256 byte order).

use brink_extension::{BrinkExtension, ExtArg};
use ext::ExtensionRegistry;
use sha2::{Digest, Sha256};

pub struct Sha256Ext;

impl BrinkExtension for Sha256Ext {
    fn name(&self) -> &str {
        "std::sha256"
    }

    fn size(&self) -> usize {
        32
    }

    fn execute<'a>(&self, args: &[ExtArg<'a>], out_buffer: &mut [u8]) -> Result<(), String> {
        let ExtArg::Section { data: img_buffer, .. } = args.first().ok_or(
            "std::sha256: expected ExtArg::Section as args[0]".to_string(),
        )? else {
            return Err("std::sha256: args[0] must be ExtArg::Section (use section-name form)".to_string());
        };
        let digest = Sha256::digest(img_buffer);
        out_buffer.copy_from_slice(&digest);
        Ok(())
    }
}

/// Registers `std::sha256` into the given registry.
/// Call this once during process startup, before compiling any scripts.
pub fn register(registry: &mut ExtensionRegistry) {
    registry.register(Box::new(Sha256Ext));
}
