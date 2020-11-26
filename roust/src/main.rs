// Copyright (c) 2020 Steve King
// See license.txt.

#![warn(clippy::all)]

use std::vec::Vec;
use std::{io,fs};
use std::fs::File;
use std::io::prelude::*;
use anyhow::{Context,Result,bail};
use indextree::NodeId;
extern crate clap;
use clap::{Arg, App};

// Local libraries
use diags::Diags;

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
        let mut children = nid.children(&ast.arena);
        let str_nid = children.next().unwrap();
        let str_tinfo = ast.get_tinfo(str_nid);
        // trim the leading and trailing quote characters
        let strout = str_tinfo.val.trim_matches('\"');
        debug!("WrsActionInfo::new: output string at nid {} is {}", str_nid, strout);
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
 * ActionDB
 * The ActionDB contains a map of the logical size in bytes of all items with a
 * size in the AST. The key is the AST NodeID, the value is the size.
 *****************************************************************************/
struct ActionDB<'toks> {
    actions : Vec<Box<dyn ActionInfo + 'toks>>,
    file_name_str: &'toks str,
}

use ast::{Ast,AstDb};

impl<'toks> ActionDB<'toks> {

    /// Dump the DB for debug
    pub fn dump(&self) {
        for a in &self.actions {
            debug!("ActionDB: nid {} is {} bytes at absolute address {}",
                    a.get_nid(), a.get_size(), a.get_abs_addr());
        }
    }

    pub fn new(linear_db: &LinearDB, _diags: &mut Diags, ast: &'toks Ast,
               _ast_db: &'toks AstDb, abs_start: usize) -> ActionDB<'toks> {

        debug!("ActionDB::new: >>>> ENTER for output nid: {} at {}", linear_db.output_nid,
                abs_start);
        let mut actions : Vec<Box<dyn ActionInfo + 'toks>> = Vec::new();
        let output_nid = linear_db.output_nid;


        // Using the name of the section, use the AST database to get a reference
        // to the section object.  ast_db processing has already guaranteed
        // that the section name is legitimate, so unwrap().
        let mut children = output_nid.children(&ast.arena);
        let sec_name_nid = children.next().unwrap();
        let sec_tinfo = ast.get_tinfo(sec_name_nid);
        let sec_str = sec_tinfo.val;
        debug!("ActionDB::new: output section name is {}", sec_str);

        let file_name_nid = children.next().unwrap();
        let file_tinfo = ast.get_tinfo(file_name_nid);
        // strip the surrounding quote chars from the string
        let file_name_str = file_tinfo.val.trim_matches('\"');
        debug!("ActionDB::new: output file name is {}", file_name_str);

        // Iterate until the size of the section stops changing.
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
        loop {
            new_size = 0;
            for ainfo in &actions {
                debug!("ActionDB::new: Iterating for {} at nid {}", ainfo.get_type_str(), ainfo.get_nid());
                let sz = ainfo.get_size();
                start += sz;
                new_size += sz;
            }

            if old_size == new_size {
                break;
            }
            debug!("ActionDB::new: Size for iteration {} is {}", iteration, new_size);
            old_size = new_size;
            iteration += 1;
        }

        debug!("ActionDB::new: <<<< EXIT with size {}", new_size);
        ActionDB { actions, file_name_str }
    }

    pub fn write(&self) -> anyhow::Result<()> {
        let mut file = File::create(self.file_name_str)
                .context(format!("Unable to create output file {}", self.file_name_str))?;

        for ainfo in &self.actions {
            debug!("ActionDB::write: writing {} at nid {}", ainfo.get_type_str(), ainfo.get_nid());
            ainfo.write(&mut file)?;
        }

        Ok(())
    }
}

struct LinearDB {
    output_nid: NodeId,
    nidvec : Vec<NodeId>,
}

impl<'toks> LinearDB {

    // Control recursion to some safe level.  100 is just a guesstimate.
    const MAX_RECURSION_DEPTH:usize = 100;

