// Const-time evaluation for brink.
//
// The public interface is a single free function, `evaluate()`, which drives
// both passes of the const pipeline:
//
//   1. Lowering — walks the AST const statements and flattens them into a
//      private `ConstIR` (parallel LinIR instruction and LinOperand vectors).
//   2. Evaluation — walks `ConstIR` sequentially, resolving every const to a
//      `ParameterValue` and storing the result in a `SymbolTable`.
//
// The caller receives a fully resolved `SymbolTable`; `ConstIR` is an
// internal implementation detail and never exposed outside this crate.

use depth_guard::{DepthGuard, MAX_RECURSION_DEPTH};
use diags::Diags;
use diags::SourceSpan;
use indextree::NodeId;
use parse_int::parse;
use std::collections::HashMap;

#[allow(unused_imports)]
use tracing::{debug, trace};

use ast::{Ast, AstDb, LexToken};
use ir::{ConstBuiltins, IRKind, ParameterValue, RegionBinding, strip_kmg};

use linearizer::{LinIR, LinOperand, Linearizer, tok_to_irkind};
use symtable::SymbolTable;

// ── Internal error type for const arithmetic ─────────────────────────────────

/// Error returned by `calc_u64_op` / `calc_i64_op` before a diagnostic is emitted.
enum CalcErr {
    /// Arithmetic overflow or underflow; carries a human-readable message.
    Overflow(String),
    /// Division or modulo by zero.
    DivByZero,
}

// ── Internal intermediate ─────────────────────────────────────────────────────

/// Flattened const-time IR produced by the lowering pass.
/// Private to this crate; the public API is the `evaluate()` free function.
struct ConstIR {
    ir_vec: Vec<LinIR>,
    operand_vec: Vec<LinOperand>,
}

impl ConstIR {
    fn dump(&self) {
        for (idx, ir) in self.ir_vec.iter().enumerate() {
            let mut op = format!("const lid {}: nid {} is {:?}", idx, ir.nid, ir.op);
            let mut first = true;
            for child in &ir.operand_vec {
                let operand = &self.operand_vec[*child];
                if !first {
                    op.push(',');
                } else {
                    first = false;
                }
                match operand {
                    LinOperand::Literal { sval, .. }
                    | LinOperand::Ref { sval, .. }
                    | LinOperand::NameDef { sval, .. } => op.push_str(&format!(" {}", sval)),
                    LinOperand::Output { ir_lid, .. } => {
                        op.push_str(&format!(" tmp{}, output of lid {}", *child, ir_lid))
                    }
                }
            }
            debug!("ConstIR: {}", op);
        }
    }
}

// ── Lowering pass (AST → LinIR) ───────────────────────────────────────────────

