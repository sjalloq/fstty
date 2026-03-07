# fstty — Product Requirements Document

## What is fstty?

fstty is a waveform tool for hardware engineers. It reads FST and VCD simulation dumps, lets users browse the signal hierarchy and select signals of interest, and can:

1. **Export filtered FST files** — write a new FST containing only selected signals and time ranges, fast enough to be practical on multi-GB files.
2. **Serve signal data to external tools** — run as a signal server so users can query waveform data from Python/Polars/DuckDB for analysis, without reloading the file each time.

## Core data model

Waveform files (FST, VCD) contain:

- **Hierarchy** — a tree of scopes (modules, blocks, etc.) containing variables (signals).
- **Value changes** — a time-ordered stream of (time, signal, new_value) tuples. This is the fundamental data: signals change value at discrete times, and that's all the file records.

Everything fstty does is built on two operations:

1. **Browse the hierarchy** — navigate scopes, inspect signal metadata (name, width, type, direction), search by pattern.
2. **Stream value changes** — for a set of signals over a time range, emit (time, signal, value) tuples in time order.

That's it. Filtered export, clocked sampling, Arrow serving — all are consumers of these two primitives.

## Architecture

```
                    Consumers
         ┌────────────┼────────────┐
         │            │            │
    TUI (Browse    Signal       Filtered
     + Export)     Server       FST Export
         │            │            │
         └────────────┼────────────┘
                      │
              ┌───────┴───────┐
              │  fstty-core   │
              │               │
              │  Hierarchy    │  ← concrete struct, built once at load
              │  WaveformSource │  ← trait: stream value changes
              └───────┬───────┘
                      │
            ┌─────────┼─────────┐
            │                   │
       FST backend         VCD/GHW backend
```

### Backend responsibilities

Two libraries, two jobs — no overlap:

| Operation              | FST        | VCD/GHW |
|------------------------|------------|---------|
| Parse hierarchy        | wellen     | wellen  |
| Read signal values     | fst-reader | wellen  |
| Block-level raw export | fst-reader | N/A     |

**wellen** is the hierarchy library. It parses FST, VCD, and GHW headers and builds a navigable hierarchy. It is good at this and already works. Its signal value loading API is *not* used for FST — that's fst-reader's job.

**fst-reader** is the data library for FST. It provides selective signal reading (only decompress what you ask for), time-range filtering, and raw block-level access for fast filtered export. This is why it exists — to bypass wellen's "decompress everything" approach.

Neither library's types appear in fstty-core's public API. Both are wrapped behind fstty's own types (`Hierarchy`, `ScopeId`, `VarId`, `SignalId`, `WaveformSource`).

### fstty-core

Owns the data model. No dependency on any UI, server, or file format crate at this layer's public API.

#### Types

```rust
/// Opaque IDs — owned by fstty, not by any backend.
/// All derive Clone, Copy, PartialEq, Eq, Hash.
pub struct ScopeId(u32);
pub struct VarId(u32);
pub struct SignalId(u32);  // unique signal; multiple vars can alias one signal

/// Signal value as returned by the change stream.
pub enum SignalValue<'a> {
    Binary(&'a [u8]),   // bit-string, one ASCII char per bit ('0','1','x','z')
    Real(f64),
}

/// Metadata about the waveform file.
pub struct WaveformMetadata {
    pub timescale_exponent: i8,  // timescale = 10^exponent seconds
    pub start_time: u64,
    pub end_time: u64,
    pub var_count: u64,
    pub signal_count: usize,     // unique signals (after alias resolution)
}

/// Our own enums — not tied to any backend.
pub enum ScopeType { Module, Generate, Interface, Begin, ... }
pub enum VarType { Wire, Reg, Logic, Real, Integer, ... }
pub enum VarDirection { Implicit, Input, Output, InOut, ... }
```

#### Hierarchy

A concrete struct, not a trait. Built once when a file is opened, read-only after that. Every backend constructs the same type.

```rust
pub struct Hierarchy { ... }

impl Hierarchy {
    // Navigation
    pub fn top_scopes(&self) -> &[ScopeId];
    pub fn scope_children(&self, id: ScopeId) -> &[ScopeId];
    pub fn scope_vars(&self, id: ScopeId) -> &[VarId];
    pub fn scope_parent(&self, id: ScopeId) -> Option<ScopeId>;

    // Scope metadata
    pub fn scope_name(&self, id: ScopeId) -> &str;
    pub fn scope_full_path(&self, id: ScopeId) -> String;
    pub fn scope_type(&self, id: ScopeId) -> ScopeType;

    // Variable metadata
    pub fn var_name(&self, id: VarId) -> &str;
    pub fn var_full_path(&self, id: VarId) -> String;
    pub fn var_width(&self, id: VarId) -> u32;
    pub fn var_type(&self, id: VarId) -> VarType;
    pub fn var_direction(&self, id: VarId) -> VarDirection;
    pub fn var_signal_id(&self, id: VarId) -> SignalId;

    // Search
    pub fn find_vars(&self, pattern: &str) -> Vec<VarId>;

    // Counts
    pub fn scope_count(&self) -> usize;
    pub fn var_count(&self) -> usize;
    pub fn signal_count(&self) -> usize;
}
```

