use indextree::{NodeId};
pub type Span = std::ops::Range<usize>;
use diags::Diags;

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use ast::{Ast,AstDb,LexToken};
use ir_base::{IRKind,OperandKind,DataType};
use std::ops::Range;

pub struct LinOperand {
    pub nid: NodeId,
    pub src_loc: Range<usize>,
    pub val: String,
    pub kind: OperandKind,
    pub data_type: DataType,
}

fn lex_to_data_type(lxt: LexToken) -> DataType {
    match lxt {
        LexToken::Int => DataType::Int,
        LexToken::QuotedString => DataType::QuotedString,
        LexToken::Identifier => DataType::Identifier,
        // In some cases, like the result of operations, we don't
        // know the type of the operand during linearization.
        _ => DataType::Unknown
    }
}

impl<'toks> LinOperand {
    pub fn new(nid: NodeId, ast: &'toks Ast, kind: OperandKind,
               data_type: DataType) -> LinOperand {
        let tinfo = ast.get_tinfo(nid);
        let src_loc = tinfo.loc.clone();
        LinOperand { nid, src_loc, val: tinfo.val.to_string(), kind, data_type }
    }    
}

pub struct LinIR {
    pub nid: NodeId,
    pub src_loc: Range<usize>,
    pub op: IRKind,
    // usize is the index into the operand vec
    pub operand_vec: Vec<usize>,
}

impl<'toks> LinIR {
    pub fn new(nid: NodeId, ast: &'toks Ast, op: IRKind) -> Self {
        let tinfo = ast.get_tinfo(nid);
        let src_loc = tinfo.loc.clone();
        Self { nid, src_loc, op, operand_vec: Vec::new() }
    }

    pub fn add_operand(&mut self, oper_num: usize) {
        self.operand_vec.push(oper_num);
    }
}

pub struct LinearDb {
    pub output_nid: NodeId,
    pub ir_vec: Vec<LinIR>,
    pub operand_vec: Vec<LinOperand>, 
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

    // Adds an existing operand by it's operanc_vec index to the specified LinIR
    pub fn add_operand_idx_to_ir(&mut self, ir_lid: usize, idx: usize) {
        self.ir_vec[ir_lid].add_operand(idx);
    }

    // Returns the linear operand index occupied by the new operand
    pub fn add_operand_to_ir(&mut self, ir_lid: usize, oper: LinOperand) -> usize {
        let idx = self.operand_vec.len();
        self.operand_vec.push(oper);
        self.add_operand_idx_to_ir(ir_lid, idx);
        idx
    }

