# Brink Region System — Implementation Plan

## Overview

This plan adds two related features to brink:

1. **`--max-output-size` flag** (Step 1) — immediate fix for the fuzz-found
   infinite-loop bug caused by pathological `set_addr_offset` values.
2. **`region` declaration system** (Steps 2–8) — first-class memory map
   support for embedded targets, allowing sections to be spatially constrained
   to named hardware regions with automatic enforcement.

Design decisions established before this plan:

- `region` uses `addr` and `size` as the two required properties; `end` is
  a derived read-only expression (`addr + size`), not declarable.
- Region property values are not prefixed with `const` inside region blocks;
  the `region` keyword implies immutability.
- `in REGION` is the annotation syntax for both section-to-region and
  region-to-region (sub-region) relationships.
- Region property access uses dot notation: `FLASH.addr`, `FLASH.size`,
  `FLASH.end`.
- `output` remains explicit; the address argument becomes optional when the
  output section is `in` a region.
- `--max-output-size` and the region system are independent mechanisms.
  `--max-output-size` is a file-size failsafe; regions enforce spatial
  correctness. Neither implies the other.

---

## Step 1 — `--max-output-size` flag  *(fixes fuzz bug immediately)*

### Problem

After iterate converges, `execute_wrx` writes pad bytes one-at-a-time in a
`while repeat_count > 0` loop. A pathological `set_addr_offset 0xFFFFFFFFFFFFF`
produces a pad repeat count of ~4.5 quadrillion, causing a hang. The fix is a
pre-execute size check that rejects the output before any bytes are written.

### Changes

**`src/main.rs` — add CLI field to `Cli` struct:**

```rust
/// Maximum output file size in bytes.  Default is 256 MiB (268435456).
/// Accepts a plain integer (e.g. 67108864) or a suffix: K, M, G
/// (e.g. 64M, 512K, 1G).  Case-insensitive suffix.
#[arg(
    long = "max-output-size",
    value_name = "SIZE",
    default_value = "268435456",
    value_parser = parse_size,
)]
pub max_output_size: u64,
```

Add a free function in `main.rs`:

```rust
/// Parse a size string with optional K/M/G suffix.
/// "256M" -> 268435456, "64K" -> 65536, "1G" -> 1073741824, "1024" -> 1024.
fn parse_size(s: &str) -> Result<u64, String> {
    let s = s.trim();
    let (digits, shift) = if let Some(n) = s.strip_suffix_ci('K') {
        (n, 10)
    } else if let Some(n) = s.strip_suffix_ci('M') {
        (n, 20)
    } else if let Some(n) = s.strip_suffix_ci('G') {
        (n, 30)
    } else {
        (s, 0)
    };
    let base: u64 = parse_int::parse(digits)
        .map_err(|e| format!("invalid size '{}': {}", s, e))?;
    base.checked_shl(shift)
        .ok_or_else(|| format!("size '{}' overflows u64", s))
}
```

*(Implement `strip_suffix_ci` as a local helper or inline the case logic.)*

Pass `max_output_size` to `process()`:

```rust
process(
    in_file_name,
    &str_in,
    cli.output.as_deref(),
    verbosity,
    cli.noprint,
    &cli.defines,
    cli.max_output_size,   // new
    map_csv,
    ...
)
```

**`process/process.rs` — add parameter and check:**

```rust
pub fn process(
    name: &str,
    fstr: &str,
    output_file: Option<&str>,
    verbosity: u64,
    noprint: bool,
    defines: &[String],
    max_output_size: u64,   // new
    map_csv: Option<&str>,
    ...
) -> Result<()>
```

After `Engine::new(...)` succeeds and before `engine.execute(...)`:

```rust
// Check image size against --max-output-size before writing any bytes.
let final_size = engine
    .wr_dispatches
    .last()
    .map_or(0, |d| d.file_offset + d.size);
if final_size > max_output_size {
    let msg = format!(
        "Output image size {} bytes exceeds maximum {} bytes. \
         Use --max-output-size to increase the limit.",
        final_size, max_output_size
    );
    diags.err0("PROC_7", &msg);
    return Err(anyhow!("[PROC_7]: Error detected, halting."));
}
```

