# oxide-feed-v1-app `src/observation.rs`

## Intention and purpose

This module turns one admitted feed-v1 run into strict, bounded, durable evidence. It owns the success/failure JSON schemas, physical component/clip serialization, live device/display-link snapshots, ready/failure/completion notifications, nonce validation, and atomic result-file publication. Keeping transport outside the app state machine makes the measured drawing path small and makes every publication boundary independently testable.

## Relation to the rest of the code

- `src/lib.rs` captures visible components and callback pairs into fixed arrays, asks this module for ready/completion environment snapshots, and persists exactly one terminal record.
- [`contract.rs`](contract.md) supplies fixture identity, geometry, schemas, component names, result names, and notification prefixes.
- The production iOS host supplies display-link range, thermal/low-power endpoints, maximum refresh, and cumulative environment-transition counts through small C ABI queries.
- The device harness receives Darwin notifications only after the corresponding synchronized file/state boundary. The reducer then strictly re-parses the emitted schema.

Call graph:

- warm-frame submit -> `capture_device_state` + internal transition snapshot -> `post_ready`
- closing-frame submit -> endpoint/transition delta -> `persist_success_and_post`
- any terminal error -> `persist_failure_and_post` (or notification-only last resort)
- persistence -> nonce/path validation -> exclusive temporary file -> JSON writer -> file sync -> rename -> directory sync -> Darwin notification

## Entry points list

- `oxide_feed_v1_app::observation::MAX_CALLBACK_SAMPLES: usize` fixes callback storage at 1,024; `MAX_VISIBLE_COMPONENTS: usize` fixes ready-frame observation storage at 96.
- `CallbackSample { timestamp_ns, target_timestamp_ns }` stores one `CADisplayLink` pair in integer nanoseconds.
- `ObservedRect { x, y, width, height }` stores a renderer-observed content-space point rectangle.
- `ObservedComponent { row_index, kind_index, content_rect_points }` associates an observed rectangle with frozen row/component order.
- `DeviceState { thermal_state, low_power_mode, maximum_frames_per_second, range_minimum_frames_per_second, range_maximum_frames_per_second, range_preferred_frames_per_second }` is one endpoint snapshot. `DeviceState::thermal_name(self) -> &'static str` maps native numeric thermal state to the strict record spelling.
- `RunRecord<'a>` borrows nonce/phase, fixture, visible components, and callbacks while carrying indices, endpoint offsets/timestamps, required `inertia_observed`, before/after device states, and required `thermal_state_change_count`/`low_power_mode_change_count` `u32` deltas.
- `FailureRecord<'a> { nonce, treatment, stage, message }` represents a separate non-measurable terminal record; nonce/treatment may be absent only when launch parsing could not establish them.
- `ObservationError::{InvalidNonce, MissingHome, Io, Notification}` reports validation, sandbox, durable-I/O, and Darwin-notification failure. Its `Display`, `Error`, and `From<io::Error>` implementations preserve a concise causal message.
- `capture_device_state() -> Option<DeviceState>` returns a validated live iOS endpoint snapshot or deterministic nominal native-test state.
- `post_ready(nonce) -> Result<(), ObservationError>` and `post_failure(nonce) -> Result<(), ObservationError>` post nonce-scoped Darwin notifications after validation.
- `nonce_is_valid(nonce) -> bool` exposes the shared filename/notification-safe predicate: 1 through 128 ASCII alphanumeric-or-hyphen bytes.
- `persist_success_and_post(record) -> Result<PathBuf, ObservationError>` durably publishes a complete record before posting completion.
- `persist_failure_and_post(record) -> Result<PathBuf, ObservationError>` durably publishes a failure record before posting failure.
- `write_record_json(record, writer) -> io::Result<()>` serializes the exact nested success schema without an intermediate JSON value tree.
- `write_failure_json(record, writer) -> io::Result<()>` serializes the exact flat failure schema.

## Logic narrative

Device capture reads thermal state, low-power mode, maximum refresh, and the configured display-link range. All range values must be finite and positive. Separately, an internal helper snapshots the host's cumulative atomic thermal and low-power transition counters. Ready admission takes that counter baseline before its endpoint queries so a transient change-and-return during those queries remains visible at completion. The app serializes saturating ready-to-completion differences only after each difference fits `u32`.

Success serialization writes fixture/run identity, canvas, actual admitted geometry, environment endpoints and transition deltas, gesture travel/duration/direct inertia observation, raw callback pairs, and terminal status in one forward pass. Observed point rectangles are validated, rounded once to physical pixels, clipped against the frozen viewport, and emitted in observed order. Unknown/out-of-range components are skipped defensively; app admission independently requires the exact complete sequence.

