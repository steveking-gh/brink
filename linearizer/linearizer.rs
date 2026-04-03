// Shared expression linearization for brink.
//
// This crate provides the core types and expression-lowering logic shared by
// both the const-time (ConstDb) and layout-time (LayoutDb) pipeline stages.
// The crate carries no knowledge of const vs. layout context.  Each caller
// owns a Linearizer instance and the resulting IR/operand vectors.

use diags::Diags;
use diags::SourceSpan;
use indextree::NodeId;

#[allow(unused_imports)]
use tracing::debug;

use ast::{Ast, LexToken, TokenInfo};
use ir::IRKind;

/// An operand in the linearized IR. Two design choices greatly simplify lifetime
/// management:
/// * Callers hold indices into the Linearizer's operand_vec, not direct reference
///   to these structs.
/// * LinOperand owns its own data.
pub struct LinOperand {
    /// IR index that produced this operand; None for immediate (leaf) operands.
    pub ir_lid: Option<usize>,
    pub src_loc: SourceSpan,
    pub tok: LexToken,
    pub sval: String,
}

impl LinOperand {
    /// Construct a LinOperand from a token.  Pass `ir_lid` as `Some(lid)` for
    /// output slots produced by an IR instruction, or `None` for leaf literals.
    pub fn new(ir_lid: Option<usize>, tinfo: &TokenInfo<'_>) -> LinOperand {
        LinOperand {
            ir_lid,
            src_loc: tinfo.loc.clone(),
            sval: tinfo.val.to_string(),
            tok: tinfo.tok,
        }
    }

    /// Construct an output-slot operand for an extension call result.
    /// Use this instead of `new` when no source token describes the output type —
    /// only the source location is meaningful.  Sets `tok` to `LexToken::OutputSlot`
    /// and `sval` to an empty string.
    pub fn new_output(ir_lid: usize, src_loc: SourceSpan) -> LinOperand {
        LinOperand {
            ir_lid: Some(ir_lid),
            src_loc,
            tok: LexToken::OutputSlot,
            sval: String::new(),
        }
    }

    /// Return the IR index that produced this operand, or None for leaf literals.
    pub fn is_output_of(&self) -> Option<usize> {
        self.ir_lid
    }
}

/// A single instruction in the linearized IR.
pub struct LinIR {
    /// AST node that generated this instruction.
    pub nid: NodeId,
    /// Source location for diagnostics.
    pub src_loc: SourceSpan,
    /// Operation kind.
    pub op: IRKind,
    /// Indices into the owning Linearizer's operand_vec.
    pub operand_vec: Vec<usize>,
}

impl LinIR {
    /// Construct a LinIR from an AST node and operation kind.
    pub fn new(nid: NodeId, ast: &Ast<'_>, op: IRKind) -> Self {
        let tinfo = ast.get_tinfo(nid);
        let src_loc = tinfo.loc.clone();
        Self {
            nid,
            src_loc,
            op,
            operand_vec: Vec::new(),
        }
    }

    /// Append an operand index to this instruction's operand list.
    pub fn add_operand(&mut self, operand_num: usize) {
        self.operand_vec.push(operand_num);
    }
}

/// Map a LexToken to the corresponding IRKind.  This function maps only
/// tokens that appear inside expressions or as IR opcodes.  Passing a
/// statement-level token (e.g. LexToken::Section or LexToken::Output) that
/// has no IRKind equivalent causes a panic.
pub fn tok_to_irkind(tok: LexToken) -> IRKind {
    match tok {
        LexToken::Addr => IRKind::Addr,
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
        LexToken::Gt => IRKind::Gt,
        LexToken::AddrOffset => IRKind::AddrOffset,
        LexToken::LEq => IRKind::LEq,
        LexToken::Lt => IRKind::Lt,
        LexToken::Minus => IRKind::Subtract,
        LexToken::NEq => IRKind::NEq,
        LexToken::Percent => IRKind::Modulo,
        LexToken::Pipe => IRKind::BitOr,
        LexToken::Plus => IRKind::Add,
        LexToken::Print => IRKind::Print,
        LexToken::SecOffset => IRKind::SecOffset,
        LexToken::FileOffset => IRKind::FileOffset,
        LexToken::SetAddr => IRKind::SetAddr,
        LexToken::SetAddrOffset => IRKind::SetAddrOffset,
        LexToken::SetSecOffset => IRKind::SetSecOffset,
        LexToken::SetFileOffset => IRKind::SetFileOffset,
        LexToken::ToI64 => IRKind::ToI64,
        LexToken::ToU64 => IRKind::ToU64,
        LexToken::Wr8 => IRKind::Wr(1),
        LexToken::Wr16 => IRKind::Wr(2),
        LexToken::Wr24 => IRKind::Wr(3),
        LexToken::Wr32 => IRKind::Wr(4),
        LexToken::Wr40 => IRKind::Wr(5),
        LexToken::Wr48 => IRKind::Wr(6),
        LexToken::Wr56 => IRKind::Wr(7),
        LexToken::Wr64 => IRKind::Wr(8),
        LexToken::Wrf => IRKind::Wrf,
        LexToken::Wrs => IRKind::Wrs,
        LexToken::BuiltinOutputSize => IRKind::BuiltinOutputSize,
        LexToken::BuiltinOutputAddr => IRKind::BuiltinOutputAddr,
        LexToken::BuiltinVersionString => IRKind::BuiltinVersionString,
        LexToken::BuiltinVersionMajor => IRKind::BuiltinVersionMajor,
        LexToken::BuiltinVersionMinor => IRKind::BuiltinVersionMinor,
        LexToken::BuiltinVersionPatch => IRKind::BuiltinVersionPatch,
        bug => {
            panic!(
                "Failed to convert LexToken to IRKind for {:?} — \
                 this token should not reach tok_to_irkind",
                bug
            );
        }
    }
}

