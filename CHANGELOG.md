# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.1.1] - 2026-03-08
- Fixed accidental integer infix operations in the Pratt parser.
- Corrected bitwise AND (`&`) and OR (`|`) operator precedence bug.

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

