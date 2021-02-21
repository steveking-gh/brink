use indextree::{NodeId};
pub type Span = std::ops::Range<usize>;
use diags::Diags;

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use ast::{Ast, AstDb, LexToken, TokenInfo};
use ir_base::{IRKind,OperandKind,DataType};
use std::{collections::{HashMap}, ops::Range};

pub struct LinOperand {
    /// linear ID of source operation if this operand is an output.
    pub src_lid: Option<usize>,
    pub src_loc: Range<usize>,
    pub val: String,
    pub kind: OperandKind,
    pub data_type: DataType,
}

fn lex_to_data_type(lxt: LexToken) -> DataType {
    match lxt {
        LexToken::U64 => DataType::Int,
        LexToken::QuotedString => DataType::QuotedString,
        LexToken::Identifier => DataType::Identifier,
        // In some cases, like the result of operations, we don't
        // know the type of the operand during linearization.
        _ => DataType::Unknown
    }
}

impl<'toks> LinOperand {
    pub fn new(src_lid: Option<usize>, nid: NodeId, ast: &'toks Ast, kind: OperandKind,
               data_type: DataType) -> LinOperand {
        let tinfo = ast.get_tinfo(nid);
        let src_loc = tinfo.loc.clone();
        LinOperand { src_lid, src_loc, val: tinfo.val.to_string(), kind, data_type }
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

    pub fn add_operand(&mut self, operand_num: usize) {
        self.operand_vec.push(operand_num);
    }
}

fn tok_to_irkind(tok: LexToken) -> IRKind {
    match tok {
        LexToken::Wrs => { IRKind::Wrs }
        LexToken::NEq => { IRKind::NEq }
        LexToken::DoubleEq => { IRKind::DoubleEq }
        LexToken::GEq => { IRKind::GEq }
        LexToken::LEq => { IRKind::LEq }
        LexToken::DoubleGreater => { IRKind::RightShift }
        LexToken::DoubleLess => { IRKind::LeftShift }
        LexToken::Plus => { IRKind::Add }
        LexToken::Minus => { IRKind::Subtract }
        LexToken::Asterisk => { IRKind::Multiply }
        LexToken::FSlash => { IRKind::Divide }
        LexToken::Ampersand => { IRKind::BitAnd }
        LexToken::DoubleAmpersand => { IRKind::LogicalAnd }
        LexToken::Pipe => { IRKind::BitOr }
        LexToken::DoublePipe => { IRKind::LogicalOr }
        LexToken::Sizeof => { IRKind::Sizeof }
        LexToken::Abs => { IRKind::Abs }
        LexToken::Img => { IRKind::Img }
        LexToken::Sec => { IRKind::Sec }
        bug => {
            assert!( false, "Failed to convert LexToken to IRKind for {:?}", bug);
            IRKind::Assert // keep compiler happy
        }
    }
}

pub struct LinearDb {
    pub ir_vec: Vec<LinIR>,
    pub operand_vec: Vec<LinOperand>,
    pub output_sec_str: String,
    pub output_sec_loc: Range<usize>,
    pub output_addr_str: Option<String>,
    pub output_addr_loc: Option<Range<usize>>,
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

    // Adds an existing operand by it's operand_vec index to the specified LinIR
    pub fn add_operand_idx_to_ir(&mut self, ir_lid: usize, idx: usize) {
        self.ir_vec[ir_lid].add_operand(idx);
    }

