# benchmark-spec apple_plan

## Intention and purpose

This module owns the transitive Apple PR plan identity. The plan SHA-256 hashes one canonical root document that binds the exact acquisition expansion, budget, platform-specific comparator audits, and all six ordered scenario manifests.

## Contract

`validate_apple_pr_plan` requires the canonical Apple PR platform/tier identity and exact relative paths. Comparator audit bindings use complete platform/framework/implementation/variant identities, remain strictly sorted by that identity, reject duplicates, and point only into the canonical `audits/` root. The validator hashes every declared artifact from the spec root and rejects missing bytes, path substitution, audit drift, scenario reordering, or scenario-manifest drift. Because each scenario manifest already binds its fixture, assets, fonts, style, layout, traces, checkpoint state, accessibility, and screenshots, changing any effective benchmark input requires rematerializing the scenario identity and therefore the root plan hash.

`apple_pr_plan_sha256` hashes the canonical pretty-JSON representation with one trailing newline. The committed `plans/apple-pr.json` must remain byte-identical to that representation.

## Testing

`crates/benchmark-spec/tests/apple_plan_tests.rs` validates the committed plan and copies the bounded root inputs into a temporary directory to prove that one changed scenario byte invalidates the old plan and changes the rematerialized plan SHA-256. Comparator admission itself is covered separately because a plan may truthfully bind a rejected audit during Stage 1 while measured acquisition remains forbidden.

## Changelog

- 2026-07-19: bound canonically ordered, platform-specific comparator audit artifacts into the Apple PR root identity.
