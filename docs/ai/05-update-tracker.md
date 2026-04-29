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

---

## 2026-04-21 — Remove output address argument (region plan step 2)

**output statement address removed**
`output <section> <addr>;` syntax removed.  Use `set_addr <addr>;` as the first
statement inside the section instead.  Old syntax emits AST_55.

**Pipeline cleanup**
Removed: `Output.addr_nid`, `LayoutDb.output_addr_str/loc`,
`IRDb.start_addr`, `Engine.start_addr`, `Engine::new abs_start` param,
`Engine::iterate abs_start` param.  `MapDb.base_addr` now derived from
the first WrDispatch address rather than `engine.start_addr`.

**build_dispatches: leading set_addr skipped for section address**
When a section starts with one or more `set_addr` instructions,
`build_dispatches` now scans past them (to `content_idx`) so the
WrDispatch `addr` field reflects the address after the first `set_addr`
rather than the address at SectionStart (before set_addr runs).
This keeps map output consistent with what `addr()` reads inside the section.

**`__OUTPUT_ADDR` behavior**
`__OUTPUT_ADDR` is now always `addr(<output-section>)`, which equals 0 at
SectionStart before any `set_addr`.  It no longer tracks a separate base set
in the output statement.

**New error code**
AST_55: `output` address argument; use `set_addr` or region binding.

**Test migration**
~80 `.brink` test files updated.  Zero-address files: remove address.
Non-zero-address files: remove address + add `set_addr <addr>;` as first
statement inside the output section.  Special cases: `wrx_4` expected byte
values recalculated with `addr(foo)=0`; `output_addr_1` and
`output_addr_section_set_addr_with_output_base` repurposed to test
`__OUTPUT_ADDR == 0`; `const_as_output_addr_1` uses `set_addr LOAD_ADDR;`.

320 tests pass.

---

## 2026-04-25 — validation_phase extraction + region plan Steps 3+4 AST layer

**validation_phase crate**
Extracted `execute_validation`, `execute_assert`, `assert_info`, and
`assert_info_operand` from `exec_phase` into a new `validation_phase` crate.
`process.rs` now calls `ValidationPhase::validate` between `LayoutPhase::build`
and `ExecPhase::execute` (error code PROC_8).  Assert evaluation confirmed
independent of generated bytes: `ParmValDb` is immutable during exec and no IR
instruction reads back output-file bytes.

**Region plan Steps 3+4 — AST layer**
Lexer, parser, and AstDb changes for region declarations and section bindings.

New tokens:

- `LexToken::Region`, `LexToken::In` — keywords (added to lexer scan_word).
- `LexToken::RegionProp` — synthetic; children of Region root, val = property
  name ("addr", "size", "default_align", "default_fill"), one expression child.
- `LexToken::RegionRef` — synthetic; second child of Section root when
  `section NAME in REGION` is parsed, val = region name, no children.

New types:

- `RegionEntry` in `ast` crate: holds `nid`, `src_loc`, `addr`, `size`,
  `default_align` (sentinel 1), `default_fill` (sentinel 0xFF).
  `addr`/`size` filled by const_eval (Step 3 const_eval, not yet done).

Parser changes:

- `parse_region` / `parse_region_contents`: parse `region NAME { prop = expr; }`.
- `parse_section`: optional `in REGION` binding after section name.

AstDb changes:

- `AstDb::regions: HashMap<String, RegionEntry>` added.
- `record_region`: reserved name (AST_61), duplicate name (AST_60).
- `AstDb::new`: Region nodes processed at root; region/section/const name
  conflicts detected (AST_48, AST_63); section region-ref validation (AST_56,
  AST_57).
- `Section::region: Option<String>` added; set from RegionRef child if present.

Linearizer: `RegionRef` added to syntactic noise (no IR emitted).

New error codes: AST_45–49, AST_56–64, PROC_8.

**Region integration tests**
15 new test fixtures and functions covering all region error codes (AST_45–64)
plus one success case (`region_valid`).

