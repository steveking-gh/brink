// AST to linear IR lowering for brink.
//
// LinearDb is the second stage of the compiler pipeline.  It walks the AST
// produced by the AST stage and flattens the tree into two parallel vectors:
// a sequence of LinIR instruction records and a sequence of LinOperand operand
// records.  During this pass, section nesting and expression structure are
// resolved into a linear order, operand indices are assigned, and basic
// structural constraints (operand counts, recursion depth) are checked.
// Values are still stored as raw strings at this point; type conversion
// and expression evaluation happens in the next stage.
//
// Order of operations: lineardb runs after ast.  Its output — a LinearDb
// containing ir_vec, const_ir_vec, and operand_vec — is consumed by irdb.

use indextree::NodeId;
pub type Span = std::ops::Range<usize>;
use diags::Diags;

#[allow(unused_imports)]
use tracing::{debug, error, info, trace, warn};

use ast::{Ast, AstDb, LexToken, TokenInfo, is_reserved_identifier};
use ir::IRKind;
use std::{collections::HashMap, ops::Range};

/// The operand type for linear IRs.  This operand type is very similar to the
/// IROperand type, with the critical distinction that LinOperand creation
/// cannot fail.  This is a valuable simplification during the AST to Linear
/// conversion process.
pub struct LinOperand {
    /// linear ID of source operation if this operand is an output.
    pub ir_lid: Option<usize>,
    pub src_loc: Range<usize>,
    pub tok: LexToken,
    pub sval: String,
}

impl LinOperand {
    /// Create a new linear operand.  If the ir_lid exists, then this
    /// operand is the output of the specified lid.
    // pseudo functions like align.
    pub fn new(ir_lid: Option<usize>, tinfo: &TokenInfo) -> LinOperand {
        let src_loc = tinfo.loc.clone();
        LinOperand {
            ir_lid,
            src_loc,
            sval: tinfo.val.to_string(),
            tok: tinfo.tok,
        }
    }

    pub fn is_output_of(&self) -> Option<usize> {
        self.ir_lid
    }
}

/// The type for linear IRs.  This type is similar to the IR type, with the
/// critical distinction that LinIR creation cannot fail.  This is a valuable
/// simplification during the AST to Linear conversion process.
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
        Self {
            nid,
            src_loc,
            op,
            operand_vec: Vec::new(),
        }
    }

    pub fn add_operand(&mut self, operand_num: usize) {
        self.operand_vec.push(operand_num);
    }
}

fn tok_to_irkind(tok: LexToken) -> IRKind {
    match tok {
        LexToken::Abs => IRKind::Abs,
        LexToken::Align => IRKind::Align,
        LexToken::Ampersand => IRKind::BitAnd,
        LexToken::Assert => IRKind::Assert,
        LexToken::Asterisk => IRKind::Multiply,
        LexToken::Const => IRKind::Const,
        LexToken::DoubleAmpersand => IRKind::LogicalAnd,
        LexToken::DoubleEq => IRKind::DoubleEq,
        LexToken::DoubleGreater => IRKind::RightShift,
        LexToken::DoubleLess => IRKind::LeftShift,
        LexToken::DoublePipe => IRKind::LogicalOr,
        LexToken::Eq => IRKind::Eq,
        LexToken::FSlash => IRKind::Divide,
        LexToken::GEq => IRKind::GEq,
        LexToken::Img => IRKind::Img,
        LexToken::LEq => IRKind::LEq,
        LexToken::Minus => IRKind::Subtract,
        LexToken::NEq => IRKind::NEq,
        LexToken::Percent => IRKind::Modulo,
        LexToken::Pipe => IRKind::BitOr,
        LexToken::Plus => IRKind::Add,
        LexToken::Print => IRKind::Print,
        LexToken::Sec => IRKind::Sec,
        LexToken::SetAbs => IRKind::SetAbs,
        LexToken::SetImg => IRKind::SetImg,
        LexToken::SetSec => IRKind::SetSec,
        LexToken::Sizeof => IRKind::Sizeof,
        LexToken::ToI64 => IRKind::ToI64,
        LexToken::ToU64 => IRKind::ToU64,
        LexToken::Wr16 => IRKind::Wr(2),
        LexToken::Wr24 => IRKind::Wr(3),
        LexToken::Wr32 => IRKind::Wr(4),
        LexToken::Wr40 => IRKind::Wr(5),
        LexToken::Wr48 => IRKind::Wr(6),
        LexToken::Wr56 => IRKind::Wr(7),
        LexToken::Wr64 => IRKind::Wr(8),
        LexToken::Wr8 => IRKind::Wr(1),
        LexToken::Wrf => IRKind::Wrf,
        LexToken::Wrs => IRKind::Wrs,
        bug => {
            panic!("Failed to convert LexToken to IRKind for {:?}", bug);
        }
    }
}

