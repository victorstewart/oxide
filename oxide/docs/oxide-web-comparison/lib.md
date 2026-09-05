# oxide-web-comparison: `lib`

## Intention and purpose

This module owns the standalone browser comparison laboratory's native support. It validates the three product tracks, serves canonical comparison fixtures, renders the HTML-first reference on the server, and computes shipping payload manifests without adding benchmark code to production Oxide crates.

## Relation to the rest of the code

- `oxide-web-comparison` binary calls `validate_source_tree`, `build_shipping_manifest`, `write_manifest`, and `serve`.
- The HTTP server reads browser sources from `host/web-comparison/sites`.
- HTML-first route rendering reads the same versioned fixtures used by `oxide-test-scenes` from `benchmarks/comparative/specs/v1/fixtures`.
- Browser pages install the frozen `window.oxideComparisonV1` control surface after the server-rendered document arrives. Its `advance(checkpointId)` operation applies the deterministic `rapid-v1` checkpoint transition and returns the elapsed time to output-ready.

```text
oxide-web-comparison binary
   -> lib::serve
      -> render_html_first_page
         -> canonical fixture JSON
         -> semantic initial HTML
      -> static JS, CSS, font, and image assets
```

## Entry points

- `oxide_web_comparison::validate_source_tree(root: &Path) -> anyhow::Result<()>`: validates required tracks and control/campaign restrictions; called by the binary and server startup.
- `oxide_web_comparison::build_shipping_manifest(root: &Path, implementation_id: &str) -> anyhow::Result<ShippingManifest>`: measures raw, gzip-9, and Brotli-11 payload sizes; called by the manifest command.
- `oxide_web_comparison::build_route_shipping_manifest(root: &Path, package_root: &Path, implementation_id: &str) -> anyhow::Result<ShippingManifest>`: materializes the canonical suite's route-aware production entity graph, including dynamic HTML-first responses, client fixtures, shared DOM fonts/assets, Oxide JS/WASM, and the lazy full-image source.
- `oxide_web_comparison::write_manifest(path: &Path, manifest: &ShippingManifest) -> anyhow::Result<()>`: atomically persists a manifest; called by the manifest command.
- `oxide_web_comparison::serve(root: &Path, address: &str) -> anyhow::Result<()>`: serves the three comparison tracks and canonical specification assets; called by the serve command.
- `oxide-web-comparison compare-exact`: retained extended diagnostic for zero-tolerance full-frame comparison; it is not the `rapid-v1` admission gate.
- `oxide_web_comparison::ShippingManifest`, `ShippingFile`, and `ShippingTotals`: serialized delivery-measurement records consumed by comparison reporting.
- `oxide_web_comparison::CONTROL_API_NAME`, `CONTROL_API_VERSION`, `IMPLEMENTATIONS`, and `SCENARIOS`: frozen contract identities consumed by validation and tests.

## Logic narrative

For an HTML-first request, the server selects the requested canonical scenario before writing response bytes. It parses the matching fixture once, renders only the scene's initially visible semantic nodes, and embeds the full fixture JSON in the response. This preserves normal HTML-first delivery while keeping feed and chat DOM construction equivalent to Oxide's visible-range materialization. The enhancement script subsequently adopts those nodes and attaches trusted-input listeners; it does not replace the initial tree.

Static asset requests are normalized before they are joined to the source root. `/specs` requests are routed to the versioned comparative specification directory. Manifest construction walks an implementation distribution, excludes development-only artifacts, hashes every shipped file, and invokes pinned compressor modes for delivery-size accounting.

Shipping manifest schema v2 persists per-file classification plus exact aggregate totals by category and route class. The route manifest is deterministic production inventory across all six canonical routes; observed Resource Timing/CDP rows remain the authority for which entities and encoded bytes a particular navigation actually requested.

Route-aware shipping manifests, trusted-input campaigns, presentation traces, browser process accounting, and delivery-size claims remain available as extended diagnostics. They are not prerequisites for the bounded `rapid-v1` visual-equivalence and output-ready comparison.

Each track exposes the same benchmark-owned `advance(checkpointId)` operation. It reaches fresh-install readiness, dashboard `leaf-updated`, feed `favorite-applied`, chat `append-settled`, navigation `modal-100`, image `first-visible`, or image `pan-mid` without OS input automation. Oxide replays the canonical trace prefix through `oxide-test-scenes`; both DOM tracks apply the same frozen state transition and visible work. The wrapper records `oxide-comparison-output-ready` when the renderer has submitted the frame or the DOM mutation has committed. Screenshot settling occurs afterward through `ready()` and is outside that timed interval.