The hierarchy is built from parser callbacks. Each backend produces the same events:

```rust
pub enum HierarchyEvent {
    EnterScope { name: String, scope_type: ScopeType },
    ExitScope,
    Var { name: String, var_type: VarType, direction: VarDirection,
          width: u32, signal_id: SignalId, is_alias: bool },
}
```

A `HierarchyBuilder` consumes these events and produces a `Hierarchy`.

#### WaveformSource trait

The one trait that backends implement:

```rust
pub trait WaveformSource {
    fn metadata(&self) -> &WaveformMetadata;
    fn hierarchy(&self) -> &Hierarchy;

    /// Stream value changes for selected signals in a time range.
    /// Calls back with (time, signal_id, value) in time order within each VC block.
    /// This is the fundamental data access primitive.
    fn read_signals(
        &mut self,
        signals: &[SignalId],
        time_range: std::ops::Range<u64>,
        callback: &mut dyn FnMut(u64, SignalId, SignalValue),
    ) -> Result<()>;
}
```

That's the entire trait. One method for data access.

### FST backend

Uses **wellen** for hierarchy parsing and **fst-reader** for signal data access and block operations.

```rust
pub struct FstSource {
    hierarchy: Hierarchy,              // built from wellen at open time
    metadata: WaveformMetadata,
    reader: fst_reader::FstReader<...>, // for signal reading + block access
    signal_map: Vec<FstSignalHandle>,   // maps SignalId → fst-reader handle
}

impl FstSource {
    pub fn open(path: &Path) -> Result<Self> {
        // 1. Open with wellen to parse hierarchy (fast, header-only)
        // 2. Walk wellen's hierarchy, build our Hierarchy via HierarchyBuilder
        // 3. Open with fst-reader for data access
        // 4. Build signal_map to translate between ID spaces
    }
}

impl WaveformSource for FstSource {
    fn hierarchy(&self) -> &Hierarchy { &self.hierarchy }
    fn metadata(&self) -> &WaveformMetadata { &self.metadata }

    fn read_signals(...) {
        // Delegates to fst-reader::FstReader::read_signals()
        // Maps SignalId → FstSignalHandle, calls fst-reader, maps back
    }
}
```

Additionally exposes FST-specific block operations for the fast export path:

```rust
impl FstSource {
    /// VC block metadata — used by the Export tab.
    pub fn block_infos(&self) -> Vec<BlockInfo>;

    /// Filtered export using raw block copy (no decompression of value changes).
    /// This is the fast path for FST-to-FST filtering.
    pub fn export_filtered(&mut self, config: &ExportConfig) -> Result<ExportResult>;
}
```

This is not on the trait because raw block copy is FST-specific. Other backends would export by reading through `read_signals` and writing through a generic writer — slower, but correct.

### VCD/GHW backend

Uses **wellen** for both hierarchy parsing and signal value reading.

```rust
pub struct VcdSource {
    hierarchy: Hierarchy,        // built from wellen
    metadata: WaveformMetadata,
    wellen_handle: ...,          // wellen's internal reader for signal access
}

impl WaveformSource for VcdSource {
    // hierarchy() and metadata() — return our types
    // read_signals() — delegates to wellen's signal loading
}
```

VCD/GHW have no block structure, so no block-level export. But these files can be *exported as FST* by reading through `read_signals` and writing with `fst-writer`. This is also how "open VCD, write multi-block FST" works — it's a conversion.

## Consumers

### TUI

Two tabs: **Browse** and **Export**.

- **Browse**: navigates the `Hierarchy`, lets user select signals and scopes. Codes against `Hierarchy` (concrete type) directly.
- **Export**: for FST files, shows VC blocks as a timeline. User selects a block range. Pressing export triggers `FstSource::export_filtered()` for the fast path.

The TUI opens a file via `FstSource::open()` or `VcdSource::open()` and holds a `Box<dyn WaveformSource>`. Browse works identically regardless of backend. Export's block UI is only shown for FST sources (checked by downcast or feature flag).

### Signal server

Runs as a daemon. Holds a `Box<dyn WaveformSource>`. Serves queries over Arrow Flight.

**Queries** (all built on `read_signals`):