/// Owns a flat IR vector and operand vector built up during a single
/// linearization pass.  Has no lifetime parameters and owns all stored data.
/// Callers hold usize indices into the vectors rather than references to simplify
/// borrow checking.
pub struct Linearizer {
    pub ir_vec: Vec<LinIR>,
    pub operand_vec: Vec<LinOperand>,
}

impl Linearizer {
    /// Create an empty Linearizer with no IR or operands.
    pub fn new() -> Self {
        Self {
            ir_vec: Vec::new(),
            operand_vec: Vec::new(),
        }
    }

    /// Append a new IR entry and return its index (lid).
    pub fn new_ir(&mut self, nid: NodeId, ast: &Ast<'_>, op: IRKind) -> usize {
        let lir = LinIR::new(nid, ast, op);
        let lid = self.ir_vec.len();
        self.ir_vec.push(lir);
        lid
    }

    /// Allocate and initialize a new operand in the operand vector, then
    /// append the operand's index to the IR's own operand vector.
    pub fn add_new_operand_to_ir(&mut self, ir_lid: usize, operand: LinOperand) -> usize {
        let idx = self.operand_vec.len();
        self.operand_vec.push(operand);
        self.ir_vec[ir_lid].add_operand(idx);
        idx
    }

    /// Append an existing operand's index to the given IR.
    pub fn add_existing_operand_to_ir(&mut self, ir_lid: usize, idx: usize) {
        self.ir_vec[ir_lid].add_operand(idx);
    }

    /// Maximum expression nesting depth before LINEAR_1 fires.
    const MAX_RECURSION_DEPTH: usize = 100;

    /// Return false and emit an error if `rdepth` exceeds MAX_RECURSION_DEPTH.
    pub fn depth_sanity(
        &self,
        rdepth: usize,
        parent_nid: NodeId,
        diags: &mut Diags,
        ast: &Ast<'_>,
    ) -> bool {
        if rdepth > Self::MAX_RECURSION_DEPTH {
            let tinfo = ast.get_tinfo(parent_nid);
            let m = format!(
                "Maximum recursion depth ({}) exceeded when processing '{}'.",
                Self::MAX_RECURSION_DEPTH,
                tinfo.val
            );
            diags.err1("LINEAR_1", &m, tinfo.span());
            return false;
        }
        true
    }

    /// Return false and emit an error if this IR does not have exactly the
    /// expected number of operands.
    pub fn operand_count_is_valid(
        &self,
        expected: usize,
        lops: &[usize],
        diags: &mut Diags,
        tinfo: &TokenInfo<'_>,
    ) -> bool {
        let found = lops.len();
        if found != expected {
            let m = format!(
                "Expected {} operand(s), but found {} for '{}' expression",
                expected, found, tinfo.val
            );
            diags.err1("LINEAR_2", &m, tinfo.span());
            return false;
        }
        true
    }

    /// Validate that the number of operands for this IR is correct then append
    /// them to the IR's operand vector.  Return false if validation fails.
    pub fn process_operands(
        &mut self,
        expected: usize,
        lops: &mut Vec<usize>,
        ir_lid: usize,
        diags: &mut Diags,
        tinfo: &TokenInfo<'_>,
    ) -> bool {
        if self.operand_count_is_valid(expected, lops, diags, tinfo) {
            for idx in lops {
                self.add_existing_operand_to_ir(ir_lid, *idx);
            }
            true
        } else {
            false
        }
    }

