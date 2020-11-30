use logos::{Logos};
use indextree::{Arena,NodeId};
pub type Span = std::ops::Range<usize>;
use std::collections::HashMap;
use anyhow::{bail};
use diags::Diags;

#[allow(unused_imports)]
#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use ast::{Ast,AstDb};

pub struct LinearDb {
    pub output_nid: NodeId,
    pub nidvec : Vec<NodeId>,
}

impl<'toks> LinearDb {

    // Control recursion to some safe level.  100 is just a guesstimate.
    const MAX_RECURSION_DEPTH:usize = 100;

    /// Recursively record information about the children of an AST object.
    fn record_r(&mut self, rdepth: usize, parent_nid: NodeId, diags: &mut Diags,
                            ast: &'toks Ast, ast_db: &AstDb) -> bool {

        debug!("LinearDb::record_children_info: >>>> ENTER at depth {} for parent nid: {}",
                rdepth, parent_nid);

        self.nidvec.push(parent_nid);

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
               ast_db: &'toks AstDb) -> Option<LinearDb> {
        debug!("LinearDb::new: >>>> ENTER");
        if ast_db.output.is_none() {
            diags.err0("MAIN_1", "Missing output statement.");
            return None;
        }

        let output_nid = ast_db.output.as_ref()?.nid;
        let mut linear_db = LinearDb { output_nid, nidvec: Vec::new() };

        let sec_name_str = ast.get_child_str(output_nid, 0);
        debug!("LinearDb::new: output section name is {}", sec_name_str);

        // Using the name of the section, use the AST database to get a reference
        // to the section object.  ast_db processing has already guaranteed
        // that the section name is legitimate, so unwrap().
        let section = ast_db.sections.get(sec_name_str).unwrap();
        let sec_nid = section.nid;

        // To start recursion, rdepth = 1
        if !linear_db.record_r(1, sec_nid, diags, ast, ast_db) {
            return None;
        }

        debug!("LinearDb::new: <<<< EXIT for nid: {}", output_nid);
        Some(linear_db)
    }

    pub fn dump(&self, ast: &Ast) {
        debug!("LinearDb: Output NID {}", self.output_nid);
        for &nid in &self.nidvec {
            let tinfo = ast.get_tinfo(nid);
            debug!("LinearDb: {}: {}", nid, tinfo.val);
        }
    }
}
