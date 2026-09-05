# macOS detection coverage v1

`macos-v1.json` is the canonical, harness-owned fault-coverage contract for
macOS comparison planning. It is benchmark specification data; it is not linked
into an Oxide production binary.

The corpus contains 14 fault families at small, medium, and large severity with
three deterministic seeds per expectation: 126 injected cases. Medium and
large faults are mandatory detections. Small faults are reviewed weighted
sensitivity evidence. Performance faults require the declared metric direction
on at least two seeds plus the declared median boundary. Terminal correctness
validators must fire on all three seeds. Null controls are capped at a 5% false
positive rate, and terminal validators permit none.

The manifest maps the corpus to all 12 required production risk dimensions and
records the deterministic scenario set for each default campaign tier. Every
omitted scenario has an explicit redundancy reason, marginal risk inventory,
eligible-expectation count, and occupied-time estimate.

Run:

```text
cargo xtask compare-ui validate
cargo xtask compare-ui plan --platform macos --tier pr --explain-coverage --explain-budget
```

The planning report proves only that the selected scenario set is eligible to
detect the declared corpus. It deliberately labels itself pre-acquisition; it
does not claim that faults were injected or detected. Empirical detection and
null-control results remain separate campaign evidence.
