#![cfg(target_os = "macos")]

use oxide_comparison_runtime::{oxide_comparison_apply_trace_event, oxide_comparison_checkpoint_json, oxide_comparison_geometry_nodes_json, oxide_comparison_init, oxide_comparison_interaction_generation, oxide_comparison_macos_key, oxide_comparison_macos_pointer_event, oxide_comparison_macos_text, oxide_comparison_prepare_frame, oxide_comparison_prepare_release_candidate, oxide_comparison_prepare_scenario, oxide_comparison_quiesce, oxide_comparison_renderer_diagnostics, oxide_comparison_reset_scenario, oxide_comparison_role_count, oxide_comparison_set_renderer_diagnostics_enabled, oxide_comparison_shutdown, oxide_comparison_snapshot_png, oxide_comparison_submit_prepared_frame, oxide_comparison_take_snapshot, oxide_comparison_teardown_scenario, OxideComparisonRendererDiagnostics};
use std::path::PathBuf;
use std::sync::Mutex;

static RUNTIME_TEST_LOCK: Mutex<()> = Mutex::new(());

struct RuntimeGuard;

impl Drop for RuntimeGuard
{
   fn drop(&mut self)
   {
      oxide_comparison_shutdown();
   }
}

#[test]
fn isolated_runtime_prepares_renders_and_resets_canonical_scenario()
{
   let _runtime_lock = RUNTIME_TEST_LOCK.lock().expect("runtime test lock");
   let _guard = RuntimeGuard;
   assert_eq!(oxide_comparison_init(1_170, 2_532, 3.0), 0);
   let root = comparison_spec_root();
   let root = root.to_str().expect("comparison spec root must be UTF-8").as_bytes();
   let scenario = b"feed.variable-scroll";
   assert_eq!(oxide_comparison_prepare_scenario(root.as_ptr(), root.len(), scenario.as_ptr(), scenario.len()), 0);
   assert!(oxide_comparison_role_count() > 0);
   oxide_comparison_set_renderer_diagnostics_enabled(1);
   assert_eq!(oxide_comparison_prepare_frame(1_170, 2_532, 3.0), 0);
   assert_eq!(oxide_comparison_submit_prepared_frame(core::ptr::null_mut()), 0);
   assert_eq!(oxide_comparison_quiesce(), 0);
   let mut diagnostics = OxideComparisonRendererDiagnostics::default();
   assert_eq!(oxide_comparison_renderer_diagnostics(&mut diagnostics), 1);
   assert_eq!(diagnostics.schema_version, 1);
   assert!(diagnostics.submitted_frame_id > 0);
   assert_eq!(diagnostics.completed_frame_id, diagnostics.submitted_frame_id);
   assert!(diagnostics.render_prepare_begin_ticks > 0);
   assert!(diagnostics.render_prepare_begin_ticks <= diagnostics.render_prepare_end_ticks);
   assert!(diagnostics.render_prepare_end_ticks <= diagnostics.encode_begin_ticks);
   assert!(diagnostics.encode_begin_ticks <= diagnostics.encode_end_ticks);
   assert!(diagnostics.encode_end_ticks <= diagnostics.command_submit_ticks);
   assert!(diagnostics.encoded_bytes > 0);
   assert!(diagnostics.draw_calls > 0);
   assert!(diagnostics.damage_pixels > 0);
   assert!(diagnostics.damage_rects > 0);
   assert!(diagnostics.gpu_duration_ns > 0);
   assert_eq!(oxide_comparison_take_snapshot(), 0);
   let snapshot_length = oxide_comparison_snapshot_png(core::ptr::null_mut(), 0);
   let mut snapshot = vec![0_u8; snapshot_length];
   assert_eq!(oxide_comparison_snapshot_png(snapshot.as_mut_ptr(), snapshot.len()), snapshot_length);
   assert_eq!(&snapshot[..8], b"\x89PNG\r\n\x1a\n");

   let checkpoint = b"initial";
   let state_bytes = oxide_comparison_checkpoint_json(checkpoint.as_ptr(), checkpoint.len(), 0, core::ptr::null_mut(), 0);
   let accessibility_bytes = oxide_comparison_checkpoint_json(checkpoint.as_ptr(), checkpoint.len(), 1, core::ptr::null_mut(), 0);
   assert!(state_bytes > 0);
   assert!(accessibility_bytes > 0);
   assert_eq!(oxide_comparison_reset_scenario(), 0);
   oxide_comparison_teardown_scenario();
   assert_eq!(oxide_comparison_prepare_scenario(root.as_ptr(), root.len(), scenario.as_ptr(), scenario.len()), 0);
}

fn comparison_spec_root() -> PathBuf
{
   PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks/comparative/specs/v1")
}

