# C12 native damage storage evidence

This directory records C12 correctness and performance evidence for retaining native damage-vector capacity across router extraction, prepared-frame retry, and renderer submission.

The performance hypothesis is that `Router::take_damage` transfers the router's allocation to a short-lived frame object, leaving `last_damage` with zero capacity and forcing the next damage push to allocate. Caller-owned `take_damage_into` storage swaps a reusable capacity back into the router and recovers the submitted vector after `begin_frame`. The affected phase is native damage generation/handoff; target workloads are blinking-caret (one rect), moving-node old/new bounds (two rects), and many-small-rect (256 rects). Expected counters are at least one parent allocation per frame versus zero candidate allocations after warmup. The correctness risk is losing old or new bounds when a drawable is skipped or submission fails; macOS unit and ownership tests freeze ordering, retry, and capacity behavior, while iOS tests freeze its existing clear-on-cancel/error policy.

The locally ignored `raw/` directory records balanced parent-transfer/candidate-reuse allocation, byte, CPU-time, p95, and p99 evidence. Official `latest.*` baselines remain unchanged until C62.

Each optimized A/B uses 15 balanced fresh-process pairs, 20 warmups, and 200 measured frames per side/process (3,000 samples per workload). Every parent caret and moving-node frame allocated once for 64 bytes, and every parent 256-rect frame allocated once for 4,096 bytes; every candidate frame reported zero allocations and zero bytes. Caret p95/p99 fell from 0.000208/0.000250 ms to 0.000083/0.000084 ms. Moving-node p95 held at 0.000042 ms while p99 fell from 0.000084 to 0.000042 ms. Many-small-rect p50/p95/p99 moved from 0.000083/0.000084/0.000125 ms to 0.000042/0.000084/0.000084 ms. All three no-material-regression gates accepted; the many-rect paired median improved 49.40%.

The macOS host passes its capacity/old-new-bounds unit test, headless test, and five integration tests. The iOS host passes all 32 tests. No production benchmark hook remains after evidence collection.