pub struct LinearDb {
    /// Flat, ordered sequence of all IR instructions produced from the AST and
    /// evaluated by the engine.  This vector contains all IR instruction except
    /// those for const expressions.
    pub ir_vec: Vec<LinIR>,

    /// Flat sequence of all IR instructions for const declaration expressions.
    /// Being constants, we evaluate these expressions before engine execution.
    /// This separate vector avoids contaminating the mutable IR vector.
    pub const_ir_vec: Vec<LinIR>,

    /// Vector of all operands referenced by instructions in `ir_vec`.
    /// Instructions index into this vector rather than owning their operands directly.
    pub operand_vec: Vec<LinOperand>,

    /// Flat sequence of all operands referenced by instructions in `const_ir_vec`.
    /// Kept separate from `operand_vec` so downstream stages can iterate section
    /// operands without range-bounding.
    pub const_operand_vec: Vec<LinOperand>,

    /// Maps each const identifier name to the const_ir_vec index of its Const IR.
    pub const_map: HashMap<String, usize>,

    /// Name of the section specified by the `output` statement.
    pub output_sec_str: String,

    /// Source location of the section name token in the `output` statement.
    pub output_sec_loc: Range<usize>,

    /// Optional absolute base address supplied in the `output` statement.
    pub output_addr_str: Option<String>,

    /// Source location of the address token in the `output` statement, if present.
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
    pub fn add_existing_operand_to_ir(&mut self, ir_lid: usize, idx: usize, in_const: bool) {
        if in_const {
            self.const_ir_vec[ir_lid].add_operand(idx);
        } else {
            self.ir_vec[ir_lid].add_operand(idx);
        }
    }

    // Returns the linear operand index occupied by the new operand
    pub fn add_new_operand_to_ir(
        &mut self,
        ir_lid: usize,
        operand: LinOperand,
        in_const: bool,
    ) -> usize {
        if in_const {
            let idx = self.const_operand_vec.len();
            self.const_operand_vec.push(operand);
            self.add_existing_operand_to_ir(ir_lid, idx, in_const);
            idx
        } else {
            let idx = self.operand_vec.len();
            self.operand_vec.push(operand);
            self.add_existing_operand_to_ir(ir_lid, idx, in_const);
            idx
        }
    }

    // returns the linear ID for the new LinIR
    fn new_ir(&mut self, nid: NodeId, ast: &'toks Ast, op: IRKind, in_const_expr: bool) -> usize {
        let lir = LinIR::new(nid, ast, op);
        if in_const_expr {
            let lid = self.const_ir_vec.len();
            self.const_ir_vec.push(lir);
            lid
        } else {
            let lid = self.ir_vec.len();
            self.ir_vec.push(lir);
            lid
        }
    }

    // Control recursion to some safe level.  100 is just a guesstimate.
    const MAX_RECURSION_DEPTH: usize = 100;

    fn depth_sanity(
        &self,
        rdepth: usize,
        parent_nid: NodeId,
        diags: &mut Diags,
        ast: &Ast,
    ) -> bool {
        if rdepth > LinearDb::MAX_RECURSION_DEPTH {
            let tinfo = ast.get_tinfo(parent_nid);
            let m = format!(
                "Maximum recursion depth ({}) exceeded when processing '{}'.",
                LinearDb::MAX_RECURSION_DEPTH,
                tinfo.val
            );
            diags.err1("LINEAR_1", &m, tinfo.span());
            return false;
        }
        true
    }

    fn record_children_r(
        &mut self,
        rdepth: usize,
        parent_nid: NodeId,
        lops: &mut Vec<usize>,
        diags: &mut Diags,
        ast: &'toks Ast,
        ast_db: &AstDb,
        in_const_expr: bool,
    ) -> bool {
        // Easy linearizing without dereferencing through a name.
        // When no children exist, this case terminates recursion.
        let children = ast.children(parent_nid);
        let mut result = true;
        for nid in children {
            result &= self.record_r(rdepth, nid, lops, diags, ast, ast_db, in_const_expr);
        }
        result
    }

    fn operand_count_is_valid(
        &self,
        expected: usize,
        lops: &[usize],
        diags: &mut Diags,
        tinfo: &TokenInfo,
    ) -> bool {
        let found = lops.len();
        if found != expected {
            let m = format!(
                "Expected {} operand(s), but found {} for '{}' expression",
                expected, found, tinfo.val
            );
            diags.err1("LINEAR_5", &m, tinfo.span());
            return false;
        }
        true
    }

    // Process the expected number of operands.
    fn process_operands(
        &mut self,
        expected: usize,
        lops: &mut Vec<usize>,
        ir_lid: usize,
        diags: &mut Diags,
        tinfo: &TokenInfo,
        in_const: bool,
    ) -> bool {
        // If we found the expected number of operands, then add them to the new IR
        // Otherwise, do nothing but indicate the error.
        if self.operand_count_is_valid(expected, lops, diags, tinfo) {
            // Preserve the order of the operands front to back.
            for idx in lops {
                self.add_existing_operand_to_ir(ir_lid, *idx, in_const);
            }
        } else {
            return false;
        }
        true
    }

