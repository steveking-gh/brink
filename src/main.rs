// Copyright (c) 2020 Steve King
// See license.txt.
#![forbid(unsafe_code)]
#![warn(clippy::all, rust_2018_idioms)]

use anyhow::{Context, Result};
use clap::Parser;
use std::env;
use std::{fs, io};

// Local libraries
use process::process;

// Logging
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};

fn init_log(verbosity: u64) -> Result<(), fern::InitError> {
    let mut base_cfg = fern::Dispatch::new();

    base_cfg = match verbosity {
        0 => base_cfg.level(log::LevelFilter::Error), // Quiet
        1 => base_cfg.level(log::LevelFilter::Warn),  // Default
        2 => base_cfg.level(log::LevelFilter::Info),  // -v
        3 => base_cfg.level(log::LevelFilter::Debug), // -v -v
        _4_or_more => base_cfg.level(log::LevelFilter::Trace), // -v -v -v
    };

    let stdout_cfg = fern::Dispatch::new()
        .format(|out, message, record| out.finish(format_args!("[{}] {}", record.level(), message)))
        .chain(io::stdout());

    base_cfg.chain(stdout_cfg).apply()?;
    Ok(())
}

#[derive(Parser, Debug)]
#[command(version, author, about)]
pub struct Cli {
    /// The input source file.
    #[arg(index = 1)]
    pub input: String,

    /// Sets the verbosity level. Use up to 4 times.
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count)]
    pub verbosity: u8,

    /// Specifies output file name. Default is output.bin.
    #[arg(short = 'o', long = "output", value_name = "output_file")]
    pub output: Option<String>,

    /// Suppresses console print statements in source code. Default is false.
    #[arg(long = "noprint")]
    pub noprint: bool,

    /// Suppress console output, including error messages.
    /// Useful for fuzz testing. Overrides -v.
    #[arg(short = 'q', long = "quiet")]
    pub quiet: bool,
}

fn main() -> Result<()> {
    // clap processes args
    let cli = Cli::parse();

    // Default verbosity
    let verbosity = if cli.quiet {
        0
    } else {
        1 + cli.verbosity as u64
    };

    init_log(verbosity).expect("Unknown error initializing logging.");

    info!("brink version {}", env!("CARGO_PKG_VERSION"));

    let in_file_name = &cli.input;

    // remove carriage return from line endings for windows platforms
    let str_in = fs::read_to_string(in_file_name)
        .with_context(|| {
            format!(
                "Failed to read from file {}.\nWorking directory is {}",
                in_file_name,
                env::current_dir().unwrap().display()
            )
        })?
        .replace("\r\n", "\n");

    process(
        in_file_name,
        &str_in,
        cli.output.as_deref(),
        verbosity,
        cli.noprint,
    )
}
