#![no_main]
use libfuzzer_sys::fuzz_target;

use roust::Ast;

// The fuzzer calls this function repeatedly
fuzz_target!(|data: &[u8]| {
    if let Ok(str_in) = std::str::from_utf8(data) {
            let _ = Ast::new("fuzz_test", str_in);
    }
});