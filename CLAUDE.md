# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and test

```sh
cargo build                              # full workspace
cargo test -p fstty-core                 # core tests only
cargo test -p fstty-tui                  # TUI tests only
cargo test -p fstty-core -- test_name    # single test
cargo clippy                             # lint
cargo run -p fstty-tui -- path/to/file.fst  # run the TUI
```

**Local sibling dependencies**: `fst-reader` and `fst-writer` are referenced as local path deps (`../fst-reader`, `../fst-writer`). These repos must exist alongside this one for builds to work.

## Architecture

fstty is a two-crate workspace: `fstty-core` (data model + backends) and `fstty-tui` (terminal UI). A strict abstraction boundary separates them.

### Two-library backend strategy

The FST backend uses **two** libraries with no overlap:
- **wellen** — parses the FST header to build the signal hierarchy (scopes, vars, metadata)
- **fst-reader** — reads signal value-change data and provides raw block access for fast filtered export

Neither library's types appear in fstty-core's public API. Both are wrapped behind fstty's own types (`Hierarchy`, `ScopeId`, `VarId`, `SignalId`, `WaveformSource`).

### fstty-core key modules

- `types.rs` — Opaque IDs (`ScopeId`, `VarId`, `SignalId`), enums (`ScopeType`, `VarType`, `VarDirection`), `SignalValue`, `WaveformMetadata`
- `hierarchy.rs` — `Hierarchy` struct (built once at load, read-only) + `HierarchyBuilder` consuming `HierarchyEvent`s
- `waveform.rs` — `WaveformSource` trait with single data method: `read_signals(signals, time_range, callback)`
- `wellen_adapter.rs` — bridge that walks wellen's hierarchy arena and emits `HierarchyEvent`s
- `fst/source.rs` — `FstSource` implementing `WaveformSource` (wellen for hierarchy, fst-reader for data)
- `fst/export.rs` — filtered FST export via raw block copy (no decompression), `BlockInfo`, `ExportConfig`, `ExportResult`

### fstty-tui key modules

- `app.rs` — main `App` struct, event loop, two tabs (Browse + Export), rendering
- `hierarchy_browser.rs` — tree navigation over `Hierarchy`, tri-state selection (Recursive/ScopeOnly/None), filtering by scope type
- `export_state.rs` — state machine for VC block range selection (idle → anchor → finalized → clear)
- `file_picker.rs` — FST file selection dialog

### Critical type boundary

The TUI crate **must not** import `wellen` or `fst-reader` types. It only sees fstty-core's public API. ID types have `pub(crate)` inner fields — backends can construct them but the TUI cannot.

## Project rules

- Follow `PRD.md` for architecture and design decisions.
- Follow `IMPLEMENTATION.md` for the step-by-step build plan.
- Log all changes, issues, and decisions in the Log section of `IMPLEMENTATION.md` as work progresses.
- Each implementation step must be self-contained: it compiles and its tests pass independently.
- Write tests before or alongside implementation (TDD). Do not skip tests.
- No wellen types in fstty-tui. All wellen usage is confined to fstty-core backend wrappers.
- No fst-reader types in fstty-tui. The TUI only sees fstty-core's public API.
- Do not make architectural changes without updating the PRD first.

## Test data

- Do NOT use FST files in the repo root (e.g. `waves.fst`) for tests.
- Use the local test fixtures in `crates/fstty-core/tests/fixtures/` (copied from fst-reader, covering different encodings).
- Reference fixtures with `concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/<file>.fst")`.
- Available fixtures and their encoding coverage:
  - `rv32_soc_TB.vcd.fst` — LZ4 + DynamicAlias2 (common case, Icarus Verilog)
  - `des.fst` — Zlib + DynamicAlias (GTKWave)
  - `transaction.fst` — Zlib + Standard block kind (GTKWave)
  - `waveform.vcd.fastlz.fst` — FastLZ compression (SystemC)
  - `multi_vc_block.fst` — multiple VC blocks (fst-writer generated)
  - `sigmoid_tb.vcd.fst` — real (floating point) signals (MyHDL)

## TODO: Before release

- ~~`Cargo.toml` updated to point at GitHub repos instead of local paths.~~
- `fst-reader` and `fst-writer`: changes are on `fst-filter` branches — merge to main when stable.
