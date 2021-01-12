use anyhow::{Error, Result};
use indextree::{NodeId};
pub type Span = std::ops::Range<usize>;
use diags::Diags;
use std::any::{Any, TypeId};

#[allow(unused_imports)]
#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use ast::{Ast,AstDb};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OperandKind {
    TempVar,
    Immediate,
}

pub struct IROperand {
    nid: NodeId,
    val: String,
    kind: OperandKind,
}

impl<'toks> IROperand {
    pub fn new(nid: NodeId, ast: &'toks Ast, kind: OperandKind) -> IROperand {
        let tinfo = ast.get_tinfo(nid);
        IROperand { nid, val: tinfo.val.to_string(), kind }
    }    
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IRKind {
    Assert,
    Begin,
    EqEq,
    Int,
    Load,
    Multiply,
    Add,
    SectionStart,
    SectionEnd,
    Wrs,
}

pub struct IR {
    nid: NodeId,
    op: IRKind,
    // usize is the index into the operand vec
    operand_vec: Vec<usize>,
}

impl IR {
    pub fn new(nid: NodeId, op: IRKind) -> Self {
        Self { nid, op, operand_vec: Vec::new() }
    }

    pub fn add_operand(&mut self, oper_num: usize) {
        self.operand_vec.push(oper_num);
    }
}

pub struct LinearDb {
    pub output_nid: NodeId,
    pub ir_vec: Vec<IR>,
    pub operand_vec: Vec<IROperand>, 
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

    // Adds an existing operand by it's operanc_vec index to the specified IR
    pub fn add_operand_idx_to_ir(&mut self, ir_lid: usize, idx: usize) {
        self.ir_vec[ir_lid].add_operand(idx);
    }

    // Returns the inear operand index occupied by the new operand
    pub fn add_operand_to_ir(&mut self, ir_lid: usize, oper: IROperand) -> usize {
        let idx = self.operand_vec.len();
        self.operand_vec.push(oper);
        self.add_operand_idx_to_ir(ir_lid, idx);
        idx
    }

    // returns the linear ID for the new IR
    fn new_ir(&mut self, nid: NodeId, op: IRKind) -> usize {
        let lid = self.ir_vec.len();
        self.ir_vec.push(IR::new(nid,op));
        lid
    }

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

    fn record_children_r(&mut self, rdepth: usize, parent_nid: NodeId, local_operands: &mut Vec<usize>,
                         diags: &mut Diags, ast: &'toks Ast, ast_db: &AstDb) -> bool {
        // Easy linearizing without dereferencing through a name.
        // When no children exist, this case terminates recursion.
        let mut result = true;
        let children = parent_nid.children(&ast.arena);
        for nid in children {
            result &= self.record_r(rdepth + 1, nid, local_operands, diags, ast, ast_db);
        }
        result
    }

