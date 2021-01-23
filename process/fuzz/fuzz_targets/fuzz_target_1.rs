#![no_main]
use libfuzzer_sys::fuzz_target;
use process::process;
use clap::{Arg, App};

fuzz_target!(|data: &[u8]| {
    if let Ok(str_in) = std::str::from_utf8(data) {
        let args = App::new("brink").get_matches_from( vec![""]);
        let _result = process("!! FUZZ TEST !!", str_in, &args, 0);
    }
});
