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
- Use the test FST files in `http://github.com/sjalloq/fst-reader/fsts/` instead (many small examples from various simulators).

## Build and test

```sh
cargo build                  # full workspace
cargo test -p fstty-core     # core tests
cargo test -p fstty-tui      # TUI tests
cargo clippy                 # lint
```

## TODO: Before release

- Commit changes to `fst-reader` and `fst-writer` repos and push to GitHub.
- Update `Cargo.toml` workspace dependencies to point to GitHub repos instead of local paths (`../fst-reader`, `../fst-writer`).

## Repo layout

- `crates/fstty-core/` — core data model, types, hierarchy, WaveformSource trait, backends
- `crates/fstty-tui/` — TUI application
- `PRD.md` — product requirements document (source of truth for architecture)
- `IMPLEMENTATION.md` — step-by-step plan with progress log