// Need 'toks lifetime because the lowering methods borrow data tied
// to the AST token lifespan.  Once constructed, we don't delete the AST
// so the lifetime is effectively infinite, but we still need the formality.
impl<'toks> ConstIR {
    /// Lower a `const NAME = <expr>` full definition into ir_vec.
    /// Returns true on success, false otherwise.
    fn record_const_decl(
        lz: &mut Linearizer,
        const_nid: NodeId,
        diags: &mut Diags,
        ast: &'toks Ast,
    ) -> bool {
        let ir_lid = lz.new_ir(const_nid, ast, IRKind::Const);

        let mut children = ast.children(const_nid);

        // Child 0: name identifier
        let name_nid = children.next().unwrap();
        let name_tinfo = ast.get_tinfo(name_nid);
        let name_idx = lz.operand_vec.len();
        lz.operand_vec.push(LinOperand::new_name(name_tinfo));
        lz.add_existing_operand_to_ir(ir_lid, name_idx);

        // Child 1: `=` sign
        let eq_nid = children.next().unwrap();
        let eq_tinfo = ast.get_tinfo(eq_nid);

        // Child 2: RHS expression
        let rhs_nid = children.next().unwrap();
        let mut rhs_lops = Vec::new();
        if !lz.record_expr_r(rhs_nid, &mut rhs_lops, diags, ast) {
            return false;
        }
        if rhs_lops.len() != 1 {
            unreachable!(
                "record_expr_r returned {} results for const RHS; \
                 parser guarantees exactly one expression node",
                rhs_lops.len()
            );
        }
        lz.add_existing_operand_to_ir(ir_lid, rhs_lops[0]);

        // Output slot: the Eq token carries the output type info
        lz.add_new_operand_to_ir(ir_lid, LinOperand::new_output(ir_lid, eq_tinfo.loc.clone()));

        true
    }

    /// Lower a `const NAME;` declare-only statement into ir_vec.
    fn record_const_declare(lz: &mut Linearizer, const_nid: NodeId, ast: &'toks Ast) -> bool {
        let mut children = ast.children(const_nid);
        let name_nid = children.next().unwrap();
        let name_tinfo = ast.get_tinfo(name_nid);
        let ir_lid = lz.new_ir(const_nid, ast, IRKind::ConstDeclare);
        lz.add_new_operand_to_ir(ir_lid, LinOperand::new_name(name_tinfo));
        true
    }

    /// Lower a deferred assignment `IDENT = expr;` (inside an if/else body).
    fn record_deferred_assign(
        lz: &mut Linearizer,
        eq_nid: NodeId,
        diags: &mut Diags,
        ast: &'toks Ast,
    ) -> bool {
        let mut children = ast.children(eq_nid);
        let ident_nid = children.next().unwrap();
        let expr_nid = children.next().unwrap();
        let ident_tinfo = ast.get_tinfo(ident_nid);

        let ir_lid = lz.new_ir(eq_nid, ast, IRKind::BareAssign);
        lz.add_new_operand_to_ir(ir_lid, LinOperand::new_name(ident_tinfo));

        let mut rhs_lops = Vec::new();
        if !lz.record_expr_r(expr_nid, &mut rhs_lops, diags, ast) {
            return false;
        }
        if rhs_lops.len() != 1 {
            unreachable!(
                "record_expr_r returned {} results for deferred-assign RHS; \
                 parser guarantees exactly one expression node",
                rhs_lops.len()
            );
        }
        lz.add_existing_operand_to_ir(ir_lid, rhs_lops[0]);
        true
    }

    /// Dispatch a single statement inside an if/else body.
    fn record_if_body_stmt(
        lz: &mut Linearizer,
        stmt_nid: NodeId,
        diags: &mut Diags,
        ast: &'toks Ast,
    ) -> bool {
        let tinfo = ast.get_tinfo(stmt_nid);
        match tinfo.tok {
            LexToken::Eq => Self::record_deferred_assign(lz, stmt_nid, diags, ast),
            LexToken::Print | LexToken::Assert => {
                let mut lops = Vec::new();
                lz.record_expr_children_r(stmt_nid, &mut lops, diags, ast);
                let ir_lid = lz.new_ir(stmt_nid, ast, tok_to_irkind(tinfo.tok));
                for idx in lops {
                    lz.add_existing_operand_to_ir(ir_lid, idx);
                }
                true
            }
            LexToken::If => Self::record_if_else(lz, stmt_nid, diags, ast),
            _ => true, // syntactic tokens already filtered by parser
        }
    }

    /// Lower an `if/else` block into ir_vec.
    fn record_if_else(
        lz: &mut Linearizer,
        if_nid: NodeId,
        diags: &mut Diags,
        ast: &'toks Ast,
    ) -> bool {
        let children: Vec<NodeId> = ast.children(if_nid).collect();
        let mut i = 0;
        let mut result = true;

        // Child 0: condition expression
        let cond_nid = children[i];
        i += 1;
        let mut cond_lops = Vec::new();
        if !lz.record_expr_r(cond_nid, &mut cond_lops, diags, ast) {
            return false;
        }
        if cond_lops.len() != 1 {
            unreachable!(
                "record_expr_r returned {} results for if condition; \
                 parser guarantees exactly one expression node",
                cond_lops.len()
            );
        }

        let ifbegin_lid = lz.new_ir(if_nid, ast, IRKind::IfBegin);
        lz.add_existing_operand_to_ir(ifbegin_lid, cond_lops[0]);

        // Child 1: '{' — skip
        i += 1;

        // Then-body: children until '}'
        while i < children.len() {
            let tok = ast.get_tinfo(children[i]).tok;
            if tok == LexToken::CloseBrace {
                i += 1;
                break;
            }
            result &= Self::record_if_body_stmt(lz, children[i], diags, ast);
            i += 1;
        }

        // Optional else clause
        if i < children.len() && ast.get_tinfo(children[i]).tok == LexToken::Else {
            let else_nid = children[i];
            i += 1;
            lz.new_ir(else_nid, ast, IRKind::ElseBegin);

            if i < children.len() {
                let next_tok = ast.get_tinfo(children[i]).tok;
                if next_tok == LexToken::If {
                    result &= Self::record_if_else(lz, children[i], diags, ast);
                } else if next_tok == LexToken::OpenBrace {
                    i += 1; // skip '{'
                    while i < children.len() {
                        let tok = ast.get_tinfo(children[i]).tok;
                        if tok == LexToken::CloseBrace {
                            break;
                        }
                        result &= Self::record_if_body_stmt(lz, children[i], diags, ast);
                        i += 1;
                    }
                }
            }
        }

        lz.new_ir(if_nid, ast, IRKind::IfEnd);
        result
    }

    // ── Evaluation pass (LinIR → SymbolTable) ─────────────────────────────────

    /// Sequential walk of the const IR that handles all ConstDb IR kinds:
    /// `Const`, `ConstDeclare`, `IfBegin`, `ElseBegin`, `IfEnd`, `BareAssign`,
    /// and `Print`/`Assert` emitted inside if/else bodies.
    fn exec_const_statements(
        const_db: &ConstIR,
        symbol_table: &mut SymbolTable,
        diags: &mut Diags,
    ) -> bool {
        /// Skip state for branches not taken.
        #[derive(Clone, Copy)]
        enum SkipState {
            /// Skip the then-body (condition was false); stop at ElseBegin (depth 0)
            /// or IfEnd (depth 0, meaning no else clause).
            SkipThen { depth: usize },
            /// Skip the else-body (condition was true); stop at IfEnd (depth 0).
            SkipElse { depth: usize },
        }

        let mut result = true;
        let mut skip_stack: Vec<SkipState> = Vec::new();

        let n = const_db.ir_vec.len();
        let mut idx = 0;
        while idx < n {
            let ir = &const_db.ir_vec[idx];
            let op = ir.op;
            let src_loc = ir.src_loc.clone();

            // If we're in a skip state, handle structural tokens to track depth.
            if let Some(&skip) = skip_stack.last() {
                match (skip, op) {
                    (SkipState::SkipThen { depth }, IRKind::IfBegin) => {
                        *skip_stack.last_mut().unwrap() = SkipState::SkipThen { depth: depth + 1 };
                    }
                    (SkipState::SkipThen { depth: 0 }, IRKind::ElseBegin) => {
                        // Found the else of the if we're skipping — resume active processing.
                        skip_stack.pop();
                    }
                    (SkipState::SkipThen { depth }, IRKind::ElseBegin) => {
                        // Nested if's ElseBegin — no depth change (it's inside a nested if).
                        let _ = depth; // depth > 0, we're still skipping
                    }
                    (SkipState::SkipThen { depth: 0 }, IRKind::IfEnd) => {
                        // No else clause — resume active processing past IfEnd.
                        skip_stack.pop();
                    }
                    (SkipState::SkipThen { depth }, IRKind::IfEnd) => {
                        *skip_stack.last_mut().unwrap() = SkipState::SkipThen { depth: depth - 1 };
                    }
                    (SkipState::SkipElse { depth }, IRKind::IfBegin) => {
                        *skip_stack.last_mut().unwrap() = SkipState::SkipElse { depth: depth + 1 };
                    }
                    (SkipState::SkipElse { depth: 0 }, IRKind::IfEnd) => {
                        // Found the IfEnd matching the if whose else-body we're skipping.
                        skip_stack.pop();
                    }
                    (SkipState::SkipElse { depth }, IRKind::IfEnd) => {
                        *skip_stack.last_mut().unwrap() = SkipState::SkipElse { depth: depth - 1 };
                    }
                    _ => { /* any other IR inside a skipped block: ignore */ }
                }
                idx += 1;
                continue;
            }

            // Active processing.
            match op {
                IRKind::Const => {
                    let name_lop = &const_db.operand_vec[ir.operand_vec[0]];
                    let LinOperand::NameDef { sval: name, .. } = name_lop else {
                        panic!("Const name operand must be a NameDef");
                    };
                    let rhs_lop_num = ir.operand_vec[1];
                    let val = Self::eval_const_expr_r(
                        symbol_table,
                        rhs_lop_num,
                        const_db,
                        diags,
                        &src_loc,
                    );
                    if let Some(v) = val {
                        if !symbol_table.contains_key(name) {
                            symbol_table.define(name.to_string(), v, Some(src_loc.clone()));
                        }
                    } else {
                        result = false;
                    }
                }
                IRKind::ConstDeclare => {
                    let name_lop = &const_db.operand_vec[ir.operand_vec[0]];
                    let LinOperand::NameDef { sval: name, .. } = name_lop else {
                        panic!("ConstDeclare name operand must be a NameDef");
                    };
                    symbol_table.declare(name.clone(), src_loc);
                }
                IRKind::IfBegin => {
                    let cond_lop_num = ir.operand_vec[0];
                    let cond_val = Self::eval_const_expr_r(
                        symbol_table,
                        cond_lop_num,
                        const_db,
                        diags,
                        &src_loc,
                    );
                    match cond_val.and_then(|v| v.to_bool()) {
                        Some(true) => {
                            // Condition true: process then-body (no skip needed)
                        }
                        Some(false) => {
                            // Condition false: skip then-body
                            skip_stack.push(SkipState::SkipThen { depth: 0 });
                        }
                        None => {
                            diags.err1(
                                "IRDB_56",
                                "if condition must evaluate to a numeric type",
                                src_loc,
                            );
                            result = false;
                            // Skip entire if/else to avoid cascading errors
                            skip_stack.push(SkipState::SkipThen { depth: 0 });
                        }
                    }
                }
                IRKind::ElseBegin => {
                    // Reached the else separator while in active then-body: skip else-body.
                    skip_stack.push(SkipState::SkipElse { depth: 0 });
                }
                IRKind::IfEnd => {
                    // End of an if/else we fully processed (no skip): nothing to do.
                }
                IRKind::BareAssign => {
                    let name_lop = &const_db.operand_vec[ir.operand_vec[0]];
                    let LinOperand::NameDef { sval: name, .. } = name_lop else {
                        panic!("BareAssign name operand must be a NameDef");
                    };
                    let name = name.clone();
                    let rhs_lop_num = ir.operand_vec[1];
                    let rhs_val = Self::eval_const_expr_r(
                        symbol_table,
                        rhs_lop_num,
                        const_db,
                        diags,
                        &src_loc,
                    );
                    match rhs_val {
                        Some(v) => {
                            result &= symbol_table.assign(&name, v, &src_loc, diags);
                        }
                        None => {
                            result = false;
                        }
                    }
                }
                IRKind::Print if !diags.noprint => {
                    let mut s = String::new();
                    for &lop_idx in &ir.operand_vec {
                        match Self::eval_const_expr_r(
                            symbol_table,
                            lop_idx,
                            const_db,
                            diags,
                            &src_loc,
                        ) {
                            Some(ParameterValue::QuotedString(ref v)) => s.push_str(v),
                            Some(ParameterValue::U64(v)) => s.push_str(&format!("{:#X}", v)),
                            Some(ParameterValue::I64(v) | ParameterValue::Integer(v)) => {
                                s.push_str(&format!("{}", v));
                            }
                            Some(_) => {
                                diags.err1(
                                    "IRDB_31",
                                    "Cannot print this value type in a const context",
                                    src_loc.clone(),
                                );
                                result = false;
                            }
                            None => {
                                result = false;
                            }
                        }
                    }
                    if result {
                        print!("{}", s);
                    }
                }
                IRKind::Assert => {
                    let cond_lop_num = ir.operand_vec[0];
                    match Self::eval_const_expr_r(
                        symbol_table,
                        cond_lop_num,
                        const_db,
                        diags,
                        &src_loc,
                    )
                    .and_then(|v| v.to_bool())
                    {
                        Some(false) => {
                            diags.err1(
                                "IRDB_32",
                                "Assert expression failed in if/else body",
                                src_loc,
                            );
                            result = false;
                        }
                        None => {
                            diags.err1(
                                "IRDB_57",
                                "assert condition must evaluate to a numeric type",
                                src_loc,
                            );
                            result = false;
                        }
                        Some(true) => {}
                    }
                }
                _ => { /* other IR kinds are not emitted into const_ir_vec */ }
            }
            idx += 1;
        }
        result
    }

    /// Evaluate a const expression operand recursively.
    /// Returns the computed `ParameterValue`, or `None` on error.
    fn eval_const_expr_r(
        symbol_table: &mut SymbolTable,
        lop_num: usize,
        const_db: &ConstIR,
        diags: &mut Diags,
        err_loc: &SourceSpan,
    ) -> Option<ParameterValue> {
        let Some(_guard) = DepthGuard::enter(MAX_RECURSION_DEPTH) else {
            diags.err1(
                "IRDB_59",
                &format!(
                    "Const expression nesting depth exceeds maximum ({}).",
                    MAX_RECURSION_DEPTH
                ),
                err_loc.clone(),
            );
            return None;
        };
        let lop = &const_db.operand_vec[lop_num];

        // Output operands: evaluate by looking up the producing instruction's IRKind.
        if let &LinOperand::Output { ir_lid, .. } = lop {
            let lin_ir = &const_db.ir_vec[ir_lid];
            let op = lin_ir.op;
            let op_loc = lin_ir.src_loc.clone();

            // Reject layout-time ops before evaluating any operands.
            match op {
                IRKind::Sizeof
                | IRKind::SizeofExt
                | IRKind::BuiltinOutputSize
                | IRKind::BuiltinOutputAddr
                | IRKind::Addr
                | IRKind::AddrOffset
                | IRKind::SecOffset
                | IRKind::FileOffset => {
                    let m = format!(
                        "Operation '{:?}' cannot be used in a const expression \
                         because it requires engine-time layout or addressing.",
                        op
                    );
                    diags.err1("IRDB_19", &m, op_loc);
                    return None;
                }
                _ => {}
            }

            // Dispatch on the producing op *before* touching any operands.  Each op
            // kind has its own operand layout; reading operand_vec[1] for a unary op
            // (e.g. ToI64 layout = [input, output]) aliases the output slot itself
            // and causes infinite recursion.
            return match op {
                // ── Zero-operand: version builtins ──────────────────────────────
                IRKind::BuiltinVersionString => Some(ParameterValue::QuotedString(
                    ConstBuiltins::get().brink_version_string.to_string(),
                )),
                IRKind::BuiltinVersionMajor => Some(ParameterValue::U64(
                    ConstBuiltins::get().brink_version_major,
                )),
                IRKind::BuiltinVersionMinor => Some(ParameterValue::U64(
                    ConstBuiltins::get().brink_version_minor,
                )),
                IRKind::BuiltinVersionPatch => Some(ParameterValue::U64(
                    ConstBuiltins::get().brink_version_patch,
                )),

                // ── Unary type conversions: operand layout = [input, output] ───
                IRKind::ToI64 | IRKind::ToU64 => {
                    let input_lop = lin_ir.operand_vec[0];
                    let val =
                        Self::eval_const_expr_r(symbol_table, input_lop, const_db, diags, err_loc)?;
                    match (&val, op) {
                        (ParameterValue::U64(v), IRKind::ToI64) => {
                            Some(ParameterValue::I64(*v as i64))
                        }
                        (ParameterValue::I64(_) | ParameterValue::Integer(_), IRKind::ToI64) => {
                            Some(ParameterValue::I64(val.to_i64()))
                        }
                        (
                            ParameterValue::U64(_)
                            | ParameterValue::I64(_)
                            | ParameterValue::Integer(_),
                            IRKind::ToU64,
                        ) => Some(ParameterValue::U64(val.to_u64())),
                        _ => {
                            let m = format!(
                                "Cannot apply '{:?}' to {:?} in a const expression.",
                                op,
                                val.data_type()
                            );
                            diags.err1("IRDB_21", &m, op_loc);
                            None
                        }
                    }
                }

                // ── Binary arithmetic: operand layout = [lhs, rhs, output] ─────
                IRKind::Add
                | IRKind::Subtract
                | IRKind::Multiply
                | IRKind::Divide
                | IRKind::Modulo
                | IRKind::BitAnd
                | IRKind::BitOr
                | IRKind::LeftShift
                | IRKind::RightShift => {
                    let lhs_val = Self::eval_const_expr_r(
                        symbol_table,
                        lin_ir.operand_vec[0],
                        const_db,
                        diags,
                        err_loc,
                    )?;
                    let rhs_val = Self::eval_const_expr_r(
                        symbol_table,
                        lin_ir.operand_vec[1],
                        const_db,
                        diags,
                        err_loc,
                    )?;
                    Self::apply_binary_op(op, lhs_val, rhs_val, &op_loc, diags)
                }

                // ── Binary comparison: operand layout = [lhs, rhs, output] ─────
                IRKind::DoubleEq
                | IRKind::NEq
                | IRKind::GEq
                | IRKind::LEq
                | IRKind::Gt
                | IRKind::Lt => {
                    let lhs_val = Self::eval_const_expr_r(
                        symbol_table,
                        lin_ir.operand_vec[0],
                        const_db,
                        diags,
                        err_loc,
                    )?;
                    let rhs_val = Self::eval_const_expr_r(
                        symbol_table,
                        lin_ir.operand_vec[1],
                        const_db,
                        diags,
                        err_loc,
                    )?;
                    Self::apply_comparison_op(op, lhs_val, rhs_val, &op_loc, diags)
                }

                // ── Binary logical: operand layout = [lhs, rhs, output] ─────────
                IRKind::LogicalAnd | IRKind::LogicalOr => {
                    let lhs_val = Self::eval_const_expr_r(
                        symbol_table,
                        lin_ir.operand_vec[0],
                        const_db,
                        diags,
                        err_loc,
                    )?;
                    let rhs_val = Self::eval_const_expr_r(
                        symbol_table,
                        lin_ir.operand_vec[1],
                        const_db,
                        diags,
                        err_loc,
                    )?;
                    let Some(lhs_b) = lhs_val.to_bool() else {
                        diags.err1(
                            "IRDB_58",
                            "'&&'/'||' operands must be numeric",
                            err_loc.clone(),
                        );
                        return None;
                    };
                    let Some(rhs_b) = rhs_val.to_bool() else {
                        diags.err1(
                            "IRDB_58",
                            "'&&'/'||' operands must be numeric",
                            err_loc.clone(),
                        );
                        return None;
                    };
                    let result = if op == IRKind::LogicalAnd {
                        lhs_b && rhs_b
                    } else {
                        lhs_b || rhs_b
                    };
                    Some(ParameterValue::U64(if result { 1 } else { 0 }))
                }

                // ── Everything else is not supported at const time ───────────────
                _ => {
                    let m = format!(
                        "Operation '{:?}' is not supported in a const expression.",
                        op
                    );
                    diags.err1("IRDB_21", &m, err_loc.clone());
                    None
                }
            };
        }

        // Ref operands: identifier reference — look up in the symbol table.
        if let LinOperand::Ref { sval, src_loc, .. } = lop {
            if let Some(val) = symbol_table.get_value(sval.as_str()) {
                symbol_table.mark_used(sval.as_str());
                return Some(val);
            } else {
                let m = format!(
                    "Unknown or uninitialized identifier '{}' in const expression. \
                     Constants must be defined before use.",
                    sval
                );
                diags.err1("IRDB_20", &m, src_loc.clone());
                return None;
            }
        }

        // Literal operands: evaluate directly from tok and sval.
        // Output and Ref variants are handled above; NameDef is the only
        // remaining variant, but const expression lowering never produces
        // NameDef operands -- those are only emitted for structural identifiers
        // in non-const IR (extension names, section names).
        let LinOperand::Literal {
            tok, sval, src_loc, ..
        } = lop
        else {
            unreachable!("NameDef operand in const expression; lowering invariant violated");
        };
        let sval = sval.clone();
        let src_loc = src_loc.clone();

        match tok {
            ast::LexToken::Integer => {
                let (base, mult) = strip_kmg(&sval);
                let v: i64 = parse::<i64>(base).ok().and_then(|v| v.checked_mul(mult as i64)).ok_or(()).ok().or_else(|| {
                    let m = format!("Malformed integer in const expression: {}", sval);
                    diags.err1("IRDB_22", &m, src_loc);
                    None
                })?;
                Some(ParameterValue::Integer(v))
            }
            ast::LexToken::U64 => {
                let no_u = sval.strip_suffix('u').unwrap_or(&sval);
                let (base, mult) = strip_kmg(no_u);
                let v: u64 = parse::<u64>(base).ok().and_then(|v| v.checked_mul(mult)).ok_or(()).ok().or_else(|| {
                    let m = format!("Malformed U64 in const expression: {}", sval);
                    diags.err1("IRDB_23", &m, src_loc);
                    None
                })?;
                Some(ParameterValue::U64(v))
            }
            ast::LexToken::I64 => {
                let no_i = sval.strip_suffix('i').unwrap_or(&sval);
                let (base, mult) = strip_kmg(no_i);
                let v: i64 = parse::<i64>(base).ok().and_then(|v| v.checked_mul(mult as i64)).ok_or(()).ok().or_else(|| {
                    let m = format!("Malformed I64 in const expression: {}", sval);
                    diags.err1("IRDB_24", &m, src_loc);
                    None
                })?;
                Some(ParameterValue::I64(v))
            }
            ast::LexToken::QuotedString => {
                let trimmed = sval
                    .strip_prefix('"')
                    .unwrap_or(&sval)
                    .strip_suffix('"')
                    .unwrap_or(&sval)
                    .to_string();
                Some(ParameterValue::QuotedString(trimmed))
            }
            _ => {
                panic!(
                    "Literal operand with unexpected token {:?} in const expression",
                    tok
                );
            }
        }
    }

    /// Reconcile an (lhs, rhs) pair for const arithmetic or comparison.
    /// Promotes `Integer` to match `U64` or `I64` when one side is untyped.
    /// Passes `(QuotedString, QuotedString)` through unchanged so that the
    /// caller's numeric dispatch can emit IRDB_26 for string arithmetic.
    /// Returns `None` and emits `err_code` on any other type mismatch.
    fn coerce_numeric_pair(
        lhs: ParameterValue,
        rhs: ParameterValue,
        err_code: &str,
        src_loc: &SourceSpan,
        diags: &mut Diags,
    ) -> Option<(ParameterValue, ParameterValue)> {
        use ParameterValue::*;
        match (&lhs, &rhs) {
            (U64(_), U64(_))
            | (I64(_), I64(_))
            | (Integer(_), Integer(_))
            | (QuotedString(_), QuotedString(_)) => Some((lhs, rhs)),
            (U64(_), Integer(v)) => Some((lhs, U64(*v as u64))),
            (Integer(v), U64(_)) => Some((U64(*v as u64), rhs)),
            (I64(_), Integer(v)) => Some((lhs, I64(*v))),
            (Integer(v), I64(_)) => Some((I64(*v), rhs)),
            _ => {
                let m = format!(
                    "Type mismatch in const expression: {:?} and {:?}.",
                    lhs.data_type(),
                    rhs.data_type()
                );
                diags.err1(err_code, &m, src_loc.clone());
                None
            }
        }
    }

    /// Apply a binary arithmetic operator to two resolved const values.
    /// Promotes `Integer` to match a `U64` or `I64` operand when needed.
    fn apply_binary_op(
        op: IRKind,
        lhs: ParameterValue,
        rhs: ParameterValue,
        src_loc: &SourceSpan,
        diags: &mut Diags,
    ) -> Option<ParameterValue> {
        use ParameterValue::*;
        let (lhs, rhs) = Self::coerce_numeric_pair(lhs, rhs, "IRDB_25", src_loc, diags)?;

        // Helper to emit the right diagnostic for a CalcErr and return None.
        let emit = |err: CalcErr, diags: &mut Diags| -> Option<ParameterValue> {
            match err {
                CalcErr::Overflow(msg) => {
                    diags.err1("IRDB_27", &msg, src_loc.clone());
                }
                CalcErr::DivByZero => {
                    diags.err1(
                        "IRDB_28",
                        "Division by zero in const expression",
                        src_loc.clone(),
                    );
                }
            }
            None
        };

        match lhs {
            U64(a) => {
                let b = rhs.to_u64();
                match Self::calc_u64_op(op, a, b) {
                    Ok(r) => Some(U64(r)),
                    Err(e) => emit(e, diags),
                }
            }
            I64(a) => {
                let b = rhs.to_i64();
                match Self::calc_i64_op(op, a, b) {
                    Ok(r) => Some(I64(r)),
                    Err(e) => emit(e, diags),
                }
            }
            Integer(a) => {
                let b = rhs.to_i64();
                match Self::calc_i64_op(op, a, b) {
                    Ok(r) => Some(Integer(r)),
                    Err(e) => emit(e, diags),
                }
            }
            _ => {
                let m = format!(
                    "Non-numeric type {:?} in arithmetic const expression.",
                    lhs.data_type()
                );
                diags.err1("IRDB_26", &m, src_loc.clone());
                None
            }
        }
    }

    /// Apply a comparison operator (==, !=, >=, <=, >, <) to two resolved const values.
    /// Returns U64(1) for true, U64(0) for false.
    /// Promotes `Integer` to match a `U64` or `I64` operand when needed.
    fn apply_comparison_op(
        op: IRKind,
        lhs: ParameterValue,
        rhs: ParameterValue,
        src_loc: &SourceSpan,
        diags: &mut Diags,
    ) -> Option<ParameterValue> {
        use ParameterValue::*;
        // String equality/inequality: supported for == and != only.
        if let (QuotedString(a), QuotedString(b)) = (&lhs, &rhs) {
            let result = match op {
                IRKind::DoubleEq => a == b,
                IRKind::NEq => a != b,
                _ => {
                    let m = "Ordered comparison (>=, <=) is not supported for strings.".to_string();
                    diags.err1("IRDB_30", &m, src_loc.clone());
                    return None;
                }
            };
            return Some(U64(if result { 1 } else { 0 }));
        }
        let (lhs, rhs) = Self::coerce_numeric_pair(lhs, rhs, "IRDB_29", src_loc, diags)?;

        let result = match lhs {
            U64(a) => {
                let b = rhs.to_u64();
                match op {
                    IRKind::DoubleEq => a == b,
                    IRKind::NEq => a != b,
                    IRKind::GEq => a >= b,
                    IRKind::LEq => a <= b,
                    IRKind::Gt => a > b,
                    IRKind::Lt => a < b,
                    _ => unreachable!(),
                }
            }
            I64(a) | Integer(a) => {
                let b = rhs.to_i64();
                match op {
                    IRKind::DoubleEq => a == b,
                    IRKind::NEq => a != b,
                    IRKind::GEq => a >= b,
                    IRKind::LEq => a <= b,
                    IRKind::Gt => a > b,
                    IRKind::Lt => a < b,
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        };

        Some(U64(if result { 1 } else { 0 }))
    }

    fn calc_u64_op(op: IRKind, a: u64, b: u64) -> Result<u64, CalcErr> {
        match op {
            IRKind::Add => a.checked_add(b).ok_or_else(|| {
                CalcErr::Overflow(format!("Add expression '{a} + {b}' will overflow type U64"))
            }),
            IRKind::Subtract => a.checked_sub(b).ok_or_else(|| {
                CalcErr::Overflow(format!(
                    "Subtract expression '{a} - {b}' will underflow type U64"
                ))
            }),
            IRKind::Multiply => a.checked_mul(b).ok_or_else(|| {
                CalcErr::Overflow(format!(
                    "Multiply expression '{a} * {b}' will overflow type U64"
                ))
            }),
            IRKind::Divide => a.checked_div(b).ok_or(CalcErr::DivByZero),

            IRKind::Modulo => {
                if b == 0 {
                    Err(CalcErr::DivByZero)
                } else {
                    Ok(a % b)
                }
            }
            IRKind::BitAnd => Ok(a & b),
            IRKind::BitOr => Ok(a | b),
            IRKind::LeftShift => Ok(a << (b & 63)),
            IRKind::RightShift => Ok(a >> (b & 63)),
            _ => Err(CalcErr::Overflow(
                "Unknown operator in U64 const expression".to_string(),
            )),
        }
    }

    fn calc_i64_op(op: IRKind, a: i64, b: i64) -> Result<i64, CalcErr> {
        match op {
            IRKind::Add => a.checked_add(b).ok_or_else(|| {
                CalcErr::Overflow(format!("Add expression '{a} + {b}' will overflow type I64"))
            }),
            IRKind::Subtract => a.checked_sub(b).ok_or_else(|| {
                CalcErr::Overflow(format!(
                    "Subtract expression '{a} - {b}' will underflow type I64"
                ))
            }),
            IRKind::Multiply => a.checked_mul(b).ok_or_else(|| {
                CalcErr::Overflow(format!(
                    "Multiply expression '{a} * {b}' will overflow type I64"
                ))
            }),
            IRKind::Divide => {
                if b == 0 {
                    Err(CalcErr::DivByZero)
                } else {
                    Ok(a / b)
                }
            }
            IRKind::Modulo => {
                if b == 0 {
                    Err(CalcErr::DivByZero)
                } else {
                    Ok(a % b)
                }
            }
            IRKind::BitAnd => Ok(a & b),
            IRKind::BitOr => Ok(a | b),
            IRKind::LeftShift => Ok(a << (b & 63)),
            IRKind::RightShift => Ok(a >> (b & 63)),
            _ => Err(CalcErr::Overflow(
                "Unknown operator in I64 const expression".to_string(),
            )),
        }
    }
}

/// Evaluate region property expressions and return a map of resolved bindings.
///
/// Called after const_eval::evaluate and prune, using the fully resolved symbol
/// table.  Returns Some(map) on success, None on failure (errors in diags).
pub fn evaluate_regions<'toks>(
    diags: &mut Diags,
    ast: &'toks Ast,
    ast_db: &AstDb<'toks>,
    symbol_table: &mut SymbolTable,
) -> Option<HashMap<String, RegionBinding>> {
    let mut bindings: HashMap<String, RegionBinding> = HashMap::new();
    let mut ok = true;

    for (name, region) in &ast_db.regions {
        debug!("Evaluating properties of region {}", name);
        let mut binding = RegionBinding {
            addr: 0,
            size: 0,
            name: name.clone(),
            src_loc: region.src_loc.clone(),
        };

        for prop_nid in ast.children(region.nid) {
            let tinfo = ast.get_tinfo(prop_nid);
            if tinfo.tok != LexToken::RegionProp {
                // Skip region name and structural tokens like {} around the
                // region properties.
                continue;
            }
            let prop_name = tinfo.val.to_string();
            let prop_loc = tinfo.loc.clone();
            debug!("Evaluating region {}, property {}", name, prop_name);

            // First child of RegionProp is the expression root.
            let Some(expr_nid) = ast.children(prop_nid).next() else {
                unreachable!(
                    "RegionProp has no expression child; parser guarantees one on success"
                );
            };
            let expr_loc = ast.get_tinfo(expr_nid).loc.clone();

            let mut lz = Linearizer::new();
            let mut lops: Vec<usize> = Vec::new();
            if !lz.record_expr_r(expr_nid, &mut lops, diags, ast) {
                ok = false;
                continue;
            }
            if lops.len() != 1 {
                unreachable!(
                    "record_expr_r returned {} operands for region property; \
                     parser guarantees exactly one expression node",
                    lops.len()
                );
            }
            let const_ir = ConstIR { ir_vec: lz.ir_vec, operand_vec: lz.operand_vec };
            match ConstIR::eval_const_expr_r(symbol_table, lops[0], &const_ir, diags, &prop_loc) {
                None => {
                    ok = false;
                }
                Some(val) => {
                    if val.to_bool().is_none() {
                        let msg = format!(
                            "Region property '{}' must evaluate to a numeric value.",
                            prop_name
                        );
                        diags.err1("EXEC_66", &msg, expr_loc);
                        ok = false;
                        continue;
                    }
                    match prop_name.as_str() {
                        "addr" => binding.addr = val.to_u64(),
                        "size" => binding.size = val.to_u64(),
                        _ => unreachable!("unexpected region property name '{}'", prop_name),
                    }
                }
            }
        }

        bindings.insert(name.clone(), binding);
    }

    // Validate each region's addr+size does not overflow u64.
    for (name, binding) in &bindings {
        if binding.size > 0 && binding.addr.checked_add(binding.size).is_none() {
            let msg = format!(
                "Region '{}' addr {:#X} + size {:#X} overflows u64.",
                name, binding.addr, binding.size
            );
            diags.err1("EXEC_75", &msg, binding.src_loc.clone());
            ok = false;
        }
    }

    if ok { Some(bindings) } else { None }
}

