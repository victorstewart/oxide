# xtask apple_comparison

## Intention and purpose

This module owns host-authoritative finalization for the Apple per-app-container transport fallback and deterministic expansion of the Apple PR controller campaign. Live macOS orchestration and exact-static correctness reduction are delegated to the isolated `oxide-apple-comparison-controller` crate; `xtask` only validates shared inputs and supplies the CLI composition root.

The macOS build composition root uses `/private/tmp/oxide-comparison-build-macos` for `SYMROOT`, `OBJROOT`, and Release product discovery by default. `--build-root` may select another absolute external directory, but a path inside the production workspace is rejected. The single external root is intentionally reusable for the build/run interval and should be deleted when its manifest is no longer needed.

The build root must be empty or carry the compare-ui ownership marker. Its allocated size is checked while `xcodebuild` runs and again before accepting the build. Crossing 4 GiB terminates the dedicated build process group, removes the owned root, and fails closed.

`compare-ui capture-release-candidates` is a separate untimed macOS correctness bootstrap. It validates the Release build and canonical capture-plan identities, launches the two comparator apps without Instruments or XCTest, requires both apps to terminate after all 18 checkpoints, and atomically promotes accepted paired evidence into a new specification root.

## Relation to the rest of the code

The two signed comparator apps durably write generation-bound artifacts and acknowledgements in their own iOS Documents containers. After `devicectl` pulls those files, `compare-ui finalize-apple-transport` validates the frozen pair identity, both raw-file hashes, both acknowledgements, and the native predecessor link before atomically committing `pair.complete.json` on the host.

## Entry points list

- `finalize_apple_transport_cli`: parses the explicit host command.
- `finalize_per_app_transport_pair`: validates pulled evidence and writes the checkpoint.
- `ExpectedAppleTransportPair`: frozen identity supplied by the controller plan.
- `AppleTransportPairCheckpoint`: durable host authority record.
- `apple_pr_dry_run`: expands the frozen acquisition into four controller acquisitions and every side/pair/scenario session slot, then validates exact PR coverage.
- `write_apple_pr_dry_run`: atomically persists that expansion for host orchestration tests.
- `validate_benchmark_telemetry`: validates the fixed OXBTEL02 header, identities, timebase, record count, integrity footer, clocks, kinds, and nested terminators.
- `finalize_apple_campaign_pair`: validates both pulled app artifacts/ACKs/telemetry and atomically commits a host-owned pair checkpoint linked to the prior host pair.

## Logic narrative

The finalizer reads raw artifact bytes, hashes those exact bytes, decodes their schema, rejects any identity or generation mismatch, verifies durable ACK hashes, and requires the native artifact to name the pulled Oxide artifact as its predecessor. Only then does it write, synchronize, rename, and parent-directory-synchronize the checkpoint.

## Preconditions and postconditions

Inputs are pulled from the two signed app containers and the expected plan hash is canonical lowercase SHA-256. Success guarantees a synchronized complete checkpoint; failure leaves no checkpoint.

## Edge cases and failure modes

Missing fields, stale generations, malformed hashes, swapped sides, non-durable ACKs, mismatched file hashes, and broken predecessor chains fail closed.

## Concurrency and memory behavior

Artifacts are small control-plane JSON files read once. The temporary checkpoint name includes process and time identity; pair ownership remains serialized by the host campaign controller.

The dry-run expansion alternates AB/BA side order by pair index, emits no measurements, and proves that every non-launch scenario receives four independent session slots per implementation.

## Performance notes

Finalization runs outside measured intervals and is not a benchmark hot path.

## Feature flags and cfgs

None.

## Testing and benchmarks

`xtask/tests/apple_comparison_tests.rs` covers stale-generation rejection and valid atomic completion.

## Examples

`cargo xtask compare-ui finalize-apple-transport --oxide-artifact oxide.json --oxide-ack oxide.ack.json --native-artifact native.json --native-ack native.ack.json --output pair.complete.json --run-id run --plan-sha <sha256> --pass-id lean --pair-index 0 --generation generation`

`cargo xtask compare-ui run --plan benchmarks/comparative/specs/v1/plans/apple-pr.json --out /tmp/apple-pr-dry-run.json --dry-run`

`cargo xtask compare-ui build --platform macos --out /tmp/oxide-comparison-build-macos.json --build-root /private/tmp/oxide-comparison-build-macos`

`cargo xtask compare-ui capture-release-candidates --platform macos --build-manifest /tmp/oxide-comparison-build-macos.json --out /tmp/oxide-release-capture --promoted-spec-root /tmp/oxide-promoted-spec-v1`

`cargo xtask compare-ui run --platform macos --plan benchmarks/comparative/specs/v1/plans/apple-pr.json --build-manifest /tmp/oxide-comparison-build-macos.json --out /tmp/oxide-macos-correctness --run-id mac-correctness-001 --correctness-only`

`cargo xtask compare-ui run --platform macos --plan benchmarks/comparative/specs/v1/plans/apple-pr.json --build-manifest /tmp/oxide-comparison-build-macos.json --out /tmp/oxide-macos-correctness --run-id mac-correctness-001 --correctness-only --resume`

`cargo xtask compare-ui run --platform macos --plan benchmarks/comparative/specs/v1/plans/apple-pr.json --build-manifest /tmp/oxide-comparison-build-macos.json --out /tmp/oxide-macos-campaign --run-id mac-pr-001`

Release plans containing the isolated energy pass additionally require `--energy-meter-config /absolute/path/direct-meter.json`. The controller records explicit unavailability and makes no energy claim when that calibrated external-meter configuration is absent.

## Changelog

- 2026-07-17: added fail-closed per-app transport finalization.
- 2026-07-18: added deterministic four-acquisition Apple PR dry-run expansion.
- 2026-07-18: added host-side binary telemetry validation and per-app campaign pair checkpointing.
- 2026-07-18: made the dry-run consume the transitive Apple PR plan instead of treating the acquisition budget as the plan identity.
- 2026-07-19: delegated live macOS acquisition to the isolated Apple comparison controller crate.
- 2026-07-19: exposed the controller's strict correctness-only scope for Stage-1 qualification without measured sessions.
- 2026-07-19: made macOS recovery explicit through `--resume` and atomic pair checkpoints.
- 2026-07-19: removed the duplicate correctness reducer and delegated the standalone command to the controller's canonical API.
- 2026-07-21: moved macOS Xcode products to one explicit external build root and rejected workspace-local build output.
- 2026-07-21: added an active 4 GiB Xcode build-root fuse with owned-root cleanup.
- 2026-07-21: added hash-bound paired capture and atomic promotion for the five screenshot-blocked macOS release candidates.