    // returns the linear ID for the new LinIR
    fn new_ir(&mut self, nid: NodeId, ast: &'toks Ast, op: IRKind) -> usize {
        let lid = self.ir_vec.len();
        self.ir_vec.push(LinIR::new(nid, ast, op));
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
                // into the children.  Instead, we redirect to the specified section.
                let sec_name_str = ast.get_child_str(parent_nid, 0);
                debug!("LinearDb::record_r: recursing into section {}", sec_name_str);

                // Using the name of the section, use the AST database to get a reference
                // to the section object.  ast_db processing has already guaranteed
                // that the section name is legitimate, so unwrap().
                let section = ast_db.sections.get(sec_name_str).unwrap();
                let sec_nid = section.nid;

                // Recurse into the referenced section.
                result &= self.record_r(rdepth + 1, sec_nid, 
                        &mut local_operands, diags, ast, ast_db);

                // we expect the section name as a local parameter, but have no need for it.
                assert!(local_operands.is_empty());
            },
            ast::LexToken::Wrs => {
                let mut local_operands = Vec::new();
                // Write a fixed string. The string is the operand
                let lid = self.new_ir(parent_nid, ast, IRKind::Wrs);
                result &= self.record_children_r(rdepth + 1, parent_nid, &mut local_operands, diags, ast, ast_db);
                assert!(local_operands.len() == 1);
                while !local_operands.is_empty() {
                    self.add_operand_idx_to_ir(lid, local_operands.pop().unwrap());
                }
            },
            ast::LexToken::Identifier |
            ast::LexToken::Int |
            ast::LexToken::QuotedString => {
                // These are immediate operands.
                // This case terminates recursion.
                let idx = self.operand_vec.len();
                self.operand_vec.push(LinOperand::new(parent_nid,ast,OperandKind::Constant,
                     lex_to_data_type(tinfo.tok)));
                returned_operands.push(idx);
            },
            ast::LexToken::Assert => {
                // Assert an expression is not zero (false)
                let mut local_operands = Vec::new();
                result &= self.record_children_r(rdepth + 1, parent_nid, &mut local_operands, diags, ast, ast_db);
                let lid = self.new_ir(parent_nid, ast, IRKind::Assert);
                assert!(local_operands.len() == 1);
                while !local_operands.is_empty() {
                    self.add_operand_idx_to_ir(lid, local_operands.pop().unwrap());
                }
            },
            ast::LexToken::EqEq => {
                let mut local_operands = Vec::new();
                result &= self.record_children_r(rdepth + 1, parent_nid, &mut local_operands, diags, ast, ast_db);
                let lid = self.new_ir(parent_nid, ast, IRKind::EqEq);
                assert!(local_operands.len() == 2);
                while !local_operands.is_empty() {
                    self.add_operand_idx_to_ir(lid, local_operands.pop().unwrap());
                }
                // Add a destination operand to the operation to hold the result
                let idx = self.add_operand_to_ir(lid, LinOperand::new(parent_nid, ast,
                                                  OperandKind::Variable,DataType::Bool));
                // Also add the detination operand to the local operands
                // The destination operand is presumably an input operand in the parent.
                returned_operands.push(idx);
            },
            ast::LexToken::Plus => {
                let mut local_operands = Vec::new();
                result &= self.record_children_r(rdepth + 1, parent_nid, &mut local_operands, diags, ast, ast_db);
                let lid = self.new_ir(parent_nid, ast, IRKind::Add);
                assert!(local_operands.len() == 2);
                while !local_operands.is_empty() {
                    self.add_operand_idx_to_ir(lid, local_operands.pop().unwrap());
                }
                // Add a destination operand to the operation to hold the result
                let idx = self.add_operand_to_ir(lid, LinOperand::new(parent_nid, ast,
                                                  OperandKind::Variable,DataType::Int));
                // Also add the detination operand to the local operands
                // The destination operand is presumably an input operand in the parent.
                returned_operands.push(idx);
            },
            ast::LexToken::Asterisk => {
                let mut local_operands = Vec::new();
                result &= self.record_children_r(rdepth + 1, parent_nid, &mut local_operands, diags, ast, ast_db);
                let lid = self.new_ir(parent_nid, ast, IRKind::Multiply);
                assert!(local_operands.len() == 2);
                while !local_operands.is_empty() {
                    self.add_operand_idx_to_ir(lid, local_operands.pop().unwrap());
                }
                // Add a destination operand to the operation to hold the result
                let idx = self.add_operand_to_ir(lid, LinOperand::new(parent_nid, ast,
                                                  OperandKind::Variable,DataType::Int));
                // Also add the detination operand to the local operands
                // The destination operand is presumably an input operand in the parent.
                returned_operands.push(idx);
            },
            ast::LexToken::Section => {
                // Record the linear start of this section.
                let mut local_operands = Vec::new();
                let start_lid = self.new_ir(parent_nid, ast, IRKind::SectionStart);
                result &= self.record_children_r(rdepth + 1, parent_nid, &mut local_operands, diags, ast, ast_db);
                let end_lid = self.new_ir(parent_nid, ast, IRKind::SectionEnd);
                assert!(local_operands.len() == 1);
                let sec_id_lid = local_operands.pop().unwrap();
                self.add_operand_idx_to_ir(start_lid, sec_id_lid);
                self.add_operand_idx_to_ir(end_lid, sec_id_lid);
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
            // display the operand for this LinIR
            let mut first = true;
            for child in &ir.operand_vec {
                let operand = &self.operand_vec[*child];
                if !first {
                    op.push_str(",");
                } else {
                    first = false;
                }
                if operand.kind == OperandKind::Constant {
                    op.push_str(&format!(" {}", operand.val));
                } else if operand.kind == OperandKind::Variable {
                    op.push_str(&format!(" tmp{}", *child));
                } else {
                    assert!(false);
                }
                //op.push_str(&format!(" temp_{}", operand.val));
            }
            debug!("LinearDb: {}", op);
        }
    }
}