    // Process the expected number of *optional* operands.  Either the number
    // number of operands must be zero or the expected number.
    fn process_optional_operands(
        &mut self,
        expected: usize,
        lops: &mut Vec<usize>,
        ir_lid: usize,
        diags: &mut Diags,
        tinfo: &TokenInfo,
        in_const: bool,
    ) -> bool {
        if lops.is_empty() {
            return true;
        }

        self.process_operands(expected, lops, ir_lid, diags, tinfo, in_const)
    }

    /// Recursively record information about the children of an AST object.
    /// This function flattens the AST into linear form.
    /// We defer most type and operand checking to reduce complexity during
    /// this stage.
    ///
    /// Sets result true on success, false on failure.
    fn record_r(
        &mut self,
        rdepth: usize,
        parent_nid: NodeId,
        returned_operands: &mut Vec<usize>,
        diags: &mut Diags,
        ast: &'toks Ast,
        ast_db: &AstDb,
        in_const_expr: bool,
    ) -> bool {
        debug!(
            "LinearDb::record_r: ENTER at depth {} for parent nid: {}",
            rdepth, parent_nid
        );

        if !self.depth_sanity(rdepth, parent_nid, diags, ast) {
            return false;
        }

        let tinfo = ast.get_tinfo(parent_nid);
        let tok = tinfo.tok;
        let mut result = true;
        match tok {
            LexToken::Const => {
                // A const expression.
                // Add const to the main operand vector and return it as a local operand.
                let idx = self.operand_vec.len();
                self.operand_vec.push(LinOperand::new(None, tinfo));
                returned_operands.push(idx);
            }
            LexToken::Wr => {
                // A vector to track the operands of this expression.
                let mut lops = Vec::new();
                // Write the contents of a section.  This isn't a simple recursion
                // into the children.  Instead, we redirect to the specified section.
                let sec_name_str = ast.get_child_str(parent_nid, 0).unwrap();
                debug!(
                    "LinearDb::record_r: recursing into section {}",
                    sec_name_str
                );

                // Using the name of the section, use the AST database to get a reference
                // to the section object.  ast_db processing has already guaranteed
                // that the section name is legitimate, so unwrap().
                let section = ast_db.sections.get(sec_name_str).unwrap();
                let sec_nid = section.nid;

                // Recurse into the referenced section.  Section content is never
                // part of a const expression, so always pass false here.
                result &= self.record_r(rdepth + 1, sec_nid, &mut lops, diags, ast, ast_db, false);
                // The 'wr' expression does not produce an IR of its own,
                // but inserts an entire section in-place.  So, we don't have a
                // linear ID for the 'wr' and expect no operands.
                result &= self.operand_count_is_valid(0, &lops, diags, tinfo);
            }
            LexToken::Sizeof => {
                // A vector to track the operands of this expression.
                let mut lops = Vec::new();
                // Get the size of the section.  Section name is an identifier operand.
                let ir_lid = self.new_ir(parent_nid, ast, IRKind::Sizeof, in_const_expr);
                // There is child, which is the identifier
                result &= self.record_children_r(
                    rdepth + 1,
                    parent_nid,
                    &mut lops,
                    diags,
                    ast,
                    ast_db,
                    in_const_expr,
                );
                // 1 operand expected
                result &= self.process_operands(1, &mut lops, ir_lid, diags, tinfo, in_const_expr);

                // Add a destination operand to the operation to hold the result
                let idx = self.add_new_operand_to_ir(
                    ir_lid,
                    LinOperand::new(Some(ir_lid), tinfo),
                    in_const_expr,
                );
                // Also add the destination operand to the local operands
                // The destination operand is presumably an input operand in the parent.
                returned_operands.push(idx);
            }
            LexToken::Abs | LexToken::Img | LexToken::Sec => {
                // A vector to track the operands of this expression.
                let mut lops = Vec::new();
                // Create the new IR
                let ir_lid = self.new_ir(parent_nid, ast, tok_to_irkind(tinfo.tok), in_const_expr);
                // There is *optional* identifier child.
                // If the child exists, we will get the address of the associated identifier
                // otherwise, we get the current address
                result &= self.record_children_r(
                    rdepth + 1,
                    parent_nid,
                    &mut lops,
                    diags,
                    ast,
                    ast_db,
                    in_const_expr,
                );
                // 1 operand expected
                result &= self.process_optional_operands(
                    1,
                    &mut lops,
                    ir_lid,
                    diags,
                    tinfo,
                    in_const_expr,
                );

                // Add a destination operand to the operation to hold the result
                let idx = self.add_new_operand_to_ir(
                    ir_lid,
                    LinOperand::new(Some(ir_lid), tinfo),
                    in_const_expr,
                );
                // Also add the destination operand to the local operands
                // The destination operand is presumably an input operand in the parent.
                returned_operands.push(idx);
            }
            LexToken::Identifier
            | LexToken::U64
            | LexToken::I64
            | LexToken::Integer
            | LexToken::QuotedString => {
                // These are immediate operands.  Add them to the appropriate operand
                // vector and return them as local operands.
                // This case terminates recursion.
                let idx = if in_const_expr {
                    let idx = self.const_operand_vec.len();
                    self.const_operand_vec.push(LinOperand::new(None, tinfo));
                    idx
                } else {
                    let idx = self.operand_vec.len();
                    self.operand_vec.push(LinOperand::new(None, tinfo));
                    idx
                };
                returned_operands.push(idx);
            }
            LexToken::SetSec | LexToken::SetImg | LexToken::SetAbs | LexToken::Align => {
                // To implement align or pad, we map to IR as follows:
                // align val, fill_val; ==> align val, count; wr8 fill_val, count;
                // pad   val, fill_val; ==> pad   val, count; wr8 fill_val, count;
                // A vector to track the operands of this expression.
                let mut lops = Vec::new();
                let ir_lid = self.new_ir(parent_nid, ast, tok_to_irkind(tinfo.tok), in_const_expr);
                result &= self.record_children_r(
                    rdepth + 1,
                    parent_nid,
                    &mut lops,
                    diags,
                    ast,
                    ast_db,
                    in_const_expr,
                );

                // We expect 1 or 2 operands
                // align value [, optional pad byte value];
                // pad   value [, optional pad byte value];
                if lops.len() != 1 && lops.len() != 2 {
                    let tinfo = ast.get_tinfo(parent_nid);
                    let m = format!(
                        "{:?} requires 2 operands, but found {}",
                        tinfo.tok,
                        lops.len()
                    );
                    diags.err1("LINEAR_8", &m, tinfo.span());
                    return false;
                }

                // Add the user specified value to the IR
                self.add_existing_operand_to_ir(ir_lid, lops[0], in_const_expr);

                // Add the destination operand to store the calculated count
                let count_output = self.add_new_operand_to_ir(
                    ir_lid,
                    LinOperand::new(Some(ir_lid), tinfo),
                    in_const_expr,
                );

                // Create a wr8_tinfo copied from the align tinfo
                let mut wr8_tinfo = tinfo.clone();
                wr8_tinfo.tok = LexToken::Wr8;
                let wr8_lid =
                    self.new_ir(parent_nid, ast, tok_to_irkind(wr8_tinfo.tok), in_const_expr);

                if lops.len() == 2 {
                    // The user specified a pad byte value.  This expression is the first operand
                    // of the wr8
                    self.add_existing_operand_to_ir(wr8_lid, lops[1], in_const_expr);
                } else {
                    // Add a default integer 0 operand
                    let mut pad_byte_tinfo = tinfo.clone();
                    pad_byte_tinfo.tok = LexToken::Integer;
                    pad_byte_tinfo.val = "0";
                    self.add_new_operand_to_ir(
                        wr8_lid,
                        LinOperand::new(None, &pad_byte_tinfo),
                        in_const_expr,
                    );
                }

                // The align result as the number of bytes to write in wr8
                self.add_existing_operand_to_ir(wr8_lid, count_output, in_const_expr);
            }

            LexToken::Assert
            | LexToken::Wr8
            | LexToken::Wr16
            | LexToken::Wr24
            | LexToken::Wr32
            | LexToken::Wr40
            | LexToken::Wr48
            | LexToken::Wr56
            | LexToken::Wr64
            | LexToken::Wrs
            | LexToken::Wrf
            | LexToken::Print => {
                // A vector to track the operands of this expression.
                let mut lops = Vec::new();
                result &= self.record_children_r(
                    rdepth + 1,
                    parent_nid,
                    &mut lops,
                    diags,
                    ast,
                    ast_db,
                    in_const_expr,
                );
                let ir_lid = self.new_ir(parent_nid, ast, tok_to_irkind(tinfo.tok), in_const_expr);

                // add the operands to this new IR.  These IRs are statements that do not
                // return a value.
                for idx in lops {
                    self.add_existing_operand_to_ir(ir_lid, idx, in_const_expr);
                }
            }
            LexToken::ToI64 | LexToken::ToU64 => {
                // A vector to track the operands of this expression.
                let mut lops = Vec::new();
                result &= self.record_children_r(
                    rdepth + 1,
                    parent_nid,
                    &mut lops,
                    diags,
                    ast,
                    ast_db,
                    in_const_expr,
                );
                let ir_lid = self.new_ir(parent_nid, ast, tok_to_irkind(tinfo.tok), in_const_expr);
                // 1 operand expected
                result &= self.process_operands(1, &mut lops, ir_lid, diags, tinfo, in_const_expr);
                // Add a destination operand to the operation to hold the result
                let idx = self.add_new_operand_to_ir(
                    ir_lid,
                    LinOperand::new(Some(ir_lid), tinfo),
                    in_const_expr,
                );
                // Also add the destination operand to the local operands
                // The destination operand is presumably an input operand in the parent.
                returned_operands.push(idx);
            }
            LexToken::Eq
            | LexToken::NEq
            | LexToken::LEq
            | LexToken::GEq
            | LexToken::DoubleEq
            | LexToken::DoubleGreater
            | LexToken::DoubleLess
            | LexToken::Asterisk
            | LexToken::Ampersand
            | LexToken::DoubleAmpersand
            | LexToken::Pipe
            | LexToken::DoublePipe
            | LexToken::FSlash
            | LexToken::Percent
            | LexToken::Minus
            | LexToken::Plus => {
                // A vector to track the operands of this expression.
                let mut lops = Vec::new();
                result &= self.record_children_r(
                    rdepth + 1,
                    parent_nid,
                    &mut lops,
                    diags,
                    ast,
                    ast_db,
                    in_const_expr,
                );
                let ir_lid = self.new_ir(parent_nid, ast, tok_to_irkind(tinfo.tok), in_const_expr);
                // 2 operands expected
                result &= self.process_operands(2, &mut lops, ir_lid, diags, tinfo, in_const_expr);

                // Add a destination operand to the operation to hold the result
                let idx = self.add_new_operand_to_ir(
                    ir_lid,
                    LinOperand::new(Some(ir_lid), tinfo),
                    in_const_expr,
                );
                // Also add the destination operand to the local operands
                // The destination operand is presumably an input operand in the parent.
                returned_operands.push(idx);
            }
            LexToken::Section => {
                // Record the linear start of this section.
                let mut lops = Vec::new();
                let start_lid = self.new_ir(parent_nid, ast, IRKind::SectionStart, in_const_expr);
                result &= self.record_children_r(
                    rdepth + 1,
                    parent_nid,
                    &mut lops,
                    diags,
                    ast,
                    ast_db,
                    in_const_expr,
                );
                let end_lid = self.new_ir(parent_nid, ast, IRKind::SectionEnd, in_const_expr);
                // 1 operand expected, which is the name of the section.
                if self.operand_count_is_valid(1, &lops, diags, tinfo) {
                    let sec_id_lid = lops.pop().unwrap();
                    self.add_existing_operand_to_ir(start_lid, sec_id_lid, in_const_expr);
                    self.add_existing_operand_to_ir(end_lid, sec_id_lid, in_const_expr);
                } else {
                    result = false;
                }
            }
            LexToken::Label => {
                // A label marking an addressable location in the output.
                // Labels have no children in the AST since they are their own identifier.
                // In the IR, the identifier becomes the only operand of the label operation.
                let ir_lid = self.new_ir(parent_nid, ast, IRKind::Label, in_const_expr);

                // Trim the trailing colon on the label.
                let name_without_colon = tinfo.val[..tinfo.val.len() - 1].to_string();

                // Add an identifier name operand
                let operand = LinOperand {
                    ir_lid: Some(ir_lid),
                    src_loc: tinfo.loc.clone(),
                    sval: name_without_colon,
                    tok,
                };
                self.add_new_operand_to_ir(ir_lid, operand, in_const_expr);
            }

            LexToken::Semicolon
            | LexToken::Comma
            | LexToken::OpenParen
            | LexToken::CloseParen
            | LexToken::OpenBrace
            | LexToken::CloseBrace => {
                // Uninteresting syntactical elements that do not appear in the IR.
            }
            LexToken::Unknown => {
                let m = "Unexpected character.";
                diags.err1("LINEAR_3", m, tinfo.span());
                result = false;
            }
            LexToken::Output => {
                let m = format!("Unexpected '{}' expression not allowed here.", tinfo.val);
                diags.err1("LINEAR_4", &m, tinfo.span());
                result = false;
            }
        }

        debug!(
            "LinearDb::record_r: EXIT({}) at depth {} for nid: {}",
            result, rdepth, parent_nid
        );
        result
    }

