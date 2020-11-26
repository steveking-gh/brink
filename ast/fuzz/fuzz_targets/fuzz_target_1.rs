#![no_main]
use libfuzzer_sys::fuzz_target;
use ast::Ast;
use std::io::Write;
use diags::Diags;

// The fuzzer calls this function repeatedly
fuzz_target!(|data: &[u8]| {
    if let Ok(str_in) = std::str::from_utf8(data) {
        let mut diags = Diags::new("fuzz_target_1",str_in);
        let _ = Ast::new(str_in, &mut diags);
    }
});