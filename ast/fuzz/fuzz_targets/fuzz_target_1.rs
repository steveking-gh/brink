#![no_main]
use libfuzzer_sys::fuzz_target;
use ast::Ast;
use std::io::Write;
use diags::Diags;

// The fuzzer calls this function repeatedly
fuzz_target!(|data: &[u8]| {
    if let Ok(str_in) = std::str::from_utf8(data) {
        // Set the verbosity to 0 to avoid console error
        // messages during the test.
        let mut diags = Diags::new("fuzz_target_1",str_in, 0);
        let _ = Ast::new(str_in, &mut diags);
    }
});