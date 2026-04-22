# Brink Region System — Implementation Plan

## Overview

This plan adds three related features to brink:

1. **`--max-output-size` flag** (Step 1) — immediate fix for the fuzz-found
   infinite-loop bug caused by pathological `set_addr_offset` values.
2. **Remove `output` address argument** (Step 2) — simplification that
   eliminates a redundant and conflict-prone feature before the region system
   lands.
3. **`region` declaration system** (Steps 3–7) — first-class memory map
   support for embedded targets, allowing sections to be spatially constrained
   to named hardware regions with automatic enforcement.

Design decisions established for this plan:

- `region` uses `addr` and `size` as the two required properties, plus optional
  `default_align` and `default_fill`. `end` is derived: `addr + size`.
- Region property values are not prefixed with `const` inside region blocks;
  the `region` keyword implies immutability.
- `in REGION` is the annotation syntax for section-to-region binding only.
  Nested regions (region-to-region) are explicitly deferred.
- Region property access uses function-call style: `addr(FLASH)`,
  `sizeof(FLASH)`. Dot notation (`FLASH.addr`) is not implemented.
- `output` takes a section name only. Section placement uses `set_addr` or
  `in REGION` binding. The address argument is removed in Step 2.
- `set_addr` is not permitted inside a region-bound section. Regions control
  section placement; a section bound to a region starts at `addr(region)`.
  `set_addr` remains valid in sections not bound to a region.
- `--max-output-size` and the region system are independent mechanisms.
  `--max-output-size` is a file-size failsafe; regions enforce spatial
  correctness. Neither implies the other.

---

## Step 1 — `--max-output-size` flag  *(COMPLETE)*

Already implemented. Adds a CLI `--max-output-size SIZE` option that caps the
output file size. Error code `PROC_7` fires if the computed image size exceeds
the limit.

---

## Step 2 — Remove `output` address argument

### Motivation

The `output` statement currently accepts an optional absolute starting address:

```brink
output image 0x0800_0000;
```

This conflicts with the region system and with `set_addr`, creating two ways
to specify the same thing with no rule for which wins. The output statement
should name the root section only; placement belongs to `set_addr` or to a
region binding.

### Migration

| Old form                        | Replacement                                           |
|---------------------------------|-------------------------------------------------------|
| `output foo 0x1000;`            | `set_addr 0x1000;` as first statement in `foo`        |
| `output foo 0x1000;` + region   | bind `foo` `in REGION` where `region.addr = 0x1000`   |
| `output foo;` (address omitted) | unchanged — continues to work                         |
| `output foo 0;`                 | `output foo;` — address 0 is the default              |

### Parser changes

In `parse_output`, after parsing the section name, require `Semicolon`
immediately. If an expression or integer literal follows the section name,
emit `AST_54` with a message directing users to `set_addr`.

### Engine changes

Remove the `abs_start` parameter from `Engine::new()`. The section's resolved
starting address — from `set_addr` or from a region binding (Step 5) — is
already present in the iterate output. No engine arithmetic uses `abs_start`
independently of section layout.

### `__OUTPUT_ADDR` behavior

`__OUTPUT_ADDR` already documents itself as equivalent to
`addr(<output-section>)`. After this change that equivalence is the only
definition: `__OUTPUT_ADDR` returns the resolved starting address of the
output section, which is 0 if no `set_addr` or region binding applies.
No behavior change for programs that did not supply an output address.

### Test updates

- Any integration test using `output SECTION 0xADDR;` must be updated to move
  the address into the section via `set_addr`.
- Add a regression test: `output foo 0x1000;` produces `AST_54`.
- Confirm `output foo;` and `output foo 0;` (the latter now an error) behave
  as expected.

### README changes

Update the `output` reference entry to remove the address syntax.  Update
`__OUTPUT_ADDR` entry to remove references to the output-statement address.
Update any examples that used `output section 0xADDR;`.

### New error codes

| Code   | Meaning                                                              |
|--------|----------------------------------------------------------------------|
| AST_54 | `output` address argument removed; use `set_addr` or region binding  |