**Bug fix: infinite loop in AST_45/AST_46 paths**
`parse_region_contents` error paths for unknown and duplicate property names
called `advance_past_semicolon()` while the property name token was still the
current (not yet consumed) token.  `advance_past_semicolon` checked the
previous token, saw a `;` from the preceding valid property, and did nothing,
looping forever.  Fix: `self.tv.skip()` before `advance_past_semicolon()` in
both paths.

334 tests pass.

---

## 2026-04-26 — Region plan Step 3 evaluation (address anchor)

**const_eval::evaluate_regions**
New public function in `const_eval/const_eval.rs`.  Called in `process.rs` after
`AstDb::new(true)` and before `LayoutDb::new`.  Walks each RegionProp child of
each Region node, lowers the expression subtree with `Linearizer::record_expr_r`,
evaluates with `ConstIR::eval_const_expr_r`, and stores `addr`, `size`,
`default_align`, `default_fill` into the matching `RegionEntry`.  Emits EXEC_66
for non-numeric property values.

**Section address anchoring in LayoutPhase**
`LayoutPhase` gains `section_anchors: HashMap<String, u64>` (section name to
region addr).  `build()` accepts this map as a new parameter.  `process.rs`
builds it from the resolved `pruned_ast_db` after `evaluate_regions` and passes
it to `LayoutPhase::build`.

`iterate_section_start` applies the anchor for region-bound sections:
sets `current.addr.addr_base = anchor` and `current.addr.addr_offset = 0`.
`ir_locs[lid]` is re-recorded after the anchor is applied so that
`addr(section_name)` returns the region-anchored address, not the pre-entry
parent address.

**New error codes**
EXEC_66: region property value is not numeric.
PROC_9: evaluate_regions failure halt.

**Tests**
`region_anchor.brink`: writes `addr(top)` as `wr32`; verifies bytes equal
`0x08000000` LE, confirming the anchor is applied correctly.
`region_exec66.brink`: string-valued region property triggers EXEC_66.

336 tests pass.

---

## 2026-04-26 — Region plan Steps 4-5 (RegionBinding, size enforcement, EXEC_70)

**RegionBinding on IRDb**
Removed `section_anchors: HashMap<String, u64>` from `LayoutPhase`.  Added
`RegionBinding { addr: u64, size: u64 }` to `ir/ir.rs` and
`section_regions: HashMap<String, RegionBinding>` to `IRDb`.  Built in
`process.rs` from `pruned_ast_db.sections` and `pruned_ast_db.regions`.
All downstream phases read region data from `irdb.section_regions`.

**default_align and default_fill removed**
These region properties were unimplemented and had no planned use in the
near term.  Removed from `RegionEntry`, `parse_region_contents`, and
`evaluate_regions`.  AST_45 message updated to "expected addr or size".

**Region size enforcement (EXEC_72, EXEC_73, EXEC_74)**
`iterate_set_addr` in `layout_phase.rs` now accepts `irdb`, `lid`, and
`diags`.  For region-bound sections:

- EXEC_72: `set_addr` target is outside `[binding.addr, binding.addr+binding.size)`.
- EXEC_74: `binding.addr + binding.size` overflows u64 (uses `checked_add`).

`validate_section_regions` called after `iterate` converges:

- EXEC_73: `sizeof(section)` exceeds `binding.size`.

Deduplication via `warned_lids: HashSet<(usize, &'static str)>` prevents
repeated errors across iterate passes.

**Overlapping region detection (EXEC_70)**
`evaluate_regions` now checks all region pairs after property evaluation.
Two regions with non-zero size that share any address range emit EXEC_70.
`region_nested_2.brink` converted from a success fixture to an EXEC_70
error fixture.

**New error codes**
EXEC_70: overlapping regions.
EXEC_72: set_addr target outside region bounds.
EXEC_73: section size exceeds region size.
EXEC_74: region addr + size overflows u64.

342 tests pass.

---

## 2026-04-27 — K/M/G magnitude suffix for integer literals

Decimal integer literals now accept an optional K/M/G magnitude suffix before
the type suffix: `64K` = 65536, `1M` = 1048576, `2Gu` = U64(2147483648),
`-1Ki` = I64(-1024).

Full token forms: `<digits>[K|M|G][u|i]?` (decimal only; hex/binary unchanged).

Changes:

- `ast/lexer.rs`: `consume_decimal_suffix` checks for K/M/G before u/i.
  `scan_negative_i64` checks for K/M/G before the optional `i` suffix.
- `ir/ir.rs`: `strip_kmg(&str) -> (&str, u64)` added as a public free function.
  All three numeric arms of `IROperand::convert_type` (U64, I64, Integer) call
  `strip_kmg` after stripping the type suffix and apply `checked_mul` for
  overflow detection.
- `const_eval/const_eval.rs`: imports `strip_kmg` from `ir`. All three integer
  arms of the inline literal evaluator (Integer/IRDB_22, U64/IRDB_23, I64/IRDB_24)
  apply `strip_kmg` + `checked_mul`. Overflow is reported via the existing
  IRDB_22/23/24 codes.
- 5 new integration tests: `kmg_suffix_1`, `kmg_suffix_2`, `kmg_suffix_3`,
  `kmg_overflow_u64`, `kmg_overflow_i64`.
- `README.md`: region size example updated to use `64K` and `1M`.

347 tests pass.

---

## 2026-04-28 — addr(REGION) and sizeof(REGION)

`addr()` and `sizeof()` now accept region names in addition to section names.

**Design**
Same `IRKind::Addr` / `IRKind::Sizeof` and same identifier operand structure as
sections.  Regions provide their values from `irdb.region_bindings` (const-
evaluated, stable across layout iterations) rather than from `ir_locs[]`.

Changes:

- `layoutdb/layoutdb.rs`: `LayoutDb` gains `region_names: HashSet<String>`,
  populated from `ast_db.regions` in `new()`.  `IdentDb::verify_operand_refs`
  checks `lindb.region_names` before emitting LINEAR_6, so region names pass
  the identifier validation gate.
- `irdb/irdb.rs`: `IRDb` gains `region_bindings: HashMap<String, RegionBinding>`
  (keyed by region name, parallel to `section_regions` which is keyed by section
  name).  `IRDb::new` accepts and stores this map.
- `process/process.rs`: passes `region_bindings` (returned by `evaluate_regions`,
  not consumed when building `section_regions`) to `IRDb::new`.
- `layout_phase/layout_phase.rs`:
  - `iterate_sizeof`: falls back to `irdb.region_bindings` when the name is not
    in `sized_locs`; returns `binding.size`.  EXEC_5 message updated to mention
    regions.  EXEC_52 message updated to "section or region names".
  - `iterate_identifier_address`: restructured into three paths — section/label,
    region, not-found.  For regions: `addr(REGION)` returns `binding.addr`;
    `addr_offset`/`sec_offset`/`file_offset` applied to a region emit EXEC_76.
    EXEC_11 message updated to "section, label, or region".
- New error code EXEC_76: offset-style address operation applied to a region name.
- `tests/region_addr_sizeof.brink`: verifies `addr()` and `sizeof()` on two
  regions with asserts and `wr32` output bytes.
- 1 new integration test: `region_addr_sizeof`.

348 tests pass.

---

## 2026-04-27 — RegionBinding diagnostic fields (name + src_loc)

`RegionBinding` gained two new fields — `name: String` and `src_loc: SourceSpan`
— so every region error site can name the region and point at its declaration
without extra lookups.  `Copy` dropped (String prevents it); `Clone` retained.

Changes:

- `ir/ir.rs`: `RegionBinding` now carries `name` and `src_loc`.  `Copy` derive removed.
- `const_eval/const_eval.rs`: binding constructor populates `name` and `src_loc`.
  EXEC_75 message includes region name, points at declaration site.
  EXEC_70 upgraded from `err1` to `err2`; both overlapping declarations highlighted.
  Redundant `name_a`/`name_b` destructures in the overlap loop replaced with `_`.
- `process/process.rs`: `*binding` (copy) changed to `binding.clone()`.
- `layout_phase/layout_phase.rs`:
  - EXEC_72 upgraded to `err2`; message includes region name; second label at declaration.
  - EXEC_73 upgraded to `err2`; message includes region name; second label at declaration.
  - EXEC_74 upgraded to `err2`; message includes region name; second label at declaration.

347 tests pass.

---

## 2026-04-28 — Nested region support (region_intersection + EXEC_77)