#[test]
fn release_candidate_capture_entrypoint_prepares_all_five_without_weakening_normal_admission()
{
   let _runtime_lock = RUNTIME_TEST_LOCK.lock().expect("runtime test lock");
   let _guard = RuntimeGuard;
   assert_eq!(oxide_comparison_init(1_170, 2_532, 3.0), 0);
   let root = comparison_spec_root();
   let root = root.to_str().expect("comparison spec root must be UTF-8").as_bytes();
   for (scenario, checkpoint) in [
      ("grid.large-scroll", "initial"),
      ("effects.layers", "cold-built"),
      ("mutation.damage", "initial"),
      ("text.multilingual", "cold-visible"),
      ("resize.theme", "initial"),
   ]
   {
      let scenario = scenario.as_bytes();
      assert_eq!(oxide_comparison_prepare_scenario(root.as_ptr(), root.len(), scenario.as_ptr(), scenario.len()), -3);
      assert_eq!(oxide_comparison_prepare_release_candidate(root.as_ptr(), root.len(), scenario.as_ptr(), scenario.len()), 0);
      assert!(oxide_comparison_role_count() > 0);
      let checkpoint = checkpoint.as_bytes();
      assert!(oxide_comparison_checkpoint_json(checkpoint.as_ptr(), checkpoint.len(), 0, core::ptr::null_mut(), 0) > 0);
      assert_eq!(oxide_comparison_reset_scenario(), 0);
      oxide_comparison_teardown_scenario();
   }
}

#[test]
fn teardown_releases_renderer_caches_owned_by_the_destroyed_scene_router()
{
   let source = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"))
      .expect("comparison runtime source");
   let start = source.find("pub extern \"C\" fn oxide_comparison_teardown_scenario").expect("teardown entry point");
   let end = source[start..].find("pub extern \"C\" fn oxide_comparison_set_virtual_time_us").expect("next entry point") + start;
   let teardown = &source[start..end];
   let prepared = teardown.find("renderer.purge_prepared_chunks();").expect("prepared cache purge");
   let layers = teardown.find("renderer.purge_layer_cache();").expect("layer cache purge");
   let router = teardown.find("runtime.router = Some(scenes::Router::new").expect("router replacement");
   assert!(prepared < router);
   assert!(layers < router);
}

#[test]
fn quiescence_uses_the_existing_out_of_measurement_readback_barrier()
{
   let source = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"))
      .expect("comparison runtime source");
   let start = source.find("pub extern \"C\" fn oxide_comparison_quiesce").expect("quiescence entry point");
   let end = source[start..].find("pub extern \"C\" fn oxide_comparison_take_snapshot").expect("next entry point") + start;
   let quiescence = &source[start..end];
   assert!(quiescence.contains("renderer.readback_bgra8().is_some()"));
}

#[test]
fn snapshot_capture_uses_the_existing_renderer_readback()
{
   let source = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"))
      .expect("comparison runtime source");
   assert!(source.contains("snapshot_bgra: Vec<u8>"));
   assert!(source.contains("snapshot_png: Vec<u8>"));
   let start = source.find("pub extern \"C\" fn oxide_comparison_take_snapshot").expect("snapshot entry point");
   let end = source[start..].find("pub extern \"C\" fn oxide_comparison_snapshot_png").expect("next entry point") + start;
   let snapshot = &source[start..end];
   assert!(snapshot.contains("renderer.readback_bgra8()"));
   assert!(snapshot.contains("*snapshot_bgra = pixels;"));
   assert!(!snapshot.contains("temp_dir"));
}

#[test]
fn immutable_image_upload_drops_the_comparison_cpu_decode()
{
   let source = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"))
      .expect("comparison runtime source");
   assert!(source.contains("let Some(bgra) = image.source_bgra.take()"));
   assert!(!source.contains("let Some(bgra) = image.source_bgra.as_ref()"));
}

