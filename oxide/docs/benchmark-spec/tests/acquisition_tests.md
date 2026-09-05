# `acquisition_tests.rs`

## Purpose

Freeze the machine-readable Apple PR campaign shape and reject changes that would silently add acquisitions or weaken pair coverage.

## Coverage

- Parses and validates the committed acquisition expansion against the committed Apple PR budget.
- Requires canonical JSON byte equality.
- Rejects a hidden lean replay.
- Rejects missing presentation pair coverage.

## Changelog

- 2026-07-17: added canonical expansion, lean-replay, and pair-coverage cases.