    /// Converts a top-level `const NAME = <expr>` declaration into linear IR.
    ///
    /// Produces one `IRKind::Const` entry in `const_ir_vec` with three operands:
    ///  `[name_identifier, rhs_result, eq_output]`
    /// and records the name → const_ir_vec index mapping in `const_map`.
    /// All IRs and operands created during this call go into const_ir_vec and
    /// the const portion of operand_vec.
    fn record_const_decl(
        &mut self,
        const_nid: NodeId,
        diags: &mut Diags,
        ast: &'toks Ast,
        ast_db: &AstDb,
    ) -> bool {
        let ir_lid = self.new_ir(const_nid, ast, IRKind::Const, true);

        let mut children = ast.children(const_nid);

        // Child 0: const name (Identifier)
        let name_nid = children.next().unwrap();
        let name_tinfo = ast.get_tinfo(name_nid);
        let name_idx = self.const_operand_vec.len();
        self.const_operand_vec
            .push(LinOperand::new(None, name_tinfo));
        self.add_existing_operand_to_ir(ir_lid, name_idx, true);

        // Child 1: `=` sign — used only for its token info (src loc + Eq tok)
        let eq_nid = children.next().unwrap();
        let eq_tinfo = ast.get_tinfo(eq_nid);

        // Child 2: RHS expression.  All IRs created during this recursion go
        // into const_ir_vec (IRDb handles their validation).
        let rhs_nid = children.next().unwrap();
        let mut rhs_lops = Vec::new();
        if !self.record_r(1, rhs_nid, &mut rhs_lops, diags, ast, ast_db, true) {
            return false;
        }

        if rhs_lops.len() != 1 {
            let m = format!(
                "Const expression RHS produced {} results, expected 1",
                rhs_lops.len()
            );
            diags.err1("LINEAR_12", &m, name_tinfo.span());
            return false;
        }

        // Second operand: the RHS result
        self.add_existing_operand_to_ir(ir_lid, rhs_lops[0], true);

        // Third operand: the output slot of the const, using Eq tok for type inference
        self.add_new_operand_to_ir(ir_lid, LinOperand::new(Some(ir_lid), eq_tinfo), true);

        // Record name to the const_ir_vec index so the IRDb resolver can find it
        self.const_map.insert(name_tinfo.val.to_string(), ir_lid);

        true
    }

