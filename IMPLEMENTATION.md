# Implementation Plan

TDD-aligned, self-contained steps derived from `PRD.md`. Each step compiles and passes tests independently.

Test FST files are available at `/home/sjalloq/Work/fst-reader/fsts/` and `./waves.fst`.

---

## Step 1: Core types

**Goal**: Define `ScopeId`, `VarId`, `SignalId`, enum types, `SignalValue`, `WaveformMetadata`.

**Files to create**:
- `crates/fstty-core/src/types.rs`

**Tests**:
- ID types: construct, copy, compare, hash, use as HashMap keys
- Enum types: construct each variant, Debug format
- SignalValue: construct Binary and Real variants

**Done when**: `cargo test -p fstty-core` passes with new types tests. Existing code unmodified (types module added alongside, not yet used).

---

## Step 2: Hierarchy struct and HierarchyBuilder

**Goal**: Define `Hierarchy` data structure and `HierarchyBuilder` that constructs it from `HierarchyEvent`s.

**Files to create/modify**:
- `crates/fstty-core/src/hierarchy.rs` — replace existing content with new `Hierarchy` + `HierarchyBuilder`

**Tests** (all pure, no file I/O):
- Build a small hierarchy programmatically: 2 scopes, 3 vars
- Verify `top_scopes()` returns correct ids
- Verify `scope_children()`, `scope_vars()`, `scope_parent()`
- Verify `scope_name()`, `scope_type()`, `scope_full_path()`
- Verify `var_name()`, `var_width()`, `var_type()`, `var_direction()`, `var_signal_id()`
- Verify `var_full_path()` constructs dotted path
- Verify `find_vars("*.clk")` glob matching
- Verify counts: `scope_count()`, `var_count()`, `signal_count()`
- Verify nested scopes (3+ levels deep)
- Verify alias: two vars with same SignalId

**Done when**: `cargo test -p fstty-core` passes. Old hierarchy code may coexist temporarily.

---

## Step 3: WaveformSource trait and error types

**Goal**: Define the `WaveformSource` trait and fstty-core's `Error`/`Result` types.

**Files to create/modify**:
- `crates/fstty-core/src/waveform.rs` — replace with trait definition
- `crates/fstty-core/src/error.rs` — update error types if needed
- `crates/fstty-core/src/lib.rs` — update module exports

**Tests**:
- Compile-time only: verify the trait is object-safe (`Box<dyn WaveformSource>` compiles)
- Mock implementation: a `TestSource` with hardcoded hierarchy and canned signal data, verify `read_signals` callback receives expected (time, signal, value) tuples

**Done when**: `cargo test -p fstty-core` passes. Trait is defined and usable.

---

## Step 4: Wellen hierarchy adapter

**Goal**: Build a `Hierarchy` from wellen's parsed hierarchy. This is the bridge: walk wellen's arena, emit `HierarchyEvent`s, feed them to `HierarchyBuilder`.

**Files to create**:
- `crates/fstty-core/src/wellen_adapter.rs`

**Tests** (require a real FST file):
- Open `waves.fst` (or a test FST from fst-reader) with wellen
- Build `Hierarchy` via the adapter
- Verify `scope_count()` > 0, `var_count()` > 0
- Verify a known scope name exists at top level
- Verify `scope_type()` returns expected variant (e.g. Module)
- Verify `var_full_path()` returns dotted path
- Verify `signal_count()` matches wellen's `num_unique_signals()`

**Done when**: `cargo test -p fstty-core` passes. Wellen hierarchy can be converted to our Hierarchy.

---

## Step 5: Add fst-reader dependency

**Goal**: Add fst-reader (and fst-writer) as workspace dependencies. Verify they build.

**Files to modify**:
- `Cargo.toml` (workspace) — add fst-reader, fst-writer git deps
- `crates/fstty-core/Cargo.toml` — add fst-reader, fst-writer

**Tests**:
- Smoke test: open an FST file with `fst_reader::FstReader::open()`, read header, assert signal count > 0

**Done when**: `cargo build` and `cargo test -p fstty-core` pass with new dependencies.

---

## Step 6: FstSource — WaveformSource for FST files

**Goal**: Implement `FstSource` that uses wellen for hierarchy (via step 4 adapter) and fst-reader for signal reading.

**Files to create**:
- `crates/fstty-core/src/fst/mod.rs`
- `crates/fstty-core/src/fst/source.rs`