**EXEC_70 relaxed for containment**
`evaluate_regions` in `const_eval.rs` previously rejected any two regions that
shared any address range, including containment (A fully inside B).  The check
now detects only *partial* overlap (ranges overlap but neither contains the other).
Containment is allowed so that inner sections can bind to sub-regions of an outer
region without triggering EXEC_70.

**region_intersection on ScopeFrame**
`layout_phase.rs` gains `region_intersection: Option<RegionBinding>` on `ScopeFrame`.
At each `SectionStart`, the effective constraint is computed as the intersection
of the parent frame's `region_intersection` and the current section's direct
region binding (from `irdb.region_for_section`):

- No constraints: `None` (no enforcement).
- Parent only or direct only: inherit as-is.
- Both: compute geometric intersection -- `addr = max`, `end = min(ends)`.
  - Non-empty: `Some(intersection)` with name `"{parent} & {direct}"`.
  - Empty (disjoint): emit EXEC_77, fall back to direct binding.

`iterate_set_addr` now reads `frame.region_intersection` instead of calling
`irdb.region_for_section`.  This means inner sections without a direct binding
still get EXEC_72 enforcement from the inherited parent region.

`intersect_regions` static helper added to `LayoutPhase`.

**New error code EXEC_77**
EXEC_77: section's direct region binding does not intersect the enclosing
region inherited from the parent scope (mutually exclusive regions in nested
sections).  Emitted in `iterate_section_start` via `warned_lids` deduplication.

Tests:

- `region_exec70_partial.brink`: two partially-overlapping regions (A straddles B)
  still trigger EXEC_70. Integration test `region_exec70_partial`.
- `region_nested_2.brink`: comment updated; test changed from failure (EXEC_70) to
  success -- FLASH1 fully inside FLASH is valid containment.
- `region_nested_containment.brink`: outer in FLASH, inner (called via `wr`) in CODE
  where CODE is a subset of FLASH; both fit; no errors. Integration test
  `region_nested_containment`.
- `region_nested_exec72.brink`: inner section (no direct binding) inherits FLASH
  from outer; `set_addr` outside FLASH triggers EXEC_72. Integration test
  `region_nested_exec72`.