    // Returns the linear operand index occupied by the new operand
    pub fn add_operand_to_ir(&mut self, ir_lid: usize, operand: LinOperand) -> usize {
        let idx = self.operand_vec.len();
        self.operand_vec.push(operand);
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

    fn record_children_r(&mut self, result: &mut bool, rdepth: usize, parent_nid: NodeId,
                        lops: &mut Vec<usize>,
                        diags: &mut Diags, ast: &'toks Ast, ast_db: &AstDb) {
        // Easy linearizing without dereferencing through a name.
        // When no children exist, this case terminates recursion.
        let children = ast.children(parent_nid);
        for nid in children {
            self.record_r(result, rdepth, nid, lops, diags, ast, ast_db);
        }
    }

    fn operand_count_is_valid(&self, expected: usize, lops: &Vec<usize>, diags: &mut Diags, tinfo: &TokenInfo) -> bool {
        let found = lops.len();
        if found != expected {
            let m = format!("Expected {} operand(s), but found {} for '{}' expression",
                                expected, found, tinfo.val);
            diags.err1("LINEAR_5",&m, tinfo.span());
            return false;
        }
        true
    }

    // Process the expected number of operands.
    fn process_operands(&mut self, result: &mut bool, expected: usize,
                        lops: &mut Vec<usize>, ir_lid: usize,
                        diags: &mut Diags, tinfo: &TokenInfo) {

        // If we found the expected number of operands, then add them to the new IR
        // Otherwise, do nothing but indicate the error.
        if self.operand_count_is_valid(expected, lops, diags, tinfo) {
            // Preserve the order of the operands front to back.
            for idx in lops {
                self.add_operand_idx_to_ir(ir_lid, *idx);
            }
        } else {
            *result = false;
        }
    }

    // Process the expected number of *optional* operands.  Either the number
    // number of operands must be zero or the expected number.
    fn process_optional_operands(&mut self, result: &mut bool, expected: usize,
                                  lops: &mut Vec<usize>, ir_lid: usize,
                                  diags: &mut Diags, tinfo: &TokenInfo) {

        if lops.is_empty() {
            return;
        }

        self.process_operands(result, expected, lops, ir_lid, diags, tinfo);
    }

    /// Recursively record information about the children of an AST object.
    fn record_r(&mut self, result: &mut bool, rdepth: usize, parent_nid: NodeId,
                returned_operands: &mut Vec<usize>,
                diags: &mut Diags, ast: &'toks Ast, ast_db: &AstDb) {

        debug!("LinearDb::record_r: >>>> ENTER at depth {} for parent nid: {}",
                rdepth, parent_nid);

        if !self.depth_sanity(rdepth, parent_nid, diags, ast) {
            *result = false;
            return;
        }

        let tinfo = ast.get_tinfo(parent_nid);
        
        match tinfo.tok {
            LexToken::Wr => {
                // A vector to track the operands of this expression.
                let mut lops = Vec::new();
                // Write the contents of a section.  This isn't a simple recursion
                // into the children.  Instead, we redirect to the specified section.
                let sec_name_str = ast.get_child_str(parent_nid, 0).unwrap();
                debug!("LinearDb::record_r: recursing into section {}", sec_name_str);

                // Using the name of the section, use the AST database to get a reference
                // to the section object.  ast_db processing has already guaranteed
                // that the section name is legitimate, so unwrap().
                let section = ast_db.sections.get(sec_name_str).unwrap();
                let sec_nid = section.nid;

                // Recurse into the referenced section.
                self.record_r(result, rdepth + 1, sec_nid, 
                &mut lops, diags, ast, ast_db);
                // The 'wr' expression does not produce an IR of its own,
                // but inserts an entire section in-place.  So, we don't have a
                // linear ID for the 'wr' and expect no operands.
                *result &= self.operand_count_is_valid(0, &lops, diags, tinfo);
            }
            LexToken::Wrs => {
                // A vector to track the operands of this expression.
                let mut lops = Vec::new();
                // Write a fixed string. The string is the operand.
                let ir_lid = self.new_ir(parent_nid, ast, IRKind::Wrs);
                self.record_children_r(result, rdepth + 1, parent_nid, &mut lops, diags, ast, ast_db);
                // 1 operand expected
                self.process_operands(result, 1, &mut lops, ir_lid, diags, tinfo);
            }
            LexToken::Sizeof => {
                // A vector to track the operands of this expression.
                let mut lops = Vec::new();
                // Get the size of the section.  Section name is an identifier operand.
                let ir_lid = self.new_ir(parent_nid, ast, IRKind::Sizeof);
                // There is child, which is the identifier
                self.record_children_r(result, rdepth + 1, parent_nid, &mut lops, diags, ast, ast_db);
                // 1 operand expected
                self.process_operands(result, 1, &mut lops, ir_lid, diags, tinfo);

                // Add a destination operand to the operation to hold the result
                let idx = self.add_operand_to_ir(ir_lid, LinOperand::new(
                        Some(ir_lid), parent_nid, ast, OperandKind::Variable, DataType::Int));
                // Also add the destination operand to the local operands
                // The destination operand is presumably an input operand in the parent.
                returned_operands.push(idx);
            }
            LexToken::Abs |
            LexToken::Img |
            LexToken::Sec => {
                // A vector to track the operands of this expression.
                let mut lops = Vec::new();
                // Create the new IR
                let ir_lid = self.new_ir(parent_nid, ast, tok_to_irkind(tinfo.tok));
                // There is *optional* identifier child.
                // If the child exists, we will get the address of the associated identifier
                // otherwise, we get the current address
                self.record_children_r(result, rdepth + 1, parent_nid, &mut lops, diags, ast, ast_db);
                // 1 operand expected
                self.process_optional_operands(result, 1, &mut lops, ir_lid, diags, tinfo);

                // Add a destination operand to the operation to hold the result
                let idx = self.add_operand_to_ir(ir_lid, LinOperand::new(
                        Some(ir_lid), parent_nid, ast, OperandKind::Variable, DataType::Int));
                // Also add the destination operand to the local operands
                // The destination operand is presumably an input operand in the parent.
                returned_operands.push(idx);
            }
            LexToken::Identifier |
            LexToken::U64 |
            LexToken::QuotedString => {
                // These are immediate operands.  Add them to the main operand vector
                // and return them as local operands.
                // This case terminates recursion.
                let idx = self.operand_vec.len();
                self.operand_vec.push(LinOperand::new(None, parent_nid,ast,OperandKind::Constant,
                                        lex_to_data_type(tinfo.tok)));
                returned_operands.push(idx);
            }
            LexToken::Assert => {
                // A vector to track the operands of this expression.
                let mut lops = Vec::new();
                self.record_children_r(result, rdepth + 1, parent_nid, &mut lops, diags, ast, ast_db);
                let ir_lid = self.new_ir(parent_nid, ast, IRKind::Assert);
                // 1 operand expected
                self.process_operands(result, 1, &mut lops, ir_lid, diags, tinfo);
            }
            LexToken::NEq |
            LexToken::LEq |
            LexToken::GEq |
            LexToken::DoubleEq |
            LexToken::DoubleGreater |
            LexToken::DoubleLess |
            LexToken::Asterisk |
            LexToken::Ampersand |
            LexToken::DoubleAmpersand |
            LexToken::Pipe |
            LexToken::DoublePipe |
            LexToken::FSlash |
            LexToken::Minus |
            LexToken::Plus => {
                // A vector to track the operands of this expression.
                let mut lops = Vec::new();
                self.record_children_r(result, rdepth + 1, parent_nid, &mut lops, diags, ast, ast_db);
                let ir_lid = self.new_ir(parent_nid, ast, tok_to_irkind(tinfo.tok));
                // 2 operands expected
                self.process_operands(result, 2, &mut lops, ir_lid, diags, tinfo);

                // Add a destination operand to the operation to hold the result
                let idx = self.add_operand_to_ir(ir_lid, LinOperand::new(
                    Some(ir_lid), parent_nid, ast, OperandKind::Variable,DataType::Int));
                // Also add the destination operand to the local operands
                // The destination operand is presumably an input operand in the parent.
                returned_operands.push(idx);
            }
            LexToken::Section => {
                // Record the linear start of this section.
                let mut lops = Vec::new();
                let start_lid = self.new_ir(parent_nid, ast, IRKind::SectionStart);
                self.record_children_r(result, rdepth + 1, parent_nid, &mut lops, diags, ast, ast_db);
                let end_lid = self.new_ir(parent_nid, ast, IRKind::SectionEnd);
                // 1 operand expected, which is the name of the section.
                if self.operand_count_is_valid(1, &lops, diags, tinfo) {
                    let sec_id_lid = lops.pop().unwrap();
                    self.add_operand_idx_to_ir(start_lid, sec_id_lid);
                    self.add_operand_idx_to_ir(end_lid, sec_id_lid);
                } else {
                    *result = false;
                }
            }
            LexToken::Label => {
                // A label marking an addressable location in the output.
                // Labels have no children in the AST since they are their own identifier.
                // In the IR, the identifier becomes the only operand of the label operation.
                let ir_lid = self.new_ir(parent_nid, ast, IRKind::Label);

                // Trim the trailing colon on the label.
                let name_without_colon = tinfo.val[..tinfo.val.len() - 1].to_string();

                // Add an identifier name operand
                let operand = LinOperand { src_lid: Some(ir_lid), src_loc: tinfo.loc.clone(),
                                val: name_without_colon, kind: OperandKind::Constant, data_type: DataType::Identifier};
                self.add_operand_to_ir(ir_lid, operand);
            }

            LexToken::Semicolon |
            LexToken::OpenParen |
            LexToken::CloseParen |
            LexToken::OpenBrace |
            LexToken::CloseBrace => {
                // Uninteresting syntactical elements that do not appear in the IR.
            }
            LexToken::Unknown => {
                let m = "Unexpected character.";
                diags.err1("LINEAR_3", &m, tinfo.span());
                *result = false;
            }
            LexToken::Output => {
                let m = format!("Unexpected '{}' expression not allowed here.", tinfo.val);
                diags.err1("LINEAR_4", &m, tinfo.span());
                *result = false;
            }
        }

        debug!("LinearDb::record_r: <<<< EXIT({}) at depth {} for nid: {}",
                result, rdepth, parent_nid);
    }

    /// The LinearDb object must start with an output statement.
    /// If the output doesn't exist, then return None.  The linear_db
    /// records only elements with size > 0.
    pub fn new(diags: &mut Diags, ast: &'toks Ast,
               ast_db: &'toks AstDb) -> Option<LinearDb> {
        debug!("LinearDb::new: >>>> ENTER");

        // AstDb already validated output exists
        let output_nid = ast_db.output.nid;
        let output_sec_tinfo = ast.get_tinfo(ast_db.output.sec_nid);
        let output_sec_str = output_sec_tinfo.val.to_string();
        let output_sec_loc = output_sec_tinfo.loc.clone();
        debug!("LinearDb::new: Output section name is {}", output_sec_str);

        let output_addr_nid = ast_db.output.addr_nid;
        let mut output_addr_str = None;
        let mut output_addr_loc = None;

        if output_addr_nid.is_some() {
            let output_addr_tinfo = ast.get_tinfo(ast_db.output.addr_nid.unwrap());
            if output_addr_tinfo.tok == LexToken::U64 {
                output_addr_str = Some(output_addr_tinfo.val.to_string());
                output_addr_loc = Some(output_addr_tinfo.loc.clone());
                debug!("LinearDb::new: Output address is {}", output_addr_str.as_ref().unwrap());
            } else {
                // If not a u64, then trailing semicolon
                assert!(output_addr_tinfo.tok == LexToken::Semicolon);
            }
        }

        let mut linear_db = LinearDb { ir_vec: Vec::new(), operand_vec: Vec::new(),
                    output_sec_str, output_sec_loc, output_addr_str, output_addr_loc };

        // Using the name of the section, use the AST database to get a reference
        // to the section object.  ast_db processing has already guaranteed
        // that the section name is legitimate, so unwrap().
        let section = ast_db.sections.get(linear_db.output_sec_str.as_str()).unwrap();
        let sec_nid = section.nid;

        // To start recursion, rdepth = 1.  The ONLY thing happening
        // here is a flattening of the AST into the logical order
        // of instructions.  We're not calculating sizes and addresses yet.
        let mut lops = Vec::new();

        // If an error occurs, result gets stuck at false.
        let mut result = true;
        linear_db.record_r(&mut result, 1, sec_nid, &mut lops,
                            diags, ast, ast_db);
        if !result {
            return None;
        }

        if !LinearCheck::check(&linear_db, diags) {
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

struct LinearCheck {
    label_idents: HashMap<String,Range<usize>>,
    section_count: HashMap<String,usize>,
}

impl LinearCheck {
    pub fn new() -> LinearCheck {
        LinearCheck { label_idents: HashMap::new(),
                      section_count: HashMap::new()
        }
    }


    pub fn check(lindb: &LinearDb, diags: &mut Diags) -> bool {
        let mut lc = LinearCheck::new();
        if !lc.inventory_identifiers(lindb, diags)  { return false; }
        if !lc.verify_global_identifier_refs(lindb, diags) { return false; }
        true
    }

    /// Adds a label identifier that is an operand to the inventory.
    /// This inventory contains only declarations of identifiers, not references.
    fn inventory_label_ident(&mut self, op_num: usize, lir: &LinIR, lindb: &LinearDb,
                                     diags: &mut Diags) -> bool {
        let mut result = true;
        let name_operand_num = lir.operand_vec[op_num];
        let name_operand = lindb.operand_vec.get(name_operand_num).unwrap();
        assert!(name_operand.kind == OperandKind::Constant);
        assert!(name_operand.data_type == DataType::Identifier);
        let name = &name_operand.val;
        if self.label_idents.contains_key(name) {
            let orig_loc = self.label_idents.get(name).unwrap();
            let msg = format!("Duplicate label name {}", name);
            diags.err2("LINEAR_2", &msg, name_operand.src_loc.clone(), orig_loc.clone());
            // keep processing after error to report other problems
            result = false;
        } else {
            self.label_idents.insert(name.clone(), name_operand.src_loc.clone());
        }
        result
    }

    /// Increment the number of occurrences of this section
    fn inventory_section_ident(&mut self, lir: &LinIR, lindb: &LinearDb) {
        let name_operand_num = lir.operand_vec[0];
        let name_operand = lindb.operand_vec.get(name_operand_num).unwrap();
        assert!(name_operand.kind == OperandKind::Constant);
        assert!(name_operand.data_type == DataType::Identifier);
        let name = &name_operand.val;

        if let Some(count) = self.section_count.get_mut(name) {
            *count += 1;
        } else {
            self.section_count.insert(name.to_string(), 1);
        }
    }

    /// Build a hash of all valid identifier names: labels, sections, etc
    /// Reports an error and returns false if duplicate labels exist.
    fn inventory_identifiers(&mut self, lindb: &LinearDb, diags: &mut Diags ) -> bool {
        let mut result = true;
        for lir in &lindb.ir_vec {
            result &= match lir.op {
                IRKind::Label => self.inventory_label_ident(0, lir, lindb, diags),
                IRKind::SectionStart => {
                    self.inventory_section_ident(lir, lindb);
                    true
                }
                _ => { true }
                }
            }

        debug!("LinearCheck::inventory_identifiers:");
        for (name, _) in &self.label_idents {
            debug!("    {}", name);
        }

        result
    }

    /// Verifies that every identifier reference exists in the inventory
    /// Must not be called before inventory_identifiers
    fn verify_global_identifier_refs(&mut self, lindb: &LinearDb, diags: &mut Diags) -> bool {
        let mut result = true;
        for lir in &lindb.ir_vec {
            result &= match lir.op {
                IRKind::Abs |
                IRKind::Img |
                IRKind::Sizeof => {
                    self.verify_global_identifer_operand_refs(lir, lindb, diags)
                }
                _ => { true }
            }
        }

        result
    }

    /// Return true if the identifier refers to a section with only a
    /// single instance.  Returns false if this is an ambiguous section ref
    /// or not a section ref.
    fn is_valid_section_ref(&mut self, lop: &LinOperand, diags: &mut Diags) -> bool {
        if let Some(count) = self.section_count.get(&lop.val) {
            if *count == 1 {
                return true;
            }
            let msg = format!("Reference to section '{}' is ambiguous. This \
                                        section occurs {} times in the output", lop.val, *count);
            diags.err1("LINEAR_7", &msg, lop.src_loc.clone());
            // keep processing after error to report other problems
        }
        false
    }

    /// Return true if the identifier refers to label.
    /// Returns false otherwise.
    fn is_valid_label_ref(&mut self, lop: &LinOperand) -> bool {
        if self.label_idents.contains_key(&lop.val) {
            return true;
        }
        false
    }

    /// For the specified linear IR, verify any operands that are identifier
    /// references are valid as global identifiers
    fn verify_global_identifer_operand_refs(&mut self, lir: &LinIR, lindb: &LinearDb,
                                            diags: &mut Diags) -> bool {
        let mut result = true;
        for &lop_num in &lir.operand_vec {
            let lop= &lindb.operand_vec[lop_num];
            if lop.data_type == DataType::Identifier {
                debug!("LinearCheck::verify_identifier_refs: Verifying reference to '{}'", lop.val);
                if self.is_valid_section_ref(lop, diags) {
                    continue;
                }
                if self.is_valid_label_ref(lop) {
                    continue;
                }

                let msg = format!("Unknown or unreachable identifier {}", lop.val);
                diags.err1("LINEAR_6", &msg, lop.src_loc.clone());
                // keep processing after error to report other problems
                result = false;
            }
        }
        result
    }

}