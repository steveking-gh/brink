use indextree::{NodeId};
pub type Span = std::ops::Range<usize>;
use diags::Diags;
use std::fs::File;
use std::io::prelude::*; // for write_all
use anyhow::Context;

#[allow(unused_imports)]
#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use ast::{Ast,AstDb};

#[derive(Debug, Clone, Copy, PartialEq)]

enum IROpcode {
    Assert,
    Wrs,
}

trait IRInstruction {
    fn set_abs_addr(&mut self, abs: usize);
    fn get_abs_addr(&self) -> usize;
    fn get_nid(&self) -> NodeId;
    fn get_size(&self) -> usize;
    fn get_opcode(&self) -> IROpcode;
    fn execute(&self, file: &mut File) -> anyhow::Result<()>;
}

pub struct LinearBase {
    nid: NodeId,
    info: Option<Box<dyn IRInstruction>>
}

pub struct LinearDb {
    pub output_nid: NodeId,
    pub basevec : Vec<LinearBase>,
}

pub struct WrsLinearInfo {
    abs_addr: usize,
    nid: NodeId,
    str_size: usize,
    strout : String,
}

impl<'toks> WrsLinearInfo {
    pub fn new(abs_addr: usize, nid: NodeId, ast: &'toks Ast) -> WrsLinearInfo {
        debug!("WrsLinearInfo::new: >>>> ENTER for nid {} at {}", nid, abs_addr);
        // To calculate the correct size of the string, we have to
        // complete all escape transforms.  Since we're changing the string
        // we're not longer referring to a slice of the original token.
        let strout = ast.get_child_str(nid, 0)
                .trim_matches('\"')
                .to_string()
                .replace("\\n", "\n")
                .replace("\\t", "\t");
        debug!("WrsLinearInfo::new: output string is {}", strout);
        let str_size = strout.len();
        debug!("WrsLinearInfo::new: <<<< EXIT for nid {}", nid);
        WrsLinearInfo{ abs_addr, nid, str_size, strout }
    }
}

impl<'toks> IRInstruction for WrsLinearInfo {
    fn set_abs_addr(&mut self, abs: usize) { self.abs_addr = abs; }
    fn get_abs_addr(&self) -> usize { self.abs_addr}
    fn get_nid(&self) -> NodeId { self.nid}
    fn get_size(&self) -> usize { self.str_size }
    fn get_opcode(&self) -> IROpcode { IROpcode::Wrs }

    fn execute(&self, file: &mut File) -> anyhow::Result<()> {
        file.write_all(self.strout.as_bytes())
                    .context(format!("WrsLinearInfo::execute: failed to write."))?;
        Ok(())
    }
}

pub struct AssertLinearInfo {
    abs_addr: usize,
    nid: NodeId,
}

impl<'toks> AssertLinearInfo {
    pub fn new(abs_addr: usize, nid: NodeId, ast: &'toks Ast) -> AssertLinearInfo {
        debug!("AssertLinearInfo::new: >>>> ENTER for nid {} at {}", nid, abs_addr);
        debug!("AssertLinearInfo::new: <<<< EXIT for nid {}", nid);
        AssertLinearInfo{ abs_addr, nid }
    }
}

impl<'toks> IRInstruction for AssertLinearInfo {
    fn set_abs_addr(&mut self, abs: usize) { self.abs_addr = abs; }
    fn get_abs_addr(&self) -> usize { self.abs_addr}
    fn get_size(&self) -> usize { 0 }
    fn get_nid(&self) -> NodeId { self.nid}
    fn get_opcode(&self) -> IROpcode { IROpcode::Assert }
    fn execute(&self, _file: &mut File) -> anyhow::Result<()> {
        Ok(())
    }
}