The image action lazily requests the full source, then performs decode, upload/display, and visible-stage checkpoint transitions on both tracks. Oxide records fetch, decode, upload, encoded-byte, decoded-byte, WebGPU resource, and WASM committed-page diagnostics. The browser campaign retains the raw Chrome accessibility tree, JavaScript heap, DOM counters, common performance metrics, and available process metadata separately from the normalized cross-implementation hashes.

DOM emoji and status graphemes use the same pinned inline-text atlas variants as Oxide instead of system emoji fallback. Normal Latin, Arabic, and Simplified Chinese text uses the pinned benchmark font pack.

## Preconditions and postconditions

- `root` must contain the validated standalone browser source tree.
- HTML-first scenario IDs must be members of `SCENARIOS`.
- Canonical fixtures must contain the fields required by their scene contract.
- A successful HTML-first response contains `data-server-rendered="true"`, the exact `data-scenario-id`, semantic `data-role` nodes, and the complete fixture payload.
- Request paths must consist only of normalized path components.

## Edge cases and failure modes

- Unknown implementations, scenarios, missing fixtures, malformed JSON, or missing required fields return errors instead of silently selecting a different workload.
- Non-normalized paths are rejected before filesystem resolution.
- Missing static files return 404.
- Compressor startup or nonzero exit status prevents an incomplete shipping manifest.
- Text and attribute content is HTML-escaped; fixture JSON escapes `<` before embedding in a script element.

## Concurrency and memory behavior

The native server handles one short-lived TCP connection at a time and owns each response buffer. HTML-first rendering allocates one response string and one parsed fixture value per request. There are no locks, atomics, unsafe blocks, or cross-request shared mutable state.

## Performance notes

HTML-first response generation is linear in delivered fixture bytes plus visible node count. Feed renders only rows intersecting the initial 792-point list viewport while preserving `row_count = 2000`; chat renders the final ten rows that occupy the 700-point thread viewport while preserving `message_count = 5000`. Dashboard materializes the canonical 301 semantic roles. No production Oxide hot path is changed.

## Feature flags and cfgs

The native server and manifest implementation compile on non-WASM targets. The crate's `wasm` module is enabled only for `wasm32` and owns the separate Oxide/WebGPU runtime.

The shipping Oxide artifact is built with `wasm-pack 0.15.0` using the workspace's existing Rust source and dependency graph. The tool upgrade does not change the repository's pinned Rust toolchain.

## Testing and benchmarks

`cargo test -p oxide-web-comparison` verifies the three product tracks, shared rapid checkpoint control, development-artifact exclusion, route/category shipping summaries, both plan-to-manifest identities, all six HTML-first scenario responses, pinned inline-text materialization, exact route selection, and the DOM GPU-timing null contract. Headed browser capture plus the shared calibrated reducer remain the authority for visual acceptance and timing eligibility.

The `rapid-v1` lane captures exactly eleven endpoint rows and reduces them with `oxide-benchmark-spec`'s frozen calibrated comparison. Exact-static output remains an on-demand localization diagnostic.

The campaign also fails before browser launch when either selected route-aware shipping manifest is missing, malformed, or differs from the SHA-256 identity frozen into the plan.

## Examples

```text
cargo run --locked -p oxide-web-comparison -- serve host/web-comparison/sites 127.0.0.1:4173
```

Then request `/html-first/index.html?scenario=feed.variable-scroll` to receive the canonical server-rendered feed route.

Calibrated admission is run after paired screenshots are acquired:

```text
cargo xtask compare-ui reduce-visual --calibrated --reference REFERENCE.png --candidate OXIDE.png --layout benchmarks/comparative/specs/v1/layout/SCENARIO.json --canonical-scale 3 --output REPORT.json
```

## Changelog

- Added query-aware server rendering for all six canonical HTML-first scenarios.
- Added fixture-derived semantic DOM, visible-range materialization, HTML escaping, and route coverage tests.
- Upgraded the standalone Oxide browser artifact build to `wasm-pack 0.15.0` and retained optimized release output.
- Added scenario-specific trusted primary actions, semantic state-hash mutation checks, paired initial screenshots, pinned DOM inline-text atlas rendering, and exact static RGB comparison output.
- Added the bounded `rapid-v1` deterministic checkpoint transition shared by Oxide/WebGPU, HTML-first DOM, and client-rendered DOM, including canonical modal and decoded-image visible states.