---

## Step 3 — `region` keyword and AST parsing

### New lexer tokens

In `ast/ast.rs` `LexToken` enum:

```rust
Region,          // region keyword
In,              // in keyword (used in section-to-region binding)
```

In `ast/lexer.rs` `scan_word` keyword table:

```rust
"region" => LexToken::Region,
"in"     => LexToken::In,
```

Note: no `Dot` token. Property access uses `addr()` and `sizeof()` calls,
not dot notation.

### Region AST structure

A region node in the indextree has this shape:

```
Region node  (tok = LexToken::Region)
  Identifier node  (region name, e.g. "FLASH")
  RegionProp node  (tok = LexToken::RegionProp, val = "addr")
    <expression subtree>
  RegionProp node  (tok = LexToken::RegionProp, val = "size")
    <expression subtree>
  [RegionProp node]  (val = "default_align")
    <expression subtree>
  [RegionProp node]  (val = "default_fill")
    <expression subtree>
```

Add to `LexToken`:

```rust
RegionProp,   // a named property inside a region block
```

### Recognized region properties

| Property        | Required | Default | Meaning                                     |
|-----------------|----------|---------|---------------------------------------------|
| `addr`          | Yes      | —       | Starting address of the region              |
| `size`          | Yes      | —       | Size in bytes                               |
| `default_align` | No       | 1       | Default alignment for writes in top section |
| `default_fill`  | No       | 0xFF    | Default fill byte for pad operations        |

### Parser — `parse_region`

New method on `Ast`:

```rust
/// Parse: region NAME { PROPERTIES }
/// Properties: addr = EXPR ; | size = EXPR ; | default_align = EXPR ;
///             | default_fill = EXPR ;
fn parse_region(&mut self, diags: &mut Diags, parent: NodeId) -> bool
```

Sequence:
1. Consume `Region` token; add Region node to arena.
2. `expect_name_leaf(...)` for the region name — produces Identifier child.
3. `expect_leaf(OpenBrace, ...)`
4. Loop: consume `name = expr ;` property assignments until `CloseBrace`.
   - Recognized property names: `"addr"`, `"size"`, `"default_align"`,
     `"default_fill"`.
   - Unknown property name: emit `AST_44` error, skip to next `;`.
   - Duplicate property: emit `AST_45` error.
   - Each property produces a `RegionProp` child node with the expression
     subtree beneath it.
5. After parsing: verify both `addr` and `size` are present; emit `AST_46`
   if either is absent.
6. `expect_leaf(CloseBrace, ...)`

### Top-level parser hook

In `parse_top_level` dispatch, add:

```rust
LexToken::Region => self.parse_region(diags, root_nid),
```

### New error codes

| Code   | Meaning                                                  |
|--------|----------------------------------------------------------|
| AST_44 | Unknown region property name                             |
| AST_45 | Duplicate region property                                |
| AST_46 | Missing required region property (`addr` or `size`)      |
| AST_47 | Region name conflicts with existing section or const name|

---

## Step 3 — Region evaluation and RegionDb

Region property values are const expressions evaluated in the `const_eval`
phase, reusing the existing const evaluation infrastructure.

### New type in `ast` crate