impl<'toks> LinearDb {

    // Control recursion to some safe level.  100 is just a guesstimate.
    const MAX_RECURSION_DEPTH:usize = 100;

    /// Recursively record information about the children of an AST object.
    fn record_r(&mut self, rdepth: usize, parent_nid: NodeId, diags: &mut Diags,
                            ast: &'toks Ast, ast_db: &AstDb) -> bool {

        debug!("LinearDb::record_children_info: >>>> ENTER at depth {} for parent nid: {}",
                rdepth, parent_nid);

        // During flattening, we just inventory the NIDs and don't yet attempt to
        // process the node semantically.
        self.basevec.push(LinearBase{nid:parent_nid, info:None});

        if rdepth > LinearDb::MAX_RECURSION_DEPTH {
            let tinfo = ast.get_tinfo(parent_nid);
            let m = format!("Maximum recursion depth ({}) exceeded when processing '{}'.",
                            LinearDb::MAX_RECURSION_DEPTH, tinfo.val);
            diags.err1("MAIN_11", &m, tinfo.span());
            return false;
        }

        let mut result = true;
        let tinfo = ast.get_tinfo(parent_nid);
        match tinfo.tok {
            ast::LexToken::Wr => {
                // Write the contents of a section by dereferencing the section name
                let sec_name_str = ast.get_child_str(parent_nid, 0);
                debug!("LinearDb::record_r: wr section name is {}", sec_name_str);

                // Using the name of the section, use the AST database to get a reference
                // to the section object.  ast_db processing has already guaranteed
                // that the section name is legitimate, so unwrap().
                let section = ast_db.sections.get(sec_name_str).unwrap();
                let sec_nid = section.nid;
                result &= self.record_r(rdepth + 1, sec_nid, diags, ast, ast_db);
            },
            _ => {
                // Easy linearizing without dereferencing through a name.
                // When no children exist, this case terminates recursion.
                let children = parent_nid.children(&ast.arena);
                for nid in children {
                    result &= self.record_r(rdepth + 1, nid, diags, ast, ast_db);
                }
            }
        }

        debug!("LinearDb::record_r: <<<< EXIT({}) at depth {} for nid: {}",
                result, rdepth, parent_nid);
        result
    }

    /// The LinearDb object must start with an output statement.
    /// If the output doesn't exist, then we return None
    pub fn new(diags: &mut Diags, ast: &'toks Ast,
               ast_db: &'toks AstDb, abs_start: usize) -> Option<LinearDb> {
        debug!("LinearDb::new: >>>> ENTER");
        // AstDb already validated output exists
        let output_nid = ast_db.output.nid;
        let mut linear_db = LinearDb { output_nid, basevec: Vec::new() };

        let sec_name_str = ast.get_child_str(output_nid, 0);
        debug!("LinearDb::new: output section name is {}", sec_name_str);

        // Using the name of the section, use the AST database to get a reference
        // to the section object.  ast_db processing has already guaranteed
        // that the section name is legitimate, so unwrap().
        let section = ast_db.sections.get(sec_name_str).unwrap();
        let sec_nid = section.nid;

        // To start recursion, rdepth = 1.  The ONLY thing happening
        // here is a flattening of the AST into the logical order
        // of actions.
        if !linear_db.record_r(1, sec_nid, diags, ast, ast_db) {
            return None;
        }

        // We have now linearized the content by node ID.
        // Compute the address and size of each element.
        // Note that many elements don't have an address or size, e.g.
        // basic syntactical elements like ';'.
        debug!("Calculating sizes");
        let mut start = abs_start;
        let mut new_size = 0;
        for base in &mut linear_db.basevec {
            let tinfo = ast.get_tinfo(base.nid);
            match tinfo.tok {
                ast::LexToken::Wrs => {
                    let wrsa = Box::new(WrsLinearInfo::new(start, base.nid, ast));
                    let sz = wrsa.get_size();
                    start += sz;
                    new_size += sz;
                    debug!("Setting size {} for nid {}", sz, base.nid);
                    base.info = Some(wrsa);
                },
                ast::LexToken::Assert => {
                    let asrt = Box::new(AssertLinearInfo::new(start, base.nid, ast));
                    base.info = Some(asrt);
                },
                _ => () // trivial zero size token like ';'.
            };
        }

        // Sizes are known, iterate until addresses stabilize
        let mut old_size = new_size;
        let mut iteration = 1;
        start = abs_start;

        loop {
            new_size = 0;
            for base in &mut linear_db.basevec {
                // We skip uninteresting elements that didn't create an info object
                if let Some(info) = base.info.as_mut() {
                    debug!("LinearDb::new: Iterating for {:?} at nid {}",
                    info.get_opcode(), base.nid);
                    info.set_abs_addr(start);
                    let sz = info.get_size();
                    start += sz;
                    new_size += sz;
                }
            }

            if old_size == new_size {
                break;
            }

            debug!("LinearDb::new: Size for iteration {} is {}", iteration, new_size);
            old_size = new_size;
            iteration += 1;
        }

        // Sizes and addresses are known, so we can now evaluate assert statements
        for base in &mut linear_db.basevec {
            // We skip uninteresting elements that didn't create an info object
            if let Some(info) = base.info.as_ref() {
                if info.get_opcode() == IROpcode::Assert {
                    debug!("LinearDb::new: Assert");
                }
            }
        }


        debug!("LinearDb::new: <<<< EXIT for nid: {}", output_nid);
        Some(linear_db)
    }

    pub fn execute(&self, file: &mut File) -> anyhow::Result<()> {

        for base in &self.basevec {
            if let Some(info) = &base.info {
                debug!("LinearDb::execute: writing {:?} for nid {}", info.get_opcode(),
                                                                   info.get_nid());
                info.execute(file).context(format!("Execution failed for {:?}",
                                                info.get_opcode()))?;
            }
        }

        Ok(())
    }

    pub fn dump(&self) {
        for base in &self.basevec {
            if let Some(info) = &base.info {
                debug!("LinearDb: {}: {:?} at {:X}", base.nid, info.get_opcode(),
                                                     info.get_abs_addr());
            }
        }
    }
}
