// Prune pass: eliminate if/else nodes from the AST.
//
// This pass runs after const_eval and before LayoutDb.  It performs two steps:
//
//   1. Top-level prune — walks the root's direct children, finds if/else nodes,
//      evaluates their conditions, and promotes only Section (and nested If) nodes
//      from the taken branch.  Non-section content (deferred assigns, print, assert)
//      has already been handled by const_eval and is silently discarded.
//
//   2. Section-body prune — for every Section node now visible at root level
//      (both unconditional sections and those just promoted in step 1), walks the
//      section body and replaces any if/else nodes with the statements from the
//      taken branch.
//
// The original AST is left untouched: a clone is made, pruned, and returned.
// The original Ast and AstDb remain valid for debugging (e.g. ast.dump).

use anyhow::bail;
use ast::{Ast, LexToken};
use diags::Diags;
use symtable::SymbolTable;

#[allow(unused_imports)]
use tracing::debug;

/// Clone `ast`, prune all if/else nodes at root level and in section bodies,
/// and return the pruned clone.
///
/// Top-level if/else blocks may contain `section` definitions.  Only Section
/// nodes (and nested If nodes that may themselves contain sections) are promoted
/// from the taken branch; const-only statements are silently dropped since
/// `const_eval` has already evaluated them.
///
/// Conditions must be fully resolvable from `symbol_table`; any unresolvable
/// condition is a compile-time error.
pub fn prune<'toks>(
    ast: &Ast<'toks>,
    symbol_table: &mut SymbolTable,
    diags: &mut Diags,
) -> anyhow::Result<Ast<'toks>> {
    debug!("prune::prune: ENTER");
    let mut pruned = ast.clone();

    // Step 1: prune top-level if/else, keeping only Section and nested If nodes.
    // root_nid is a Copy value captured before the &mut pruned borrow begins.
    let root_nid = pruned.root();
    prune_body(
        &mut pruned,
        root_nid,
        symbol_table,
        diags,
        |tok| matches!(tok, LexToken::Section | LexToken::If),
    )?;

    // Step 2: prune if/else inside every section body now at root level.
    // This covers both unconditional sections and those promoted in step 1.
    let section_nids: Vec<_> = pruned
        .children(pruned.root())
        .filter(|&nid| pruned.get_tinfo(nid).tok == LexToken::Section)
        .collect();
    for sec_nid in section_nids {
        prune_body(&mut pruned, sec_nid, symbol_table, diags, |_| true)?;
    }

    debug!("prune::prune: EXIT");
    Ok(pruned)
}

/// Prune all if/else nodes that are direct children of `parent_nid`.
///
/// `keep` controls which statement tokens are promoted from the taken branch.
/// Uses a loop-until-no-more-ifs approach so that promoted If nodes (from
/// else-if chains or nested top-level ifs) are caught on the next iteration.
fn prune_body(
    pruned: &mut Ast,
    parent_nid: indextree::NodeId,
    symbol_table: &mut SymbolTable,
    diags: &mut Diags,
    keep: fn(LexToken) -> bool,
) -> anyhow::Result<()> {
    loop {
        let children: Vec<_> = pruned.children(parent_nid).collect();
        let maybe_if = children
            .iter()
            .find(|&&nid| pruned.get_tinfo(nid).tok == LexToken::If)
            .copied();
        match maybe_if {
            None => break,
            Some(if_nid) => prune_if_node(pruned, if_nid, symbol_table, diags, keep)?,
        }
    }
    Ok(())
}

/// Evaluate the condition of `if_nid`, promote statements from the taken branch
/// that satisfy `keep`, then detach `if_nid` from the tree.
fn prune_if_node(
    pruned: &mut Ast,
    if_nid: indextree::NodeId,
    symbol_table: &mut SymbolTable,
    diags: &mut Diags,
    keep: fn(LexToken) -> bool,
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
                    .unwrap_or_else(|| {
                        unreachable!("Malformed if node: no closing brace for else-branch")
                    });
                children[after_else_idx + 1..else_close_idx].to_vec()
            }
        }
    } else {
        vec![]
    };

    // Choose which statements to promote.
    let stmts_to_promote = if cond_val { &then_stmts } else { &else_stmts };

    // Detach each kept statement from if_nid and insert it before if_nid.
    for &stmt_nid in stmts_to_promote {
        if keep(pruned.get_tinfo(stmt_nid).tok) {
            stmt_nid.detach(pruned.arena_mut());
            if_nid.insert_before(stmt_nid, pruned.arena_mut());
        }
    }

    // Remove if_nid from the tree entirely.
    if_nid.detach(pruned.arena_mut());
    Ok(())
}