    /// The LinearDb object must start with an output statement.
    /// If the output doesn't exist, then return None.  The linear_db
    /// records only elements with size > 0.
    pub fn new(diags: &mut Diags, ast: &'toks Ast, ast_db: &'toks AstDb) -> Result<LinearDb, ()> {
        debug!("LinearDb::new: ENTER");

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
            if [LexToken::U64, LexToken::Integer, LexToken::Identifier]
                .contains(&output_addr_tinfo.tok)
            {
                output_addr_str = Some(output_addr_tinfo.val.to_string());
                output_addr_loc = Some(output_addr_tinfo.loc.clone());
                debug!(
                    "LinearDb::new: Output address is {}",
                    output_addr_str.as_ref().unwrap()
                );
            } else {
                // If not a numeric or identifier, then trailing semicolon
                assert!(output_addr_tinfo.tok == LexToken::Semicolon);
            }
        }

        let mut linear_db = LinearDb {
            ir_vec: Vec::new(),
            const_ir_vec: Vec::new(),
            operand_vec: Vec::new(),
            const_operand_vec: Vec::new(),
            const_map: HashMap::new(),
            output_sec_str,
            output_sec_loc,
            output_addr_str,
            output_addr_loc,
        };

        // Using the name of the output section, use the AST database to get a reference
        // to the section object.  ast_db processing has already guaranteed
        // that the section name is legitimate, so unwrap().
        let section = ast_db
            .sections
            .get(linear_db.output_sec_str.as_str())
            .unwrap();
        let sec_nid = section.nid;