#[test]
fn macos_responder_input_advances_generation_only_for_rust_state_changes()
{
   let _runtime_lock = RUNTIME_TEST_LOCK.lock().expect("runtime test lock");
   let _guard = RuntimeGuard;
   assert_eq!(oxide_comparison_init(1_170, 2_532, 3.0), 0);
   let root = comparison_spec_root();
   let root = root.to_str().expect("comparison spec root must be UTF-8").as_bytes();
   let feed = b"feed.variable-scroll";
   assert_eq!(oxide_comparison_prepare_scenario(root.as_ptr(), root.len(), feed.as_ptr(), feed.len()), 0);
   let initial = oxide_comparison_interaction_generation();
   assert_eq!(oxide_comparison_macos_pointer_event(0, 340.0, 67.0, 1.0), 0);
   assert_eq!(oxide_comparison_interaction_generation(), initial);
   assert_eq!(oxide_comparison_macos_pointer_event(2, 340.0, 67.0, 1.1), 1);
   assert_eq!(oxide_comparison_interaction_generation(), initial + 1);

   assert_eq!(oxide_comparison_reset_scenario(), 0);
   let before_drag = oxide_comparison_interaction_generation();
   assert_eq!(oxide_comparison_macos_pointer_event(0, 340.0, 67.0, 2.0), 0);
   assert_eq!(oxide_comparison_macos_pointer_event(1, 340.0, 47.0, 2.1), 1);
   assert_eq!(oxide_comparison_macos_pointer_event(2, 340.0, 47.0, 2.2), 0);
   assert_eq!(oxide_comparison_interaction_generation(), before_drag + 1);
   let checkpoint = b"drag";
   let needed = oxide_comparison_checkpoint_json(checkpoint.as_ptr(), checkpoint.len(), 0, core::ptr::null_mut(), 0);
   let mut state = vec![0_u8; needed as usize];
   assert_eq!(oxide_comparison_checkpoint_json(checkpoint.as_ptr(), checkpoint.len(), 0, state.as_mut_ptr(), state.len()), needed);
   assert_eq!(serde_json::from_slice::<serde_json::Value>(&state).expect("feed state JSON")["model"]["favorite_id"], serde_json::Value::Null);

   oxide_comparison_teardown_scenario();
   let chat = b"chat.live-update";
   assert_eq!(oxide_comparison_prepare_scenario(root.as_ptr(), root.len(), chat.as_ptr(), chat.len()), 0);
   let before_text = oxide_comparison_interaction_generation();
   let typed = b"Oxide";
   assert_eq!(oxide_comparison_macos_text(typed.as_ptr(), typed.len()), 1);
   assert_eq!(oxide_comparison_interaction_generation(), before_text + 1);
   assert_eq!(oxide_comparison_macos_text(typed.as_ptr(), 0), 0);
   assert_eq!(oxide_comparison_interaction_generation(), before_text + 1);
}

#[test]
fn macos_responder_keys_replace_the_visible_chat_target_without_composer_misrouting()
{
   let _runtime_lock = RUNTIME_TEST_LOCK.lock().expect("runtime test lock");
   let _guard = RuntimeGuard;
   assert_eq!(oxide_comparison_init(1_170, 2_532, 3.0), 0);
   let root = comparison_spec_root();
   let root = root.to_str().expect("comparison spec root must be UTF-8").as_bytes();
   let chat = b"chat.live-update";
   assert_eq!(oxide_comparison_prepare_scenario(root.as_ptr(), root.len(), chat.as_ptr(), chat.len()), 0);
   for (phase, count) in [(b"prepend-50".as_slice(), 1_usize), (b"append-10hz".as_slice(), 20), (b"type-100".as_slice(), 100), (b"paste-10kib".as_slice(), 1)]
   {
      for index in 0..count
      {
         assert_eq!(oxide_comparison_apply_trace_event(phase.as_ptr(), phase.len(), index), 0);
      }
   }
   assert_eq!(oxide_comparison_prepare_frame(1_170, 2_532, 3.0), 0);
   assert_eq!(oxide_comparison_submit_prepared_frame(core::ptr::null_mut()), 0);
   let needed = oxide_comparison_geometry_nodes_json(core::ptr::null_mut(), 0);
   let mut geometry = vec![0_u8; needed as usize];
   assert_eq!(oxide_comparison_geometry_nodes_json(geometry.as_mut_ptr(), geometry.len()), needed);
   let nodes = serde_json::from_slice::<serde_json::Value>(&geometry).expect("chat geometry JSON");
   let target = nodes.as_array().expect("chat geometry nodes").iter().find(|node| node["identifier"] == "chat:append:16").expect("visible chat target");
   let x = target["bounds"]["x"].as_f64().expect("target x") as f32 + 8.0;
   let y = target["bounds"]["y"].as_f64().expect("target y") as f32 + 8.0;
   let mut generation = oxide_comparison_interaction_generation();
   assert_eq!(oxide_comparison_macos_pointer_event(0, x, y, 1.0), 0);
   assert_eq!(oxide_comparison_macos_pointer_event(2, x, y, 1.1), 1);
   generation += 1;
   assert_eq!(oxide_comparison_interaction_generation(), generation);
   assert_eq!(oxide_comparison_macos_key(115, 0, core::ptr::null(), 0), 1);
   generation += 1;
   for _ in 0..6
   {
      assert_eq!(oxide_comparison_macos_key(124, 1, core::ptr::null(), 0), 1);
      generation += 1;
   }
   for character in [b"O".as_slice(), b"x".as_slice(), b"i".as_slice(), b"d".as_slice(), b"e".as_slice()]
   {
      assert_eq!(oxide_comparison_macos_key(0, 0, character.as_ptr(), character.len()), 1);
      generation += 1;
   }
   assert_eq!(oxide_comparison_interaction_generation(), generation);
   let checkpoint = b"selection-replaced";
   let needed = oxide_comparison_checkpoint_json(checkpoint.as_ptr(), checkpoint.len(), 0, core::ptr::null_mut(), 0);
   let mut state = vec![0_u8; needed as usize];
   assert_eq!(oxide_comparison_checkpoint_json(checkpoint.as_ptr(), checkpoint.len(), 0, state.as_mut_ptr(), state.len()), needed);
   let state = serde_json::from_slice::<serde_json::Value>(&state).expect("chat state JSON");
   assert_eq!(state["model"]["focused_message_id"], "chat:append:16");
   assert_eq!(state["model"]["selection_active"], false);
   assert_eq!(state["model"]["replacement_applied"], true);
   assert_eq!(state["model"]["composer_utf8_count"], 10_340);
}

