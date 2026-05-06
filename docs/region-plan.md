# Brink Region System — Implementation Plan

## Overview

This plan adds three related features to brink:

1. **`--max-output-size` flag** (Step 1) — immediate fix for the fuzz-found
   infinite-loop bug caused by pathological `pad_addr_offset` values.
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
- `set_addr` is permitted inside a region-bound section. Brink reports an error
  if the `set_addr` target address falls outside the region. Regions set the
  starting address of the top-level section; `set_addr` may adjust the address
  within the section, but only within region bounds.
- `--max-output-size` and the region system are independent mechanisms.
  `--max-output-size` is a file-size failsafe; regions enforce spatial
  correctness. Neither implies the other.

---

## Step 1 — `--max-output-size` flag  *(COMPLETE)*


## Step 2 — Remove `output` address argument  *(COMPLETE 2026-04-21)*

---

## Step 3 — `region` keyword, parsing, and AstDb  *(COMPLETE)*

### New lexer tokens

In `ast/ast.rs` `LexToken` enum:

```rust
Region,      // region keyword
In,          // in keyword (used in section-to-region binding)
RegionProp,  // synthetic; val = property name; one expression child
RegionRef,   // synthetic; val = region name; no children
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

### Recognized region properties

| Property        | Required | Default | Meaning                                     |
|-----------------|----------|---------|---------------------------------------------|
| `addr`          | Yes      | —       | Starting address of the region              |
| `size`          | Yes      | —       | Size in bytes                               |
| `default_align` | No       | 1       | Default alignment for writes in top section |
| `default_fill`  | No       | 0xFF    | Default fill byte for pad operations        |

### Parser functions

`parse_region` — top-level entry point. Consumes the `Region` token, calls
`expect_name_leaf` (ERR_53), calls `expect_leaf(OpenBrace)` (ERR_54), delegates
the body to `parse_region_contents`.

`parse_region_contents` — body loop. Iterates `name = expr ;` assignments.
Dispatches on `prop_val` with one match arm per recognized property name; the
`_` arm emits ERR_40 and skips to the next `;`. Checks for duplicates (ERR_41).
Calls `parse_region_property` for each valid, non-duplicate property. After the
loop, verifies `addr` (ERR_42) and `size` (ERR_59) were present.

`parse_region_property` — per-property helper. Consumes the property name token,
expects `=` (ERR_57), synthesizes a `RegionProp` node, parses the expression,
and parses the terminating `;`.

### Top-level parser hook

In `parse_top_level` dispatch:

```rust
LexToken::Region => self.parse_region(diags, root_nid),
```

### `RegionEntry` struct

```rust
pub struct RegionEntry {
    pub nid: NodeId,           // Region node in the arena
    pub src_loc: SourceSpan,   // location of the region keyword
    pub addr: u64,             // sentinel 0; filled by evaluate_regions
    pub size: u64,             // sentinel 0; filled by evaluate_regions
    pub default_align: u64,    // sentinel 1
    pub default_fill: u8,      // sentinel 0xFF
}
```

Methods: `end()`, `contains_addr()`, `contains_range()`.

### AstDb changes

```rust
pub regions: HashMap<String, RegionEntry>,
```

`AstDb::new` processes Region nodes at root; calls `record_region` per node,
which checks for reserved names (ERR_56) and duplicate names (ERR_55).
After all root nodes are recorded, checks for region/section name conflicts
(ERR_43) and region/const name conflicts (ERR_58).

`Section::region: Option<String>` — set from the `RegionRef` child when a
`section NAME in REGION` binding is parsed.

### New error codes

| Code   | Meaning                                                  |
|--------|----------------------------------------------------------|
| ERR_40 | Unknown region property name                             |
| ERR_41 | Duplicate region property                                |
| ERR_42 | Region is missing required property `addr`               |
| ERR_43 | Region name conflicts with a section name                |
| ERR_53 | Expected identifier after `region` keyword               |
| ERR_54 | Expected `{` after region name                           |
| ERR_55 | Duplicate region name                                    |
| ERR_56 | Reserved identifier used as region name                  |
| ERR_57 | Expected `=` after region property name                  |
| ERR_58 | Region name conflicts with a const name                  |
| ERR_59 | Region is missing required property `size`               |

### Integration tests

15 fixtures and test functions cover all error codes above plus one success
case (`region_valid.brink`).

---

## Step 3 — Region evaluation  *(PARTIAL — anchor complete; validation pending)*

Region property values are const expressions evaluated in the `const_eval`
phase, reusing the existing const evaluation infrastructure.

### const_eval changes  *(COMPLETE)*

After the existing const evaluation pass, `evaluate_regions` evaluates region
property expressions using the already-resolved `SymbolTable`:

```rust
pub fn evaluate_regions<'toks>(
    diags: &mut Diags,
    ast: &'toks Ast,
    ast_db: &mut AstDb<'toks>,
    symbol_table: &mut SymbolTable,
) -> bool
```

Walks each Region node's `RegionProp` children, evaluates their expression
subtrees using `eval_const_expr_r`, and stores resolved values into
`RegionEntry`.  Emits ERR_180 for non-numeric property values.

Called in `process.rs` after `AstDb::new(true)` and before `LayoutDb::new`.
`process.rs` then builds `section_anchors: HashMap<String, u64>` (section name
→ `region.addr`) and passes it to `LayoutPhase::build`.

### layout_phase changes  *(COMPLETE — address anchor only)*

`LayoutPhase` gains `section_anchors: HashMap<String, u64>`.
`build()` accepts `section_anchors` as a parameter.
`iterate_section_start` applies the anchor for region-bound sections:
sets `current.addr.addr_base = anchor` and `current.addr.addr_offset = 0`.
`ir_locs[lid]` is re-recorded after `iterate_section_start` so that
`addr(section_name)` returns the anchored address.

### Validation still pending

The following validations are deferred to a later step:

- `default_align` must be a power of two and non-zero (ERR_182).
- No two regions may have overlapping address ranges (ERR_183).
- Cyclic dependency detection in region property expressions (ERR_184).

### New error codes

| Code    | Meaning                                                       |
|---------|---------------------------------------------------------------|
| ERR_180 | Region property value is not numeric                          |
| ERR_182 | default_align not a power of two or zero (pending)            |
| ERR_183 | Two regions have overlapping address ranges (pending)         |
| ERR_184 | Cyclic dependency in region property expressions (pending)    |

---

## Step 4 — `section NAME in REGION` binding  *(COMPLETE)*

### Parser changes

In `parse_section`, after parsing the section name, check for `In`:

```rust
// After consuming the section name identifier:
let region_name = if self.tv.peek().tok == LexToken::In {
    self.tv.skip();  // consume 'in'
    let tinfo = self.tv.peek();
    if tinfo.tok != LexToken::Identifier {
        self.err_expected_after(diags, "ERR_44", "'in': expected region name");
        return self.dbg_exit("parse_section", false);
    }
    let name = tinfo.val.to_string();
    self.tv.skip();
    Some(name)
} else {
    None
};
```

The region name is stored as a `RegionRef` synthetic child node of the Section
AST node. The linearizer treats `RegionRef` as syntactic noise (no IR emitted).

### AstDb — Section gains region field

`Section::region: Option<String>` is populated from the `RegionRef` child
during `Section::new`. `AstDb::new` validates:

- The region name referenced in `section NAME in REGION` exists in
  `ast_db.regions`. Unknown reference produces ERR_51.
- At most one section is bound to each region. A second binding produces ERR_52.

### New error codes

| Code   | Meaning                                                         |
|--------|-----------------------------------------------------------------|
| ERR_44 | `in` keyword not followed by region name in section declaration |
| ERR_51 | Section references undeclared region                            |
| ERR_52 | Second section bound to same region                             |

---

## Step 5 — Implicit region anchoring and enforcement

### Implicit anchor in Engine

For sections declared `in REGION`, the engine sets the section's starting
address to `region.addr` during the iterate loop in `iterate_section_start`.
`set_addr` is permitted inside a region-bound section, but Brink reports
`ERR_185` if the target address falls outside the region bounds.

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
    regions: &HashMap<String, RegionEntry>,
    diags: &mut Diags,
) -> bool
```