No `Engine` changes are needed. The final file size is already available via
`engine.wr_dispatches` (populated by `build_dispatches()` at the end of
`Engine::new()`). The last dispatch's `file_offset + size` equals total bytes
that `execute()` would write.

**`process/fuzz/fuzz_targets/fuzz_target_1.rs` — pass limit:**

```rust
let _ = process(
    "fuzz_target",
    str_in,
    Some("/dev/null"),
    0,       // verbosity
    true,    // noprint
    &[],     // defines
    65_536,  // max_output_size: 64 KiB ceiling for fast fuzzing
    None, None, None, None,
);
```

### New error codes

| Code   | Location   | Meaning                                       |
|--------|------------|-----------------------------------------------|
| PROC_7 | process.rs | Output size exceeds `--max-output-size` limit |

### Tests

- `tests/fuzz_found_19.brink` (the actual fuzz artifact): regression test
  expecting `[PROC_7]`
- `max_output_size_flag`: inline integration test; `--max-output-size 0` on
  `wr_single.brink` (1-byte output) expects `[PROC_7]`

---

## Step 2 — `region` keyword and AST parsing

### New lexer token

In `ast/ast.rs` `LexToken` enum:

```rust
Region,          // region keyword
In,              // in keyword (also used in Step 5 for sections)
Dot,             // . for property access (FLASH.addr)
```

In `ast/lexer.rs` `scan_word` keyword table:

```rust
"region" => LexToken::Region,
"in"     => LexToken::In,
```

`LexToken::Dot` is added to `scan_operator`:

```rust
b'.' => LexToken::Dot,
```

### Region AST structure

A region node in the indextree has this shape:

```
Region node  (tok = LexToken::Region)
  Identifier node  (region name, e.g. "FLASH")
  [Identifier node]  (optional parent name after "in", e.g. "RAM")
  RegionProp node  (tok = LexToken::RegionProp, val = "addr")
    <expression subtree>
  RegionProp node  (tok = LexToken::RegionProp, val = "size")
    <expression subtree>
  [RegionProp node]  (optional future properties: fill, warn_at, etc.)
```

Add to `LexToken`:

```rust
RegionProp,   // a named property inside a region block (addr, size, fill, ...)
```

### Parser — `parse_region`

New method on `Ast`:

```rust
/// Parse: region NAME [in PARENT_NAME] { PROPERTIES }
/// Properties: addr = EXPR ; | size = EXPR ;
fn parse_region(&mut self, diags: &mut Diags, parent: NodeId) -> bool
```

Sequence:
1. Consume `Region` token; add Region node to arena.
2. `expect_name_leaf(...)` for the region name — produces Identifier child.
3. If next token is `In`: consume it, `expect_name_leaf(...)` for parent
   region name — produces second Identifier child flagged as parent reference.
4. `expect_leaf(OpenBrace, ...)`
5. Loop: consume `name = expr ;` property assignments until `CloseBrace`.
   - Recognized property names: `"addr"`, `"size"` (future: `"fill"`,
     `"warn_at"`, `"endian"`).
   - Unknown property name: emit `AST_44` error, skip to next `;`.
   - Duplicate property: emit `AST_45` error.
   - Each property produces a `RegionProp` child node with the expression
     subtree beneath it.
6. `expect_leaf(CloseBrace, ...)`

### Top-level parser hook

In `parse_top_level` (or equivalent dispatch), add:

```rust
LexToken::Region => self.parse_region(diags, root_nid),
```

### New error codes

| Code   | Meaning                                                  |
|--------|----------------------------------------------------------|
| AST_44 | Unknown region property name                             |
| AST_45 | Duplicate region property                                |
| AST_46 | Missing required region property (`addr` or `size`)      |
| AST_47 | Region name conflicts with existing section or const name |

---

## Step 3 — Region evaluation and RegionDb

Region property values are const expressions evaluated in the `const_eval`
phase. This reuses the existing const evaluation infrastructure.

### New type in `ast` crate

