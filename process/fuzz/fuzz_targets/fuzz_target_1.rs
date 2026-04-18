#![no_main]
use libfuzzer_sys::fuzz_target;
use process::process;

fuzz_target!(|data: &[u8]| {
    if let Ok(str_in) = std::str::from_utf8(data) {
        // Discard output to /dev/null -- no binary image is needed.
        // Nearly all fuzz inputs fail before execute(); the rare input
        // that reaches execute() will get an Err from the file write,
        // which is handled gracefully.
        let _ = process(
            "fuzz_target",
            str_in,
            Some("/dev/null"),
            0,       // verbosity: suppress all diagnostic output
            true,    // noprint: suppress print statements
            &[],     // defines: none
            65_536,  // max_output_size: 64 KiB ceiling for fast fuzzing
            None,    // map_csv
            None,    // map_json
            None,    // map_c99
            None,    // map_rs
        );
    }
});