**Tests**:
- `FstSource::open()` on a test FST: verify metadata (timescale, time range, counts)
- `hierarchy()` returns navigable hierarchy (spot-check a scope name)
- `read_signals()` with one signal over full time range: verify callback fires with correct signal id
- `read_signals()` with time filter: verify no callbacks outside range
- `read_signals()` with multiple signals: verify both signal ids appear
- Verify `FstSource` can be used as `Box<dyn WaveformSource>`

**Done when**: `cargo test -p fstty-core` passes. FST files can be opened and queried through the trait.

---

## Step 7: FstSource — block info and export

**Goal**: Expose block metadata and implement filtered FST export (fast path).

**Files to create**:
- `crates/fstty-core/src/fst/export.rs`

**Tests**:
- `block_infos()`: verify block count > 0, time ranges are contiguous/non-overlapping
- `export_filtered()`: export a subset of signals from a test FST, verify output file exists
- Round-trip: open exported FST with fst-reader, verify it has correct signal count
- Round-trip: read a signal from exported FST, verify values match source

**Done when**: `cargo test -p fstty-core` passes. Filtered FST export produces valid files.

---

## Step 8: Migrate TUI to fstty-core types

**Goal**: Replace all wellen imports in fstty-tui with fstty-core types. Remove wellen from fstty-tui's dependencies.

**Files to modify**:
- `crates/fstty-tui/Cargo.toml` — remove wellen dep
- `crates/fstty-tui/src/app.rs` — use `FstSource` / `Box<dyn WaveformSource>` instead of `WaveformFile`
- `crates/fstty-tui/src/hierarchy_browser.rs` — use `ScopeId`, `VarId`, `Hierarchy` instead of wellen types; remove unsafe transmutes
- `crates/fstty-tui/src/components/tree.rs` — use fstty-core types

**Tests**:
- `cargo build -p fstty-tui` succeeds with zero wellen imports
- `cargo test -p fstty-tui` passes (existing tests still work)
- Grep confirms no `use wellen` in fstty-tui

**Done when**: TUI compiles and works using only fstty-core's public API. No wellen types leak.

---

## Step 9: Simplify tabs to Browse + Export

**Goal**: Remove Convert, Filter, Analyze tabs. Add Export tab placeholder.

**Files to modify**:
- `crates/fstty-tui/src/app.rs`

**Tests**:
- Only Browse and Export in `Tab::ALL`
- Tab switching via 1/2 and Tab/Shift-Tab
- `cargo build -p fstty-tui` succeeds

**Done when**: Two-tab UI compiles and runs.

---

## Step 10: Export tab UI and wiring

**Goal**: Build the Export tab with VC block timeline, selection, and wired-up export.

**Files to create**:
- `crates/fstty-tui/src/export_state.rs`

**Files to modify**:
- `crates/fstty-tui/src/app.rs`

**Tests**:
- `ExportState` unit tests: state machine (no selection -> start -> range -> clear)
- `ExportState::move_cursor()` clamped to bounds
- `ExportState::has_valid_range()` correctness
- Integration: export produces valid FST (delegates to step 7's `export_filtered`)

**Done when**: User can browse hierarchy, select signals, select block range, export filtered FST. Full workflow end-to-end.

---

## Step 11: Clean up old code

**Goal**: Remove dead code from pre-refactor.

**Files to remove/modify**:
- Remove old `crates/fstty-core/src/writer.rs` (replaced by fst/export.rs)
- Remove `crates/fstty-core/examples/load_hierarchy.rs` (uses wellen directly)
- Remove `crates/fstty-tui/src/components/tree.rs` if fully replaced by hierarchy_browser
- Clean up any remaining unused imports, dead modules
- Remove `fstapi` from workspace deps if no longer used

**Tests**:
- `cargo build` succeeds
- `cargo test` passes
- `cargo clippy` clean (no dead code warnings)

**Done when**: No dead code. Clean build.

---

## Future steps (not in scope now)

- **Signal server**: fstty-server crate with Arrow Flight
- **VCD/GHW backend**: `VcdSource` implementing `WaveformSource` via wellen
- **Enhanced title bar**: metadata display with timescale formatting
- **Screenshot test infrastructure**

---

## Log

All changes, decisions, and issues are logged below as work progresses.

### Template

```
#### STEP-N: Title — YYYY-MM-DD

**Status**: not started | in progress | complete | blocked

**Changes**:
- file: description of change

**Tests added**:
- description

**Issues**:
- description

**Decisions**:
- description
```
