#![no_main]
use libfuzzer_sys::fuzz_target;
use process::process;
use clap::App;

fuzz_target!(|data: &[u8]| {
    if let Ok(str_in) = std::str::from_utf8(data) {
        // Get matches from a fake arg string, since we don't
        // want to process the fuzz testers actually command line!
        let args = App::new("brink").get_matches_from( vec![""]);
        let _result = process("!! FUZZ TEST !!", str_in, &args, 0, false);
    }
});
