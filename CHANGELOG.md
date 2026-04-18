# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [6.x.x] - 2026-04-17
- FEATURE: added --max-output-size CLI option.
- BUG FIX: Fixed assert when wrf had too man args
- BUG FIX: Fixed assert when wr8..wr64 had too man args
- BUG FIX: Fixed bad bool conversion assert
- BUG FIX: Fixed unlimited const evaluation recursion
- INTERNAL: Upgrade to latest md-5 crate.
- INTERNAL: Upgrade to latest parse_int crate.
- INTERNAL: Remove unused codespan-reporting crate
- INTERNAL: Upgrade to latest ariadne crate
- INTERNAL: Upgrade to latest predicates crate
- INTERNAL: Upgrade to latest assert crate
- INTERNAL: Upgrade to latest serial_test crate
- INTERNAL: Renamed and improved the fuzz_help.md


## [6.0.0] - 2026-04-16

- EXTENSION API BREAKING CHANGE: Generalized and unified extension parameter passing.
- FEATURE: Extensions support named arguments, as in foo:bar(stuff=42);
- BUG FIX: Fixed to_i64() and to_u64() in const expressions
- BUG FIX: Use checked arithmetic for extension size.
- INTERNAL: Fixed unused import clippy warning
- INTERNAL: Cleaned up dead code in const evaluation.
- INTERNAL: Refactored common code into coerce_numeric_pair
- INTERNAL: Fixed is_none + unwrap anti-pattern.
- INTERNAL: Refactor counter advance into one function
- INTERNAL: Changed a saturating add to checked add.
- INTERNAL: Simplified extension call handling in the IR.
- INTERNAL: Implemented Rustc-style AST storage for synthetic nodes.
- INTERNAL: Added large comment about how brink handles include files.
- INTERNAL: renamed ext to extension_registry

## [5.0.6] - 2026-04-10

- Since Brink is an application, added cargo lock to github.
  The audit-check action on github requires the lockfile.
- Added std::md5 extension.
- Added a vscode syntax highlighting extension.  See the bottom of the README for instructions.
- `if/else` expressions still require const conditional evaluation, but the
  conditional blocks can now contain structural statements, e.g. wr, set_addr, etc.
- Added prebuilt binaries for homebrew

## [5.0.5] - 2026-04-09

- Internal refactoring: Replace Logos with a hand-rolled lexer.
  This reduces build dependencies and eliminated a known security vulnerability.
- Removed octal format -D defines.  Brink has never supported octal constants.
- Added security audit workflow.
- Added security audit badge to README.
- Added MIT license badge to README.

## [5.0.4] - 2026-04-09

- Automated github actions now create release binaries.

## [5.0.3] - 2026-04-09

- Trying again to automate github actions to create release binaries.

## [5.0.2] - 2026-04-09

- Trying again to automate github actions to create release binaries.

## [5.0.1] - 2026-04-09

- Trying to automate github actions to create release binaries.

## [5.0.0] - 2026-04-09

- Support for if/else in const expressions
- Support for Rust style map file output
- Many more unit tests
- std::crc32c extension
- std::sha256 extension

## [3.0.0] - 2026-03-25

- Large upgrade of Brink capability with many breaking changes.
- Completed extension infrastructure
- Completed section scoping for address and offset ranges
- Completed address overwrite detection
- Many commands have more obvious names now.

## [2.5.0] - 2026-03-25

- Support for extensions
- Global scope asserts

## [2.4.0] - 2026-03-21

- Added --map-c99 option.
- Changed --map/--map-hf to --map-csv.
- Updated --map-csv output to be more spreadsheet friendly.

## [2.3.0] - 2026-03-21

- Added include directive.

## [2.2.2] - 2026-03-20

- Resolved clippy warnings.

## [2.2.1] - 2026-03-12

- Minor human friendly map file fixes.

## [2.2.0] - 2026-03-12

- Added command line const definitions.

## [2.1.0] - 2026-03-12

- Added command line option to generate human friendly map file.

## [2.0.0] - 2026-03-12

- Added const support
- Catch use of reserved words
- Readme cleanup and new sections.

## [1.2.7] - 2026-03-12

- Alphabetize lexer tokens.

## [1.2.6] - 2026-03-12

- Extended `error_codes_are_unique` scanner to include `ir/ir.rs`.
- Renamed duplicate error code `IR_3` (ambiguous-integer overflow) to `IR_4`; updated `fuzz_found_10` test accordingly.
- Added tests `integer_overflow_i64` (`[IR_4]`) and `integer_overflow_u64` (`[IR_1]`) to cover integer literal overflow paths in `ir/ir.rs`.

## [1.2.5] - 2026-03-12

- Added `error_codes_are_unique` test that scans all source files and asserts no diagnostic error code string appears at more than one call site.
- Fixed pre-existing duplicate: `IRDB_9` was used at two sites in `irdb/irdb.rs`; the second site is now `IRDB_15`.

## [1.2.4] - 2026-03-12

- Pipeline stage constructors (`Ast::new`, `LinearDb::new`, `IRDb::new`, `Engine::new`) now return `Result<T, ()>` instead of `Option<T>`, enabling idiomatic `?`-based error propagation.

## [1.2.3] - 2026-03-12

- Reduce wr8...wr64 boilerplate with a parameterized enum.

## [1.2.2] - 2026-03-08

- Replaced the outdated `fern` and `log` crates with the modern `tracing` and `tracing-subscriber` ecosystem for structured CLI diagnostics.

## [1.2.1] - 2026-03-08

- Upgraded the error reporting engine to use `ariadne` for compiler diagnostics.

## [1.2.0] - 2026-03-08

- Fixed accidental integer infix operations in the Pratt parser.
- Corrected bitwise AND (`&`) and OR (`|`) operator precedence bug.  Behavior now
  matches brink documentation.

## [1.1.0] - 2026-03-07

- Refactored CLI argument parsing to use the `clap` v4 Derive API.
- Implemented Cargo Workspace Inheritance
- Fixed unchecked overflow bugs on the location counter
- Added tests to guarantee arithmetic boundary safety.

## [1.0.2] - 2026-02-25

- Updated Rust edition to 2024
- Updated dependent package versions with `cargo update`
- Fixed all `cargo clippy` warnings.

## [1.0.1] - 2021-07-05

- Warn on `rust_2018_idioms`
- Remove unused extern crate `clap` in main.rs
- Forbid unsafe Rust
- Added this changelog file