- `changes(signals, time_range)` — calls `read_signals` directly, streams results as Arrow.
- `clocked(signals, clock, edge, time_range)` — calls `read_signals` for clock + all signals, post-processes the change stream: at each clock edge, emit the current value of each signal as a dense row.
- `sample(signals, interval, time_range)` — same idea, but at fixed time intervals instead of clock edges.
- `list_signals(pattern)` — calls `hierarchy().find_vars(pattern)`.

**Client layers** (Python):

1. **fstty** — thin Arrow Flight client: `changes()`, `clocked()`, `sample()`, returns Polars DataFrames.
2. **fstty.primitives** — Polars expressions for common waveform operations: `handshake(valid, ready)`, `stall(valid, ready)`, `outstanding_count(incr, decr)`, etc.
3. **fstty-axi** etc. — protocol-specific analysis libraries built on primitives.

### Filtered FST export

Two paths:

1. **Fast path (FST only):** raw block copy. Does not decompress value change data. Copies compressed blobs and rewrites position tables/frames with only selected signals. Implemented in `FstSource::export_filtered()`, using `fst-reader`'s `VcBlockReader` and `fst-writer`'s `VcBlockWriter`. Already proven in the `fst-filter` example.

2. **Generic path (any format):** reads through `read_signals()`, writes through `fst-writer`. Slower (decompresses and recompresses) but works for VCD→FST conversion and for partial-block time ranges.

## Dependencies

| Crate | Used by | Purpose |
|-------|---------|---------|
| `wellen` | fstty-core (all backends) | Hierarchy parsing for FST, VCD, GHW |
| `fst-reader` | fstty-core (FST backend) | FST signal reading + block-level access |
| `fst-writer` | fstty-core (export) | Write FST files |
| `ratatui` + `crossterm` | fstty-tui | Terminal UI |
| `tui-tree-widget` | fstty-tui | Hierarchy tree widget |
| `arrow` + `arrow-flight` | fstty-server (future) | Arrow data serving |
| `tokio` | fstty-tui, fstty-server | Async runtime |

**wellen stays** as the hierarchy parser for all formats, and as the signal reader for VCD/GHW. It is wrapped behind fstty-core's own types — no wellen types appear in the public API. **fst-reader** handles FST signal data and block access. If wellen ever needs to be replaced, only the backend wrappers in fstty-core change.

## Crate structure

```
fstty-rs/
├── crates/
│   ├── fstty-core/          Core data model + backends
│   │   ├── src/
│   │   │   ├── types.rs         ScopeId, VarId, SignalId, enums
│   │   │   ├── hierarchy.rs     Hierarchy struct + HierarchyBuilder
│   │   │   ├── waveform.rs      WaveformSource trait, WaveformMetadata
│   │   │   ├── fst/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── source.rs    FstSource (WaveformSource impl)
│   │   │   │   └── export.rs    Block-level filtered export
│   │   │   └── vcd/
│   │   │       ├── mod.rs
│   │   │       └── source.rs    VcdSource (WaveformSource impl)
│   │   └── Cargo.toml
│   ├── fstty-tui/           TUI application
│   │   └── ...
│   └── fstty-server/        Signal server (future)
│       └── ...
└── Cargo.toml               Workspace
```

## What this means for existing code

- `wellen` stays as a dependency of fstty-core but is **no longer imported by fstty-tui**. All wellen usage is confined to backend wrappers inside fstty-core.
- The current `hierarchy.rs` (HierarchyNavigator, VisibleNodeIterator) is replaced by the new `Hierarchy` struct, which is built from wellen's hierarchy at load time.
- The current `waveform.rs` (WaveformFile wrapping wellen) is replaced by `FstSource` which uses wellen for hierarchy and fst-reader for signal data.
- The current `filter.rs` (SignalSelection) stays mostly as-is but uses `VarId`/`SignalId` instead of wellen types.
- The current `writer.rs` (placeholder) is replaced by `fst/export.rs` using proven fst-filter code.
- The TUI's `hierarchy_browser.rs` drops all wellen imports and unsafe transmutes; uses `ScopeId`/`VarId` directly (which derive `Hash` natively).
- The TUI's `components/tree.rs` similarly simplified.
- **No wellen types in fstty-tui's dependency graph.** The TUI only sees fstty-core's types.

## Open questions

1. **Signal server transport**: Arrow Flight is the plan, but should the TUI embed the server or connect to it? Embedding avoids a separate process but couples the crates.
2. **Caching in the server**: LRU cache of decompressed signal data per block? Or re-read from disk each query? Depends on expected query patterns.
3. **X/Z representation in Arrow**: how to represent 4-state values? Separate validity column? Enum? This affects the Arrow schema design.
4. **wellen for VCD signal reading**: wellen's signal loading decompresses eagerly. For small VCD files this is fine. For large ones, we may eventually want a streaming VCD parser — but this is a future optimisation, not a blocker.
