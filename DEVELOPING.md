# Developing preload-rs

This document is for contributors (human or AI). It explains the repository
layout, runtime design, contracts, and how to work on the codebase.

## Repository layout

- `crates/cli` — binary entrypoint and CLI parsing.
- `crates/config` — configuration types + TOML loading/merging.
- `crates/orchestrator` — core engine, model, prediction, planning, and
  persistence.
- `docs/` — ADRs, example config, and the original 2006 thesis PDF.
- `preload-0.6.4/` — original C reference source (for feature parity checks).

All crates are part of a Cargo workspace under `crates/`.

## Design overview (high level)

The system is organized around a small set of replaceable components that follow
clear contracts. The orchestrator owns all runtime state, and all behavior
flows through a single engine loop.

### Runtime pipeline

Each cycle follows this order:

1. **Scan**: collect a stream of observation events (processes + maps + memstat).
2. **Update**: update the model (exes, maps, markov edges, active set).
3. **Predict**: compute exe and map scores for the next cycle.
4. **Plan**: select maps to prefetch within a memory budget.
5. **Prefetch**: execute the plan with `posix_fadvise`.

The orchestrator exposes:

- `PreloadEngine::tick()` — run one full cycle, no sleeping.
- `PreloadEngine::run_until(cancel, control_rx)` — continuous loop with sleep
  pacing, autosave, and control events.

### Runtime control (signals)

Signals are handled in the CLI, converted to control events, and fed to the
engine loop:

- **SIGHUP** → reload config (rebuilds admission/updater/predictor/planner/prefetcher).
- **SIGUSR1** → dump current config and state summary.
- **SIGUSR2** → save state immediately.
- **Ctrl‑C** → graceful shutdown (save if configured).

## Core domain vocabulary

These names show up throughout the codebase:

- `ExeKey` — stable identifier for an executable (path).
- `MapSegment` — a mapped file region (path, offset, length, update_time).
- `MarkovEdge` — statistics for exe transitions and co‑running time.
- `ActiveSet` — recently‑seen executables used to bound Markov edges.
- `Stores` — in‑memory state container (exes, maps, exe→map index, markov graph).

Active‑set Markov edges are **lazy**: edges exist only among recently observed
exes, reducing O(N^2) growth. Missing edges are treated as neutral evidence in
prediction.

## Key contracts (traits)

All core behavior is behind small traits so components can be swapped in tests
or future features.

- `Scanner`: produces `ObservationEvent` streams (default: procfs scanner).
- `AdmissionPolicy`: decides which exes/maps enter the model.
- `ModelUpdater`: mutates stores given observations + admission policy.
- `Predictor`: produces exe/map scores (default: Markov predictor).
- `PrefetchPlanner`: converts scores + memstat into a prefetch plan.
- `Prefetcher`: executes a plan (default: `posix_fadvise`).
- `StateRepository`: persists snapshots (default: SQLite).
- `Clock`: abstracts time/sleep for deterministic tests.

## Persistence model

Persistence is snapshot‑based and keyed by external identifiers (paths), not
internal IDs. The SQLite repository stores:

- model time + last accounting time
- exes (path + runtime stats)
- maps (path + offset + length + update_time)
- exe_maps (exe_path + map_key)
- markov edges (exe_a + exe_b + time_to_leave + transition_prob + both_running_time)

Runtime‑only data (active set, prediction scores, memstat) is not persisted.

## Config system

- The config crate provides a typed `Config` and TOML merging.
- Config files are merged in order; later files override earlier values.
- `model.half_life` overrides `model.decay` for exponential smoothing.

See `docs/config.example.toml` for a complete example.

## Testing strategy

- **Unit tests**: policy logic, model invariants, etc.
- **Integration tests**:
  - `procfs_integration.rs`: real `/proc` scan of the current executable.
  - `engine_pipeline.rs`: deterministic pipeline test with injected components.
  - `engine_persists_and_loads_state`: sqlite round‑trip.

All tests should pass on Linux. The procfs test is required because Linux is the
target platform.

## Development setup

1. Install `sqlx-cli`.
2. Add `.env` at repo root:
   ```
   DATABASE_URL="sqlite://./dev.db"
   ```
3. Create and migrate the dev DB:
   ```bash
   sqlx database create
   sqlx migrate run --source crates/orchestrator/migrations
   ```
4. When SQL changes, refresh offline data:
   ```bash
   cargo sqlx prepare --workspace
   ```

## Working conventions

- **No unsafe:** every crate has `#![forbid(unsafe_code)]`.
- **Logging:** use `tracing` macros (not `println!`).
- **Error handling:** avoid `unwrap`, use `Result<T, E>`.
- **Formatting/lints/tests:**
  ```bash
  cargo fmt --check --all
  cargo check --workspace --all-features
  cargo clippy --all-targets -- -D warnings
  cargo test --workspace --all-features
  ```

## Design references

- `docs/ADR.md` contains the architectural decisions.
- `docs/preload-thesis.pdf` is the original 2006 description.