```rust
/// Fully resolved region declaration, stored in AstDb after const_eval.
#[derive(Clone, Debug)]
pub struct RegionEntry {
    /// Base address of the region.
    pub addr: u64,
    /// Size of the region in bytes.
    pub size: u64,
    /// Optional parent region name (from `in PARENT`).
    pub parent: Option<String>,
    /// Source location of the region keyword, for diagnostics.
    pub src_loc: SourceSpan,
}

impl RegionEntry {
    /// Exclusive end address: addr + size.
    pub fn end(&self) -> u64 {
        self.addr + self.size
    }

    /// True if `addr` is within [self.addr, self.addr + self.size).
    pub fn contains_addr(&self, addr: u64) -> bool {
        addr >= self.addr && addr < self.end()
    }

    /// True if the range [addr, addr+size) is fully contained.
    pub fn contains_range(&self, addr: u64, size: u64) -> bool {
        addr >= self.addr
            && size <= self.size
            && addr - self.addr <= self.size - size
    }
}
```

### AstDb changes

Add to `AstDb`:

```rust
/// Regions declared in this source, keyed by name, in declaration order.
pub regions: IndexMap<String, RegionEntry>,
```

`AstDb::new()` traverses Region nodes at the root of the AST and records their
names and source locations. Property expressions are NOT evaluated here —
only structural validation (name uniqueness, required properties present).

### const_eval changes

After the existing const evaluation pass, a new sub-pass evaluates region
property expressions using the already-resolved `SymbolTable`:

```rust
/// Evaluate all region property expressions and populate RegionEntry.addr
/// and RegionEntry.size in ast_db.regions.
pub fn evaluate_regions(
    diags: &mut Diags,
    ast: &Ast,
    ast_db: &mut AstDb,
    symbol_table: &SymbolTable,
) -> Result<()>
```

This function walks each Region node's `RegionProp` children, evaluates their
expression subtrees using the existing `eval_const_expr_r` machinery, and
stores resolved `u64` values into `RegionEntry`. Expressions that reference
other region properties (`FLASH.addr`) are deferred until all same-depth
regions are resolved. The evaluator must implement cycle detection to trap cyclic dependencies and emit `EXEC_68`. Forward references to undefined regions produce `EXEC_62`.

### New error codes

| Code    | Meaning                                                |
|---------|--------------------------------------------------------|
| EXEC_62 | Region property expression references undefined region |
| EXEC_68 | Cyclic dependency in region properties                 |

---

## Step 4 — Region-on-region containment validation

After `evaluate_regions()` resolves all `RegionEntry` values, a validation
pass checks that every sub-region is fully contained within its declared
parent.

```rust
/// Validate that every region declared `in PARENT` is fully contained
/// within PARENT's bounds.  Called immediately after evaluate_regions().
pub fn validate_region_containment(
    diags: &mut Diags,
    ast_db: &AstDb,
) -> Result<()>
```

For each `(name, entry)` in `ast_db.regions` where `entry.parent.is_some()`:

```rust
let parent_name = entry.parent.as_ref().unwrap();
let Some(parent) = ast_db.regions.get(parent_name) else {
    diags.err1("EXEC_63", &format!("region '{}': parent region '{}' not declared",
        name, parent_name), entry.src_loc.clone());
    continue;
};
if !parent.contains_range(entry.addr, entry.size) {
    diags.err1("EXEC_64", &format!(
        "region '{}' [0x{:X}..0x{:X}) is not contained within \
         parent '{}' [0x{:X}..0x{:X})",
        name, entry.addr, entry.end(),
        parent_name, parent.addr, parent.end(),
    ), entry.src_loc.clone());
}
```

Overlap between sibling regions (two regions with the same parent that overlap
each other) produces `EXEC_65`.

### New error codes

| Code    | Meaning                                                         |
|---------|-----------------------------------------------------------------|
| EXEC_63 | `in PARENT` references undeclared region                        |
| EXEC_64 | Sub-region address range falls outside parent region            |
| EXEC_65 | Sibling regions overlap within the same parent                  |

---

## Step 5 — `section NAME in REGION` annotation

### Parser changes

In `parse_section`, after parsing the section name, check for `In`:

```rust
// After consuming the section name identifier:
let region_name = if self.tv.peek().tok == LexToken::In {
    self.tv.skip();  // consume 'in'
    // parse region name — must be a known identifier
    let tinfo = self.tv.peek();
    if tinfo.tok != LexToken::Identifier {
        self.err_expected_after(diags, "AST_48", "'in': expected region name");
        return self.dbg_exit("parse_section", false);
    }
    let name = tinfo.val.to_string();
    self.tv.skip();
    Some(name)
} else {
    None
};
```

