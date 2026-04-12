# Update Tracker

Meaningful changes only. Format: `YYYY-MM-DD — title — brief description`.

---

## 2026-04-11 — Top-level if/else section support

**Strategy B — eval_ast_condition refactor**
Eliminated `eval_ast_expr` (~140 lines). `eval_ast_condition` now reuses
`Linearizer::record_expr_r` + `ConstIR::eval_const_expr_r`. Symbol table
parameter changed from `&SymbolTable` to `&mut SymbolTable` throughout
(`const_eval`, `prune`, `process`).

**unreachable! conversion**
Three structural invariant `bail!` calls in `prune/prune.rs` converted to
`unreachable!()`. User-reachable failure paths retain `bail!`.

**Top-level if/else section support**
Sections may now be defined inside top-level `if/else` blocks.

Changes:
- `ast/ast.rs`: `ParseIfContext` enum (`TopLevel | Section`) threads through
  `parse_if_r` and `parse_if_body_r`. `TopLevel` context allows `Section`
  token in if body. `AstDb::new` gains `validate: bool` parameter; nesting
  validation skipped when `false`.
- `prune/prune.rs`: Removed `AstDb` dependency. Added `keep: fn(LexToken)->bool`
  filter. Root-level prune keeps `Section | If` only; section-body prune keeps
  all. Two-step prune: root if/else first, then section-body if/else.
- `process/process.rs`: Two AstDb passes — `validate=false` before const_eval,
  `validate=true` on pruned AST before LayoutDb.
- `layoutdb/fuzz/fuzz_targets/fuzz_target_1.rs`: Updated `AstDb::new` call to
  three-arg signature.
- 4 new test fixtures: `toplevel_if_section_true.brink`,
  `toplevel_if_section_false.brink`, `toplevel_if_else_section.brink`,
  `toplevel_if_nested_if.brink`.
- 4 new integration tests in `tests/integration.rs`.
- All 286 tests pass.

**AI context files created**
`docs/ai/01-meta.yaml`, `02-system.yaml`, `03-structure.yaml`, `04-memory.yaml`,
`05-update-tracker.md` (this file). Added `.claude/CLAUDE.md` workspace
instructions.