// ── AST condition evaluator for the prune pass ───────────────────────────────

/// Evaluate an AST if-condition expression against a resolved symbol table.
///
/// Called by the `prune` crate to determine which branch of a section-level
/// `if/else` to keep.  Lowers `cond_nid` to LinIR via `Linearizer::record_expr_r`,
/// then evaluates the resulting `ConstIR` with the existing `eval_const_expr_r`
/// pipeline.  Returns `Some(true/false)` on success, or `None` after emitting
/// a diagnostic on error.
pub fn eval_ast_condition(
    ast: &Ast,
    cond_nid: NodeId,
    symbol_table: &mut SymbolTable,
    diags: &mut Diags,
) -> Option<bool> {
    let src_loc = ast.get_tinfo(cond_nid).loc.clone();
    let mut lz = Linearizer::new();
    let mut lops: Vec<usize> = Vec::new();
    if !lz.record_expr_r(cond_nid, &mut lops, diags, ast) {
        return None;
    }
    if lops.len() != 1 {
        unreachable!(
            "record_expr_r returned {} operands for if condition; \
             parser guarantees exactly one expression node",
            lops.len()
        );
    }
    let const_ir = ConstIR {
        ir_vec: lz.ir_vec,
        operand_vec: lz.operand_vec,
    };
    let val = ConstIR::eval_const_expr_r(symbol_table, lops[0], &const_ir, diags, &src_loc)?;
    match val.to_bool() {
        Some(b) => Some(b),
        None => {
            diags.err1(
                "IRDB_56",
                "if condition must evaluate to a numeric type",
                src_loc,
            );
            None
        }
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Lower all const AST statements into `ConstIR`, evaluate them sequentially,
/// and return a fully resolved `SymbolTable`.
///
/// `defines` pre-populates the table with command-line `-D` values before
/// source consts are processed, allowing defines to override same-named
/// source consts.
pub fn evaluate<'toks>(
    diags: &mut Diags,
    ast: &'toks Ast,
    ast_db: &'toks AstDb,
    defines: &HashMap<String, ParameterValue>,
) -> anyhow::Result<SymbolTable> {
    debug!("const_eval::evaluate: ENTER");

    let mut lz = Linearizer::new();

    for &nid in &ast_db.const_statements {
        let tinfo = ast.get_tinfo(nid);
        match tinfo.tok {
            LexToken::Const => {
                let second_child_tok = ast.children(nid).nth(1).map(|c| ast.get_tinfo(c).tok);
                if second_child_tok == Some(LexToken::Eq) {
                    if !ConstIR::record_const_decl(&mut lz, nid, diags, ast) {
                        anyhow::bail!("const_eval lowering failed.");
                    }
                } else {
                    if !ConstIR::record_const_declare(&mut lz, nid, ast) {
                        anyhow::bail!("const_eval lowering failed.");
                    }
                }
            }
            LexToken::If => {
                if !ConstIR::record_if_else(&mut lz, nid, diags, ast) {
                    anyhow::bail!("const_eval lowering failed.");
                }
            }
            LexToken::Eq => {
                if !ConstIR::record_deferred_assign(&mut lz, nid, diags, ast) {
                    anyhow::bail!("const_eval lowering failed.");
                }
            }
            _ => {
                panic!("Unexpected token in const_statements: {:?}", tinfo.tok);
            }
        }
    }

    let const_ir = ConstIR {
        ir_vec: lz.ir_vec,
        operand_vec: lz.operand_vec,
    };

    const_ir.dump();

    // Pre-populate the symbol table with command-line defines so they are
    // available to source const expressions and can override source consts.
    let mut symbol_table = SymbolTable::new();
    for (name, value) in defines {
        symbol_table.define(name.clone(), value.clone(), None);
    }

    if !ConstIR::exec_const_statements(&const_ir, &mut symbol_table, diags) {
        anyhow::bail!("const_eval evaluation failed.");
    }

    debug!("const_eval::evaluate: EXIT");
    Ok(symbol_table)
}
