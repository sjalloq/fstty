# Implementation Plan

TDD-aligned, self-contained steps derived from `PRD.md`. Each step compiles and passes tests independently.

Test FST fixtures are in `crates/fstty-core/tests/fixtures/`. Do NOT use root-level FST files (e.g. `waves.fst`) for tests — they contain proprietary data.

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

#### STEP-1: Core types — 2026-03-07

**Status**: complete

**Changes**:
- `crates/fstty-core/src/types.rs`: created with `ScopeId`, `VarId`, `SignalId`, `ScopeType`, `VarType`, `VarDirection`, `SignalValue`, `WaveformMetadata`
- `crates/fstty-core/src/lib.rs`: added `pub mod types;`

**Tests added**:
- `id_copy_and_compare`: construct, copy, compare all ID types
- `id_as_hashmap_key`: use ScopeId, VarId, SignalId as HashMap keys
- `id_debug_format`: verify Debug output format
- `scope_type_variants`: construct and Debug-format all ScopeType variants
- `var_type_variants`: construct and Debug-format all VarType variants
- `var_direction_variants`: construct and Debug-format all VarDirection variants
- `signal_value_binary`: construct Binary variant, verify contents
- `signal_value_real`: construct Real variant, verify value
- `waveform_metadata_construct`: construct WaveformMetadata, verify all fields

**Issues**: none

**Decisions**:
- ID inner fields are `pub(crate)` so backends can construct them but TUI cannot
- Enum variants based on FST/VCD spec coverage (12 scope types, 22 var types, 6 directions)

#### STEP-2: Hierarchy struct and HierarchyBuilder — 2026-03-07

**Status**: complete

**Changes**:
- `crates/fstty-core/src/hierarchy.rs`: replaced with new `Hierarchy`, `HierarchyBuilder`, `HierarchyEvent` using fstty-core's own types
- `crates/fstty-core/src/hierarchy_legacy.rs`: old wellen-based `HierarchyNavigator`/`HierarchyNode`/`VisibleNodeIterator` preserved here for TUI compatibility
- `crates/fstty-core/src/lib.rs`: added `pub mod hierarchy_legacy;`, re-exports point to legacy module
- `crates/fstty-core/src/filter.rs`: updated import to `hierarchy_legacy`
- `crates/fstty-tui/src/components/tree.rs`: updated import to `hierarchy_legacy`

**Tests added** (all pure, no file I/O):
- `top_scopes`: verify top-level scope returned correctly
- `scope_children`: verify child scopes
- `scope_vars`: verify vars in a scope
- `scope_parent`: verify parent/None for top-level
- `scope_name_and_type`: verify name and ScopeType
- `scope_full_path`: verify dotted path construction
- `var_metadata`: verify name, width, type, direction, signal_id
- `var_full_path`: verify dotted path for vars
- `find_vars_glob`: glob matching on full paths
- `find_vars_invalid_pattern`: invalid glob returns empty
- `counts`: scope_count, var_count, signal_count
- `deep_nesting`: 4-level deep hierarchy, verify full path
- `alias_same_signal_id`: two vars with same SignalId, signal_count=1
- `empty_hierarchy`: empty builder produces empty hierarchy

**Issues**: none

**Decisions**:
- Old hierarchy code moved to `hierarchy_legacy.rs` to coexist (TUI still depends on wellen types until Step 8)
- Glob `*` matches dots in signal paths (e.g. `top.*` matches `top.sub.data`) — standard glob behavior since paths use `.` not `/` as separator
- `HierarchyBuilder` tracks unique signals via `HashSet<SignalId>` for correct `signal_count()` with aliases

#### STEP-3: WaveformSource trait and error types — 2026-03-07

**Status**: complete

**Changes**:
- `crates/fstty-core/src/waveform.rs`: replaced with `WaveformSource` trait definition
- `crates/fstty-core/src/waveform_legacy.rs`: old `WaveformFile`/`WaveformFormat` preserved here for TUI compatibility
- `crates/fstty-core/src/lib.rs`: added `pub mod waveform_legacy;`, re-exports updated (`WaveformSource` from new module, `WaveformFile`/`WaveformFormat` from legacy)
- `crates/fstty-core/src/writer.rs`: updated import to `waveform_legacy`

**Tests added**:
- `object_safe`: verifies `Box<dyn WaveformSource>` compiles, metadata and hierarchy accessible through trait object
- `mock_read_signals_all`: TestSource with canned data, read all signals over full range, verify all 6 changes received
- `mock_read_signals_time_filter`: verify time range filtering (only changes in 10..25)
- `mock_read_signals_signal_filter`: verify signal filtering (only signal 1)
- `mock_via_trait_object`: verify `read_signals` works through `Box<dyn WaveformSource>` dynamic dispatch

**Issues**: none

**Decisions**:
- Same legacy-module pattern as Step 2: old waveform code moved to `waveform_legacy.rs` to coexist until TUI migration in Step 8
- Error types (`error.rs`) unchanged — existing variants are sufficient for the trait
- `WaveformSource::read_signals` takes `Range<u64>` for time filtering, matching PRD spec

#### STEP-4: Wellen hierarchy adapter — 2026-03-07

**Status**: complete

**Changes**:
- `crates/fstty-core/src/wellen_adapter.rs`: created with `build_hierarchy_from_wellen()` function that walks wellen's hierarchy arena and emits `HierarchyEvent`s into `HierarchyBuilder`
- `crates/fstty-core/src/lib.rs`: added `pub mod wellen_adapter;`

