// Copyright (c) 2020 Steve King
// See license.txt.
#![forbid(unsafe_code)]
#![warn(clippy::all, rust_2018_idioms)]

use anyhow::{Context, Result};
use clap::Parser;
use std::env;
use std::fs;

// Local libraries
use process::process;

// Logging
use tracing::{Level, info, warn};
use tracing_subscriber::FmtSubscriber;

fn init_log(verbosity: u64) -> Result<()> {
    let level = match verbosity {
        0 => Level::ERROR, // Quiet
        1 => Level::WARN,  // Default
        2 => Level::INFO,  // -v
        3 => Level::DEBUG, // -v -v
        _ => Level::TRACE, // -v -v -v
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_target(false)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .map_err(|e| anyhow::anyhow!("Failed to set subscriber: {}", e))?;

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
