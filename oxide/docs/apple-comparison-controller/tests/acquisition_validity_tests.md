# acquisition_validity_tests.rs

These focused tests freeze the macOS acquisition-validity reducer contract without launching measured applications.

- `complete_receipts_stay_blocked_by_diagnostic_input_scope` proves canonical receipt completeness cannot override the current diagnostic envelope scope.
- `forged_gate_flag_is_rejected` proves persisted booleans cannot override insufficient opportunities.
- `unavailable_calibrations_remain_explicit_and_non_authoritative` proves absent trace-overhead, external-sensor, and common-GPU evidence is not fabricated or promoted.
- `diagnostic_direct_callback_input_remains_non_authoritative` starts from otherwise-passing gates and proves the current Swift diagnostic injection scope cannot be promoted.
- `unavailable_gpu_observation_cannot_support_comparable_gpu_claims` proves missing supported GPU observation disables only comparable device-GPU claims.
- `unavailable_room_temperature_only_blocks_short_run_energy_equivalence` proves record-only room temperature does not invalidate unrelated global claims.
- `missing_pre_pair_observation_is_valid_but_fail_closed` preserves honest partial evidence while refusing global authority.
- `forged_comparable_gpu_availability_is_rejected` proves a family claim cannot be promoted over unavailable GPU evidence.
- `section_nine_cpu_and_wakeup_limits_are_enforced` freezes the 5%-median, 20%-peak, and five-wakeups-per-second limits.
- `schema_owned_environment_policy_cannot_be_rewritten` rejects policy deletion or substitution in a persisted report.
- `live_background_observer_collects_exact_ten_second_sample_contract` exercises the supported macOS `top` path and verifies the exact bounded ten-sample artifact contract.

The analyzer bridge tests additionally bind validity opportunity counts and calibration statuses to the actual hashed correlation files before publication.
The module-level `common_gpu_evidence_includes_attribution_sessions` regression proves real common-GPU hashes are collected from Attribution sessions independently of Primary presentation-opportunity selection.
The module-level `measured_input_rereads_diagnostic_envelope_and_complete_receipt_manifest` regression proves acquisition rereads the Primary complete envelope and hash-bound receipt manifest while preserving the independent `direct-callback-diagnostic` authority block.
