# renderer-api `render_chunk_tests.rs`

## Purpose

This suite freezes immutable chunk canonicalization, persistent sequence composition, snapshot validation, and the checked flat compatibility adapter.

## C26 coverage

- `dynamic_property_ids_pack_dense_indices_and_generation_checks` proves dense index/generation round trips and rejects two live generations for one index.
- `dynamic_clip_uses_its_transform_slot_in_flat_translation_fallback` proves clip references resolve as transforms and translation-only flat replay emits matching push/pop commands.
- Existing snapshot validation coverage rejects missing, duplicate, non-finite, invalid-opacity, invalid-clip, and lossy affine property states.

Passing these tests means slot reuse cannot silently alias within one snapshot and compatibility replay cannot discard scale or rotation.

## Changelog

- 2026-07-13: added C26 dynamic property generation and clip validation coverage.
