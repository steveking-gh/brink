// AST to linear IR lowering for const-time statements.
//
// ConstDb walks the AST top-level const statements and flattens them into
// two parallel vectors: a sequence of LinIR instructions and a sequence of
// LinOperand operands. This includes const declarations, if/else blocks, and
// deferred assignments.

use diags::Diags;
use indextree::NodeId;

#[allow(unused_imports)]
use tracing::{debug, trace};

use ast::{Ast, AstDb, LexToken};
use ir::IRKind;

use linearizer::{LinIR, LinOperand, Linearizer, tok_to_irkind};

pub struct ConstDb {
    pub ir_vec: Vec<LinIR>,
    pub operand_vec: Vec<LinOperand>,
}

impl ConstDb {
    pub fn dump(&self) {
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
                    LinOperand::Literal { sval, .. } => op.push_str(&format!(" {}", sval)),
                    LinOperand::Output { ir_lid, .. } => {
                        op.push_str(&format!(" tmp{}, output of lid {}", *child, ir_lid))
                    }
                }
            }
            debug!("ConstDb: {}", op);
        }
    }
}

// Need 'toks lifetime because the constdb methods borrow data tied
// to the AST token lifespan.  Once constructed, we don't delete the AST
// so the lifetime is effectively infinite, but we still need the formality.
impl<'toks> ConstDb {
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
        lz.operand_vec.push(LinOperand::new_literal(name_tinfo));
        lz.add_existing_operand_to_ir(ir_lid, name_idx);

        // Child 1: `=` sign
        let eq_nid = children.next().unwrap();
        let eq_tinfo = ast.get_tinfo(eq_nid);

        // Child 2: RHS expression
        let rhs_nid = children.next().unwrap();
        let mut rhs_lops = Vec::new();
        if !lz.record_expr_r(1, rhs_nid, &mut rhs_lops, diags, ast) {
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
        lz.add_new_operand_to_ir(ir_lid, LinOperand::new_literal(name_tinfo));
        true
    }

    /// Lower a deferred assignment `IDENT = expr;` (inside an if/else body).
    fn record_deferred_assign(
        lz: &mut Linearizer,
        eq_nid: NodeId,
        rdepth: usize,
        diags: &mut Diags,
        ast: &'toks Ast,
    ) -> bool {
        let mut children = ast.children(eq_nid);
        let ident_nid = children.next().unwrap();
        let expr_nid = children.next().unwrap();
        let ident_tinfo = ast.get_tinfo(ident_nid);

        let ir_lid = lz.new_ir(eq_nid, ast, IRKind::BareAssign);
        lz.add_new_operand_to_ir(ir_lid, LinOperand::new_literal(ident_tinfo));

        let mut rhs_lops = Vec::new();
        if !lz.record_expr_r(rdepth + 1, expr_nid, &mut rhs_lops, diags, ast) {
            return false;
        }
        if rhs_lops.len() != 1 {
            let m = format!(
                "Deferred assignment RHS produced {} result(s), expected 1",
                rhs_lops.len()
            );
            diags.err1("LINEAR_14", &m, ast.get_tinfo(eq_nid).span());
            return false;
        }
        lz.add_existing_operand_to_ir(ir_lid, rhs_lops[0]);
        true
    }

    /// Dispatch a single statement inside an if/else body.
    fn record_if_body_stmt(
        lz: &mut Linearizer,
        stmt_nid: NodeId,
        rdepth: usize,
        diags: &mut Diags,
        ast: &'toks Ast,
    ) -> bool {
        let tinfo = ast.get_tinfo(stmt_nid);
        match tinfo.tok {
            LexToken::Eq => Self::record_deferred_assign(lz, stmt_nid, rdepth, diags, ast),
            LexToken::Print | LexToken::Assert => {
                let mut lops = Vec::new();
                lz.record_expr_children_r(rdepth, stmt_nid, &mut lops, diags, ast);
                let ir_lid = lz.new_ir(stmt_nid, ast, tok_to_irkind(tinfo.tok));
                for idx in lops {
                    lz.add_existing_operand_to_ir(ir_lid, idx);
                }
                true
            }
            LexToken::If => Self::record_if_else(lz, stmt_nid, rdepth, diags, ast),
            _ => true, // syntactic tokens already filtered by parser
        }
    }

    /// Lower an `if/else` block into ir_vec.
    fn record_if_else(
        lz: &mut Linearizer,
        if_nid: NodeId,
        rdepth: usize,
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
        if !lz.record_expr_r(rdepth + 1, cond_nid, &mut cond_lops, diags, ast) {
            return false;
        }
        if cond_lops.len() != 1 {
            let tinfo = ast.get_tinfo(if_nid);
            diags.err1(
                "LINEAR_15",
                "if condition must produce exactly one value",
                tinfo.span(),
            );
            return false;
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
            result &= Self::record_if_body_stmt(lz, children[i], rdepth + 1, diags, ast);
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
                    result &= Self::record_if_else(lz, children[i], rdepth + 1, diags, ast);
                } else if next_tok == LexToken::OpenBrace {
                    i += 1; // skip '{'
                    while i < children.len() {
                        let tok = ast.get_tinfo(children[i]).tok;
                        if tok == LexToken::CloseBrace {
                            break;
                        }
                        result &= Self::record_if_body_stmt(
                            lz,
                            children[i],
                            rdepth + 1,
                            diags,
                            ast,
                        );
                        i += 1;
                    }
                }
            }
        }

        lz.new_ir(if_nid, ast, IRKind::IfEnd);
        result
    }

    pub fn new(diags: &mut Diags, ast: &'toks Ast, ast_db: &'toks AstDb) -> anyhow::Result<Self> {
        debug!("ConstDb::new: ENTER");

        let mut lz = Linearizer::new();

        for &nid in &ast_db.const_statements {
            let tinfo = ast.get_tinfo(nid);
            match tinfo.tok {
                LexToken::Const => {
                    let second_child_tok = ast.children(nid).nth(1).map(|c| ast.get_tinfo(c).tok);
                    if second_child_tok == Some(LexToken::Eq) {
                        if !Self::record_const_decl(&mut lz, nid, diags, ast) {
                            anyhow::bail!("ConstDb construction failed.");
                        }
                    } else {
                        if !Self::record_const_declare(&mut lz, nid, ast) {
                            anyhow::bail!("ConstDb construction failed.");
                        }
                    }
                }
                LexToken::If => {
                    if !Self::record_if_else(&mut lz, nid, 1, diags, ast) {
                        anyhow::bail!("ConstDb construction failed.");
                    }
                }
                LexToken::Eq => {
                    if !Self::record_deferred_assign(&mut lz, nid, 0, diags, ast) {
                        anyhow::bail!("ConstDb construction failed.");
                    }
                }
                _ => {
                    panic!("Unexpected token in const_statements: {:?}", tinfo.tok);
                }
            }
        }

        let const_db = ConstDb {
            ir_vec: lz.ir_vec,
            operand_vec: lz.operand_vec,
        };

        const_db.dump();
        debug!("ConstDb::new: EXIT");
        Ok(const_db)
    }
}