#[test]
fn macos_responder_drags_drive_canonical_image_zoom_and_navigation_cancel()
{
   let _runtime_lock = RUNTIME_TEST_LOCK.lock().expect("runtime test lock");
   let _guard = RuntimeGuard;
   assert_eq!(oxide_comparison_init(1_170, 2_532, 3.0), 0);
   let root = comparison_spec_root();
   let root = root.to_str().expect("comparison spec root must be UTF-8").as_bytes();
   let image = b"image.decode-zoom";
   assert_eq!(oxide_comparison_prepare_scenario(root.as_ptr(), root.len(), image.as_ptr(), image.len()), 0);
   let mut generation = oxide_comparison_interaction_generation();
   assert_eq!(oxide_comparison_macos_pointer_event(0, 195.0, 814.0, 1.0), 0);
   assert_eq!(oxide_comparison_macos_pointer_event(1, 302.4, 814.0, 1.1), 1);
   generation += 1;
   assert_eq!(oxide_comparison_macos_pointer_event(2, 374.0, 814.0, 1.2), 1);
   generation += 1;
   assert_eq!(oxide_comparison_interaction_generation(), generation);
   let checkpoint = b"pinch-mid";
   let needed = oxide_comparison_checkpoint_json(checkpoint.as_ptr(), checkpoint.len(), 0, core::ptr::null_mut(), 0);
   let mut state = vec![0_u8; needed as usize];
   assert_eq!(oxide_comparison_checkpoint_json(checkpoint.as_ptr(), checkpoint.len(), 0, state.as_mut_ptr(), state.len()), needed);
   assert_eq!(serde_json::from_slice::<serde_json::Value>(&state).expect("image state JSON")["model"]["scale_millionths"], 2_000_000);

   oxide_comparison_teardown_scenario();
   let navigation = b"navigation.modal";
   assert_eq!(oxide_comparison_prepare_scenario(root.as_ptr(), root.len(), navigation.as_ptr(), navigation.len()), 0);
   generation = oxide_comparison_interaction_generation();
   assert_eq!(oxide_comparison_macos_pointer_event(0, 370.5, 422.0, 2.0), 0);
   assert_eq!(oxide_comparison_macos_pointer_event(1, 292.5, 422.0, 2.1), 1);
   generation += 1;
   assert_eq!(oxide_comparison_macos_pointer_event(1, 195.0, 422.0, 2.2), 1);
   generation += 1;
   let mid = b"cancel-mid";
   let needed = oxide_comparison_checkpoint_json(mid.as_ptr(), mid.len(), 0, core::ptr::null_mut(), 0);
   let mut state = vec![0_u8; needed as usize];
   assert_eq!(oxide_comparison_checkpoint_json(mid.as_ptr(), mid.len(), 0, state.as_mut_ptr(), state.len()), needed);
   let state = serde_json::from_slice::<serde_json::Value>(&state).expect("navigation midpoint JSON");
   assert_eq!(state["model"]["route"], "detail");
   assert_eq!(state["model"]["modal_visible"], true);
   assert_eq!(oxide_comparison_macos_pointer_event(2, 195.0, 422.0, 2.3), 1);
   generation += 1;
   assert_eq!(oxide_comparison_interaction_generation(), generation);
   let restored = b"cancel-restored";
   let needed = oxide_comparison_checkpoint_json(restored.as_ptr(), restored.len(), 0, core::ptr::null_mut(), 0);
   let mut state = vec![0_u8; needed as usize];
   assert_eq!(oxide_comparison_checkpoint_json(restored.as_ptr(), restored.len(), 0, state.as_mut_ptr(), state.len()), needed);
   let state = serde_json::from_slice::<serde_json::Value>(&state).expect("navigation restored JSON");
   assert_eq!(state["model"]["route"], "list");
   assert_eq!(state["model"]["modal_visible"], false);
}