For each region-bound section, if `sizeof(section) > region.size`, emit
`ERR_186` with both sizes and the excess.

### New error codes

| Code    | Meaning                                                          |
|---------|------------------------------------------------------------------|
| ERR_185 | `set_addr` targets an address outside the containing region      |
| ERR_186 | Region-bound section exceeds region size                         |

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

## Step 7 — *(Superseded by Step 2)*

Step 2 removed the `output` address argument entirely (error `ERR_50`).
Step 7 had proposed making that argument optional for region-bound sections,
but Step 2 completed first and established the authoritative design: `output`
takes a section name only; placement uses `set_addr` or an `in REGION`
binding. No work remains here.

---

## Nested Regions — Deferred

Region-to-region nesting (`region FOO in FLASH { ... }`) was considered and
explicitly deferred. The current design supports exactly one top-level section
per region. If a user needs a fixed-address sub-range, the recommended pattern
is two sibling top-level regions with non-overlapping address ranges, enforced
by ERR_183.

When nested regions are added in the future, they slot in as an extension to
`parse_region` (accept an optional `in PARENT` clause) and to Step 3's
overlap validation, without breaking any existing region or section grammar.

---

## Implementation Order and Dependencies

```
Step 1  (COMPLETE)
Step 2  (COMPLETE)
Step 3  parser + AstDb  (COMPLETE)  -- region keyword, parsing, RegionEntry, AstDb
Step 3  evaluation      (PARTIAL)   -- evaluate_regions + anchor COMPLETE; ERR_182/70/71 pending
Step 4  (COMPLETE)                  -- section-to-region binding
Step 5  (requires Step 3 eval)      -- bounds enforcement (ERR_185, ERR_186)
Step 6  (requires Step 3 eval)      -- addr()/sizeof() region resolution
Step 7  (SUPERSEDED by Step 2)
```

