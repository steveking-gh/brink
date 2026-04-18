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

---

## 2026-04-13 -- ExtArg typed extension API (Steps 1-4)

**BrinkRangedExtension removed; ExtArg introduced**
Eliminated the two-trait extension design (`BrinkExtension` + `BrinkRangedExtension`)
in favor of a single typed-argument API.

`ExtArg<'a>` enum added to `brink_extension`:

- `Int(u64)` -- numeric arg
- `Str(&'a str)` -- quoted string arg
- `Section { start: u64, len: u64, data: &'a [u8] }` -- section name arg,
  resolved by the engine to a zero-copy mmap slice

All extensions now implement `BrinkExtension::execute<'a>(&self, args: &[ExtArg<'a>], out: &mut [u8])`.
Section-bound extensions (formerly BrinkRangedExtension) receive image data via
`args[0]` as `ExtArg::Section` when called with the section-name form.

**IRKind::ExtensionCallRanged removed**
Only `ExtensionCall` (plain args) and `ExtensionCallSection` (first arg is a
known section name) remain. `disambiguate_extension_call` now checks for section
name in first arg for all extensions, not just ranged ones.

**IRDB_47 updated**
Now allows QuotedString in addition to numeric types. String args pass IRDb
validation; extensions reject invalid types at runtime.

**IRDB_45, IRDB_46 retired**
Error codes for ranged-extension-specific constraints removed along with the
ranged extension concept.

**Engine execute_extensions updated**
Builds `Vec<ExtArg>` instead of `Vec<u64>`. Uses a block scope to isolate the
immutable `&mmap[..]` borrow held by `ExtArg::Section` before the mutable mmap
patch write.

**std extensions updated**
`std::crc32c`, `std::sha256`, `std::md5` all switch to `BrinkExtension` and
receive image data from `args[0]` as `ExtArg::Section`.

Changes:

- `brink_extension/lib.rs`: Added `ExtArg`, rewrote `BrinkExtension` trait,
  removed `BrinkRangedExtension`.
- `ext/ext.rs`: Removed `RegisteredExtension` enum; `ExtensionEntry.extension`
  is now `Box<dyn BrinkExtension>` directly. Removed `register_ranged`,
  `is_ranged`. Re-exports `ExtArg`.
- `ext/test_mocks.rs`: All 7 mocks updated to new signature; ranged mocks
  receive image via `ExtArg::Section`.
- `std/crc32c`, `std/sha256`, `std/md5`: Converted to `BrinkExtension`.
- `irdb/irdb.rs`: Removed `ExtensionCallRanged` arm; updated disambiguation
  and IRDB_47 validation.
- `ir/ir.rs`: Removed `ExtensionCallRanged` variant.
- `engine/engine.rs`: Removed `RegisteredExtension` dispatch; builds `ExtArg`
  list per call.
- `tests/integration.rs`: Removed 4 obsolete tests; updated 3 tests to reflect
  runtime-vs-compiletime error shift. 294 tests pass.

---

## 2026-04-14 -- WrExt eliminated (Step 5)

**IRKind::WrExt removed**
The two-node `ExtensionCall + WrExt` structure collapsed into a single
`ExtensionCall` node, mirroring how `Wrf` works.

Key changes:

- `IRKind::WrExt` removed from `ir/ir.rs`.  `ExtensionCall` and
  `ExtensionCallSection` are now write statements — they advance the location
  cursor and pre-pad zeroed bytes directly.
- `layoutdb/layoutdb.rs`: `wr <extension>` no longer creates a `WrExt` wrapper.
  The `ExtensionCall` LinIR produced by `record_expr_r` is the statement.
