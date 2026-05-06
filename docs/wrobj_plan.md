# wrobj Implementation Plan

## Overview

`wrobj` reads a named section from an object file and writes the section bytes
into the Brink output:

```
wrobj(".text", "/path/to/file.elf");
```

`sizeof` gains a two-argument form that returns the byte size of a named section
without writing it:

```
assert sizeof(".text", "/path/to/file.elf") == 4096;
```

Both commands share a common parse cache built during IRDb validation.  IRDb
runs before layout_phase, so the cache is fully populated before any layout
iteration begins — no separate pre-pass is needed.

First operand (both commands): section name (quoted string).
Second operand (both commands): file path (quoted string).

Supports any format recognized by the `object` crate: ELF (32/64-bit,
little/big endian), Mach-O, PE.  Format detection is automatic.

**Mach-O naming note**: section names in Mach-O carry a double-underscore
prefix and no dot (e.g., `__text`, not `.text`).  The `object` crate's
`section_by_name` matches on these raw names, so callers must use the
platform-native form.

---

## New Error Codes

| Code    | Location                 | Condition                                                                               |
|---------|--------------------------|-----------------------------------------------------------------------------------------|
| ERR_60  | ast/ast.rs               | `wrobj` argument count is not exactly 2                                                 |
| ERR_61  | ast/ast.rs               | `sizeof` with a quoted first arg: count is not exactly 2, or second arg is missing      |
| ERR_117 | irdb/irdb.rs             | `wrobj` or `sizeof` operand is not a quoted string                                      |
| ERR_118 | irdb/irdb.rs             | File not found, not readable, or not a recognized object format                         |
| ERR_119 | irdb/irdb.rs             | Named section not found in the object file, or section has no file data (e.g. `.bss`)    |
| ERR_193 | exec_phase/exec_phase.rs | OS error opening or reading the object file at exec time                                |

---

## New Data Structures (irdb/irdb.rs)

```rust
pub struct ObjSectionInfo {
    pub path: String,
    pub section_name: String,
    pub file_offset: u64,   // byte offset of section data within the object file
    pub size: u64,
    pub src_loc: SourceSpan,
}
```

Two new fields on `IRDb`:

```rust
// Final per-section results; key = (section_name, file_path).
// Populated by both validate_wrobj_operands and validate_sizeof_obj_operands.
pub obj_sections: HashMap<(String, String), ObjSectionInfo>,

// Per-file parse cache; key = file_path, value = section_name -> (file_offset, size).
// Ensures each file is opened and parsed at most once across all wrobj and sizeof calls.
// Fully populated during IRDb validation, before layout_phase runs.
parsed_obj_files: HashMap<String, HashMap<String, (u64, u64)>>,
```

---

## Steps

### Step 1 — irdb/Cargo.toml: add `object` dependency

```toml
object = { version = "0.36", default-features = false, features = ["read", "elf", "macho", "pe"] }
```

`read` implies `read_core` + `std`.  Excludes write, wasm, xcoff, coff, archive,
and compression — none are needed.

---

### Step 2 — ast/lexer.rs: add keyword

In `scan_word`, add one entry to the keyword match:

```rust
"wrobj" => LexToken::Wrobj,
```

Add `Wrobj` to the `LexToken` enum.  No lexer change is needed for `sizeof` — the
existing `LexToken::Sizeof` token is reused; the two-argument form is distinguished
at the IRDb stage.

---

### Step 3 — ir/ir.rs: add IRKind variants

Add two variants alongside `Wrf` and `Sizeof`:

```rust
Wrobj,      // write named section from object file
SizeofObj,  // size of named section from object file
```

---

### Step 4 — ast/ast.rs: add to parser

**4a.** Add `LexToken::Wrobj` to `is_section_expr_tok`.

**4b.** In the comma-loop inside `parse_expr`, add two guards that fire when the
statement node is `LexToken::Wrobj`.  Check the child count of `print_nid` after
appending each argument:

- After the first argument is appended: if the next token is not `Comma`, emit
  ERR_60 "wrobj requires exactly 2 arguments", call `advance_past_semicolon`,
  and break.
- After the second argument is appended: if the next token is `Comma`, emit
  ERR_60 "wrobj takes exactly 2 arguments", call `advance_past_semicolon`,
  and break.

Place these checks immediately after the `wrf` guard, using
`print_nid.children(&self.arena).count()` to distinguish the first from the
second argument.

**4c.** `sizeof` already accepts one argument via `parse_pratt`.  Extend the
sizeof argument parsing to allow a second quoted-string argument: if a comma
follows the first argument and the first argument is a quoted string, parse the
second argument and produce a two-child sizeof node.  If the first argument is
a quoted string but no comma follows (only one quoted-string arg), emit ERR_61
"sizeof with a section name requires a file path as the second argument".  IRDb
distinguishes single-identifier `sizeof` from two-quoted-string `sizeof` by
operand type and count, then sets the IR kind to `SizeofObj` (see Step 6).