- `region_nested_exec77.brink`: inner bound to SRAM (disjoint from outer's FLASH);
  EXEC_77 fires. Integration test `region_nested_exec77`.

352 tests pass.

---

## 2026-04-28 — Allow partial region overlap; intersection-based EXEC_73

**EXEC_70 retired**
The compile-time region overlap check (EXEC_70) is removed entirely.  Any two
regions may share address space -- containment, partial overlap, or identical
ranges are all valid.  Enforcement that writes stay within the effective
intersection is handled at layout time by the `section_effective_regions` map
and `validate_section_regions`.

**section_effective_regions on LayoutPhase**
`LayoutPhase` gains `section_effective_regions: HashMap<String, RegionBinding>`.
`iterate_section_start` writes the converged `region_intersection` into this map
on every iterate pass; the final pass holds the post-convergence value.

`validate_section_regions` now prefers `section_effective_regions[sec]` over the
raw `irdb.region_for_section()` lookup.  When two regions partially overlap, the
stored intersection is tighter than the direct binding, so EXEC_73 fires if the
section's byte count exceeds the intersection size rather than the (wider) direct
binding size.

Tests:

- `region_exec70_partial.brink` repurposed: outer in A, inner in B (partial
  overlap A & B = 128 bytes); inner writes exactly 128 bytes -- succeeds.
  Integration test `region_exec70_partial` changed from failure to success.
- `region_exec73_partial_overlap.brink`: same regions, inner writes 192 bytes;
  exceeds intersection 128 -> EXEC_73.  Integration test `region_exec73_partial_overlap`.

353 tests pass.

---

## 2026-04-28 — EffectiveRegion backtrace + EXEC_78 starting-address check

**EffectiveRegion struct**
Replaced the ad-hoc `region_intersection: Option<RegionBinding>` + separate
`region_contributors: Vec<RegionBinding>` fields on `ScopeFrame` with a single
`effective_region: Option<EffectiveRegion>` field.  `EffectiveRegion` carries:

- `binding: RegionBinding` — the geometric intersection (used for EXEC_72/73/74
  enforcement, unchanged).
- `contributors: Vec<RegionBinding>` — every region that narrowed the bound,
  outermost first, for use in EXEC_73 backtrace diagnostics.

`section_effective_regions: HashMap<String, EffectiveRegion>` (was
`HashMap<String, RegionBinding>`) persists the converged value for
`validate_section_regions`.

**EXEC_73 backtrace via err_with_locs**
`diags::Diags` gains `err_with_locs(code, msg, primary, &[(SourceSpan, String)])`:
builds an ariadne report with one red primary label and N yellow secondary labels,
each carrying its own `.with_message(text)`.  When EXEC_73 fires and
`contributors.len() > 0`, one yellow label per contributing region is emitted,
each annotated with `"region 'X': addr=0x…, size=…"`.  The addr/size text is
necessary because ariadne shows only the `region NAME {` declaration line, not
the property values.

**Anchor fix (README consistency)**
`iterate_section_start` previously anchored `addr_base` to `intersection.addr`
(the effective intersection start).  The README states that a section `in R`
always starts at `R.addr`.  Anchor changed to `d.addr` (direct region's addr).

**EXEC_78 — starting address outside parent intersection**
When a section has both a parent inherited region and a direct binding, and the
intersection is non-empty but `d.addr < intersection.addr` (the direct region
starts before the parent region), EXEC_78 fires.  This covers the case where
the anchor `d.addr` would lie outside the effective parent constraint.  Emitted
via `err2` with both region declaration sites highlighted.

New error code: EXEC_78.

Tests:

- `region_exec78_bad_start.brink`: A=[0x1080,0x100), B=[0x1000,0x200); B starts
  before A; intersection non-empty but B.addr (0x1000) < intersection start
  (0x1080); EXEC_78 fires.  Integration test `region_exec78_bad_start`.

354 tests pass.

---

## 2026-04-28 — RegionDb crate; EXEC_79 region-bound section re-use

**RegionDb crate**
New first-class pipeline stage inserted between IRDb and LayoutPhase.

`regiondb/regiondb.rs` walks `irdb.ir_vec` once, tracking section nesting via
a scope stack, and computes `EffectiveRegion` for every section.  Because
region `addr` and `size` are const-evaluated before IRDb exists, the resulting
intersections are stable — they do not change across layout passes.

`EffectiveRegion` moved from `layout_phase.rs` to `ir/ir.rs` so both `regiondb`
and `layout_phase` can share the type without a circular dependency.

`intersect_regions` helper moved from `LayoutPhase` (a private static method
recomputed on every iterate pass) to `regiondb/regiondb.rs` (a module-level
free function called exactly once during `RegionDb::build`).

EXEC_77 and EXEC_78 detection moved from `layout_phase::iterate_section_start`
(fired with `warned_lids` deduplication across every iterate pass) into
`RegionDb::build` (fired once, naturally deduplicated).

LayoutPhase simplification:

- `EffectiveRegion` struct definition removed (now in `ir`).
- `intersect_regions` static method removed (now in `regiondb`).
- `section_effective_regions: HashMap<String, EffectiveRegion>` field removed.
- `iterate_section_start`: reduced to a RegionDb lookup + scope push + anchor.
  No intersection computation, no error emission, no `lid`/`diags` parameters.
  Returns `()` instead of `bool`.
- `validate_section_regions`: reads from `RegionDb` instead of the removed map.
- `build` and `iterate` gain `region_db: &RegionDb` parameter.

EXEC_79 — region-bound section used more than once: Region-bound sections
anchor to a fixed address on every inclusion; a second `wr` is guaranteed to
produce an address conflict.  `RegionDb::build` detects the second `SectionStart`
for any region-bound section and emits EXEC_79.

Pipeline ordering: `process.rs` builds `RegionDb` immediately after `IRDb`,
before `LayoutPhase`.  Failure halts with PROC_10.

New error codes: EXEC_79, PROC_10.

Tests:

- `region_exec79_reuse.brink`: `wr pinned` twice where `pinned in FLASH`;
  EXEC_79 fires.  Integration test `region_exec79_reuse`.

355 tests pass.
