# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