        // To start recursion, set rdepth = 1.  The ONLY goal here
        // is a flattening of the AST into the logical order
        // of instructions.  We're not calculating sizes and addresses yet and
        // we defer a lot of error cases for later.
        let mut lops = Vec::new();

        // If an error occurs, result gets stuck at false.
        if !linear_db.record_r(1, sec_nid, &mut lops, diags, ast, ast_db, false) {
            return Err(());
        }

        // Linearize all top-level const declarations. All IRs and operands
        // created during this loop go into const_ir_vec and const_operand_vec.
        for (_, const_item) in &ast_db.consts {
            if !linear_db.record_const_decl(const_item.nid, diags, ast, ast_db) {
                return Err(());
            }
        }

        // debug
        linear_db.dump();

        if !IdentDb::check_globals(&linear_db, diags) {
            return Err(());
        }

        if !IdentDb::check_locals(&linear_db, diags) {
            return Err(());
        }

        debug!("LinearDb::new: EXIT for nid: {}", output_nid);
        Ok(linear_db)
    }

    pub fn dump(&self) {
        for (idx, ir) in self.ir_vec.iter().enumerate() {
            let mut op = format!("lid {}: nid {} is {:?}", idx, ir.nid, ir.op);
            // display the operand for this LinIR
            let mut first = true;
            for child in &ir.operand_vec {
                let operand = &self.operand_vec[*child];
                if !first {
                    op.push(',');
                } else {
                    first = false;
                }
                if let Some(ir_lid) = operand.is_output_of() {
                    op.push_str(&format!(" tmp{}, output of lid {}", *child, ir_lid));
                } else {
                    op.push_str(&format!(" {}", operand.sval));
                }
            }
            debug!("LinearDb: {}", op);
        }
        for (idx, ir) in self.const_ir_vec.iter().enumerate() {
            let mut op = format!("const lid {}: nid {} is {:?}", idx, ir.nid, ir.op);
            let mut first = true;
            for child in &ir.operand_vec {
                let operand = &self.const_operand_vec[*child];
                if !first {
                    op.push(',');
                } else {
                    first = false;
                }
                if let Some(ir_lid) = operand.is_output_of() {
                    op.push_str(&format!(" tmp{}, output of lid {}", *child, ir_lid));
                } else {
                    op.push_str(&format!(" {}", operand.sval));
                }
            }
            debug!("LinearDb: {}", op);
        }
    }
}

