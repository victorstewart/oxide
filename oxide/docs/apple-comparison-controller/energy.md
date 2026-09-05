# `oxide-apple-comparison-controller::energy`

## Intention and purpose

This benchmark-only module acquires phase-bounded macOS energy from a configured direct external meter. Release sessions stabilize for exactly 120 seconds and measure for exactly 120 seconds per Oxide or AppKit side. No energy number is claimed without complete calibrated meter evidence.

## Relation to the rest of the code

The benchmark-spec release plans provide the exact timing overlay. The comparison controller launches an independently hashed meter adapter after the exact comparator PID is ready, waits for hardware readiness, and uses the comparator's existing start and stop Darwin notifications as meter boundaries. This code is confined to the separate Apple comparison-controller crate and never enters an Oxide production artifact.

## Entry points list

- `load_macos_external_meter_config` loads and verifies the adapter and calibration contract.
- `validate_macos_external_meter_config` requires an executable adapter whose bytes match its SHA-256.
- `validate_macos_energy_adapter_request` enforces the isolated direct-meter protocol and forbids profiler, screen-recording, and debug transport.
- `reduce_macos_energy` combines raw watt samples with durable comparator telemetry.
- `macos_energy_unavailable_artifact` records explicit unavailability without claiming a measurement.

## Logic narrative

Configuration records meter model and serial, calibration identity and time, sampling rate, integration uncertainty, baseline watts, display inclusion and luminance, refresh behavior, power topology, battery state and charge range, and room temperature. The adapter receives a durable request and must publish a matching hardware-ready receipt before the comparator starts.

The adapter samples throughout stabilization and measurement. It writes a bounded partial response, exits after the stop notification, and the controller promotes the decoded response to durable raw JSON. The reducer validates identities, integrity-protected `OXBTEL02` telemetry, timebase agreement, contiguous phase boundaries, sample coverage, monotonicity, and sample cadence. Trapezoidal integration yields raw and baseline-adjusted joules and average watts for the complete measured window and every measured phase.

## Preconditions and postconditions

The output root and all adapter paths are absolute. The adapter is a direct external meter, its executable and configuration are content-addressed, and its calibration metadata passes strict category and numeric validation. Successful evidence contains the request, ready receipt, raw samples, comparator telemetry, calibration-bearing summary, and hashes bound into the campaign checkpoint.

## Edge cases and failure modes

No configured meter produces `energy.unavailable.json` and aborts the energy campaign. Adapter identity drift, missing hardware readiness, output over 64 MiB, timeout, descendant survival, incomplete samples, gaps larger than two configured sample periods, boundary mismatch beyond 50 ms, wrong timebase, non-contiguous phases, or profiler evidence fail closed. Partial adapter output is removed on failure; durable raw evidence is never overwritten.

## Concurrency and memory behavior

The controller owns one process group for the adapter and terminates the group on timeout, error, or drop. Polling is bounded and the raw response is capped at 64 MiB. Integration allocates only after the measured process has stopped.

## Performance notes

The energy pass starts no Instruments template, screen recording, common-GPU sampler, process resource sampler, or debug transport. Meter readiness and configuration work happen before the start boundary. The exact 120-second stabilization and 120-second measurement windows are symmetric across Oxide and AppKit.

## Feature flags and cfgs

None. Real acquisition requires macOS and an operator-supplied calibrated adapter. The reducer and fake-meter tests are deterministic.

## Testing and benchmarks

Run `cargo test --locked -p oxide-apple-comparison-controller --test energy_tests`. These tests make no hardware-energy or product-performance claim.

## Examples

Pass `--energy-meter-config /absolute/path/meter.json` to `cargo xtask compare-ui run` for a release campaign. Omitting it records explicit unavailability instead of substituting an Apple software estimate.

## Changelog

- 2026-07-21: added isolated direct-meter acquisition, exact 120/120 release timing, phase-bound integration, calibration metadata, and fail-closed unavailability.
