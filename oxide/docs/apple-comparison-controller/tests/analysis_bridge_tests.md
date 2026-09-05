# `analysis_bridge_tests.rs`

## Purpose

These tests freeze the macOS campaign-to-generic-analyzer conversion boundary without launching AppKit, Oxide, Instruments, or a production application.

## Coverage

- `completed_authoritative_campaign_materializes_resource_summary_rows` builds accepted instrumentation, trusted-input, presentation, resource, checkpoint, and campaign evidence, then proves a validated process-CPU summary becomes a typed whole-session analyzer row.
- `completed_authoritative_campaign_preserves_presentation_rows` proves the expanded bridge retains all 30 correlated presentation observations.
- `bridge_reopens_telemetry_coverage_and_surface_before_publication` independently tampers each publication source and proves no analyzer directory is published.
- `unsupported_required_metric_fails_before_publication` and `bridge_rejects_ineligible_campaign_before_publication` prove unsupported provenance and incomplete campaign eligibility fail before the destination exists.
- `specialized_source_families_reject_wrong_pass_or_claim_scope` proves launch and direct-energy sources cannot be attached to an unrelated pass and comparison-ineligible common-GPU evidence cannot enter a claim cell.

## Performance and isolation

The tests use temporary directories and small JSON fixtures. They exercise only benchmark harness code and never touch production runtime files or shared Cargo targets.

## Changelog

- 2026-07-21: added independent telemetry, coverage, and surface-receipt tamper rejection.
- 2026-07-21: added authoritative success fixtures for validated resource summaries and presentation-row preservation.
- 2026-07-21: added canonical bundle publication and fail-closed eligibility/metric coverage.
