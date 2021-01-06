use indextree::{NodeId};
pub type Span = std::ops::Range<usize>;
use diags::Diags;
use std::{collections::HashMap, fs::File};
use std::io::prelude::*; // for write_all
use anyhow::Context;

#[allow(unused_imports)]
#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use ast::{Ast,AstDb};

#[derive(Debug, Clone, Copy, PartialEq)]
enum LinearKind {
    Assert,
    SectionStart,
    SectionEnd,
    Wrs,
}
#[derive(Debug)]
pub struct LinearInfo {
    nid: NodeId,
    lid: usize,
    kind: LinearKind,
}

pub struct LinearDb {
    pub output_nid: NodeId,
    pub info_vec : Vec<LinearInfo>,
}

/**
To linearize, create a vector of all AST NIDs in logical order.
The same NID may appear *multiple times* in the linear vector,
e.g. a section written more than once to the output. Other than
computing the exact logical order and byte size of each NID, we don't yet
process NIDs semantically.  NIDs with size > 0 have an associated
boxed info object.
*/
impl<'toks> LinearDb {

    // Control recursion to some safe level.  100 is just a guesstimate.
    const MAX_RECURSION_DEPTH:usize = 100;

    fn depth_sanity(&self, rdepth: usize, parent_nid: NodeId, diags: &mut Diags, ast: &Ast) -> bool {
        if rdepth > LinearDb::MAX_RECURSION_DEPTH {
            let tinfo = ast.get_tinfo(parent_nid);
            let m = format!("Maximum recursion depth ({}) exceeded when processing '{}'.",
                            LinearDb::MAX_RECURSION_DEPTH, tinfo.val);
            diags.err1("LINEAR_1", &m, tinfo.span());
            return false;
        }
        true
    }

    /// Recursively record information about the children of an AST object.
    fn record_r(&mut self, rdepth: usize, parent_nid: NodeId, diags: &mut Diags,
                            ast: &'toks Ast, ast_db: &AstDb) -> bool {

        debug!("LinearDb::record_r: >>>> ENTER at depth {} for parent nid: {}",
                rdepth, parent_nid);

        if !self.depth_sanity(rdepth, parent_nid, diags, ast) {
            return false;
        }

        let mut result = true;
        let tinfo = ast.get_tinfo(parent_nid);

        match tinfo.tok {
            ast::LexToken::Wr => {
                // Write the contents of a section.  This isn't a simple recursion
                // into the children.  Instead, we recurse into the specified section.
                let sec_name_str = ast.get_child_str(parent_nid, 0);
                debug!("LinearDb::record_r: recursing into section {}", sec_name_str);

                // Using the name of the section, use the AST database to get a reference
                // to the section object.  ast_db processing has already guaranteed
                // that the section name is legitimate, so unwrap().
                let section = ast_db.sections.get(sec_name_str).unwrap();
                let sec_nid = section.nid;

                // Record the linear start of this section.
                self.info_vec.push(LinearInfo {nid: sec_nid, lid: self.info_vec.len(), kind: LinearKind::SectionStart});
                result &= self.record_r(rdepth + 1, sec_nid, diags, ast, ast_db);
                self.info_vec.push(LinearInfo {nid: sec_nid, lid: self.info_vec.len(), kind: LinearKind::SectionEnd});
            },
            ast::LexToken::Wrs => {
                // Write a fixed string
                self.info_vec.push( LinearInfo {nid:parent_nid, lid: self.info_vec.len(), kind: LinearKind::Wrs});
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
    /// If the output doesn't exist, then return None.  The linear_db
    /// records only elements with size > 0.
    pub fn new(diags: &mut Diags, ast: &'toks Ast,
               ast_db: &'toks AstDb, abs_start: usize) -> Option<LinearDb> {
        debug!("LinearDb::new: >>>> ENTER");
        // AstDb already validated output exists
        let output_nid = ast_db.output.nid;
        let mut linear_db = LinearDb { output_nid, info_vec: Vec::new() };

        let sec_name_str = ast.get_child_str(output_nid, 0);
        debug!("LinearDb::new: output section name is {}", sec_name_str);

        // Using the name of the section, use the AST database to get a reference
        // to the section object.  ast_db processing has already guaranteed
        // that the section name is legitimate, so unwrap().
        let section = ast_db.sections.get(sec_name_str).unwrap();
        let sec_nid = section.nid;

        // Record the linear start of this section.
        linear_db.info_vec.push(LinearInfo {nid: sec_nid, lid: linear_db.info_vec.len(),
                                kind: LinearKind::SectionStart});

        // To start recursion, rdepth = 1.  The ONLY thing happening
        // here is a flattening of the AST into the logical order
        // of actions.
        if !linear_db.record_r(1, sec_nid, diags, ast, ast_db) {
            return None;
        }

        // Record the linear end of this section.
        linear_db.info_vec.push(LinearInfo {nid: sec_nid, lid: linear_db.info_vec.len(),
                                kind: LinearKind::SectionEnd});

        debug!("LinearDb::new: <<<< EXIT for nid: {}", output_nid);
        Some(linear_db)
    }

    pub fn dump(&self) {
        for info in &self.info_vec {
            debug!("LinearDb: lid {}: nid {} is {:?}", info.lid, info.nid, info.kind);
        }
    }
}