    /// Recursively record information about the children of an AST object.
    fn record_r(&mut self, rdepth: usize, parent_nid: NodeId, diags: &mut Diags,
                            ast: &'toks Ast, ast_db: &AstDb) -> bool {

        debug!("LinearDB::record_children_info: >>>> ENTER at depth {} for parent nid: {}",
                rdepth, parent_nid);

        if rdepth > LinearDB::MAX_RECURSION_DEPTH {
            let tinfo = ast.get_tinfo(parent_nid);
            let m = format!("Maximum recursion depth ({}) exceeded when processing '{}'.",
                            LinearDB::MAX_RECURSION_DEPTH, tinfo.val);
            diags.err1("MAIN_11", &m, tinfo.span());
            return false;
        }

        self.nidvec.push(parent_nid);
        let children = parent_nid.children(&ast.arena);
        let mut result = true;
        for nid in children {
            result &= self.record_r(rdepth + 1, nid, diags, ast, ast_db);
        }
        debug!("LinearDB::record_r: <<<< EXIT({}) at depth {} for nid: {}",
                result, rdepth, parent_nid);
        result
    }

    /// The ActionDB object must start with an output statement
    pub fn new(output_nid: NodeId, diags: &mut Diags, ast: &'toks Ast,
               ast_db: &'toks AstDb) -> Option<LinearDB> {

        debug!("LinearDB::new: >>>> ENTER for output nid: {}", output_nid);
        let mut linear_db = LinearDB { output_nid, nidvec: Vec::new() };

        let mut children = output_nid.children(&ast.arena);
        let sec_name_nid = children.next().unwrap();
        let sec_tinfo = ast.get_tinfo(sec_name_nid);
        let sec_str = sec_tinfo.val;
        debug!("LinearDB::new: output section name is {}", sec_str);

        // Using the name of the section, use the AST database to get a reference
        // to the section object.  ast_db processing has already guaranteed
        // that the section name is legitimate, so unwrap().
        let section = ast_db.sections.get(sec_str).unwrap();
        let sec_nid = section.nid;

        // To start recursion, rdepth = 1
        if !linear_db.record_r(1, sec_nid, diags, ast, ast_db) {
            return None;
        }

        debug!("LinearDB::new: <<<< EXIT for nid: {}", output_nid);
        Some(linear_db)
    }

    fn dump(&self) {
        debug!("LinearDB: Output NID {}", self.output_nid);
        for nid in &self.nidvec {
            debug!("LinearDB: {}", nid);
        }
    }
}

/// Entry point for all processing on the input source file
/// name: The name of the file
/// fstr: A string containing the file
pub fn process(name: &str, fstr: &str) -> anyhow::Result<()> {
    info!("Processing {}", name);
    debug!("File contains: {}", fstr);

    let mut diags = Diags::new(name,fstr);

    let ast = Ast::new(fstr, &mut diags)
              .context("Error[MAIN_1]: Abstract syntax tree creation failed")?;

    ast.dump();

    let ast_db = AstDb::new(&mut diags, &ast)?;

    if ast_db.outputs.is_empty() {
        diags.warn("MAIN_10", "No output statement, nothing to do.");
        // this is not a bail
    }

    // Take the reference to the ast_db to avoid a move due to the
    // implicit into_iter().
    // http://xion.io/post/code/rust-for-loop.html
    // https://stackoverflow.com/q/43036279/233981
    for outp in &ast_db.outputs {
        let linear_db = LinearDB::new(outp.nid, &mut diags, &ast, &ast_db);
        if linear_db.is_none() {
            bail!("Failed to construct the linear database.");
        }
        let linear_db = linear_db.unwrap();
        linear_db.dump();
        let action_db = ActionDB::new(&linear_db, &mut diags, &ast, &ast_db, 0);
        action_db.dump();
        action_db.write()?;
    }
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
    let args = App::new("roust")
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
            .arg(Arg::with_name("quiet")
                .short("q")
                .long("quiet")
                .help("Suppress non-error console output.  Overrides -v."))
            .get_matches();

    // Default verbosity
    let verbosity = if args.is_present("quiet") {
        0
    } else {
        1 + args.occurrences_of("verbosity")
    };

    init_log(verbosity).expect("Unknown error initializing logging.");

    info!("roust version {}", env!("CARGO_PKG_VERSION"));

    // Read the roust file into a string and pass to parser.
    // A bland error message here is fine since clap already
    // provides nice error messages.
    let in_file_name = args.value_of("INPUT")
            .context("Unknown input file argument error.")?;

    let str_in = fs::read_to_string(in_file_name)
        .with_context(|| format!("Failed to read from file {}", in_file_name))?;

    process(&in_file_name, &str_in)?;

    Ok(())
}
