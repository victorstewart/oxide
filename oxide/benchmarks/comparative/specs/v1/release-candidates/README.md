# Release scenario candidates

These five files freeze the framework-neutral release workload inputs from the comparison matrix. They are deliberately outside `scenarios/` and are not loadable `ScenarioSpec` manifests.

Every candidate binds its fixture, shared assets, pinned font pack, style tokens, layout assertions, traces, state, and accessibility bytes by SHA-256. Each also freezes exactly one primary metric, one within-session estimator, one owning pass, named measured phases, and viewport-specific visible-role counts.

The only promotion blocker is canonical visual evidence. An adapter must render every checkpoint at the declared viewport, persist its canonical PNG bytes, and prove static pixel parity. Only then may those PNG identities be added and the candidate be materialized as `scenarios/<id>.json`. Do not copy an unrelated image, invent a digest, or make the production loader accept a screenshot-free candidate.