The region name is stored as a synthetic child node of the Section AST node
with a new token kind `LexToken::RegionRef`.

### AstDb — Section entry gains region field

```rust
pub struct SectionEntry {          // new wrapper (or extend existing map value)
    pub node_id: NodeId,
    pub region: Option<String>,    // name of declared region, if any
    pub src_loc: SourceSpan,
}
```

`AstDb::new()` validates that any region name referenced in `section NAME in REGION`
actually exists in `ast_db.regions`. Unknown region reference produces `AST_49`.

### New error codes

| Code   | Meaning                                            |
|--------|----------------------------------------------------|
| AST_48 | `in` keyword in section declaration not followed by region name |
| AST_49 | Section references undeclared region               |

---

## Step 6 — Implicit Region Anchoring and Post-Iterate Enforcement

### Implicit Anchor in Engine

During the `iterate` loop, `iterate_section_start` processes sections. For sections declared `in REGION`, the engine checks the current location. If `current.addr_base` is uninitialized or differs from `region.addr`, the engine sets `current.addr_base = region.addr`. This action establishes the section as the root of the region, eliminating the need for an explicit `set_addr` instruction. Subsequent sections written to the same region inherit the updated address sequentially.

### Post-Iterate Enforcement

After `iterate` converges and before `execute` begins, the engine validates that every section
annotated `in REGION` has its resolved address range fully within the region.

### Engine changes

`Engine::new()` receives the region table:

```rust
pub fn new(
    ir_db: &IRDb,
    ext_registry: &ExtensionRegistry,
    diags: &mut Diags,
    abs_start: usize,
    regions: &IndexMap<String, RegionEntry>,   // new
) -> Result<Self>
```

After iterate converges, a new method runs:

```rust
fn validate_section_regions(
    &self,
    ir_db: &IRDb,
    regions: &IndexMap<String, RegionEntry>,
    diags: &mut Diags,
) -> bool
```

For each `WrDispatch` entry (one per `wr section_name` site), if the
corresponding section has a `region` annotation in `ir_db`, check:

```rust
let region = &regions[region_name];
if !region.contains_range(dispatch.addr, dispatch.size) {
    diags.err1("EXEC_66", &format!(
        "section '{}' [0x{:X}..0x{:X}) lies outside region '{}' \
         [0x{:X}..0x{:X})",
        dispatch.name, dispatch.addr, dispatch.addr + dispatch.size,
        region_name, region.addr, region.end(),
    ), dispatch.src_loc.clone());
    return false;
}
```

Also check for overlap between sibling sections in the same region using the
same `WrittenRanges` BTreeMap logic that already exists for execute-phase
overlap detection — but applied here per-region rather than globally.

### `set_addr` outside region bounds

If a section annotated `in REGION` contains a `set_addr` that resolves to an
address outside the region, the above range check catches it automatically
because the final `WrDispatch` address and size reflect the post-`set_addr`
layout.

### New error codes

| Code    | Meaning                                                          |
|---------|------------------------------------------------------------------|
| EXEC_66 | Section address range falls outside its declared region          |
| EXEC_67 | Two sections in the same region have overlapping address ranges  |

---

## Step 7 — Region property dot-access expressions

`FLASH.addr`, `FLASH.size`, and `FLASH.end` must be usable in expressions
everywhere a const expression is valid.

### Lexer

`LexToken::Dot` already added in Step 2. No further lexer change needed.

### Pratt parser — dot expression

In `parse_pratt`, in the `LexToken::Identifier` branch, after the identifier
is consumed, check for `Dot` followed by a property name:

```rust
if self.tv.peek().tok == LexToken::Dot {
    self.tv.skip();  // consume '.'
    let prop_tinfo = self.tv.peek();
    let prop = prop_tinfo.val;
    match prop {
        "addr" | "size" | "end" => {
            self.tv.skip();
            // produce a RegionAccess AST node with (region_name, property)
            let node = self.arena.new_node(region_name_tok_idx);
            self.arena[node].get_mut() ...  // attach property as child
        }
        _ => {
            self.err_expected_after(diags, "AST_50",
                "dot notation: expected 'addr', 'size', or 'end'");
            return self.dbg_exit("parse_pratt", false);
        }
    }
}
```