Each step is independently testable; a step is complete when its new tests
pass and no existing tests regress.

---

## Error Code Summary

| Code    | Step | Crate      | Meaning                                                    |
|---------|------|------------|------------------------------------------------------------|
| ERR_40  | 3    | ast        | Unknown region property name                               |
| ERR_41  | 3    | ast        | Duplicate region property                                  |
| ERR_42  | 3    | ast        | Region is missing required property `addr`                 |
| ERR_43  | 3    | ast        | Region name conflicts with a section name                  |
| ERR_44  | 4    | ast        | `in` not followed by region name in section declaration    |
| ERR_50  | 2    | ast        | `output` address arg removed; use `set_addr` (COMPLETE)    |
| ERR_51  | 4    | ast        | Section references undeclared region                       |
| ERR_52  | 4    | ast        | Second section bound to same region                        |
| ERR_53  | 3    | ast        | Expected identifier after `region` keyword                 |
| ERR_54  | 3    | ast        | Expected `{` after region name                             |
| ERR_55  | 3    | ast        | Duplicate region name                                      |
| ERR_56  | 3    | ast        | Reserved identifier used as region name                    |
| ERR_57  | 3    | ast        | Expected `=` after region property name                    |
| ERR_58  | 3    | ast        | Region name conflicts with a const name                    |
| ERR_59  | 3    | ast        | Region is missing required property `size`                 |
| ERR_224  | 1    | process    | Output size exceeds `--max-output-size` (COMPLETE)         |
| ERR_225  | —    | process    | ValidationPhase failure (assert evaluation)                |
| ERR_226  | 3    | process    | evaluate_regions failed                                    |
| ERR_180 | 3    | const_eval | Region property value is not numeric                       |
| ERR_182 | 3    | const_eval | `default_align` not a power of two or is zero (pending)    |
| ERR_183 | 3    | const_eval | Two regions have overlapping address ranges (pending)      |
| ERR_184 | 3    | const_eval | Cyclic dependency in region property expressions (pending) |
| ERR_185 | 5    | engine     | `set_addr` targets an address outside the region           |
| ERR_186 | 5    | engine     | Region-bound section exceeds region size                   |
