# C05 Metal layer-cache evidence

This directory records the C05 correctness and performance evidence for replacing Metal's independent layer prescan and main-lowering decisions with one generation-based plan per nesting range.

The parent path prerenders a dirty or missing cache body, then independently rebuilds and renders that body inline because its prescan count hash and lowering content hash disagree. The candidate gives every layer one owner: unsupported bodies draw inline once, refresh bodies draw offscreen once and composite once, and clean bodies composite without body materialization or geometry inspection. Same-size dirty refreshes reuse the retained private texture.

The production Metal counters cover structural body scans, copied body commands, layer texture creates, cache hits/misses, offscreen draws, inline draws, and prevented duplicate renders. Real-Metal tests exercise missing, clean, dirty, nested invalidation, and nested unsupported-effect states. The committed snapshot matrix also reruns its nested composition/effect fixtures through the cache-disabled inline reference path.

The raw evidence records the detached parent-instrumentation commit, candidate reports, paired-process plans/results, environment controls, commands, and hashes under the ignored `raw/` directory so evidence collection cannot change the staged candidate tree identity. Official `latest.*` baselines remain unchanged until C62.