- `engine/engine.rs`:
  - `iterate_wrext` renamed to `iterate_ext`; no longer crawls backward via
    `is_output_of()`.  `ExtensionCall | ExtensionCallSection` now handled in the
    iterate match arm (previously in the no-op list).
  - `execute_core_operations`: `WrExt` arm replaced by
    `ExtensionCall | ExtensionCallSection` arm.
  - `execute_extensions`: `operand_consumer: HashMap` eliminated.  Patch offset
    now read from `ir_locs[idx]` (the extension call's own location) instead of
    `ir_locs[consumer_idx]` (the WrExt's location).
- `irdb/irdb.rs`: `WrExt` validation arm removed.

**Output operand retained**
The trailing output operand on `ExtensionCall` and `ExtensionCallSection` is
kept for type-checking: if the extension result is used in arithmetic, `wr8..64`,
`wrs`, or `const`, `DataType::Extension` propagates and IRDb rejects it.  In the
valid `wr <extension>` case the output operand is orphaned (not consumed by any
IR) but harmless.

294 tests pass.


---

## 2026-04-17 — max-output-size flag (region plan step 1)

**Fuzz bug fix**
`set_addr_offset 0xFFFFFFFFFFFFF; wrs "123";` caused a hang in `execute_wrx`
(petabyte-range pad count).  Fix: pre-execute size check in `process.rs` using
`engine.wr_dispatches.last().map_or(0, |d| d.file_offset + d.size)`.

**New CLI flag**
`--max-output-size SIZE` (default 256M, accepts K/M/G suffix).  Parsed by
`parse_size()` in `src/main.rs`.  Passed as new `max_output_size: u64` parameter
to `process()`.  Rejects before `execute()` with PROC_7.

**New error code**
PROC_7: output image size exceeds `--max-output-size` limit.

**Fuzz target**
`process/fuzz/fuzz_targets/fuzz_target_1.rs` updated to pass `max_output_size`
(64 KiB for fast fuzzing).

**Regression test**
`tests/fuzz_found_19.brink` added to integration suite; expects PROC_7.
`max_output_size_flag` inline test: `--max-output-size 0` on 1-byte output.

---

## 2026-04-17 — fuzz fixes and to_bool() hardening

**fuzz_found_20: wrf extra args (AST_42)**
`wrf(f), "n"` caused IRDb assert.  Parser now rejects extra args after the
first wrf argument with AST_42 and advances past `;`.

**fuzz_found_21: wr32 extra args (IRDB_55)**
`wr32 a, b, (12)` caused IRDb assert.  `validate_numeric_1_or_2` replaced
`assert!` with graceful IRDB_55 diagnostic.  Covers all Wr(n), Align,
SetAddr*, SetSecOffset, SetFileOffset.

**fuzz_found_22: if with string condition (IRDB_56)**
`const MODE = ""; if MODE { }` panicked in `to_bool()`.  Fixed two call
sites in `const_eval.rs` (IfBegin arm and eval_ast_condition) with IRDB_56.

**to_bool() Option A hardening**
Changed `ParameterValue::to_bool()` return type from `bool` to `Option<bool>`.
All five call sites updated:

- `engine/engine.rs` execute_assert: `expect()` (invariant protected by IRDb)
- `const_eval.rs` IfBegin: `and_then` with IRDB_56
- `const_eval.rs` Assert in if/else body: `and_then` with IRDB_57
- `const_eval.rs` LogicalAnd/Or: let-else with IRDB_58
- `const_eval.rs` eval_ast_condition: match on `to_bool()` with IRDB_56

**New error codes**
IRDB_57: assert condition in if/else body is non-numeric.
IRDB_58: `&&`/`||` operand is non-numeric.

**New test files**
`fuzz_found_20.brink`, `fuzz_found_21.brink`, `fuzz_found_22.brink`,
`const_bool_string_assert.brink`, `const_bool_string_and.brink`,
`const_bool_string_or.brink`.

**New error code registry**
`docs/error-codes.md`: unified list of all error codes by prefix with
next-available summary line.

312 tests pass.

---

## 2026-04-18 -- Recursive depth guards (AST_43, AST_44, IRDB_59)

**parse_pratt depth guard (AST_43)**
Added `MAX_PRATT_DEPTH = 200` check at the top of `parse_pratt`.  All recursive
call sites inside `parse_pratt` pass `depth + 1`; top-level entry points pass `0`.

**parse_if_r / parse_if_body_r mutual recursion guard (AST_44)**
Added `MAX_IF_DEPTH = 100` check at the top of `parse_if_r`.  `parse_if_body_r`
carries the depth parameter and passes `depth + 1` when recursing into nested `if`.

**parse_function_args depth threading (root cause of fuzz SIGSEGV)**
`parse_function_args` was calling `parse_pratt(0, 0, ...)` internally, resetting
the depth counter to zero and bypassing AST_43.  Fixed by threading `depth` through
`parse_function_args` and passing `depth + 1` at both call sites inside `parse_pratt`
and both internal `parse_pratt` calls within `parse_function_args`.

**eval_const_expr_r depth guard (IRDB_59)**
Added `MAX_EVAL_DEPTH = 100` check (matching `Linearizer::MAX_RECURSION_DEPTH`) to
`eval_const_expr_r`.  Adds `depth: usize` parameter threaded through all 7 internal
recursive calls.  Six external call sites pass `0`.  For inputs going through the
linearizer, LINEAR_1 fires first; IRDB_59 provides defense-in-depth for eval paths
that bypass the linearizer.

**Error codes**
AST_43, AST_44, IRDB_59 added to `docs/error-codes.md`.
Next available: AST_45, EXEC_62, IR_5, IRDB_60, LINEAR_18, PROC_8, SYMTAB_5.

Regression tests: `fuzz_found_23` (250-level `f(f(...))` fires AST_43),
`fuzz_found_24` (5000-term flat `1 + 1 + ...` fires LINEAR_1).

314 tests pass.
