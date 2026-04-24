# Engine Refactor Plan

## 1. Goal
Split the current monolithic `engine` crate (which handles iteration, layout extraction, and execution) into three distinct, decoupled phases/crates. This enforces the Single Responsibility Principle and eliminates leaky abstractions (like exposing `wr_dispatches` and `label_dispatches` from `Engine` to `map.rs`).

## 2. The Three Phases

### Phase 1: `LayoutPhase` (Pure Layout Phase)
- **Role:** Iteratively evaluate the intermediate representation (`IRDb`) until memory locations (sizes, offsets, absolute addresses) stabilize.
- **Input:** `IRDb`, `ExtensionRegistry` (needed for `sizeof_ext` iteration).
- **Output:** `LocationDb` (a new data structure encapsulating the stabilized `ir_locs` array and basic location methods).
- **Architecture:** This phase strictly computes *where* things go. It does not perform semantic grouping of locations into "Sections" or "Labels". It fully replaces `Engine::iterate`.

### Phase 2: `MapPhase` (Post-Processing & Mapping Phase)
- **Role:** Semantically process the raw location data. Pairs `SectionStart` with `SectionEnd`, finds `Label` addresses, and builds containers of useful layout information.
- **Input:** `IRDb`, `LocationDb`, and `SymbolTable` (for consts).
- **Output:** `MapDb` (an expanded version of the existing `MapDb` that serves as the definitive source of truth for section bounds, labels, and constants).
- **Architecture:** We will absorb the existing `map` crate and expand it. We'll move the `build_dispatches` logic out of the old Engine and into `MapDb::new()`. This completely removes the redundant `WrDispatch` and `LabelDispatch` structs, using `SectionEntry` and `LabelEntry` as the sole source of truth.

### Phase 3: `ExecPhase` (Execution & Generation Phase)
- **Role:** Write the actual binary image, run extensions, and validate layout-time assertions.
- **Input:** `IRDb`, `LocationDb`, `MapDb`, and `ExtensionRegistry`.
- **Output:** The final binary file on disk.
- **Architecture:** Contains `execute_core_operations` (writing bytes), `execute_extensions` (patching memory map), and `execute_validation` (assertions). `execute_extensions` will directly query `MapDb` to resolve `Slice` parameter boundaries instead of relying on internal dispatch lists.

## 3. Pipeline Update (`process/process.rs`)
The pipeline execution order will be updated from:
`IRDb` -> `Engine` (which internally builds dispatches and executes) -> `Map`
To a clean, linear data flow:
`IRDb` -> `LayoutPhase` (returns `LocationDb`) -> `MapPhase` (returns semantic map) -> `ExecPhase` (writes binary, relies on `MapDb`).

## 4. Architectural Analysis (Step Forward vs. Backwards)
**This redesign is a significant step forward in maintainability.**
- **Eliminates Data Duplication:** Currently, `Engine::wr_dispatches` and `MapDb::sections` represent the exact same concept using two different types copied 1-to-1. This refactor removes that duplication.
- **Strict Encapsulation:** The executor no longer holds `pub` fields just so downstream systems can read them. Data flows strictly in one direction.
- **Testability:** Each phase can now be unit-tested in isolation (e.g., testing `MapDb` extraction from raw `LocationDb` without needing to execute a file to disk).

The only minor downside is the introduction of two new crates (`evaluator` and `executor`) replacing one (`engine`), which adds a small amount of boilerplate (`Cargo.toml` files). However, Brink's architecture is already highly decoupled and pipeline-oriented (e.g., `ast`, `const_eval`, `prune`, `layoutdb`, `irdb`), so this split aligns perfectly with the established architectural best practices of the project.
