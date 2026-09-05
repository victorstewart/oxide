# oxide-perf-runner::bin::c48_bitmap_options_probe::common

## Intention and purpose

`common` owns shared C48 bitmap-option probe data and the draw-list recording encoder used by candidate and baseline binaries.

## Relation to the rest of the code

Candidate and baseline probes call this module through `RenderEncoder`; it records renderer-neutral commands for the same option-popover workload.

Call flow: bitmap option encoding -> recording encoder -> draw list -> probe measurement.

## Entry points list

The module's probe helpers and recording encoder are binary-internal; external crates do not reach this unit.

## Logic narrative

The encoder appends geometry into owned backing arrays and records each semantic callback. `draw_image` records the square-image callback as `DrawCmd::Image` with explicit zero radii.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

Recorded spans address the encoder-owned arrays. Image callbacks preserve handle, destination, source, and square semantics. No unsafe code is introduced.

## Edge cases and failure modes

Geometry sizes that do not fit command spans are ignored by the existing checked append path. Square images never create a separate clip command.

## Concurrency and memory behavior

The encoder is caller-owned and single-threaded. It reuses its draw-list vectors across one probe path.

## Performance notes

The explicit four-float zero-radius payload is part of the updated command representation; it does not change the C48 workload's image pixels.

## Feature flags and cfgs

Candidate and baseline binaries select their implementation at compile time; this common module keeps the recording contract identical.

## Testing and benchmarks

C48 focused probes and perf-runner report tests validate the complete workload.

## Examples

Candidate and baseline encoders both call the same `draw_image` implementation in this module.

## Changelog

- 2026-07-18: recorded square image callbacks with explicit zero radii.