struct IdentDb {
    label_idents: HashMap<String, Range<usize>>,
    section_count: HashMap<String, usize>,
}

impl IdentDb {
    pub fn new() -> IdentDb {
        IdentDb {
            label_idents: HashMap::new(),
            section_count: HashMap::new(),
        }
    }

    /// Verify all global identifier references
    pub fn check_globals(lindb: &LinearDb, diags: &mut Diags) -> bool {
        let mut idb = IdentDb::new();
        if !idb.inventory_global_identifiers(lindb, diags) {
            return false;
        }
        if !idb.verify_global_refs(lindb, diags) {
            return false;
        }
        true
    }

    /// Recursively verify all local (within a section) reference
    pub fn check_locals(lindb: &LinearDb, diags: &mut Diags) -> bool {
        debug!("IdentDb::check_locals: ENTER");

        let mut result = true;
        let mut lid = 0;
        let len = lindb.ir_vec.len();

        // Search for the outermost section_start
        while lid < len && lindb.ir_vec[lid].op != IRKind::SectionStart {
            lid += 1;
        }

        // We found a section start.  Recurse
        lid += 1;
        result &= IdentDb::check_locals_r(&mut lid, lindb, diags);

        debug!("IdentDb::check_locals: EXIT({})", result);
        result
    }

    fn check_locals_r(lid: &mut usize, lindb: &LinearDb, diags: &mut Diags) -> bool {
        debug!("IdentDb::check_locals_r: ENTER at lid {}", *lid);
        let mut result = true;
        let mut idb = IdentDb::new();
        // remember the starting lid of this section
        let start_lid = *lid;
        loop {
            let lir = &lindb.ir_vec[*lid];
            *lid += 1;
            match lir.op {
                IRKind::SectionStart => {
                    // We found a section start.  Add the section name identifier
                    // to the local database and recurse.
                    idb.inventory_section_identifiers(lir, lindb);
                    result &= IdentDb::check_locals_r(lid, lindb, diags);
                }
                IRKind::Label => {
                    idb.inventory_label_identifiers(0, lir, lindb, diags);
                }

                IRKind::SectionEnd => break, // Done with local section inventory
                _ => {}
            }
        }

        if result {
            result &= idb.verify_local_refs(start_lid, lindb, diags)
        }

        // Update the caller's lid to the end of this local section
        debug!("IdentDb::check_locals_r: EXIT at lid {}", *lid);
        result
    }

    /// Recursively skip over nested sections and return to the parent section
    /// Call with the start_lid one past the nested section_start operation
    /// Returns the new final lid, which will be one past the section_end of
    /// the outermost nested section.
    fn skip_nested_sections_r(start_lid: usize, lindb: &LinearDb) -> usize {
        let mut lid = start_lid;
        loop {
            let lir = &lindb.ir_vec[lid];
            lid += 1;
            match lir.op {
                IRKind::SectionStart => {
                    lid = Self::skip_nested_sections_r(lid, lindb);
                }
                IRKind::SectionEnd => break,
                _ => {}
            }
        }
        lid
    }

    /// Verifies that every identifier reference exists in the inventory
    /// Must not be called before inventory_identifiers
    fn verify_local_refs(&self, start_lid: usize, lindb: &LinearDb, diags: &mut Diags) -> bool {
        let mut result = true;
        let mut lid = start_lid;

        loop {
            let lir = &lindb.ir_vec[lid];
            lid += 1;
            match lir.op {
                // TODO need img and abs here?
                IRKind::Sec => {
                    result &= self.verify_operand_refs(lir, lindb, diags);
                }
                IRKind::SectionStart => {
                    lid = Self::skip_nested_sections_r(lid, lindb);
                }
                IRKind::SectionEnd => break,
                _ => {}
            }
        }

        result
    }

