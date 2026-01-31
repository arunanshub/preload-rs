# ADR-0001: PreloadEngine Domain Model and Orchestration

Date: 2026-01-30
Status: Accepted

## Context

The current kernel implementation mirrors the original preload design but uses
self-referential Arc/Weak graphs (Exe <-> Markov). This complicates ownership,
locking, and testing. We want a Rust-centric, SOLID-friendly redesign with a
clear domain language, explicit responsibilities, and a small API surface that
is easy to evolve.

The design should remain faithful to the thesis model (Exe, Map, Markov,
MemStat, State) while removing cross-owned graphs and keeping the system
understandable.

## Decision

We will redesign the core as a data-first domain model with ID-based relations,
and an orchestrator named `PreloadEngine` that wires small services together.

### Domain vocabulary

- Exe: a trackable executable identified by absolute path (ExeKey).
- MapSegment: (path, offset, length) from /proc maps (MapKey).
- MarkovEdge: a 4-state Markov chain for an Exe pair (A,B) keyed by ExeId.
- Observation: a first-class event stream for a single scan cycle.
- Prediction: exe and map scores for the next cycle.
- PrefetchPlan: ordered maps within a memory budget.

### Observation (event stream)

Observation is the output of scanning. It is an ordered stream of events:

- ObsBegin { time, scan_id }
- ExeSeen { path, pid }
- MapSeen { exe_path, map }
- MemStat { mem }
- ObsEnd { time, scan_id, warnings }

Contract:

- Paths are sanitized (no deleted/prelink noise).
- Only file-backed maps are emitted.
- ExeSeen occurs before MapSeen for that exe within a scan.
- Best-effort completeness is allowed (processes may die mid-scan).

### AdmissionPolicy

AdmissionPolicy decides whether a seen exe becomes tracked. It is the only place
that enforces filters (min size, exeprefix, mapprefix). Rejections are kept in a
policy cache (TTL/LRU), not in core model state.

Partial map lists are admissible once size >= minsize, marked as Partial for
future refresh.

### Stores and invariants

We keep data in simple stores and indices:

- ExeStore: ExeId -> Exe, ExeKey(path) -> ExeId
- MapStore: MapId -> MapSegment, MapKey(path, offset, length) -> MapId
- ExeMapIndex: ExeId <-> MapId sets
- MarkovGraph: EdgeKey(ExeId, ExeId) -> MarkovEdge

Invariants:

- ExeKey and MapKey are unique.
- ExeMapIndex has no dangling ids.
- MarkovGraph has no self-edges and at most one edge per pair.

Markov edges are created eagerly for all exe pairs to match the thesis model.

### Active-Set Contract Changes

- ModelUpdater maintains an ActiveSet of recently seen exes (configurable window).
- MarkovGraph edges exist only among ActiveSet pairs; edges are pruned when an exe ages out.
- Predictor treats missing edges as neutral evidence (no contribution).
- Persistence stores only existing edges; ActiveSet is runtime-only.


### Predictor and PrefetchPlanner

Predictor computes exe start probabilities from Markov edges and derives map
scores from the exes that map them. PrefetchPlanner sorts maps by score and
selects within a memory budget computed from MemStat and configuration.

### Orchestrator

`PreloadEngine` owns stores and services. It exposes:

- tick(): one scan/update/predict/plan/prefetch step, no sleeping
- run_until(cancel_token): loop with scheduling and autosave
- save(): persist snapshot

### Persistence

We persist full snapshots by keys (not internal ids). Snapshot contents:

- model_time, last_accounting_time
- exes (path, time stats)
- maps (path, offset, length, update_time)
- exe_maps (exe_path, map_key, prob)
- markov_edges (exe_a, exe_b, time_to_leave, weight, time)

Runtime/derived data (running set, predictions, memstat) is not persisted.

Autosave is time-based (Duration) and happens at end of tick in run_until.

## Consequences

Pros:

- No self-referential graphs; ownership and locking are simpler.
- Clear separation of responsibilities and testable components.
- Stable domain vocabulary that can expand (e.g., per-user profiles).
- Persistence decoupled from internal IDs.

Cons:

- Slightly more plumbing (IDs + indices).
- Snapshot writes may be heavier than deltas (acceptable for v1).

## Alternatives Considered

- Keep Arc/Weak graphs: rejected due to complexity and deadlocks.
- Delta-only persistence: rejected for v1 due to complexity and replay risks.
- Snapshot-only API (no tick): rejected for loss of testability.

## References

- Behdad Esfahbod, "Preload — An Adaptive Prefetching Daemon", 2006.
- Existing preload-rs kernel implementation (current workspace).

## Addendum: Markov Edge Strategy (Active-Set Lazy)

We will not maintain Markov edges for all exe pairs. Instead, the model keeps
edges only among "active" exes (recently observed within a configurable time
window). Missing edges are treated as neutral evidence (no contribution) during
prediction.

Rationale:

- Reduces CPU/memory from O(N^2) to O(R^2) where R is active exes.
- Preserves prediction quality for the working set of applications.
- Avoids global recomputation for rarely used apps.

Implications:

- ModelUpdater maintains the active set and ensures edges among active exes.
- Predictor treats missing edges as "no signal" rather than zero or negative.
- Persistence stores only existing edges; reactivated exes may rebuild edges.
