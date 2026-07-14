# ui-core `anim_prop.rs`

## Purpose

These tests cover `Animator` sampling, repeat completion, dense override storage, interruption, and property cleanup.

## C26 coverage

- Finished populations compact in one retain pass, preserve allocation capacity, expose their final value for one frame, and clear the dense slot on the following frame.
- Cancel-and-restart interruption leaves one active property animation, starts from the sampled value, reaches the replacement target, and publishes a changed-node clear when complete.
- Transform, opacity, and color samples remain finite and bounded.

## Changelog

- 2026-07-13: added dense-storage reuse plus interruption/completion cleanup coverage for C26.