Persistence validates the nonce before deriving any path or notification. It creates one exclusive same-directory temporary file, streams JSON through `BufWriter`, flushes and `sync_all`s the file, atomically renames it, syncs the containing directory, and only then posts the notification. Failure records use the same durability sequence but a different schema and prefix so a failed run cannot masquerade as timing evidence.

## Preconditions and postconditions

- Nonces are 1 through 128 ASCII alphanumeric, hyphen, underscore, or dot characters. Path separators, whitespace, control characters, and empty strings are rejected.
- iOS callers run device/environment queries on the linked production host's app thread.
- A success record has an admitted fixture, actual ready geometry, a positive configured frame-rate range, at least two callback samples, direct Rust-owned inertia observation, and zero-or-greater environment transition deltas.
- `write_record_json` emits `thermal_state_change_count` and `low_power_mode_change_count` even when either is zero.
- Successful persistence means the final path exists, its file contents and directory entry have been synchronized, and the matching Darwin notification returned success.
- Failure persistence never writes the complete-run schema or timing arrays.

### Unsafe contracts

- `oxide_host_power_lowpower`, `oxide_host_thermal_state`, and `oxide_host_max_framerate_hz` take no pointers and are side-effect-free snapshots supplied by the linked iOS host.
- `oxide_host_environment_transition_counts` receives two distinct, aligned, writable `u64` pointers that remain live for the call. The host writes exactly one cumulative value to each and retains neither pointer.
- `notify_post` receives `CString::as_ptr()` from a live NUL-terminated value for the duration of the call and retains no pointer.
- No FFI value is interpreted as a reference, slice, or layout-dependent Rust type.

## Edge cases and failure modes

- Missing sandbox `HOME`, exclusive temp-file collision, short/failed writes, sync/rename failure, and notification failure are returned as `ObservationError`.
- A failed atomic write removes its temporary path on a best-effort basis; the final path is never partially overwritten.
- Non-finite, non-positive, or unavailable display-link range rejects admission/completion.
- Unknown thermal integers serialize as `"unknown"` and are subsequently rejected by strict device admission.
- Invalid observed rectangles, foreign component indices, nonvisible clips, and invalid offsets are omitted instead of generating malformed unsigned geometry; exact app admission prevents a complete record from relying on that omission.
- JSON strings escape quotes, backslashes, line controls, and all remaining control characters.
- Native host tests do not post Darwin notifications and use zero cumulative transition counts; this isolates schema/state-machine logic without claiming iOS evidence.

## Concurrency and memory behavior

Serialization is single-threaded and borrows record slices; it does not clone the callback or component populations. `BufWriter` provides one bounded I/O buffer outside the measured interval. Notification names and temp filenames allocate only during ready/terminal protocol work. The iOS host owns atomic transition-counter synchronization; snapshot values are copied into app state and need no lock here.

## Performance notes

- No environment query or file operation occurs in the per-frame render loop. Cumulative counters are sampled exactly at ready and completion.
- JSON is streamed directly to `Write`, avoiding a full `serde_json::Value` tree or duplicate record buffer.
- Geometry work is linear in the small frozen visible-component slice; callback serialization is linear in at most 1,024 samples.
- Durability costs are intentional and occur only after the measured closing frame has been successfully submitted.

## Feature flags and cfgs

No crate features. On iOS, live host queries and Darwin notifications are enabled and the module links `System`. On non-iOS targets, device state is deterministic nominal 120 Hz, transition counts are zero, and notifications are no-ops; file persistence remains real so integration tests exercise durability and schemas.

## Testing and benchmarks

[`tests/observation_tests.md`](tests/observation_tests.md) validates exact object keys, fixture identity, endpoint geometry/clip translation, required transition fields, failure separation, null launch inputs, and nonce injection resistance. [`tests/lib_tests.md`](tests/lib_tests.md) additionally completes the public app state machine and reads the persisted success record to prove zero transition deltas remain present. Physical-iPhone runs validate the actual host FFI and Darwin transport.

## Examples

```rust
use oxide_feed_v1_app::observation::nonce_is_valid;

assert!(nonce_is_valid("primary-s00-p00-o0-oxide-forward"));
assert!(!nonce_is_valid("../escape"));
```

## Changelog

- 2026-08-06: Narrowed completion identity to the shared 1-through-128-byte ASCII alphanumeric-or-hyphen grammar.
- 2026-08-06: Added required direct inertia observation and ready-to-completion thermal/low-power change counts.
- 2026-08-06: Moved schema coverage to mapped public-API integration tests.
- 2026-08-06: Added bounded observation types, strict serializers, durable atomic persistence, and nonce-scoped notifications.
