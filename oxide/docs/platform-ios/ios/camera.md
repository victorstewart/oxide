# platform-ios `src/ios/camera.m`

## Intention and purpose
- Own the iOS AVFoundation bridge used by the shared Apple camera manager when the native camera bridge is compiled.
- Deliver Oxide-owned preview frames, audio samples, recording events, and photo events without moving product rendering into UIKit or `AVCaptureVideoPreviewLayer`.

## Relation to the rest of the code
- `oxide-platform-apple` owns the Rust `AppleCameraManager` state and callback fanout.
- This file owns iOS-only AVFoundation sessions, pixel-buffer translation, recording/photo capture, and native timing instrumentation.
- `tests/abi_layout_tests.rs` freezes the public camera callback struct layouts that cross from this Objective-C bridge into Rust.

## Entry points list
- `oxide_cam_set_frame_callback`, `oxide_cam_set_audio_callback`, `oxide_cam_set_record_callback`, and `oxide_cam_set_photo_callback`
  Register Rust callbacks for translated native camera payloads.
- `oxide_cam_start`, `oxide_cam_stop`, and camera control exports
  Start, stop, and control native AVFoundation session behavior behind the shared Apple camera ABI.
- `OxideCameraFrameCallback`, `OxideCameraAudioCallback`, `OxideCameraRecordCallback`, and `OxideCameraPhotoCallback`
  Callback typedefs whose payload structs are mirrored by Rust `#[repr(C)]` types.

## Logic narrative
- AVFoundation sample buffers are translated into compact C structs before Rust callback delivery, keeping Rust responsible for visible preview composition and pacing.
- Recording and photo events preserve status, timing, byte, and error fields so Rust can report failures without decoding Objective-C objects.
- `_Static_assert` guards freeze `OxideCamFrame`, `OxideCamAudio`, `OxideCamRecordEvent`, and `OxideCamPhotoEvent` size/alignment beside the C definitions.
- Additional `_Static_assert` guards freeze `OxideCamPerfSnapshot` and `OxideCamContractSnapshot`, because those host-private camera benchmark snapshots feed the iOS host stats ABI used for device A/B evidence.

## Preconditions and postconditions
- Native callbacks must remain ABI-compatible with the Rust declarations in `oxide-platform-ios` and `oxide-platform-apple`.
- Frame/audio/photo/record payload pointers are valid only for the callback duration unless Rust copies the referenced data.
- Hosts must keep visible preview rendering in Oxide-owned renderer paths; system preview layers are diagnostic-only.

## Edge cases and failure modes
- Missing camera permission, unavailable device, busy session, invalid input, I/O failure, and unsupported capture modes map to explicit native return/event status codes.
- Photo and record errors include copied error text when available and report empty error payloads when native messages are absent.

## Concurrency and memory behavior
- AVFoundation callbacks arrive on native queues and are translated before crossing the C ABI.
- The frozen callback structs carry pointers and lengths; ownership remains with native buffers unless Rust copies during callback handling.
- The ABI guard change adds no runtime memory traffic.

## Performance notes
- The 2026-06-22 static assertions are measurement harness only and do not change camera frame acquisition, texture bridge, command encoding, present, or host tick timings.
- Camera-preview performance changes still require device A/B proof with the fine-grained pure-custom path attribution required by the repo contract.

## Feature flags and cfgs
- Compiled through the iOS native camera bridge path; host/device availability controls live camera behavior.

## Testing and benchmarks
- ABI shape and source-guard retention are covered by `cargo test --locked -j$(sysctl -n hw.ncpu) -p oxide-platform-ios --test abi_layout_tests`.
- Host-private camera snapshot guard retention is also covered by `cargo test --locked -j$(sysctl -n hw.ncpu) -p oxide-host-ios --test abi_layout_tests`.

## Changelog
- 2026-06-22: added native camera perf/contract snapshot ABI layout guards.
- 2026-06-22: added and documented native camera callback ABI layout guards.