    /// Recursively record information about the children of an AST object.
    fn record_r(&mut self, rdepth: usize, parent_nid: NodeId, returned_operands: &mut Vec<usize>,
                diags: &mut Diags, ast: &'toks Ast, ast_db: &AstDb) -> bool {

        debug!("LinearDb::record_r: >>>> ENTER at depth {} for parent nid: {}",
                rdepth, parent_nid);

        if !self.depth_sanity(rdepth, parent_nid, diags, ast) {
            return false;
        }

        let mut result = true;
        let tinfo = ast.get_tinfo(parent_nid);
        
        match tinfo.tok {
            ast::LexToken::Wr => {
                let mut local_operands = Vec::new();
                // Write the contents of a section.  This isn't a simple recursion
                // into the children.  Instead, we recurse into the specified section.
                let sec_name_str = ast.get_child_str(parent_nid, 0);
                debug!("LinearDb::record_r: recursing into section {}", sec_name_str);

                // Using the name of the section, use the AST database to get a reference
                // to the section object.  ast_db processing has already guaranteed
                // that the section name is legitimate, so unwrap().
                let section = ast_db.sections.get(sec_name_str).unwrap();
                let sec_nid = section.nid;
                result &= self.record_children_r(rdepth + 1, sec_nid, &mut local_operands, diags, ast, ast_db);
                assert!(local_operands.is_empty());
            },
            ast::LexToken::Wrs => {
                let mut local_operands = Vec::new();
                // Write a fixed string. The string is the operand
                let lid = self.new_ir(parent_nid, IRKind::Wrs);
                result &= self.record_children_r(rdepth + 1, parent_nid, &mut local_operands, diags, ast, ast_db);
                assert!(local_operands.len() == 1);
                while !local_operands.is_empty() {
                    self.add_operand_idx_to_ir(lid, local_operands.pop().unwrap());
                }
            },
            ast::LexToken::Int |
            ast::LexToken::QuotedString => {
                // These are immediate operands.
                let idx = self.operand_vec.len();
                self.operand_vec.push(IROperand::new(parent_nid,ast,OperandKind::Immediate));
                returned_operands.push(idx);
            },
            ast::LexToken::Assert => {
                // Assert an expression is not zero (false)
                let mut local_operands = Vec::new();
                result &= self.record_children_r(rdepth + 1, parent_nid, &mut local_operands, diags, ast, ast_db);
                let lid = self.new_ir(parent_nid, IRKind::Assert);
                assert!(local_operands.len() == 1);
                while !local_operands.is_empty() {
                    self.add_operand_idx_to_ir(lid, local_operands.pop().unwrap());
                }
            },
            ast::LexToken::EqEq => {
                let mut local_operands = Vec::new();
                result &= self.record_children_r(rdepth + 1, parent_nid, &mut local_operands, diags, ast, ast_db);
                let lid = self.new_ir(parent_nid, IRKind::EqEq);
                assert!(local_operands.len() == 2);
                while !local_operands.is_empty() {
                    self.add_operand_idx_to_ir(lid, local_operands.pop().unwrap());
                }
                // Add a destination operand to the operation to hold the result
                let idx = self.add_operand_to_ir(lid, IROperand::new(parent_nid, ast, OperandKind::TempVar));
                // Also add the detination operand to the local operands
                // The destination operand is presumably an input operand in the parent.
                returned_operands.push(idx);
            },
            ast::LexToken::Plus => {
                let mut local_operands = Vec::new();
                result &= self.record_children_r(rdepth + 1, parent_nid, &mut local_operands, diags, ast, ast_db);
                let lid = self.new_ir(parent_nid, IRKind::Add);
                assert!(local_operands.len() == 2);
                while !local_operands.is_empty() {
                    self.add_operand_idx_to_ir(lid, local_operands.pop().unwrap());
                }
                // Add a destination operand to the operation to hold the result
                let idx = self.add_operand_to_ir(lid, IROperand::new(parent_nid, ast, OperandKind::TempVar));
                // Also add the detination operand to the local operands
                // The destination operand is presumably an input operand in the parent.
                returned_operands.push(idx);
            },
            ast::LexToken::Asterisk => {
                let mut local_operands = Vec::new();
                result &= self.record_children_r(rdepth + 1, parent_nid, &mut local_operands, diags, ast, ast_db);
                let lid = self.new_ir(parent_nid, IRKind::Multiply);
                assert!(local_operands.len() == 2);
                while !local_operands.is_empty() {
                    self.add_operand_idx_to_ir(lid, local_operands.pop().unwrap());
                }
                // Add a destination operand to the operation to hold the result
                let idx = self.add_operand_to_ir(lid, IROperand::new(parent_nid, ast, OperandKind::TempVar));
                // Also add the detination operand to the local operands
                // The destination operand is presumably an input operand in the parent.
                returned_operands.push(idx);
            },
            ast::LexToken::Section => {
                // Record the linear start of this section.
                let mut local_operands = Vec::new();
                self.new_ir(parent_nid, IRKind::SectionStart);
                result &= self.record_children_r(rdepth + 1, parent_nid, &mut local_operands, diags, ast, ast_db);
                assert!(local_operands.is_empty());
                self.new_ir(parent_nid, IRKind::SectionEnd);
            },
            ast::LexToken::Identifier => {
                // identifiers are already processed
            },
            ast::LexToken::Semicolon |
            ast::LexToken::OpenBrace |
            ast::LexToken::CloseBrace => {
                // uninteresting syntactical elements
            }
            _ => {
                // We forgot to handle something
                let tinfo = ast.get_tinfo(parent_nid);
                error!("Unhandled lexical token {:?}", tinfo);
                assert!(false);
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
        let mut linear_db = LinearDb { output_nid, ir_vec: Vec::new(),
                                               operand_vec: Vec::new() };

        let sec_name_str = ast.get_child_str(output_nid, 0);
        debug!("LinearDb::new: output section name is {}", sec_name_str);

        // Using the name of the section, use the AST database to get a reference
        // to the section object.  ast_db processing has already guaranteed
        // that the section name is legitimate, so unwrap().
        let section = ast_db.sections.get(sec_name_str).unwrap();
        let sec_nid = section.nid;

        let begin_lid = linear_db.new_ir(output_nid,IRKind::Begin);
        // To start recursion, rdepth = 1.  The ONLY thing happening
        // here is a flattening of the AST into the logical order
        // of instructions.  We're not calculating sizes and addresses yet.
        let mut local_operands = Vec::new();
        if !linear_db.record_r(1, sec_nid, &mut local_operands, diags, ast, ast_db) {
            return None;
        }

        debug!("LinearDb::new: <<<< EXIT for nid: {}", output_nid);
        Some(linear_db)
    }

    pub fn dump(&self) {
        for (idx,ir) in self.ir_vec.iter().enumerate() {
            let mut op = format!("lid {}: nid {} is {:?}", idx, ir.nid, ir.op);
            // display the operand for this IR
            let mut first = true;
            for child in &ir.operand_vec {
                let operand = &self.operand_vec[*child];
                if !first {
                    op.push_str(",");
                } else {
                    first = false;
                }
                if operand.kind == OperandKind::Immediate {
                    op.push_str(&format!(" {}", operand.val));
                } else if operand.kind == OperandKind::TempVar {
                    op.push_str(&format!(" temp_{}", *child));
                } else {
                    assert!(false);
                }
                //op.push_str(&format!(" temp_{}", operand.val));
            }
            debug!("LinearDb: {}", op);
        }
    }
}