    /// Adds a label identifier that is an operand to the inventory.
    /// This inventory contains only declarations of identifiers, not references.
    fn inventory_label_identifiers(
        &mut self,
        op_num: usize,
        lir: &LinIR,
        lindb: &LinearDb,
        diags: &mut Diags,
    ) -> bool {
        let mut result = true;
        let name_operand_num = lir.operand_vec[op_num];
        let name_operand = lindb.operand_vec.get(name_operand_num).unwrap();
        let name = &name_operand.sval;
        if is_reserved_identifier(name) {
            let m = format!(
                "'{}' is a reserved identifier and cannot be used as a label name",
                name
            );
            diags.err1("LINEAR_13", &m, name_operand.src_loc.clone());
            return false;
        }
        if self.label_idents.contains_key(name) {
            let orig_loc = self.label_idents.get(name).unwrap();
            let msg = format!("Duplicate label name {}", name);
            diags.err2(
                "LINEAR_2",
                &msg,
                name_operand.src_loc.clone(),
                orig_loc.clone(),
            );
            // keep processing after error to report other problems
            result = false;
        } else {
            self.label_idents
                .insert(name.clone(), name_operand.src_loc.clone());
        }
        result
    }

    /// Increment the number of occurrences of this section
    fn inventory_section_identifiers(&mut self, lir: &LinIR, lindb: &LinearDb) {
        trace!("IdentDb::inventory_section_identifiers: ENTER");
        let name_operand_num = lir.operand_vec[0];
        let name_operand = lindb.operand_vec.get(name_operand_num).unwrap();
        let name = &name_operand.sval;
        debug!(
            "IdentDb::inventory_section_identifiers: Adding section name {} to inventory.",
            name
        );

        // Increment existing section name count or insert new entry with count = 1.
        *self.section_count.entry(name.to_string()).or_insert(0) += 1;
        trace!("IdentDb::inventory_section_identifiers: EXIT");
    }

    /// Build a hash of all valid global identifier names: labels, sections, consts, etc
    /// Reports an error and returns false if duplicate labels exist.
    fn inventory_global_identifiers(&mut self, lindb: &LinearDb, diags: &mut Diags) -> bool {
        let mut result = true;
        for lir in &lindb.ir_vec {
            result &= match lir.op {
                IRKind::Label => self.inventory_label_identifiers(0, lir, lindb, diags),
                IRKind::SectionStart => {
                    self.inventory_section_identifiers(lir, lindb);
                    true
                }
                _ => true,
            }
        }

        debug!("IdentDb::inventory_global_identifiers:");
        for name in self.label_idents.keys() {
            debug!("    {}", name);
        }

        result
    }

    /// Verifies that every identifier reference exists in the inventory
    /// Must not be called before inventory_identifiers
    fn verify_global_refs(&self, lindb: &LinearDb, diags: &mut Diags) -> bool {
        let mut result = true;
        for lir in &lindb.ir_vec {
            result &= match lir.op {
                IRKind::Abs | IRKind::Img | IRKind::Sizeof => {
                    self.verify_operand_refs(lir, lindb, diags)
                }
                _ => true,
            }
        }

        result
    }

    /// Return true if the identifier refers to a section with only a
    /// single instance.  Returns false if this is an ambiguous section ref
    /// or not a section ref.
    fn is_valid_section_ref(&self, lop: &LinOperand, diags: &mut Diags) -> bool {
        if let Some(count) = self.section_count.get(&lop.sval) {
            if *count == 1 {
                return true;
            }
            let msg = format!(
                "Reference to section '{}' is ambiguous. This \
                                        section occurs {} times in the output",
                lop.sval, *count
            );
            diags.err1("LINEAR_7", &msg, lop.src_loc.clone());
            // keep processing after error to report other problems
        }
        false
    }

    /// Return true if the identifier refers to label.
    /// Returns false otherwise.
    fn is_valid_label_ref(&self, lop: &LinOperand) -> bool {
        if self.label_idents.contains_key(&lop.sval) {
            return true;
        }
        false
    }

    /// For the specified linear IR, verify any operands that are identifier
    /// references are valid as global identifiers.  Note that some
    /// operations have no operands, e.g. img() and fall through this
    /// function harmlessly.
    fn verify_operand_refs(&self, lir: &LinIR, lindb: &LinearDb, diags: &mut Diags) -> bool {
        let mut result = true;
        for &lop_num in &lir.operand_vec {
            let lop = &lindb.operand_vec[lop_num];
            if lop.tok == LexToken::Identifier {
                debug!(
                    "IdentDb::verify_identifier_refs: Verifying reference to '{}'",
                    lop.sval
                );
                if self.is_valid_section_ref(lop, diags) {
                    continue;
                }
                if self.is_valid_label_ref(lop) {
                    // labels have no size, so verify the linear operation is not a sizeof()
                    if lir.op == IRKind::Sizeof {
                        let msg = "Sizeof cannot refer to a label name.  Labels have no size."
                            .to_string();
                        diags.err1("LINEAR_9", &msg, lop.src_loc.clone());
                        // keep processing after error to report other problems
                        result = false;
                    }
                    continue;
                }

                let msg = format!("Unknown or unreachable identifier {}", lop.sval);
                diags.err1("LINEAR_6", &msg, lop.src_loc.clone());
                // keep processing after error to report other problems
                result = false;
            }
        }
        result
    }
}