### IR

Add to `IRKind`:

```rust
RegionAddr,   // REGION.addr — yields u64
RegionSize,   // REGION.size — yields u64
RegionEnd,    // REGION.end  — yields u64 (addr + size)
```

Each carries the region name in the operand (stored as an Identifier
`ParameterValue`).

### Engine evaluation

In `iterate_arithmetic` (or a new `iterate_region_access`):

```rust
IRKind::RegionAddr => {
    let name = self.parms[ir.operands[0]].to_identifier();
    let entry = &regions[name];
    *self.parms[ir.operands[1]].to_u64_mut() = entry.addr;
    true
}
IRKind::RegionSize => { ... entry.size ... }
IRKind::RegionEnd  => { ... entry.addr + entry.size ... }
```

### New error codes

| Code   | Meaning                                             |
|--------|-----------------------------------------------------|
| AST_50 | Invalid property name in dot-access expression      |
| AST_51 | Dot-access on unknown region name                   |

---

## Step 8 — `output` statement with optional address

Today: `output section_name 0;`

When the output section is declared `in REGION`, the address argument becomes
redundant because the base address is already encoded in the region declaration
and the section's `set_addr`. The argument should be optional:

```
output flash_image;           // address arg omitted — valid when section is in REGION
output flash_image 0;         // still valid (backward compatible)
output flash_image 0xF000_0000;  // still valid
```

### Parser change

In `parse_output`, after parsing the section name, check if the next token is
`Semicolon`. If so, the address is omitted; default to `0`. If the output
section is not `in` a region and the address is omitted, emit warning `AST_52`
suggesting an explicit address.

### New error codes / warnings

| Code   | Meaning                                                          |
|--------|------------------------------------------------------------------|
| AST_52 | `output` address omitted on section not associated with a region |

---

## Implementation Order and Dependencies

```
Step 1  (no dependencies)       -- fixes immediate fuzz bug
Step 2  (no dependencies)       -- lexer + parser foundation
Step 3  (requires Step 2)       -- region evaluation
Step 4  (requires Step 3)       -- region-on-region containment
Step 5  (requires Steps 2, 3)   -- section annotation
Step 6  (requires Steps 3, 5)   -- engine enforcement
Step 7  (requires Steps 2, 3)   -- dot-access expressions
Step 8  (requires Steps 2, 5)   -- output simplification
```

Steps 2–8 can be developed incrementally. Each step is independently testable:
a step is complete when its new tests pass and no existing tests regress.

---

## Error Code Summary

Note: EXEC_57–EXEC_61 are already used in engine.rs for other purposes.
Region-related EXEC codes begin at EXEC_62.

| Code    | Step | Crate      | Meaning                                                   |
|---------|------|------------|-----------------------------------------------------------|
| AST_44  | 2    | ast        | Unknown region property name                              |
| AST_45  | 2    | ast        | Duplicate region property                                 |
| AST_46  | 2    | ast        | Missing required region property                          |
| AST_47  | 2    | ast        | Region name conflicts with section or const               |
| AST_48  | 5    | ast        | `in` not followed by region name in section declaration   |
| AST_49  | 5    | ast        | Section references undeclared region                      |
| AST_50  | 7    | ast        | Invalid property in dot-access expression                 |
| AST_51  | 7    | ast        | Dot-access on unknown region name                         |
| AST_52  | 8    | ast        | `output` address omitted on non-region section (warning)  |
| PROC_7  | 1    | process    | Output size exceeds `--max-output-size`                   |
| EXEC_62 | 3    | const_eval | Region property expression references undefined region    |
| EXEC_63 | 4    | engine     | Sub-region parent not declared                            |
| EXEC_64 | 4    | engine     | Sub-region falls outside parent bounds                    |
| EXEC_65 | 4    | engine     | Sibling regions overlap                                   |
| EXEC_66 | 6    | engine     | Section address range falls outside declared region       |
| EXEC_67 | 6    | engine     | Two sections in same region overlap                       |
| EXEC_68 | 3    | const_eval | Cyclic dependency in region properties                    |