---

### Step 5 — linearizer/linearizer.rs: map token to IR kind

In `tok_to_irkind`, add:

```rust
LexToken::Wrobj => IRKind::Wrobj,
```

`LexToken::Sizeof` continues to map to `IRKind::Sizeof`.  IRDb upgrades it to
`IRKind::SizeofObj` when both operands are quoted strings (Step 6).

---

### Step 6 — irdb/irdb.rs: validation, caching, and IR kind upgrade

Initialize the two new fields in `IRDb::new`:

```rust
obj_sections: HashMap::new(),
parsed_obj_files: HashMap::new(),
```

#### 6a — Shared cache helper: `load_obj_file_sections`

Extract the file-open-and-parse logic into a private helper so both
`validate_wrobj_operands` and `validate_sizeof_obj_operands` call one place:

```rust
fn load_obj_file_sections(
    &mut self,
    file_path: &str,
    src_loc: &SourceSpan,
    diags: &mut Diags,
) -> bool
```

If `parsed_obj_files` already contains `file_path`, return true immediately.
Otherwise:

1. Read the file with `std::fs::read(file_path)`.  On error, emit ERR_118 and
   return false.
2. Call `object::File::parse(bytes.as_slice())`.  On error, emit ERR_118 and
   return false.
3. Iterate `obj.sections()`.  For each section, call `section.name()` and
   `section.file_range()`.  Collect into `HashMap<String, (u64, u64)>`,
   skipping sections where `file_range()` is `None`.
4. Insert the map into `parsed_obj_files` under `file_path`.  Return true.

**`object` API reference:**

```rust
use object::{File, Object, ObjectSection};

let bytes = std::fs::read(file_path)?;
let obj   = File::parse(bytes.as_slice())?;
for section in obj.sections() {
    if let Ok(name) = section.name() {
        if let Some((offset, size)) = section.file_range() {
            section_map.insert(name.to_string(), (offset, size));
        }
    }
}
```

#### 6b — `validate_wrobj_operands`

1. Assert `ir.operands.len() == 2`.
2. Check both operands have `DataType::QuotedString`; emit ERR_117 if not.
3. Extract `section_name` from `operands[0]`, `file_path` from `operands[1]`.
4. Return true if `obj_sections` already has `(section_name, file_path)`.
5. Call `load_obj_file_sections(file_path, ...)`.  Return false on failure.
6. Look up `section_name` in `parsed_obj_files[file_path]`.  Emit ERR_119 and
   return false if absent.
7. Construct and insert `ObjSectionInfo`; return true.

Wire: `IRKind::Wrobj => self.validate_wrobj_operands(ir, diags)`.

#### 6c — `validate_sizeof_obj_operands`

Called when `IRKind::Sizeof` has 2 operands both of type `QuotedString`.
Logic mirrors `validate_wrobj_operands` (steps 1–7 above, same error codes).
After successful validation, update the IR kind in `ir_vec`:

```rust
self.ir_vec[idx].kind = IRKind::SizeofObj;
```

This upgrade ensures layout_phase and exec_phase see the correct variant without
re-checking operand types.

In `validate_ir`, route `IRKind::Sizeof` through a dispatch that checks operand
types before calling either the existing single-identifier handler or
`validate_sizeof_obj_operands`:

```rust
IRKind::Sizeof => self.validate_sizeof_dispatch(idx, ir, diags),
```

---

### Step 7 — layout_phase/layout_phase.rs: iterate_wrobj and iterate_sizeof_obj

**iterate_wrobj** (alongside `iterate_wrf`):

```rust
fn iterate_wrobj(&mut self, ir: &IR, irdb: &IRDb, diags: &mut Diags, current: &mut Location) -> bool {
    let key = (self.parms[ir.operands[0]].to_str().to_string(),
               self.parms[ir.operands[1]].to_str().to_string());
    let info = irdb.obj_sections.get(&key).unwrap(); // validated by IRDb
    current.advance(info.size, &ir.src_loc, diags)
}
```

Wire: `IRKind::Wrobj => self.iterate_wrobj(ir, irdb, diags, &mut current)`.

**SizeofObj in iterate_sizeof** (or a minimal new branch):

`iterate_sizeof` already looks up `irdb.region_bindings` as a fallback after
`sized_locs`.  Add a third lookup: if the IR kind is `SizeofObj`, read
`operands[0]` (section name) and `operands[1]` (file path), look up
`irdb.obj_sections`, and return `info.size` as the sizeof value.

---

### Step 8 — exec_phase/exec_phase.rs: execute_wrobj and SizeofObj no-op

**execute_wrobj** (alongside `execute_wrf`).  Does not need the `object` crate —
the byte range was pre-computed by IRDb:

```rust
fn execute_wrobj(...) -> Result<()> {
    let key = (argvaldb.parms[ir.operands[0]].to_str().to_owned(),
               argvaldb.parms[ir.operands[1]].to_str().to_owned());
    let info = irdb.obj_sections.get(&key).unwrap();

    let loc  = &location_db.ir_locs[lid];
    let addr = loc.addr.addr_base + loc.addr.addr_offset;
    if !Self::check_and_record_range(written_ranges, addr, info.size, ir.src_loc.clone(), diags) {
        return Err(anyhow!("Address overwrite detected"));
    }

    let mut f = File::open(&info.path).map_err(|e| {
        diags.err1("ERR_193", &format!("Opening '{}' failed: {:?}", info.path, e.raw_os_error()), ir.src_loc.clone());
        anyhow!(e)
    })?;
    f.seek(SeekFrom::Start(info.file_offset))?;
    // read info.size bytes and write to output (same chunk loop as execute_wrf)
}
```

Wire: `IRKind::Wrobj => Self::execute_wrobj(...)`.

**SizeofObj**: add `IRKind::SizeofObj` to the no-op arm alongside `IRKind::Sizeof`
— size is computed at layout time, nothing to execute.

---

### Step 9 — Test fixture

Create `tests/wrobj_test.elf`: a minimal ELF64 LE x86-64 object file with two
sections of known content:

- `.rodata` — 4 bytes: `[0xDE, 0xAD, 0xBE, 0xEF]`
- `.text`   — 2 bytes: `[0x90, 0x90]` (NOP NOP)

Generate with:

```sh
echo -e 'section .rodata\ndb 0xDE, 0xAD, 0xBE, 0xEF\nsection .text\nnop\nnop' \
    | nasm -f elf64 -o tests/wrobj_test.elf /dev/stdin
```

Check in both the source (`tests/wrobj_test.asm`) and the pre-built binary
(`tests/wrobj_test.elf`).

**wrobj fixtures:**

`tests/wrobj_rodata.brink` — success:
```
section foo { wrobj(".rodata", "tests/wrobj_test.elf"); }
output foo;
```

`tests/wrobj_bad_section.brink` — ERR_119:
```
section foo { wrobj(".nonexistent", "tests/wrobj_test.elf"); }
output foo;
```

`tests/wrobj_bad_file.brink` — ERR_118:
```
section foo { wrobj(".rodata", "tests/no_such_file.elf"); }
output foo;
```

`tests/wrobj_wrong_args.brink` — ERR_60:
```
section foo { wrobj(".rodata"); }
output foo;
```

**sizeof fixtures:**

`tests/sizeof_obj.brink` — success (assert result equals known section size):
```
section foo {
    assert sizeof(".rodata", "tests/wrobj_test.elf") == 4;
    assert sizeof(".text",   "tests/wrobj_test.elf") == 2;
    wr8 0;
}
output foo;
```

`tests/sizeof_obj_bad_section.brink` — ERR_119:
```
section foo {
    assert sizeof(".nonexistent", "tests/wrobj_test.elf") == 0;
    wr8 0;
}
output foo;
```

---

### Step 10 — Integration tests (tests/integration.rs)

```rust
#[test]
fn wrobj_rodata() {
    assert_brink_success("tests/wrobj_rodata.brink", Some("wrobj_rodata.bin"), None);
    let bytes = fs::read("wrobj_rodata.bin").unwrap();
    assert_eq!(bytes, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    fs::remove_file("wrobj_rodata.bin").unwrap();
}

#[test]
fn wrobj_bad_section() {
    assert_brink_failure("tests/wrobj_bad_section.brink", &["[ERR_119]"]);
}

#[test]
fn wrobj_bad_file() {
    assert_brink_failure("tests/wrobj_bad_file.brink", &["[ERR_118]"]);
}

#[test]
fn wrobj_wrong_args() {
    assert_brink_failure("tests/wrobj_wrong_args.brink", &["[ERR_60]"]);
}

#[test]
fn sizeof_obj() {
    assert_brink_success("tests/sizeof_obj.brink", None, None);
}

#[test]
fn sizeof_obj_bad_section() {
    assert_brink_failure("tests/sizeof_obj_bad_section.brink", &["[ERR_119]"]);
}
```

---

### Step 11 — docs/error_codes.md

Add ERR_60, ERR_61, ERR_117, ERR_118, ERR_119, ERR_193.

Update the next-available line:

```
Next available per prefix: ERR_62, ERR_194, ERR_202, ERR_120, ERR_216, ERR_228, ERR_232.
```

---

### Step 12 — docs/ai updates

**02-system.yaml** `language_features`: add both new forms:

```yaml
wrobj: write a named section from an ELF/Mach-O/PE object file
sizeof: also accepts (section_name, file_path) to query an object file section size
```

**05-update-tracker.md**: add a new entry summarizing the change, test count,
and new error codes.

---

## Implementation Order

Steps 1–5 are mechanical; complete in one pass.  Step 6 is the only step with
non-trivial logic — implement `load_obj_file_sections` first, then wire both
validators.  Steps 7–8 are straightforward once the cache is in place.  Step 9
(test fixture) can be prepared in parallel with Steps 6–8.
