// Copyright (c) 2020 Steve King
// See license.txt.
#![forbid(unsafe_code)]
#![warn(clippy::all, rust_2018_idioms)]

use anyhow::{Context, Result};
use clap::Parser;
use std::env;
use std::fs;
use std::path::Path;

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

    /// Define a const value, e.g. -DBASE=0x1000 or -DCOUNT=4.
    /// May be repeated. Overrides any same-named const in the source.
    #[arg(short = 'D', value_name = "NAME[=VALUE]", action = clap::ArgAction::Append)]
    pub defines: Vec<String>,

    /// Suppress console output, including error messages.
    /// Useful for fuzz testing. Overrides -v.
    #[arg(short = 'q', long = "quiet")]
    pub quiet: bool,

    /// Write a CSV map file.  Omit FILE to derive name from input
    /// (e.g. firmware.brink -> firmware.map.csv).  Use FILE=- for stdout.
    #[arg(long = "map-csv", value_name = "FILE", num_args(0..=1), default_missing_value = "", require_equals = true)]
    pub map_csv: Option<String>,

    /// Write a JSON map file.  Omit FILE to derive name from input
    /// (e.g. firmware.brink -> firmware.map.json).  Use FILE=- for stdout.
    #[arg(long = "map-json", value_name = "FILE", num_args(0..=1), default_missing_value = "", require_equals = true)]
    pub map_json: Option<String>,

    /// Write a C99 map file.  Omit FILE to derive name from input
    /// (e.g. firmware.brink -> firmware.map.h).  Use FILE=- for stdout.
    #[arg(long = "map-c99", value_name = "FILE", num_args(0..=1), default_missing_value = "", require_equals = true)]
    pub map_c99: Option<String>,

    /// Write a Rust module map file.  Omit FILE to derive name from input
    /// (e.g. firmware.brink -> firmware.map.rs).  Use FILE=- for stdout.
    #[arg(long = "map-rs", value_name = "FILE", num_args(0..=1), default_missing_value = "", require_equals = true)]
    pub map_rs: Option<String>,
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

    // Resolve map flags: "" sentinel -> derive basename from input + extension.
    let map_csv_resolved;
    let map_csv = match cli.map_csv.as_deref() {
        Some("") => {
            let stem = Path::new(in_file_name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");
            map_csv_resolved = format!("{stem}.map.csv");
            Some(map_csv_resolved.as_str())
        }
        other => other,
    };

    let map_json_resolved;
    let map_json = match cli.map_json.as_deref() {
        Some("") => {
            let stem = Path::new(in_file_name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");
            map_json_resolved = format!("{stem}.map.json");
            Some(map_json_resolved.as_str())
        }
        other => other,
    };

    let map_c99_resolved;
    let map_c99 = match cli.map_c99.as_deref() {
        Some("") => {
            let stem = Path::new(in_file_name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");
            map_c99_resolved = format!("{stem}.map.h");
            Some(map_c99_resolved.as_str())
        }
        other => other,
    };

    let map_rs_resolved;
    let map_rs = match cli.map_rs.as_deref() {
        Some("") => {
            let stem = Path::new(in_file_name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");
            map_rs_resolved = format!("{stem}.map.rs");
            Some(map_rs_resolved.as_str())
        }
        other => other,
    };

    process(
        in_file_name,
        &str_in,
        cli.output.as_deref(),
        verbosity,
        cli.noprint,
        &cli.defines,
        map_csv,
        map_json,
        map_c99,
        map_rs,
    )
}
