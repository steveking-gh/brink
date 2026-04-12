// Prune pass: eliminate if/else nodes from section bodies.
//
// This pass runs after const_eval and before LayoutDb.  It walks every section
// in the AST, finds if/else nodes whose conditions are const expressions, evaluates
// those conditions against the fully-resolved SymbolTable, and replaces each if/else
// node with the statements from the taken branch.
//
// The original AST is left untouched.  We make a clone, prune it, and
// return the clone.  The original Ast and AstDb remain valid for debugging (e.g. ast.dump).
//
// Top-level if/else blocks (in const_statements) are NOT pruned here — they are
// fully handled by const_eval already.  Only section-body if/else nodes are pruned.

use anyhow::bail;
use ast::{Ast, AstDb, LexToken};
use diags::Diags;
use symtable::SymbolTable;

#[allow(unused_imports)]
use tracing::debug;

/// Clone `ast`, remove all if/else nodes from section bodies (replacing each
/// with the statements from the taken branch), and return the pruned clone.
///
/// Conditions must be fully resolvable from `symbol_table`; any unresolvable
/// condition is a compile-time error.
pub fn prune<'toks>(
    ast: &Ast<'toks>,
    ast_db: &AstDb<'toks>,
    symbol_table: &mut SymbolTable,
    diags: &mut Diags,
) -> anyhow::Result<Ast<'toks>> {
    debug!("prune::prune: ENTER");
    let mut pruned = ast.clone();

    // Prune each section independently.
    for (_name, section) in &ast_db.sections {
        prune_section_body(&mut pruned, section.nid, symbol_table, diags)?;
    }

    debug!("prune::prune: EXIT");
    Ok(pruned)
}

/// Prune all if/else nodes that are direct or promoted children of `parent_nid`.
///
/// Uses a loop-until-no-more-ifs approach: after each prune the children snapshot
/// is refreshed so promoted if nodes (from else-if chains) are caught on the next
/// iteration.
fn prune_section_body(
    pruned: &mut Ast,
    parent_nid: indextree::NodeId,
    symbol_table: &mut SymbolTable,
    diags: &mut Diags,
) -> anyhow::Result<()> {
    loop {
        let children: Vec<_> = pruned.children(parent_nid).collect();
        let maybe_if = children
            .iter()
            .find(|&&nid| pruned.get_tinfo(nid).tok == LexToken::If)
            .copied();
        match maybe_if {
            None => break,
            Some(if_nid) => prune_if_node(pruned, if_nid, symbol_table, diags)?,
        }
    }
    Ok(())
}

/// Evaluate the condition of `if_nid`, promote the taken branch's statements
/// as preceding siblings of `if_nid`, then detach `if_nid` from the tree.
fn prune_if_node(
    pruned: &mut Ast,
    if_nid: indextree::NodeId,
    symbol_table: &mut SymbolTable,
    diags: &mut Diags,
) -> anyhow::Result<()> {
    let children: Vec<_> = pruned.children(if_nid).collect();
    // Structure: [cond_expr, {, then_stmt*, }, [else, ({ | if), else_stmt*, }]?]

    if children.len() < 3 {
        unreachable!("Malformed if node during pruning (too few children)");
    }

    let cond_nid = children[0];

    // Evaluate the const condition.
    let cond_val = match const_eval::eval_ast_condition(pruned, cond_nid, symbol_table, diags) {
        Some(v) => v,
        None => bail!("Prune: if condition could not be evaluated as a const expression"),
    };

    debug!("prune_if_node: condition = {}", cond_val);

    // Find the closing brace of the then-branch: first CloseBrace in children[2..].
    let then_close_idx = children[2..]
        .iter()
        .position(|&n| pruned.get_tinfo(n).tok == LexToken::CloseBrace)
        .map(|i| i + 2)
        .unwrap_or_else(|| unreachable!("Malformed if node: no closing brace for then-branch"));

    // then-body statements are children[2..then_close_idx]
    let then_stmts: Vec<_> = children[2..then_close_idx].to_vec();

    // Determine else-body statements (if any).
    // After the then-close brace: [else (leaf), { or if, [stmts,] }?]
    let else_stmts: Vec<_> = if then_close_idx + 1 < children.len() {
        // children[then_close_idx + 1] == else (leaf)
        let after_else_idx = then_close_idx + 2;
        if after_else_idx >= children.len() {
            vec![]
        } else {
            let after_else_nid = children[after_else_idx];
            if pruned.get_tinfo(after_else_nid).tok == LexToken::If {
                // `else if ...` — the nested if node is the entire else body.
                vec![after_else_nid]
            } else {
                // `else { stmts }` — after_else_nid is `{`, find matching `}`.
                let else_close_idx = children[after_else_idx + 1..]
                    .iter()
                    .position(|&n| pruned.get_tinfo(n).tok == LexToken::CloseBrace)
                    .map(|i| i + after_else_idx + 1)
                    .unwrap_or_else(|| unreachable!("Malformed if node: no closing brace for else-branch"));
                children[after_else_idx + 1..else_close_idx].to_vec()
            }
        }
    } else {
        vec![]
    };

    // Choose which statements to promote.
    let stmts_to_promote = if cond_val { &then_stmts } else { &else_stmts };

    // Detach each selected statement from if_nid and insert it before if_nid.
    for &stmt_nid in stmts_to_promote {
        stmt_nid.detach(pruned.arena_mut());
        if_nid.insert_before(stmt_nid, pruned.arena_mut());
    }

    // Remove if_nid from the tree entirely.
    if_nid.detach(pruned.arena_mut());
    Ok(())
}