    /// Like `process_operands`, but succeed silently when we have no operands.
    /// Used for expressions with optional operands such as `addr()`.
    pub fn process_optional_operands(
        &mut self,
        expected: usize,
        lops: &mut Vec<usize>,
        ir_lid: usize,
        diags: &mut Diags,
        tinfo: &TokenInfo<'_>,
    ) -> bool {
        if lops.is_empty() {
            return true;
        }
        self.process_operands(expected, lops, ir_lid, diags, tinfo)
    }

    /// Recurse over each AST child of the specified AST node and call
    /// record_expr_r.  We accumulating result operand indices in the `lops`
    /// operand vector.
    pub fn record_expr_children_r(
        &mut self,
        rdepth: usize,
        parent_nid: NodeId,
        lops: &mut Vec<usize>,
        diags: &mut Diags,
        ast: &Ast<'_>,
    ) -> bool {
        let mut result = true;
        for nid in ast.children(parent_nid) {
            result &= self.record_expr_r(rdepth, nid, lops, diags, ast);
        }
        result
    }

    /// Recursively lower one expression AST node into IR/operand entries.
    ///
    /// Handles all expression tokens: literals, identifiers, binary operators,
    /// type conversions, builtin variables, and extension calls.  Caller must
    /// not pass statement tokens (wr32, section, align, etc.).
    ///
    /// Returns true on success with operand indices appended to
    /// returned_operands.
    pub fn record_expr_r(
        &mut self,
        rdepth: usize,
        parent_nid: NodeId,
        returned_operands: &mut Vec<usize>,
        diags: &mut Diags,
        ast: &Ast<'_>,
    ) -> bool {
        debug!(
            "Linearizer::record_expr_r: ENTER at depth {} for nid: {}",
            rdepth, parent_nid
        );

        if !self.depth_sanity(rdepth, parent_nid, diags, ast) {
            return false;
        }

        let tinfo = ast.get_tinfo(parent_nid);
        let tok = tinfo.tok;
        let mut result = true;

        match tok {
            LexToken::U64 | LexToken::I64 | LexToken::Integer | LexToken::QuotedString => {
                // parent_nid is a literal.  Recursion bottom.
                let idx = self.operand_vec.len();
                self.operand_vec.push(LinOperand::new(None, tinfo));
                returned_operands.push(idx);
            }

            LexToken::Identifier => {
                // parent_nid is an identifier.
                if ast.has_children(parent_nid) {
                    // An identifier with children means this is an extension call.
                    let ir_lid = self.new_ir(parent_nid, ast, IRKind::ExtensionCall);
                    self.add_new_operand_to_ir(ir_lid, LinOperand::new(None, tinfo));

                    // Now recursively add operands for the extension call arguments.
                    let mut lops = Vec::new();
                    for child in ast.children(parent_nid) {
                        result &= self.record_expr_r(rdepth + 1, child, &mut lops, diags, ast);
                    }
                    for idx in lops {
                        self.add_existing_operand_to_ir(ir_lid, idx);
                    }

                    // Output operand for the extension result.
                    let out_idx = self.add_new_operand_to_ir(
                        ir_lid,
                        LinOperand::new_output(ir_lid, tinfo.loc.clone()),
                    );
                    returned_operands.push(out_idx);
                } else {
                    // Leaf identifier — const ref, section name, etc.
                    let idx = self.operand_vec.len();
                    self.operand_vec.push(LinOperand::new(None, tinfo));
                    returned_operands.push(idx);
                }
            }

            // ── Namespace extension call: custom::foo(args…) ───────────────
            LexToken::Namespace => {
                let mut children = ast.children(parent_nid);
                let id_child = children.next().unwrap();
                let id_tinfo = ast.get_tinfo(id_child);
                let extension_name = format!("{}{}", tinfo.val, id_tinfo.val);

                let ir_lid = self.new_ir(parent_nid, ast, IRKind::ExtensionCall);
                let mut name_op = LinOperand::new(None, tinfo);
                name_op.sval = extension_name;
                self.add_new_operand_to_ir(ir_lid, name_op);

                let mut lops = Vec::new();
                for child in children {
                    result &= self.record_expr_r(rdepth + 1, child, &mut lops, diags, ast);
                }
                for idx in lops {
                    self.add_existing_operand_to_ir(ir_lid, idx);
                }

                let out_idx = self.add_new_operand_to_ir(
                    ir_lid,
                    LinOperand::new_output(ir_lid, tinfo.loc.clone()),
                );
                returned_operands.push(out_idx);
            }

            // ── Builtin variable atoms ─────────────────────────────────────
            LexToken::BuiltinOutputSize
            | LexToken::BuiltinOutputAddr
            | LexToken::BuiltinVersionString
            | LexToken::BuiltinVersionMajor
            | LexToken::BuiltinVersionMinor
            | LexToken::BuiltinVersionPatch => {
                let ir_lid = self.new_ir(parent_nid, ast, tok_to_irkind(tok));
                let idx = self.add_new_operand_to_ir(ir_lid, LinOperand::new(Some(ir_lid), tinfo));
                returned_operands.push(idx);
            }

            // ── Type conversions ───────────────────────────────────────────
            LexToken::ToI64 | LexToken::ToU64 => {
                let mut lops = Vec::new();
                result &=
                    self.record_expr_children_r(rdepth + 1, parent_nid, &mut lops, diags, ast);
                let ir_lid = self.new_ir(parent_nid, ast, tok_to_irkind(tok));
                result &= self.process_operands(1, &mut lops, ir_lid, diags, tinfo);
                let idx = self.add_new_operand_to_ir(ir_lid, LinOperand::new(Some(ir_lid), tinfo));
                returned_operands.push(idx);
            }

            // ── Binary and comparison operators ────────────────────────────
            LexToken::Eq
            | LexToken::NEq
            | LexToken::LEq
            | LexToken::GEq
            | LexToken::Lt
            | LexToken::Gt
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
                let mut lops = Vec::new();
                result &=
                    self.record_expr_children_r(rdepth + 1, parent_nid, &mut lops, diags, ast);
                let ir_lid = self.new_ir(parent_nid, ast, tok_to_irkind(tok));
                result &= self.process_operands(2, &mut lops, ir_lid, diags, tinfo);
                let idx = self.add_new_operand_to_ir(ir_lid, LinOperand::new(Some(ir_lid), tinfo));
                returned_operands.push(idx);
            }

            // ── sizeof(section) or sizeof(namespace::ext) ─────────────────
            LexToken::Sizeof => {
                let first_child = ast.children(parent_nid).next().unwrap();
                let first_child_tinfo = ast.get_tinfo(first_child);

                if first_child_tinfo.tok == LexToken::Namespace {
                    let ns_children: Vec<_> = ast.children(first_child).collect();
                    let ext_id_tinfo = ast.get_tinfo(ns_children[0]);
                    let full_name = format!("{}{}", first_child_tinfo.val, ext_id_tinfo.val);

                    let ir_lid = self.new_ir(parent_nid, ast, IRKind::SizeofExt);
                    let mut name_op = LinOperand::new(None, first_child_tinfo);
                    name_op.sval = full_name;
                    self.add_new_operand_to_ir(ir_lid, name_op);

                    let idx =
                        self.add_new_operand_to_ir(ir_lid, LinOperand::new(Some(ir_lid), tinfo));
                    returned_operands.push(idx);
                } else {
                    let mut lops = Vec::new();
                    let ir_lid = self.new_ir(parent_nid, ast, IRKind::Sizeof);
                    result &=
                        self.record_expr_children_r(rdepth + 1, parent_nid, &mut lops, diags, ast);
                    result &= self.process_operands(1, &mut lops, ir_lid, diags, tinfo);
                    let idx =
                        self.add_new_operand_to_ir(ir_lid, LinOperand::new(Some(ir_lid), tinfo));
                    returned_operands.push(idx);
                }
            }

            // ── Address queries: addr([label]), addr_offset([label]), etc. ──
            LexToken::Addr | LexToken::AddrOffset | LexToken::SecOffset | LexToken::FileOffset => {
                let mut lops = Vec::new();
                let ir_lid = self.new_ir(parent_nid, ast, tok_to_irkind(tok));
                result &=
                    self.record_expr_children_r(rdepth + 1, parent_nid, &mut lops, diags, ast);
                result &= self.process_optional_operands(1, &mut lops, ir_lid, diags, tinfo);
                let idx = self.add_new_operand_to_ir(ir_lid, LinOperand::new(Some(ir_lid), tinfo));
                returned_operands.push(idx);
            }

            // ── Syntactic noise — no IR emitted ───────────────────────────
            LexToken::Semicolon
            | LexToken::Comma
            | LexToken::OpenParen
            | LexToken::CloseParen
            | LexToken::OpenBrace
            | LexToken::CloseBrace => {}

            // ── Anything else is a bug: statement tokens must not reach here
            _ => {
                let msg = format!("'{}' is not valid in an expression context", tinfo.val);
                diags.err1("LINEAR_17", &msg, tinfo.span());
                result = false;
            }
        }

        debug!(
            "Linearizer::record_expr_r: EXIT({}) at depth {} for nid: {}",
            result, rdepth, parent_nid
        );
        result
    }
}
