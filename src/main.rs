// Copyright (c) 2020 Steve King
// See license.txt.

#![warn(clippy::all)]

use std::env;
use std::{io,fs};
use anyhow::{Result,Context};
extern crate clap;
use clap::{Arg, App};

// Local libraries
use process::process;


// Logging
#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

fn init_log(verbosity : u64) -> Result<(), fern::InitError>  {
    let mut base_cfg = fern::Dispatch::new();

    base_cfg = match verbosity {
        0 => base_cfg.level(log::LevelFilter::Error), // Quiet
        1 => base_cfg.level(log::LevelFilter::Warn),  // Default
        2 => base_cfg.level(log::LevelFilter::Info),  // -v
        3 => base_cfg.level(log::LevelFilter::Debug), // -v -v
        _4_or_more => base_cfg.level(log::LevelFilter::Trace), // -v -v -v
    };

    let stdout_cfg = fern::Dispatch::new()
            .format(|out, message, record| {
            out.finish(format_args!(
                "[{}] {}",
                record.level(),
                message))
            })
            .chain(io::stdout());

    base_cfg.chain(stdout_cfg)
            .apply()?;
    Ok(())
}

fn main() -> Result<()> {
    // clap processes args
    let args = App::new("brink")
            // See Cargo.toml for env! CARGO strings.
            .version(env!("CARGO_PKG_VERSION"))
            .author(env!("CARGO_PKG_AUTHORS"))
            .about(env!("CARGO_PKG_DESCRIPTION"))
            .arg(Arg::with_name("INPUT")
            .help("The input source file.")
            .required(true)
            .index(1))
            .arg(Arg::with_name("verbosity")
                .short("v")
                .long("verbose")
                .multiple(true)
                .help("Sets the verbosity level. Use up to 4 times."))
            .arg(Arg::with_name("output")
                .short("o")
                .long("output")
                .value_name("output_file")
                .takes_value(true)
                .help("Specifies output file name.  Default is output.bin."))
            .arg(Arg::with_name("noprint")
                .long("noprint")
                .value_name("noprint")
                .takes_value(false)
                .help("Suppresses console print statements in source code.  Default is false."))
            .arg(Arg::with_name("quiet")
                .short("q")
                .long("quiet")
                .help("Suppress console output, including error messages.  Useful for fuzz testing.  Overrides -v."))
            .get_matches();

    // Default verbosity
    let verbosity = if args.is_present("quiet") {
        0
    } else {
        1 + args.occurrences_of("verbosity")
    };

    init_log(verbosity).expect("Unknown error initializing logging.");

    info!("brink version {}", env!("CARGO_PKG_VERSION"));

    // Read the brink file into a string and pass to parser.
    // A bland error message here is fine since clap already
    // provides nice error messages.
    let in_file_name = args.value_of("INPUT")
            .context("Unknown input file argument error.")?;

    // remove carriage return from line endings for windows platforms
    let str_in = fs::read_to_string(&in_file_name)
        .with_context(|| format!(
                "Failed to read from file {}.\nWorking directory is {}",
                in_file_name, env::current_dir().unwrap().display()))?
        .replace("\r\n","\n");

    process(&in_file_name, &str_in, &args, verbosity,
             args.is_present("noprint"))
}