**Tests added** (all use a small FST file: `fst-reader/fsts/icarus/rv32_soc_TB.vcd.fst`):
- `scope_count_nonzero`: verify converted hierarchy has scopes
- `var_count_nonzero`: verify converted hierarchy has vars
- `top_scope_name`: verify top-level scope has a non-empty name
- `scope_type_is_module`: verify top-level scope type is Module (Verilog)
- `var_full_path_is_dotted`: verify at least one var has a dotted hierarchical path
- `signal_count_matches_wellen`: verify our `signal_count()` equals wellen's `num_unique_signals()`
- `scope_full_path_matches_wellen`: verify top-level scope full paths match between wellen and our hierarchy

**Issues**: none

**Decisions**:
- Uses `scope.items()` iterator for correct declaration-order traversal (scopes and vars interleaved), not separate `scopes()`/`vars()` iterators
- VHDL scope types (VhdlArchitecture, etc.) map to `ScopeType::Module` as a reasonable default
- VHDL var types (Boolean, StdLogic, etc.) map to closest VarType equivalent (Logic, Integer)
- wellen's `VarDirection::Unknown` maps to `VarDirection::Implicit`
- Signal width: BitVector uses its length, Real→64, String→0
- `is_alias` always false; `HierarchyBuilder` deduplicates via `SignalId` in its `HashSet`

#### STEP-5: Add fst-reader dependency — 2026-03-07

**Status**: complete

**Changes**:
- `Cargo.toml` (workspace): added `fst-reader` and `fst-writer` as local path dependencies (`../fst-reader`, `../fst-writer`)
- `crates/fstty-core/Cargo.toml`: added `fst-reader` and `fst-writer` workspace dependencies
- `crates/fstty-core/src/lib.rs`: added `fst_reader_smoke_tests` test module

**Tests added**:
- `open_fst_and_read_header`: opens `icarus/rv32_soc_TB.vcd.fst` with `fst_reader::FstReader::open()`, verifies `signal_count() > 0`, `var_count > 0`, and `end_time >= start_time`

**Issues**: none

**Decisions**:
- Used local path deps (`../fst-reader`, `../fst-writer`) rather than git deps, matching the development setup
- fst-reader pulls in its own fst-writer git dep; both coexist without conflict

#### STEP-6: FstSource — WaveformSource for FST files — 2026-03-07

**Status**: complete

**Changes**:
- `crates/fstty-core/src/fst/mod.rs`: created, re-exports `FstSource`
- `crates/fstty-core/src/fst/source.rs`: created with `FstSource` implementing `WaveformSource`
- `crates/fstty-core/src/lib.rs`: added `pub mod fst;` and `pub use fst::FstSource;`

**Tests added**:
- `open_and_metadata`: open test FST, verify metadata (timescale, time range, counts)
- `hierarchy_navigable`: verify hierarchy has scopes/vars, top scope has a name
- `read_signals_one_signal`: read one signal over full range, verify callbacks fire with correct signal id
- `read_signals_time_filter`: read with restricted time range, verify no callbacks outside range
- `read_signals_multiple`: read two signals, verify both signal ids appear in results
- `usable_as_trait_object`: verify `FstSource` works as `Box<dyn WaveformSource>`

**Issues**: none

**Decisions**:
- Signal mapping built by reading fst-reader's hierarchy in parallel with wellen's (both iterate FST hierarchy in declaration order), matching vars by position to get `FstSignalHandle` for each `SignalId`
- Added `Eq` and `Hash` derives to `FstSignalHandle` in fst-reader so it can be used directly as a HashMap key
- Uses `FstReader::open_and_read_time_table()` for efficient time-range filtering
- Time range conversion: WaveformSource uses `Range<u64>` (exclusive end), fst-reader uses inclusive — adjusted with `saturating_sub(1)`
- Empty time ranges short-circuit without calling fst-reader

#### STEP-7: FstSource — block info and export — 2026-03-07

**Status**: complete

**Changes**:
- `crates/fstty-core/src/fst/export.rs`: created with `BlockInfo`, `ExportConfig`, `ExportResult` types and `block_infos()` / `export_filtered()` methods on `FstSource`
- `crates/fstty-core/src/fst/mod.rs`: added `pub mod export;`, re-exports `BlockInfo`, `ExportConfig`, `ExportResult`
- `crates/fstty-core/src/fst/source.rs`: changed `FstSource` field visibility to `pub(crate)` so export.rs can access them
- `fst-reader/src/io.rs`: fixed `skip_frame` to handle uncompressed frames (`compressed_length == 0`)

**Tests added**:
- `block_infos_nonzero`: verify block count > 0
- `block_infos_times_non_overlapping`: verify block time ranges are ordered and non-overlapping
- `export_filtered_creates_file`: export 1 signal, verify output file exists and result metadata
- `export_roundtrip_signal_count`: export 2 signals, open with fst-reader, verify signal count matches
- `export_roundtrip_values_match`: read signal values from source and exported file, verify they match exactly

**Issues**:
- fst-reader's `skip_frame` had a bug: when a frame is stored uncompressed (`compressed_length == 0`), it skipped 0 bytes instead of `uncompressed_length` bytes. This caused `read_value_changes` to read wrong bytes as the pack type, triggering a `debug_assert` failure. Fixed in fst-reader.

**Decisions**:
- Uses `FstRawWriter` from fst-writer for output (handles header, geometry, hierarchy block writing and header fixup)
- Hierarchy is re-read from fst-reader and filtered (lazy scope emission: parent scopes only written when a kept variable is encountered)
- Signal data is copied raw (no decompression) with deduplication: aliased signals sharing the same compressed blob are written once
- `FstSource` fields changed to `pub(crate)` to allow the export module to access them