```rust
/// Fully resolved region declaration, stored in AstDb after const_eval.
#[derive(Clone, Debug)]
pub struct RegionEntry {
    /// Base address of the region.
    pub addr: u64,
    /// Size of the region in bytes.
    pub size: u64,
    /// Default alignment for write operations in the top-level section.
    /// 1 means no alignment (pack tightly).
    pub default_align: u64,
    /// Default fill byte for pad operations in the top-level section.
    pub default_fill: u8,
    /// Source location of the region keyword, for diagnostics.
    pub src_loc: SourceSpan,
}

impl RegionEntry {
    /// Exclusive end address: addr + size.
    pub fn end(&self) -> u64 {
        self.addr + self.size
    }

    /// True if addr is within [self.addr, self.addr + self.size).
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

`AstDb::new()` traverses Region nodes at the root of the AST and records
their names and source locations. Property expressions are NOT evaluated here
— only structural validation (name uniqueness, required properties present).

### const_eval changes

After the existing const evaluation pass, a new sub-pass evaluates region
property expressions using the already-resolved `SymbolTable`:

```rust
/// Evaluate all region property expressions and populate RegionEntry fields.
pub fn evaluate_regions(
    diags: &mut Diags,
    ast: &Ast,
    ast_db: &mut AstDb,
    symbol_table: &SymbolTable,
) -> Result<()>
```

Walks each Region node's `RegionProp` children, evaluates their expression
subtrees using `eval_const_expr_r`, and stores resolved values into
`RegionEntry`.

Validate `default_align` is a power of two and non-zero. Validate
`default_fill` fits in a `u8`.

Validate that no two regions have overlapping address ranges. Emit `EXEC_70`
per overlapping pair.

### New error codes

| Code    | Meaning                                                |
|---------|--------------------------------------------------------|
| EXEC_69 | `default_align` is not a power of two, or is zero      |
| EXEC_70 | Two regions have overlapping address ranges            |
| EXEC_71 | Cyclic dependency in region property expressions       |

---

## Step 4 — `section NAME in REGION` binding

### Parser changes

In `parse_section`, after parsing the section name, check for `In`:

```rust
// After consuming the section name identifier:
let region_name = if self.tv.peek().tok == LexToken::In {
    self.tv.skip();  // consume 'in'
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
with token kind `LexToken::RegionRef`.

### AstDb — Section entry gains region field

```rust
pub struct SectionEntry {
    pub node_id: NodeId,
    pub region: Option<String>,    // name of bound region, if any
    pub src_loc: SourceSpan,
}
```

`AstDb::new()` validates:

- The region name referenced in `section NAME in REGION` exists in
  `ast_db.regions`. Unknown reference produces `AST_49`.
- At most one section is bound to each region. A second binding produces
  `AST_53`.

### New error codes

| Code   | Meaning                                                         |
|--------|-----------------------------------------------------------------|
| AST_48 | `in` keyword not followed by region name in section declaration |
| AST_49 | Section references undeclared region                            |
| AST_53 | A second section declares `in` the same region                  |

---

## Step 5 — Implicit region anchoring and enforcement

### Implicit anchor in Engine

For sections declared `in REGION`, the engine sets the section's starting
address to `region.addr` during the iterate loop in `iterate_section_start`.
This replaces any need for an explicit `set_addr` instruction. Sections
bound to a region must not contain `set_addr`; emit `EXEC_72` if one is
encountered.

The `default_align` from the region applies to all write operations in the
top-level section unless overridden by an explicit `align` statement at that
site.  The `default_fill` from the region is the fill byte used for alignment
padding unless a fill byte is specified on the individual operation.

### Post-iterate enforcement

After `iterate` converges and before `execute` begins, validate that the
region-bound section's resolved address range is fully within the region:

```rust
fn validate_section_regions(
    &self,
    ir_db: &IRDb,
    regions: &IndexMap<String, RegionEntry>,
    diags: &mut Diags,
) -> bool
```

For each region-bound section, if `sizeof(section) > region.size`, emit
`EXEC_73` with both sizes and the excess.

### Engine changes

`Engine::new()` receives the region table:

```rust
pub fn new(
    ir_db: &IRDb,
    ext_registry: &ExtensionRegistry,
    diags: &mut Diags,
    abs_start: usize,
    regions: &IndexMap<String, RegionEntry>,
) -> Result<Self>
```

### New error codes

| Code    | Meaning                                                          |
|---------|------------------------------------------------------------------|
| EXEC_72 | `set_addr` used inside a region-bound section                    |
| EXEC_73 | Region-bound section exceeds region size                         |

---

## Step 6 — Region property access in expressions

`addr(FLASH)` and `sizeof(FLASH)` must work in expressions wherever a const
expression is valid.  The existing `addr()` and `sizeof()` machinery resolves
names against the section table; extend it to also resolve against the region
table.

### Resolution order for `addr(NAME)` and `sizeof(NAME)`

1. Check `ir_db` sections — if NAME is a section, existing behavior applies.
2. Check `regions` — if NAME is a region, return `region.addr` or
   `region.size`.
3. Neither — existing error (undefined identifier).

No new IR kinds are required if the existing `addr()` / `sizeof()` IR paths
accept a name that may be either a section or a region. If separate IR kinds
are cleaner in the implementation, add:

```rust
RegionAddr,   // addr(REGION) -> u64: yields region.addr
RegionSize,   // sizeof(REGION) -> u64: yields region.size
```

### New error codes

None beyond existing undefined-identifier errors, unless separate IR kinds
are added.

---

## Step 7 — `output` statement with optional address

When the output section is declared `in REGION`, the address argument is
redundant because the base address is encoded in the region. Make it optional:

```brink
output flash_image;              // address omitted; valid when section is in REGION
output flash_image 0;            // still valid (backward compatible)
output flash_image 0xF000_0000;  // still valid
```

### Parser change

In `parse_output`, after parsing the section name, check if the next token is
`Semicolon`. If so, address defaults to `0`. If the output section is not
bound to a region and the address is omitted, emit warning `AST_52`.

### New error codes / warnings

| Code   | Meaning                                                          |
|--------|------------------------------------------------------------------|
| AST_52 | `output` address omitted on section not associated with a region |

---

## Nested Regions — Deferred

Region-to-region nesting (`region FOO in FLASH { ... }`) was considered and
explicitly deferred. The current design supports exactly one top-level section
per region. If a user needs a fixed-address sub-range, the recommended pattern
is two sibling top-level regions with non-overlapping address ranges, enforced
by EXEC_70.

When nested regions are added in the future, they slot in as an extension to
`parse_region` (accept an optional `in PARENT` clause) and to Step 3's
overlap validation, without breaking any existing region or section grammar.

---

## Implementation Order and Dependencies

```
Step 1  (COMPLETE)
Step 2  (no dependencies)       -- lexer + parser foundation
Step 3  (requires Step 2)       -- region evaluation and overlap check
Step 4  (requires Steps 2, 3)   -- section-to-region binding
Step 5  (requires Steps 3, 4)   -- engine anchoring and enforcement
Step 6  (requires Steps 2, 3)   -- addr()/sizeof() region resolution
Step 7  (requires Steps 2, 4)   -- output simplification
```

Each step is independently testable; a step is complete when its new tests
pass and no existing tests regress.

---

## Error Code Summary

Verify all EXEC codes against `engine.rs` before implementation — codes
EXEC_57 through EXEC_62 are known to be occupied.  Region codes begin at
EXEC_69 to leave room.

| Code    | Step | Crate      | Meaning                                              |
|---------|------|------------|------------------------------------------------------|
| AST_44  | 2    | ast        | Unknown region property name                         |
| AST_45  | 2    | ast        | Duplicate region property                            |
| AST_46  | 2    | ast        | Missing required region property                     |
| AST_47  | 2    | ast        | Region name conflicts with section or const          |
| AST_48  | 4    | ast        | `in` not followed by region name in section decl     |
| AST_49  | 4    | ast        | Section references undeclared region                 |
| AST_52  | 7    | ast        | `output` address omitted on non-region section (warn)|
| AST_53  | 4    | ast        | Second section bound to same region                  |
| PROC_7  | 1    | process    | Output size exceeds `--max-output-size` (COMPLETE)   |
| EXEC_69 | 3    | const_eval | `default_align` not a power of two or is zero        |
| EXEC_70 | 3    | const_eval | Two regions have overlapping address ranges          |
| EXEC_71 | 3    | const_eval | Cyclic dependency in region property expressions     |
| EXEC_72 | 5    | engine     | `set_addr` used inside a region-bound section        |
| EXEC_73 | 5    | engine     | Region-bound section exceeds region size             |
