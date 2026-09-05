# Benchmark phase scheduler

`BenchmarkPhaseSchedule` flattens one validated scenario into elapsed-microsecond actions before measurement. It preserves manifest phase order, verifies every trace event and checkpoint lies within its owning phase, and drains already-materialized actions without sleeping or allocating a result collection.

`orderedTraceEvents` exposes the already-expanded trace order to the comparison-only macOS trusted-input compiler. This prevents overlay iteration expansion from drifting away from the exact event indices consumed by the executor.

The driver supplies the acquisition plan's one-second setup duration; manifest warmup and measured durations remain exact. Advancing across multiple deadlines drains all actions in deterministic time/priority order, so a missed display callback cannot slow the logical trace or omit work.
