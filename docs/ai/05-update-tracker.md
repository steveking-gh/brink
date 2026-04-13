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

---

## 2026-04-12 -- engine.rs refactor and correctness fixes

**Location::advance extraction**
Extracted `Location::advance(&mut self, sz, src_loc, diags) -> bool` to
deduplicate checked-arithmetic counter advance across all four iterate helpers
(`iterate_wrs`, `iterate_wrext`, `iterate_wrx`, `iterate_wrf`).  All write
overflow paths now emit EXEC_37 (file-offset overflow) or EXEC_43 (absolute
address overflow).  Retired per-helper codes EXEC_41, EXEC_60, EXEC_40.

**wrext overflow fix**
`iterate_wrext` now uses checked arithmetic via `Location::advance`.  Added
`MockHugeExt` (size = usize::MAX) in `ext/test_mocks.rs` and integration test
`wrext_overflow`.

**is_none + unwrap antipattern removal**
Replaced eight `is_none()`/`unwrap()` pairs across `engine.rs`, `ast/ast.rs`,
`irdb/irdb.rs`, and `symtable/symtable.rs` with `let Some(x) = ... else`
let-else idiom.

**execute_assert cleanup**
Removed the unnecessary `mut result` variable; failure branch now returns
`Err(...)` directly; success path returns `Ok(())`.

**build_dispatches: checked arithmetic + diags**
Added `diags: &mut Diags` parameter to `build_dispatches`.  Replaced two
`saturating_add` calls with `checked_add`; overflow emits EXEC_60 (section
address) or EXEC_61 (label address).

**execute_extensions: O(N^2) -> O(N) lookups**
Built two maps before the extension execution loop:

- `operand_consumer: HashMap<usize, usize>` -- resolves each extension call's
  consuming IR in O(1) instead of scanning ir_vec per call.
- `sec_dispatch_map: HashMap<&str, Vec<usize>>` -- resolves section dispatch
  for ExtensionCallSection in O(1) instead of two linear scans per call.

**CLAUDE.md / docs/ai ingestion**
Renamed `.claude/claude.md` to `.claude/CLAUDE.md`.  Corrected `repo_root` in
`03-structure.yaml` from Windows to WSL path.  Updated test count to 291.

All 291 tests pass.
