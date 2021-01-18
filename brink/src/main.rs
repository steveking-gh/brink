// Copyright (c) 2020 Steve King
// See license.txt.

#![warn(clippy::all)]

use std::env;
use std::{io,fs};
use std::fs::File;
use anyhow::{Context,Result,bail};
extern crate clap;
use clap::{Arg, App};

// Local libraries
use diags::Diags;
use ast::{Ast,AstDb};
use lineardb::LinearDb;
use ir::IRDb;


// Logging
#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

/// Entry point for all processing on the input source file
/// name: The name of the file
/// fstr: A string containing the file
pub fn process(name: &str, fstr: &str, args: &clap::ArgMatches, verbosity: u64)
               -> anyhow::Result<()> {
    info!("Processing {}", name);
    debug!("File contains: {}", fstr);

    let mut diags = Diags::new(name,fstr,verbosity);

    let ast = Ast::new(fstr, &mut diags);
    if ast.is_none() {
        bail!("[MAIN_2]: Failed to construct the abstract syntax tree.");
    }

    let ast = ast.unwrap();

    ast.dump("ast.dot")?;

    let ast_db = AstDb::new(&mut diags, &ast)?;
    let linear_db = LinearDb::new(&mut diags, &ast, &ast_db, 0);
    if linear_db.is_none() {
        bail!("[MAIN_3]: Failed to construct the linear database.");
    }
    let linear_db = linear_db.unwrap();
    linear_db.dump();
    let ir_db = IRDb::new(&linear_db, &mut diags);
    if ir_db.is_none() {
        bail!("[MAIN_4]: Failed to construct the IR database.");
    }
    let ir_db = ir_db.unwrap();

    debug!("Dumping ir_db");
    ir_db.dump();

    // Determine if the user specified an output file on the command line
    // Trim whitespace
    let fname_str = String::from(args.value_of("output")
                                            .unwrap_or("output.bin")
                                            .trim_matches(' '));
    debug!("process: output file name is {}", fname_str);

    let mut file = File::create(&fname_str)
            .context(format!("Unable to create output file {}", fname_str))?;

    //linear_db.execute(&mut file)?;
    Ok(())
}

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

    process(&in_file_name, &str_in, &args, verbosity)?;

    Ok(())
}
