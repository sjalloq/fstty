# CLAUDE.md

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

## Build and test

```sh
cargo build                  # full workspace
cargo test -p fstty-core     # core tests
cargo test -p fstty-tui      # TUI tests
cargo clippy                 # lint
```

## TODO: Before release

- Commit changes to `fst-reader` and `fst-writer` repos and push to GitHub.
  - `fst-reader`: added `Eq` and `Hash` derives to `FstSignalHandle` (needed for use as HashMap key in fstty-core).
  - `fst-reader`: fixed `skip_frame` in `io.rs` to handle uncompressed frames (`compressed_length == 0`).
- Update `Cargo.toml` workspace dependencies to point to GitHub repos instead of local paths (`../fst-reader`, `../fst-writer`).

## Repo layout

- `crates/fstty-core/` — core data model, types, hierarchy, WaveformSource trait, backends
- `crates/fstty-tui/` — TUI application
- `PRD.md` — product requirements document (source of truth for architecture)
- `IMPLEMENTATION.md` — step-by-step plan with progress log
