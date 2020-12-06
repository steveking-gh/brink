// Copyright (c) 2020 Steve King
// See license.txt.

#![warn(clippy::all)]

use std::vec::Vec;
use std::env;
use std::{io,fs};
use std::fs::File;
use std::io::prelude::*;
use anyhow::{Context,Result,bail};
use indextree::NodeId;
extern crate clap;
use clap::{Arg, App};

// Local libraries
use diags::Diags;
use ast::{Ast,AstDb};
use lineardb::LinearDb;


// Logging
#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

trait ActionInfo {
    fn set_abs_addr(&mut self, abs: usize);
    fn get_abs_addr(&self) -> usize;
    fn get_nid(&self) -> NodeId;
    fn get_size(&self) -> usize;
    fn write(&self, file: &mut fs::File) -> anyhow::Result<()>;
    fn get_type_str(&self) -> &'static str;
}

struct WrsActionInfo<'toks> {
    abs_addr: usize,
    nid: NodeId,
    str_size: usize,
    strout: &'toks str,
}

impl<'toks> WrsActionInfo<'toks> {
    pub fn new(abs_addr: usize, nid: NodeId, ast: &'toks Ast) -> WrsActionInfo<'toks> {
        debug!("WrsActionInfo::new: >>>> ENTER for nid {} at {}", nid, abs_addr);
        let strout = ast.get_child_str(nid, 0).trim_matches('\"');
        debug!("WrsActionInfo::new: output string is {}", strout);
        let str_size = strout.len();
        debug!("WrsActionInfo::new: <<<< EXIT for nid {}", nid);
        WrsActionInfo{ abs_addr, nid, str_size, strout}
    }
}

impl<'toks> ActionInfo for WrsActionInfo<'toks> {
    fn set_abs_addr(&mut self, abs: usize) { self.abs_addr = abs; }
    fn get_abs_addr(&self) -> usize { self.abs_addr}
    fn get_nid(&self) -> NodeId { self.nid}
    fn get_size(&self) -> usize { self.str_size }
    fn write(&self, file: &mut fs::File) -> anyhow::Result<()> {
        let s = self.strout.trim_matches('\"').to_string()
                    .replace("\\n", "\n")
                    .replace("\\t", "\t");
        file.write_all(s.as_bytes())
                    .context(format!("Wrs failed to write string {}", s))?;
        Ok(())
    }
    fn get_type_str(&self) -> &'static str {
        "wrs"
    }
}

/*****************************************************************************
 * ActionDb
 * The ActionDb contains a map of the logical size in bytes of all items with a
 * size in the AST. The key is the AST NodeID, the value is the size.
 *****************************************************************************/
struct ActionDb<'toks> {
    actions : Vec<Box<dyn ActionInfo + 'toks>>,
    file_name_str: String,
}

impl<'toks> ActionDb<'toks> {

    /// Dump the DB for debug
    pub fn dump(&self) {
        for a in &self.actions {
            debug!("ActionDb: nid {} is {} bytes at absolute address {}",
                    a.get_nid(), a.get_size(), a.get_abs_addr());
        }
    }

    pub fn new(linear_db: &LinearDb, _diags: &mut Diags, args: &'toks clap::ArgMatches,
               ast: &'toks Ast, _ast_db: &'toks AstDb, abs_start: usize)
               -> ActionDb<'toks> {

        debug!("ActionDb::new: >>>> ENTER for output nid: {} at {}", linear_db.output_nid,
                abs_start);
        let mut actions : Vec<Box<dyn ActionInfo + 'toks>> = Vec::new();

        // First pass to build sizes
        let mut start = abs_start;
        let mut new_size = 0;
        for &nid in &linear_db.nidvec {
            let tinfo = ast.get_tinfo(nid);
            match tinfo.tok {
                ast::LexToken::Wrs => {
                    let wrsa = Box::new(WrsActionInfo::new(start, nid, ast));
                    let sz = wrsa.get_size();
                    start += sz;
                    new_size += sz;
                    actions.push(wrsa);
                },
                _ => () // trivial zero size token like ';'.
            };
        }

        let mut old_size = new_size;
        let mut iteration = 1;
        // Iterate until the size of the section stops changing.
        loop {
            new_size = 0;
            for ainfo in &actions {
                debug!("ActionDb::new: Iterating for {} at nid {}", ainfo.get_type_str(), ainfo.get_nid());
                let sz = ainfo.get_size();
                start += sz;
                new_size += sz;
            }

            if old_size == new_size {
                break;
            }
            debug!("ActionDb::new: Size for iteration {} is {}", iteration, new_size);
            old_size = new_size;
            iteration += 1;
        }

        // Determine if the user specified an output file on the command line
        // Trim whitespace
        let file_name_str = String::from(args.value_of("output")
                                             .unwrap_or("output.bin")
                                             .trim_matches(' '));
        debug!("ActionDb::new: output file name is {}", file_name_str);

        debug!("ActionDb::new: <<<< EXIT with size {}", new_size);
        ActionDb { actions, file_name_str }
    }

    pub fn write(&self) -> anyhow::Result<()> {
        let mut file = File::create(&self.file_name_str)
                .context(format!("Unable to create output file {}", self.file_name_str))?;

        for ainfo in &self.actions {
            debug!("ActionDb::write: writing {} at nid {}", ainfo.get_type_str(), ainfo.get_nid());
            ainfo.write(&mut file)?;
        }

        Ok(())
    }
}


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

    ast.dump();

    let ast_db = AstDb::new(&mut diags, &ast)?;
    let linear_db = LinearDb::new(&mut diags, &ast, &ast_db);
    if linear_db.is_none() {
        bail!("[MAIN_3]: Failed to construct the linear database.");
    }
    let linear_db = linear_db.unwrap();
    linear_db.dump(&ast);
    let action_db = ActionDb::new(&linear_db, &mut diags, args, &ast, &ast_db, 0);
    action_db.dump();
    action_db.write()?;
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

    let str_in = fs::read_to_string(&in_file_name)
        .with_context(|| format!(
                "Failed to read from file {}.\nWorking directory is {}",
                in_file_name, env::current_dir().unwrap().display()))?;

    process(&in_file_name, &str_in, &args, verbosity)?;

    Ok(())
}
