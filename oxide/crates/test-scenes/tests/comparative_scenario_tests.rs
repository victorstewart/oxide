use oxide_benchmark_spec::{load_apple_pr_scenarios, load_pr_vertical_scenarios, load_scenario, validate_apple_pr_scenario_set, validate_nightly_endurance_scenario, ArtifactIdentity, AssetManifest, FairnessContract, FontPackIdentity, ImageDecodeZoomFixture, MacOsComparatorScale, MacOsComparatorScaleDimension, MacOsComparatorScaleOverlay, MacOsComparatorScaleTransform, ScenarioSpec, SceneContract, TraceEvent};
use oxide_renderer_api as gfx;
use oxide_test_scenes::Router;
use oxide_text::Font;
use oxide_ui_core::DrawListBuilder;
use std::fs;
use std::path::{Path, PathBuf};

mod helpers;

use helpers::NullUploader;

const VIEWPORT: gfx::RectF = gfx::RectF {x: 0.0, y: 0.0, w: 390.0, h: 844.0};

fn workspace_root() -> PathBuf
{
   Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn spec_root() -> PathBuf
{
   workspace_root().join("benchmarks/comparative/specs/v1")
}

fn prepare(scenario: &ScenarioSpec) -> Router<NullUploader>
{
   let root = spec_root();
   let fixture = fs::read(root.join(&scenario.fixture.path)).expect("fixture bytes");
   let mut router = Router::new(NullUploader);
   router.prepare_comparison_scenario(scenario, &fixture).expect("comparison scene");
   router.set_comparison_resources(gfx::ImageHandle(7), [0, 1, 2]).expect("comparison resources");
   let manifest = serde_json::from_slice::<AssetManifest>(&fs::read(root.join(&scenario.assets.path)).expect("asset manifest")).expect("asset manifest JSON");
   if let Some(inline_text) = manifest.inline_text_atlas
   {
      let images = inline_text.variants.iter().enumerate().map(|(index, _)| gfx::ImageHandle(17 + index as u32)).collect();
      router.set_comparison_inline_text_resources(images, inline_text).expect("inline-text resources");
   }
   if scenario.id == "image.decode-zoom"
   {
      let image = serde_json::from_slice::<ImageDecodeZoomFixture>(&fixture).expect("image fixture");
      router.set_comparison_image_resources(gfx::ImageHandle(11), &image.source.artifact.sha256, gfx::ImageHandle(12), &image.thumbnail.artifact.sha256).expect("comparison image resources");
   }
   router
}

fn prepare_release(id: &str) -> Router<NullUploader>
{
   let root = spec_root();
   let fixture = fs::read(root.join("fixtures").join(format!("{id}.json"))).expect("release fixture bytes");
   let mut router = Router::new(NullUploader);
   router.prepare_comparison_scenario_id(id, &fixture).expect("release comparison scene");
   router.set_comparison_resources(gfx::ImageHandle(7), [0, 1, 2]).expect("release comparison resources");
   let manifest = serde_json::from_slice::<AssetManifest>(&fs::read(root.join("assets/neutral-v1.json")).expect("release asset manifest")).expect("release asset manifest JSON");
   if let Some(inline_text) = manifest.inline_text_atlas
   {
      let images = inline_text.variants.iter().enumerate().map(|(index, _)| gfx::ImageHandle(17 + index as u32)).collect();
      router.set_comparison_inline_text_resources(images, inline_text).expect("release inline-text resources");
   }
   router
}

fn release_scenario(id: &str) -> ScenarioSpec
{
   let artifact = |path: &str| ArtifactIdentity {path: String::from(path), sha256: String::new()};
   ScenarioSpec {
      schema_version: 1,
      id: String::from(id),
      fixture: artifact(&format!("fixtures/{id}.json")),
      assets: artifact("assets/neutral-v1.json"),
      font_pack: FontPackIdentity {id: String::from("oxide-bench-fonts-v1"), manifest: String::from("font-packs/oxide-bench-fonts-v1.json"), sha256: String::new()},
      viewport_class: String::from("phone-portrait"),
      scene: SceneContract {roles: Vec::new(), style_tokens: artifact("styles/neutral-v1.json"), layout_assertions: artifact(&format!("layout/{id}.json"))},
      phases: Vec::new(),
      primary_metric: String::new(),
      required_metrics: Vec::new(),
      optional_metrics: Vec::new(),
      parity_checkpoints: Vec::new(),
      fairness_contract: FairnessContract {
         locale: String::from("en_US_POSIX"),
         timezone: String::from("UTC"),
         direction: String::from("ltr"),
         logical_viewport_width: 390,
         logical_viewport_height: 844,
         expected_visible_role_counts: Vec::new(),
         schedule_tolerance_us: 0,
         coordinate_tolerance_microunits: 0,
         elapsed_time_driven: true,
      },
   }
}

#[test]
fn two_x_scale_owns_a_second_semantic_shard_without_changing_visible_evidence()
{
   let root = spec_root();
   for (id, dimension, transform, base, trace) in [
      ("feed.variable-scroll", MacOsComparatorScaleDimension::DatasetCardinality, MacOsComparatorScaleTransform::NamespacedDatasetShards, 2_000, "feed-favorite-one.json"),
      ("effects.layers", MacOsComparatorScaleDimension::OperationCardinality, MacOsComparatorScaleTransform::IsolatedOperationShadow, 3, "effects-dirty-layer.json"),
   ]
   {
      let scenario = release_scenario(id);
      let fixture = fs::read(root.join(&scenario.fixture.path)).expect("scale fixture");
      let overlay = MacOsComparatorScaleOverlay {
         schema_version: 1,
         scenario_id: String::from(id),
         scale: MacOsComparatorScale::TwoX,
         dimension,
         transform,
         base_cardinality: base,
         effective_cardinality: base * 2,
         fixture: scenario.fixture.clone(),
      };
      let mut router = Router::new(NullUploader);
      router.prepare_scaled_comparison_scenario(&scenario, &fixture, &overlay).expect("scaled comparison scene");
      let before = router.comparison_checkpoint_json("scale").expect("visible primary evidence");
      let events = serde_json::from_slice::<Vec<TraceEvent>>(&fs::read(root.join("traces").join(trace)).expect("scale trace")).expect("scale trace JSON");
      for event in &events
      {
         router.apply_comparison_event(event).expect("scaled event");
      }
      let (_, completed) = router.comparison_scale_attestation().expect("scale attestation");
      assert!(completed);
      assert_eq!(router.comparison_scale_attestation().expect("scale cardinality").0, base * 2);
      assert_ne!(before, router.comparison_checkpoint_json("scale").expect("updated primary evidence"));
   }
}

#[test]
fn release_scenes_replay_frozen_traces_and_match_every_semantic_checkpoint()
{
   let root = spec_root();
   let cases = [
      ("grid.large-scroll", &["initial", "scrolled-75-percent", "reversed", "detail", "restored"][..], &["grid-scroll-to-75-percent.json", "grid-reverse-scroll.json", "grid-select-detail-back.json"][..]),
      ("effects.layers", &["cold-built", "animation-mid", "dirty-layer"][..], &["effects-cold-build.json", "effects-warm-animation.json", "effects-dirty-layer.json"][..]),
      ("mutation.damage", &["initial", "mutated-1-percent", "mutated-10-percent", "mutated-100-percent"][..], &["mutation-1-percent.json", "mutation-10-percent.json", "mutation-100-percent.json"][..]),
      ("text.multilingual", &["cold-visible", "warm-replayed", "scale-wrap-changed"][..], &["text-font-atlas-cold.json", "text-warm-replay.json", "text-scale-wrap-change.json"][..]),
      ("resize.theme", &["initial", "change-05", "change-10"][..], &["resize-theme-ten-changes.json"][..]),
   ];
   for (id, checkpoints, traces) in cases
   {
      let mut router = prepare_release(id);
      assert_release_checkpoint(&root, id, checkpoints[0], &router);
      match id
      {
         "mutation.damage" =>
         {
            for (trace, checkpoint) in traces.iter().zip(&checkpoints[1..])
            {
               replay_release_trace(&root, trace, &mut router);
               assert_release_checkpoint(&root, id, checkpoint, &router);
            }
         }
         "grid.large-scroll" =>
         {
            replay_release_trace(&root, "grid-scroll-to-75-percent.json", &mut router);
            assert_release_checkpoint(&root, id, "scrolled-75-percent", &router);
            replay_release_trace(&root, "grid-reverse-scroll.json", &mut router);
            assert_release_checkpoint(&root, id, "reversed", &router);
            let trace = serde_json::from_slice::<Vec<TraceEvent>>(&fs::read(root.join("traces/grid-select-detail-back.json")).expect("grid detail trace")).expect("grid detail events");
            for event in &trace[..3] {router.apply_comparison_event(event).expect("grid detail event");}
            assert_release_checkpoint(&root, id, "detail", &router);
            for event in &trace[3..] {router.apply_comparison_event(event).expect("grid restore event");}
            assert_release_checkpoint(&root, id, "restored", &router);
         }
         "resize.theme" =>
         {
            let trace = serde_json::from_slice::<Vec<TraceEvent>>(&fs::read(root.join("traces/resize-theme-ten-changes.json")).expect("resize trace")).expect("resize events");
            for event in &trace[..15] {router.apply_comparison_event(event).expect("first five resize changes");}
            assert_release_checkpoint(&root, id, "change-05", &router);
            for event in &trace[15..] {router.apply_comparison_event(event).expect("last five resize changes");}
            assert_release_checkpoint(&root, id, "change-10", &router);
         }
         "effects.layers" =>
         {
            replay_release_trace(&root, "effects-cold-build.json", &mut router);
            assert_release_checkpoint(&root, id, "cold-built", &router);
            let trace = serde_json::from_slice::<Vec<TraceEvent>>(&fs::read(root.join("traces/effects-warm-animation.json")).expect("effects trace")).expect("effects events");
            for event in &trace[..3] {router.apply_comparison_event(event).expect("effects animation event");}
            assert_release_checkpoint(&root, id, "animation-mid", &router);
            for event in &trace[3..] {router.apply_comparison_event(event).expect("remaining effects animation event");}
            replay_release_trace(&root, "effects-dirty-layer.json", &mut router);
            assert_release_checkpoint(&root, id, "dirty-layer", &router);
         }
         _ =>
         {
            for (trace, checkpoint) in traces.iter().zip(checkpoints)
            {
               replay_release_trace(&root, trace, &mut router);
               assert_release_checkpoint(&root, id, checkpoint, &router);
            }
         }
      }
      let builder = draw(&mut router);
      assert!(!builder.drawlist().items.is_empty(), "{id} must emit visible work");
   }
}

#[test]
fn capture_only_identity_preparation_accepts_every_release_candidate_and_rejects_unknown_ids()
{
   let root = spec_root();
   for id in oxide_benchmark_spec::RELEASE_CANDIDATE_IDS
   {
      assert!(!root.join("scenarios").join(format!("{id}.json")).exists());
      let fixture = fs::read(root.join("fixtures").join(format!("{id}.json"))).expect("release fixture bytes");
      let mut router = Router::new(NullUploader);
      router.prepare_comparison_scenario_id(id, &fixture).expect("release candidate scene");
   }
   let mut router = Router::new(NullUploader);
   assert_eq!(router.prepare_comparison_scenario_id("unknown.release-candidate", b"{}"), Err(String::from("unsupported comparison scenario unknown.release-candidate")));
}

fn replay_release_trace(root: &Path, trace: &str, router: &mut Router<NullUploader>)
{
   let events = serde_json::from_slice::<Vec<TraceEvent>>(&fs::read(root.join("traces").join(trace)).expect("release trace bytes")).expect("release trace events");
   for event in events
   {
      router.apply_comparison_event(&event).expect("release trace event");
   }
}

fn assert_release_checkpoint(root: &Path, id: &str, checkpoint: &str, router: &Router<NullUploader>)
{
   let (state, accessibility) = router.comparison_checkpoint_json(checkpoint).expect("release checkpoint JSON");
   let expected_state = fs::read(root.join(format!("checkpoints/{id}/{checkpoint}/state.json"))).expect("release expected state");
   let expected_accessibility = fs::read(root.join(format!("checkpoints/{id}/{checkpoint}/accessibility.json"))).expect("release expected accessibility");
   assert_eq!(serde_json::from_slice::<serde_json::Value>(&state).expect("release state JSON"), serde_json::from_slice::<serde_json::Value>(&expected_state).expect("release expected state JSON"), "{id}:{checkpoint}:state");
   assert_eq!(serde_json::from_slice::<serde_json::Value>(&accessibility).expect("release accessibility JSON"), serde_json::from_slice::<serde_json::Value>(&expected_accessibility).expect("release expected accessibility JSON"), "{id}:{checkpoint}:accessibility");
}

fn install_comparison_latin_font(router: &mut Router<NullUploader>)
{
   let font = Font::from_bytes(
      fs::read(spec_root().join("font-packs/oxide-bench-fonts-v1/NotoSans-VF.ttf")).expect("Latin comparison font"),
   );
   let font_id = router.text.fonts.add_font(font);
   router.set_comparison_fonts([font_id; 3]).expect("comparison fonts");
}

fn load_trace(root: &Path, scenario: &ScenarioSpec, phase_id: &str) -> Vec<TraceEvent>
{
   let phase = scenario.phases.iter().find(|phase| phase.id == phase_id).expect("scenario phase");
   let trace = phase.trace.as_ref().expect("phase trace");
   serde_json::from_slice(&fs::read(root.join(&trace.path)).expect("trace bytes")).expect("trace events")
}

fn draw(router: &mut Router<NullUploader>) -> DrawListBuilder
{
   let mut builder = DrawListBuilder::new();
   router.draw(VIEWPORT, 3.0, &mut builder);
   builder
}

#[cfg(feature = "comparison-geometry")]
#[test]
fn semantic_geometry_uses_the_same_composition_path_and_observes_layout_drift()
{
   let (_, scenario) = load_apple_pr_scenarios(&workspace_root()).expect("Apple PR scenarios").into_iter().find(|(_, scenario)| scenario.id == "dashboard.mixed-static").expect("dashboard scenario");
   let mut router = prepare(&scenario);
   install_comparison_latin_font(&mut router);
   let baseline = router.capture_comparison_geometry(VIEWPORT, 3.0).expect("baseline geometry");
   let drifted_viewport = gfx::RectF::new(0.0, 0.0, 420.0, 844.0);
   let drifted = router.capture_comparison_geometry(drifted_viewport, 3.0).expect("drifted geometry");
   let bounds = |nodes: &[oxide_test_scenes::ComparisonGeometryNode], identifier: &str| nodes.iter().find(|node| node.identifier == identifier).map(|node| node.bounds).expect("stable semantic node");
   assert_ne!(bounds(&baseline, "dashboard"), bounds(&drifted, "dashboard"));
   assert_ne!(bounds(&baseline, "dashboard:card:00"), bounds(&drifted, "dashboard:card:00"));
   assert_eq!(baseline.iter().find(|node| node.identifier == "dashboard:label:000").expect("dashboard label").text_line_bounds.len(), 1);
   assert!(baseline.iter().all(|node| !node.role.is_empty() && !node.identifier.is_empty()));
}

#[cfg(feature = "comparison-geometry")]
#[test]
fn rust_semantic_geometry_exposes_every_xcui_resolved_command_target()
{
   let scenarios = load_apple_pr_scenarios(&workspace_root()).expect("Apple PR scenarios");
   let scenario = |id: &str| scenarios.iter().find(|(_, scenario)| scenario.id == id).map(|(_, scenario)| scenario).expect("comparison scenario");
   let identifiers = |router: &mut Router<NullUploader>| router.capture_comparison_geometry(VIEWPORT, 3.0).expect("semantic geometry").into_iter().map(|node| node.identifier).collect::<Vec<_>>();

   let mut chat = prepare(scenario("chat.live-update"));
   let chat = identifiers(&mut chat);
   assert!(chat.iter().any(|identifier| identifier == "chat.heading"));
   assert!(chat.iter().any(|identifier| identifier == "chat.composer"));

   let mut grid = prepare_release("grid.large-scroll");
   assert!(identifiers(&mut grid).iter().any(|identifier| identifier == "grid.collection"));

   let navigation_spec = scenario("navigation.modal");
   let mut navigation = prepare(navigation_spec);
   let events = load_trace(&spec_root(), navigation_spec, "canonical-cycles");
   let list = identifiers(&mut navigation);
   assert!(list.iter().any(|identifier| identifier == "navigation.heading"));
   assert!(list.iter().any(|identifier| identifier == "navigation:item:05"));
   navigation.apply_comparison_event(&events[0]).expect("navigate to detail");
   let detail = identifiers(&mut navigation);
   assert!(detail.iter().any(|identifier| identifier == "navigation.detail"));
   assert!(detail.iter().any(|identifier| identifier == "navigation.detail-heading"));
   assert!(detail.iter().any(|identifier| identifier == "navigation.detail-title"));
   assert!(detail.iter().any(|identifier| identifier == "navigation.detail-subtitle"));
   assert!(detail.iter().any(|identifier| identifier == "navigation.back"));
   navigation.apply_comparison_event(&events[1]).expect("open modal");
   let modal = identifiers(&mut navigation);
   assert!(modal.iter().any(|identifier| identifier == "navigation.dismiss"));
   assert!(modal.iter().any(|identifier| identifier == "navigation.modal-heading"));
   assert!(modal.iter().any(|identifier| identifier == "navigation.modal-body-first"));
   assert!(modal.iter().any(|identifier| identifier == "navigation.modal-body-second"));

   let mut image = prepare(scenario("image.decode-zoom"));
   assert!(identifiers(&mut image).iter().any(|identifier| identifier == "image.heading"));

   let feed_spec = scenario("feed.variable-scroll");
   let mut feed = prepare(feed_spec);
   for phase in &feed_spec.phases
   {
      if phase.id == "favorite-one" {break}
      if phase.trace.is_none() {continue}
      for event in load_trace(&spec_root(), feed_spec, &phase.id)
      {
         feed.apply_comparison_event(&event).expect("feed setup event");
      }
   }
   assert!(identifiers(&mut feed).iter().any(|identifier| identifier == "feed:item:0300:favorite"));
}

#[cfg(feature = "comparison-geometry")]
#[test]
fn grid_geometry_uses_the_native_continuous_scroll_origin()
{
   let root = spec_root();
   let mut router = prepare_release("grid.large-scroll");
   replay_release_trace(&root, "grid-scroll-to-75-percent.json", &mut router);
   let geometry = router.capture_comparison_geometry(VIEWPORT, 3.0).expect("scrolled grid geometry");
   let first = geometry.iter().find(|node| node.role == "grid-tile").expect("first visible grid tile");
   assert_eq!(first.identifier, "grid:tile:07488");
   assert_eq!(first.bounds, gfx::RectF::new(12.0, -41.0, 116.0, 144.0));

   replay_release_trace(&root, "grid-reverse-scroll.json", &mut router);
   let geometry = router.capture_comparison_geometry(VIEWPORT, 3.0).expect("reverse-scrolled grid geometry");
   let first = geometry.iter().find(|node| node.role == "grid-tile").expect("first reverse-visible grid tile");
   assert_eq!(first.identifier, "grid:tile:02496");
   assert_eq!(first.bounds, gfx::RectF::new(12.0, 21.0, 116.0, 144.0));
}

#[cfg(feature = "comparison-geometry")]
#[test]
fn resize_geometry_matches_the_native_portrait_and_landscape_grids()
{
   let root = spec_root();
   let mut router = prepare_release("resize.theme");
   let geometry = router.capture_comparison_geometry(VIEWPORT, 3.0).expect("portrait resize geometry");
   let label = geometry.iter().find(|node| node.identifier == "dashboard:label:000").expect("portrait dashboard label");
   assert_eq!(label.bounds, gfx::RectF::new(58.0, 142.0 / 3.0, 125.0, 11.0));

   let trace = serde_json::from_slice::<Vec<TraceEvent>>(&fs::read(root.join("traces/resize-theme-ten-changes.json")).expect("resize trace")).expect("resize events");
   for event in &trace[..15] {router.apply_comparison_event(event).expect("first five resize changes");}
   let landscape = gfx::RectF::new(0.0, 0.0, 844.0, 390.0);
   let geometry = router.capture_comparison_geometry(landscape, 3.0).expect("landscape resize geometry");
   let last = geometry.iter().find(|node| node.identifier == "dashboard:card:31").expect("last landscape card");
   assert_eq!(last.bounds, gfx::RectF::new(630.0, 316.0, 190.0, 38.0));
}

#[cfg(feature = "comparison-geometry")]
#[test]
fn macos_chat_keys_replace_the_exact_visible_selection_without_touching_the_composer()
{
   let (_, scenario) = load_apple_pr_scenarios(&workspace_root()).expect("Apple PR scenarios").into_iter().find(|(_, scenario)| scenario.id == "chat.live-update").expect("chat scenario");
   let mut router = prepare(&scenario);
   for phase in &scenario.phases
   {
      if phase.id == "select-replace" {break}
      if phase.trace.is_none() {continue}
      for event in load_trace(&spec_root(), &scenario, &phase.id)
      {
         router.apply_comparison_event(&event).expect("chat predecessor event");
      }
   }
   let before = router.comparison_host_chat_selection_text().expect("visible selection target").to_string();
   let geometry = router.capture_comparison_geometry(VIEWPORT, 3.0).expect("chat geometry");
   let target = geometry.iter().find(|node| node.identifier == "chat:append:16").expect("visible chat selection target").bounds;
   assert!(router.comparison_host_click(target.x + target.w * 0.5, target.y + target.h * 0.5));
   assert!(router.comparison_host_key(115, false, ""));
   for _ in 0..6
   {
      assert!(router.comparison_host_key(124, true, ""));
   }
   for character in ["O", "x", "i", "d", "e"]
   {
      assert!(router.comparison_host_key(0, false, character));
      assert!(router.capture_comparison_geometry(VIEWPORT, 3.0).expect("post-key geometry").iter().any(|node| node.identifier == "chat:append:16"));
   }
   let expected = format!("Oxide{}", &before[6..]);
   assert_eq!(router.comparison_host_chat_selection_text(), Some(expected.as_str()));
   let (state, _) = router.comparison_checkpoint_json("selection-replaced").expect("chat state");
   let model = &serde_json::from_slice::<serde_json::Value>(&state).expect("chat state JSON")["model"];
   assert_eq!(model["focused_message_id"], "chat:append:16");
   assert_eq!(model["selection_active"], false);
   assert_eq!(model["replacement_applied"], true);
   assert_eq!(model["composer_utf8_count"], 10_340);
}

#[test]
fn macos_image_zoom_drag_finishes_at_canonical_two_x_without_changing_control_geometry()
{
   let (_, scenario) = load_apple_pr_scenarios(&workspace_root()).expect("Apple PR scenarios").into_iter().find(|(_, scenario)| scenario.id == "image.decode-zoom").expect("image scenario");
   let mut router = prepare(&scenario);
   let before = router.capture_comparison_geometry(VIEWPORT, 3.0).expect("initial image geometry");
   let zoom = before.iter().find(|node| node.identifier == "image.zoom").expect("image zoom geometry").bounds;
   assert!(!router.comparison_host_pointer_down(zoom.x + zoom.w * 0.5, zoom.y + zoom.h * 0.5));
   let canonical_x = zoom.x + zoom.w;
   assert!(router.comparison_host_pointer_move(canonical_x, zoom.y + zoom.h * 0.5, canonical_x - (zoom.x + zoom.w * 0.5), 0.0));
   assert!(!router.comparison_host_pointer_up());
   let (state, _) = router.comparison_checkpoint_json("pinch-mid").expect("image state");
   let model = &serde_json::from_slice::<serde_json::Value>(&state).expect("image state JSON")["model"];
   assert_eq!(model["scale_millionths"], 2_000_000);
   let after = router.capture_comparison_geometry(VIEWPORT, 3.0).expect("zoomed image geometry");
   assert_eq!(after.iter().find(|node| node.identifier == "image.zoom").expect("zoomed image control").bounds, zoom);

   let mut noncanonical = prepare(&scenario);
   assert!(!noncanonical.comparison_host_pointer_down(zoom.x + zoom.w * 0.5, zoom.y + zoom.h * 0.5));
   assert!(noncanonical.comparison_host_pointer_move(zoom.x + zoom.w * 0.8, zoom.y + zoom.h * 0.5, zoom.w * 0.3, 0.0));
   assert!(!noncanonical.comparison_host_pointer_up());
   let (state, _) = noncanonical.comparison_checkpoint_json("pinch-mid").expect("noncanonical image state");
   assert_ne!(serde_json::from_slice::<serde_json::Value>(&state).expect("noncanonical image state JSON")["model"]["scale_millionths"], 2_000_000);
}

#[test]
fn macos_navigation_drag_reveals_canonical_midpoint_and_mouse_up_restores_list()
{
   let (_, scenario) = load_apple_pr_scenarios(&workspace_root()).expect("Apple PR scenarios").into_iter().find(|(_, scenario)| scenario.id == "navigation.modal").expect("navigation scenario");
   let mut router = prepare(&scenario);
   assert!(!router.comparison_host_pointer_down(370.5, 422.0));
   assert!(router.comparison_host_pointer_move(292.5, 422.0, -78.0, 0.0));
   assert!(router.comparison_host_pointer_move(195.0, 422.0, -97.5, 0.0));
   let (mid, _) = router.comparison_checkpoint_json("cancel-mid").expect("navigation midpoint");
   let mid = &serde_json::from_slice::<serde_json::Value>(&mid).expect("navigation midpoint JSON")["model"];
   assert_eq!(mid["route"], "detail");
   assert_eq!(mid["modal_visible"], true);
   assert!(router.comparison_host_pointer_up());
   let (restored, _) = router.comparison_checkpoint_json("cancel-restored").expect("navigation restored state");
   let restored = &serde_json::from_slice::<serde_json::Value>(&restored).expect("navigation restored JSON")["model"];
   assert_eq!(restored["route"], "list");
   assert_eq!(restored["modal_visible"], false);
}

#[test]
fn all_pr_comparison_scenes_match_initial_role_contracts_and_pinned_draw_resources()
{
   let root = workspace_root();
   let specs = spec_root();
   let scenarios = load_apple_pr_scenarios(&root).expect("Apple PR scenarios");
   validate_apple_pr_scenario_set(&specs, &scenarios).expect("valid Apple PR scenarios");

   for (_, scenario) in scenarios
   {
      let mut router = prepare(&scenario);
      assert_eq!(router.comparison_role_counts().expect("role counts"), scenario.fairness_contract.expected_visible_role_counts, "{} role contract", scenario.id);
      let builder = draw(&mut router);
      let images: Vec<_> = builder.drawlist().items.iter().filter_map(|item| match item
      {
         gfx::DrawCmd::Image {tex, src, ..} => Some((*tex, *src)),
         _ => None,
      }).collect();

      let thumbnail_meshes = builder.drawlist().items.iter().filter(|item| matches!(item, gfx::DrawCmd::ImageMesh {tex, ..} if *tex == gfx::ImageHandle(7))).count();
      let thumbnails = images.iter().filter(|(handle, _)| *handle == gfx::ImageHandle(7)).count() + thumbnail_meshes;
      let inline = images.iter().filter(|(handle, _)| [gfx::ImageHandle(17), gfx::ImageHandle(18), gfx::ImageHandle(19)].contains(handle)).collect::<Vec<_>>();
      assert_inline_draws_are_clipped(&builder, &[gfx::ImageHandle(17), gfx::ImageHandle(18), gfx::ImageHandle(19)]);
      assert_inline_draws_are_pixel_aligned(&builder, &[gfx::ImageHandle(17), gfx::ImageHandle(18), gfx::ImageHandle(19)], 3.0);
      match scenario.id.as_str()
      {
         "startup.first-screen" =>
         {
            assert_eq!(thumbnails, 6);
            assert_eq!(thumbnail_meshes, 6);
         }
         "dashboard.mixed-static" =>
         {
            assert_eq!(thumbnails, 64);
            assert_eq!(builder.drawlist().items.iter().filter(|item| matches!(item, gfx::DrawCmd::Backdrop {..})).count(), 4);
            let first_backdrop = builder.drawlist().items.iter().position(|item| matches!(item, gfx::DrawCmd::Backdrop {..})).expect("dashboard backdrop");
            let first_image = builder.drawlist().items.iter().position(|item| matches!(item, gfx::DrawCmd::Image {..})).expect("dashboard image");
            assert!(first_backdrop < first_image, "dashboard material must remain behind interactive card content");
            assert!(builder.drawlist().items.iter().any(|item| matches!(
               item,
               gfx::DrawCmd::Solid {vb, ib, color}
                  if vb.len == 96
                     && ib.len == 144
                     && *color == gfx::Color::rgba(0.046665086, 0.155926464, 0.863157213, 1.0)
            )));
            assert!(!builder.drawlist().items.iter().any(|item| matches!(
               item,
               gfx::DrawCmd::RRect {radii, color, ..}
                  if *radii == [0.0; 4]
                     && *color == gfx::Color::rgba(0.046665086, 0.155926464, 0.863157213, 1.0)
            )));
         }
         "feed.variable-scroll" =>
         {
            assert_eq!(thumbnails, 9);
            assert_eq!(thumbnail_meshes, 9);
            assert_eq!(inline.len(), 4);
            assert!(builder.drawlist().items.iter().filter_map(|item| match item
            {
               gfx::DrawCmd::Image {tex, dst, ..} if [gfx::ImageHandle(17), gfx::ImageHandle(18), gfx::ImageHandle(19)].contains(tex) => Some(dst),
               _ => None,
            }).all(|dst| dst.y >= 738.0 && dst.y + dst.h <= 762.0), "feed inline images must remain inside the title line");
            assert!(builder.drawlist().items.iter().any(|item| matches!(
               item,
               gfx::DrawCmd::ClipPush {rect} if *rect == gfx::RectI::new(0, 52, 390, 792)
            )));
         }
         "chat.live-update" =>
         {
            assert_eq!(thumbnails, 10);
            assert_eq!(thumbnail_meshes, 10);
            assert!(!inline.is_empty());
            assert_eq!(builder.drawlist().items.iter().filter(|item| matches!(item, gfx::DrawCmd::RRect {color, ..} if *color == gfx::Color::rgba(0.014443844, 0.017641954, 0.025186860, 0.160784314))).count(), 10);
         }
         "navigation.modal" => assert!(images.is_empty()),
         "image.decode-zoom" =>
         {
            assert_eq!(images, [(gfx::ImageHandle(12), gfx::RectF::new(0.0, 0.0, 384.0, 288.0))]);
            assert!(builder.drawlist().items.iter().any(|item| matches!(item, gfx::DrawCmd::RRect {rect, radii, color} if *rect == gfx::RectF::new(16.0, 811.0, 358.0, 6.0) && *radii == [0.0; 4] && *color == gfx::Color::rgba(0.799102738, 0.814846572, 0.822785754, 1.0))));
            continue;
         }
         id => panic!("unexpected comparison scenario {id}"),
      }
      assert!(images.iter().filter(|(handle, _)| *handle == gfx::ImageHandle(7)).all(|(_, source)| source.w == 24.0 && source.h == 24.0));
      assert!(inline.iter().all(|(handle, source)| *handle == gfx::ImageHandle(18) && source.w == 45.0 && source.h == 45.0));
   }
}

#[test]
fn idle_scene_reuses_the_full_dashboard_output_and_rejects_logical_work()
{
   let workspace = workspace_root();
   let (_, idle) = load_scenario(&workspace, "idle.steady.json").expect("idle scenario");
   let (_, dashboard) = load_scenario(&workspace, "dashboard.mixed-static.json").expect("dashboard scenario");
   let mut idle_router = prepare(&idle);
   let mut dashboard_router = prepare(&dashboard);
   assert_eq!(idle_router.comparison_role_counts().expect("idle roles"), idle.fairness_contract.expected_visible_role_counts);
   assert_eq!(idle_router.comparison_role_counts().expect("idle roles"), dashboard_router.comparison_role_counts().expect("dashboard roles"));
   assert_eq!(draw(&mut idle_router).drawlist(), draw(&mut dashboard_router).drawlist());
   let event = TraceEvent {
      at_us: 1,
      op: oxide_benchmark_spec::TraceOperation::Mutate,
      pointer: None,
      x_millionths: None,
      y_millionths: None,
      delta_x_millionths: None,
      delta_y_millionths: None,
      target: Some(String::from("dashboard:label:003")),
      value: Some(oxide_benchmark_spec::TraceValue::Integer(1)),
      state_id: Some(String::from("forbidden-idle-mutation")),
   };
   assert!(idle_router.apply_comparison_event(&event).is_err());
   assert!(!idle_router.wants_next_frame());
}

#[test]
fn endurance_scene_executes_exact_churn_and_recovers_dashboard_output()
{
   let workspace = workspace_root();
   let root = spec_root();
   let (_, scenario) = load_scenario(&workspace, "endurance.churn.json").expect("endurance scenario");
   validate_nightly_endurance_scenario(&root, &scenario).expect("valid endurance scenario");
   let (_, dashboard) = load_scenario(&workspace, "dashboard.mixed-static.json").expect("dashboard scenario");
   let mut router = prepare(&scenario);
   let mut dashboard_router = prepare(&dashboard);
   assert_eq!(draw(&mut router).drawlist(), draw(&mut dashboard_router).drawlist());

   let heavy = load_trace(&root, &scenario, "open-close-heavy-screen");
   router.apply_comparison_event(&heavy[0]).expect("close heavy screen");
   assert_eq!(router.comparison_role_counts().expect("closed roles")[1].count, 0);
   for event in &heavy[1..]
   {
      router.apply_comparison_event(event).expect("remaining heavy-screen event");
   }
   assert_checkpoint_matches(&root, &scenario, &router, "heavy-screen-recovered");

   for event in load_trace(&root, &scenario, "tab-switch-heavy")
   {
      router.apply_comparison_event(&event).expect("tab-switch event");
   }
   assert_checkpoint_matches(&root, &scenario, &router, "tab-restored");
   for event in load_trace(&root, &scenario, "idle-animation")
   {
      router.apply_comparison_event(&event).expect("animation event");
   }
   assert_checkpoint_matches(&root, &scenario, &router, "animation-settled");
   assert_checkpoint_matches(&root, &scenario, &router, "recovered");
   assert_eq!(draw(&mut router).drawlist(), draw(&mut dashboard_router).drawlist());
}

fn assert_checkpoint_matches(root: &Path, scenario: &ScenarioSpec, router: &Router<NullUploader>, checkpoint_id: &str)
{
   let checkpoint = scenario.parity_checkpoints.iter().find(|checkpoint| checkpoint.id == checkpoint_id).expect("endurance checkpoint");
   let (state, accessibility) = router.comparison_checkpoint_json(checkpoint_id).expect("endurance checkpoint JSON");
   let expected_state = fs::read(root.join(&checkpoint.state.path)).expect("expected endurance state");
   let expected_accessibility = fs::read(root.join(&checkpoint.accessibility.path)).expect("expected endurance accessibility");
   assert_eq!(serde_json::from_slice::<serde_json::Value>(&state).expect("state JSON"), serde_json::from_slice::<serde_json::Value>(&expected_state).expect("expected state JSON"));
   assert_eq!(serde_json::from_slice::<serde_json::Value>(&accessibility).expect("accessibility JSON"), serde_json::from_slice::<serde_json::Value>(&expected_accessibility).expect("expected accessibility JSON"));
}

#[test]
fn dashboard_and_feed_traces_preserve_bounded_scene_contracts()
{
   let workspace = workspace_root();
   let root = spec_root();
   let scenarios = load_pr_vertical_scenarios(&workspace).expect("PR vertical scenarios");

   for phase_id in ["leaf-updates", "update-10-percent"]
   {
      let scenario = &scenarios[0].1;
      let mut router = prepare(scenario);
      for event in load_trace(&root, scenario, phase_id)
      {
         router.apply_comparison_event(&event).expect("dashboard trace event");
      }
      assert_eq!(router.comparison_role_counts().expect("dashboard roles"), scenario.fairness_contract.expected_visible_role_counts);
   }

   let scenario = &scenarios[1].1;
   let mut router = prepare(scenario);
   for phase_id in ["forward-fling", "reverse-fling", "favorite-one", "prepend-20"]
   {
      for event in load_trace(&root, scenario, phase_id)
      {
         router.apply_comparison_event(&event).expect("feed trace event");
      }
      let expected = &scenario.parity_checkpoints.iter().find(|checkpoint| checkpoint.phase_id == phase_id).expect("feed phase checkpoint").expected_visible_role_counts;
      let expected_images = expected.iter().find(|count| count.role == "feed-card").expect("feed card role").count as usize;
      let builder = draw(&mut router);
      assert_eq!(builder.drawlist().items.iter().filter(|item| matches!(item, gfx::DrawCmd::ImageMesh {tex, ..} if *tex == gfx::ImageHandle(7))).count(), expected_images, "{phase_id} visible thumbnail work");
      assert!(builder.drawlist().items.iter().filter_map(|item| match item
      {
         gfx::DrawCmd::RRect {rect, radii, ..} if *radii != [0.0; 4] => Some(rect.y),
         gfx::DrawCmd::Image {dst, tex, ..} if *tex == gfx::ImageHandle(7) => Some(dst.y),
         _ => None,
      }).all(|y| (y * 3.0 - (y * 3.0).round()).abs() < 0.000_1), "{phase_id} feed visual origins use the canonical three-X pixel grid");
      assert_eq!(router.comparison_role_counts().expect("feed roles"), *expected, "{phase_id} role contract");
   }
}

#[test]
fn navigation_transition_is_elapsed_time_driven_and_restores_the_list()
{
   let workspace = workspace_root();
   let root = spec_root();
   let scenarios = load_pr_vertical_scenarios(&workspace).expect("PR vertical scenarios");
   let scenario = &scenarios[2].1;
   let mut router = prepare(scenario);
   let events = load_trace(&root, scenario, "canonical-cycles");
   let _ = draw(&mut router);

   for event in events
   {
      router.apply_comparison_event(&event).expect("navigation trace event");
      router.update(event.at_us / 1_000, 325);
   }

   assert!(!router.wants_next_frame());
   assert_eq!(router.comparison_role_counts().expect("navigation roles"), scenario.fairness_contract.expected_visible_role_counts);
}

#[test]
fn navigation_modal_transition_origin_stays_on_the_canonical_three_x_pixel_grid()
{
   let workspace = workspace_root();
   let root = spec_root();
   let scenarios = load_pr_vertical_scenarios(&workspace).expect("PR vertical scenarios");
   let scenario = &scenarios[2].1;
   let mut router = prepare(scenario);
   let events = load_trace(&root, scenario, "canonical-cycles");
   router.apply_comparison_event(&events[0]).expect("detail navigation event");
   router.apply_comparison_event(&events[1]).expect("modal navigation event");

   for (elapsed_ms, expected_x) in [(75, 353.0), (75, 219.0), (75, 85.0), (75, 24.0)]
   {
      router.update(u64::from(elapsed_ms), elapsed_ms);
      let builder = draw(&mut router);
      let modal = builder.drawlist().items.iter().find_map(|item| match item
      {
         gfx::DrawCmd::RRect {rect, radii, ..} if rect.y == 132.0 && rect.w == 342.0 && rect.h == 580.0 && *radii == [16.0; 4] => Some(*rect),
         _ => None,
      }).expect("modal surface");
      assert_close(modal.x, expected_x);
      assert!((modal.x * 3.0 - (modal.x * 3.0).round()).abs() < 0.000_1);
   }
}

#[test]
fn navigation_done_label_has_frozen_centered_glyph_bounds()
{
   let workspace = workspace_root();
   let root = spec_root();
   let scenarios = load_pr_vertical_scenarios(&workspace).expect("PR vertical scenarios");
   let scenario = &scenarios[2].1;
   let mut router = prepare(scenario);
   install_comparison_latin_font(&mut router);
   let events = load_trace(&root, scenario, "canonical-cycles");
   for event in events.iter().take(2)
   {
      router.apply_comparison_event(event).expect("navigation trace event");
      router.update(event.at_us / 1_000, 325);
   }
   let builder = draw(&mut router);
   let run = builder.drawlist().items.iter().filter_map(|item| match item
   {
      gfx::DrawCmd::GlyphRun {run} => Some(run),
      _ => None,
   }).last().expect("Done glyph run");
   let vertices = &builder.drawlist().vertices[run.vb.offset as usize..(run.vb.offset + run.vb.len) as usize];
   let minimum_x = vertices.iter().map(|vertex| vertex.x).fold(f32::INFINITY, f32::min);
   let maximum_x = vertices.iter().map(|vertex| vertex.x).fold(f32::NEG_INFINITY, f32::max);
   assert!((minimum_x - 297.333_34).abs() < 0.000_1, "Done minimum x changed: {minimum_x}");
   assert!((maximum_x - 335.848_97).abs() < 0.000_1, "Done maximum x changed: {maximum_x}");
}

#[test]
fn startup_continue_label_has_frozen_centered_glyph_bounds()
{
   let workspace = workspace_root();
   let scenarios = load_apple_pr_scenarios(&workspace).expect("Apple PR scenarios");
   let scenario = &scenarios[0].1;
   let mut router = prepare(scenario);
   install_comparison_latin_font(&mut router);
   let builder = draw(&mut router);
   let run = builder.drawlist().items.iter().filter_map(|item| match item
   {
      gfx::DrawCmd::GlyphRun {run} => Some(run),
      _ => None,
   }).last().expect("Continue glyph run");
   let vertices = &builder.drawlist().vertices[run.vb.offset as usize..(run.vb.offset + run.vb.len) as usize];
   let minimum_x = vertices.iter().map(|vertex| vertex.x).fold(f32::INFINITY, f32::min);
   let maximum_x = vertices.iter().map(|vertex| vertex.x).fold(f32::NEG_INFINITY, f32::max);
   assert!((minimum_x - 160.333_33).abs() < 0.000_1, "Continue minimum x changed: {minimum_x}");
   assert!((maximum_x - 228.302_08).abs() < 0.000_1, "Continue maximum x changed: {maximum_x}");
}

#[test]
fn startup_and_chat_traces_preserve_exact_bounded_roles()
{
   let workspace = workspace_root();
   let root = spec_root();
   let scenarios = load_apple_pr_scenarios(&workspace).expect("Apple PR scenarios");

   let startup = &scenarios[0].1;
   let mut startup_router = prepare(startup);
   for phase_id in ["terminated-warm-cache", "fresh-install-first-launch", "warm-resume"]
   {
      for event in load_trace(&root, startup, phase_id)
      {
         startup_router.apply_comparison_event(&event).expect("startup trace event");
      }
      assert_eq!(startup_router.comparison_role_counts().expect("startup roles"), startup.fairness_contract.expected_visible_role_counts);
      assert_eq!(draw(&mut startup_router).drawlist().items.iter().filter(|item| matches!(item, gfx::DrawCmd::ImageMesh {tex, ..} if *tex == gfx::ImageHandle(7))).count(), 6);
   }

   let chat = &scenarios[3].1;
   let mut chat_router = prepare(chat);
   for phase_id in ["prepend-50", "append-10hz", "type-100", "paste-10kib", "select-replace"]
   {
      for event in load_trace(&root, chat, phase_id)
      {
         chat_router.apply_comparison_event(&event).expect("chat trace event");
      }
      assert_eq!(chat_router.comparison_role_counts().expect("chat roles"), chat.fairness_contract.expected_visible_role_counts, "{phase_id} role contract");
      assert_eq!(draw(&mut chat_router).drawlist().items.iter().filter(|item| matches!(item, gfx::DrawCmd::ImageMesh {tex, ..} if *tex == gfx::ImageHandle(7))).count(), 10, "{phase_id} bounded avatar work");
   }

   let reset = prepare(chat);
   assert_eq!(reset.comparison_role_counts().expect("reset chat roles"), chat.fairness_contract.expected_visible_role_counts);
}

#[test]
fn image_trace_uses_exact_uploaded_assets_and_elapsed_touch_geometry()
{
   let workspace = workspace_root();
   let root = spec_root();
   let scenarios = load_apple_pr_scenarios(&workspace).expect("Apple PR scenarios");
   let scenario = &scenarios[5].1;
   let fixture_bytes = fs::read(root.join(&scenario.fixture.path)).expect("image fixture bytes");
   let fixture = serde_json::from_slice::<ImageDecodeZoomFixture>(&fixture_bytes).expect("image fixture");
   assert_eq!(png_dimensions(&fs::read(root.join(&fixture.source.artifact.path)).expect("source PNG")), (4_096, 3_072));
   assert_eq!(png_dimensions(&fs::read(root.join(&fixture.thumbnail.artifact.path)).expect("thumbnail PNG")), (384, 288));

   let mut rejected = Router::new(NullUploader);
   rejected.prepare_comparison_scenario(scenario, &fixture_bytes).expect("image comparison scene");
   assert!(rejected.set_comparison_image_resources(gfx::ImageHandle(11), &"0".repeat(64), gfx::ImageHandle(12), &fixture.thumbnail.artifact.sha256).is_err());

   let mut staged = Router::new(NullUploader);
   staged.prepare_comparison_scenario(scenario, &fixture_bytes).expect("staged image comparison scene");
   staged.set_comparison_resources(gfx::ImageHandle(7), [0, 1, 2]).expect("staged shared resources");
   staged.set_comparison_image_thumbnail_resource(gfx::ImageHandle(12), &fixture.thumbnail.artifact.sha256).expect("staged thumbnail resource");
   assert_eq!(image_draws(&draw(&mut staged))[0].0, gfx::ImageHandle(12));
   staged.set_comparison_image_source_resource(gfx::ImageHandle(11), &fixture.source.artifact.sha256).expect("staged source resource");

   let mut router = prepare(scenario);
   assert_eq!(image_draws(&draw(&mut router)), [(gfx::ImageHandle(12), gfx::RectF::new(0.0, 0.0, 384.0, 288.0), gfx::RectF::new(16.0, 68.0, 128.0, 96.0))]);
   for phase_id in ["bytes-ready", "decode", "upload", "first-visible"]
   {
      for event in load_trace(&root, scenario, phase_id)
      {
         router.apply_comparison_event(&event).expect("image resource trace event");
      }
   }
   let visible_builder = draw(&mut router);
   let visible = image_draws(&visible_builder);
   assert_eq!(visible.len(), 1);
   assert_eq!(visible[0].0, gfx::ImageHandle(11));
   assert_eq!(visible[0].1, gfx::RectF::new(0.0, 0.0, 4_096.0, 3_072.0));
   assert_close(visible[0].2.w * 3.0, 2_048.0);
   assert_close(visible[0].2.h * 3.0, 1_536.0);
   assert_close((visible[0].2.x * 3.0).rem_euclid(1.0), 0.25);
   assert_close((visible[0].2.y * 3.0).rem_euclid(1.0), 0.25);
   assert!(visible_builder.drawlist().items.iter().any(|item| matches!(
      item,
      gfx::DrawCmd::Solid {vb, ib, color}
         if vb.len == 8
            && ib.len == 12
            && *color == gfx::Color::rgba(0.896269353, 0.913098652, 0.938685728, 1.0)
   )));

   for event in load_trace(&root, scenario, "pan")
   {
      router.apply_comparison_event(&event).expect("image pan event");
   }
   let panned = image_draws(&draw(&mut router));
   assert!(panned[0].2.x < visible[0].2.x);
   assert_close((panned[0].2.x * 3.0).rem_euclid(1.0), 0.25);

   for event in load_trace(&root, scenario, "pinch")
   {
      router.apply_comparison_event(&event).expect("image pinch event");
   }
   let pinched = image_draws(&draw(&mut router));
   assert_eq!(pinched[0].2.w, visible[0].2.w * 2.0);
   assert_close(pinched[0].2.w * 3.0, 4_096.0);
   assert_close(pinched[0].2.h * 3.0, 3_072.0);
   assert_close((pinched[0].2.x * 3.0).rem_euclid(1.0), 0.0);
   assert_close((pinched[0].2.y * 3.0).rem_euclid(1.0), 0.0);
   assert_eq!(router.comparison_role_counts().expect("image roles"), scenario.fairness_contract.expected_visible_role_counts);

   let mut reset = prepare(scenario);
   assert_eq!(image_draws(&draw(&mut reset))[0].0, gfx::ImageHandle(12));
}

#[test]
fn replayed_scene_state_and_accessibility_match_every_frozen_checkpoint()
{
   let workspace = workspace_root();
   let root = spec_root();
   let scenarios = load_apple_pr_scenarios(&workspace).expect("Apple PR scenarios");

   for (_, scenario) in scenarios
   {
      let mut router = prepare(&scenario);
      for phase in &scenario.phases
      {
         let events = phase.trace.as_ref().map(|trace|
         {
            serde_json::from_slice::<Vec<TraceEvent>>(&fs::read(root.join(&trace.path)).expect("trace bytes")).expect("trace events")
         }).unwrap_or_default();
         let mut event_index = 0;
         let mut checkpoints = scenario.parity_checkpoints.iter().filter(|checkpoint| checkpoint.phase_id == phase.id).collect::<Vec<_>>();
         checkpoints.sort_by_key(|checkpoint| checkpoint.at_us.unwrap_or(0));
         for checkpoint in checkpoints
         {
            let checkpoint_at = checkpoint.at_us.unwrap_or(0);
            while event_index < events.len() && events[event_index].at_us <= checkpoint_at
            {
               router.apply_comparison_event(&events[event_index]).expect("comparison trace event");
               event_index += 1;
            }
            let (state, accessibility) = router.comparison_checkpoint_json(&checkpoint.id).expect("semantic checkpoint");
            let expected_state = fs::read(root.join(&checkpoint.state.path)).expect("expected state");
            let expected_accessibility = fs::read(root.join(&checkpoint.accessibility.path)).expect("expected accessibility");
            assert_eq!(serde_json::from_slice::<serde_json::Value>(&state).expect("state JSON"), serde_json::from_slice::<serde_json::Value>(&expected_state).expect("expected state JSON"), "{}:{} state", scenario.id, checkpoint.id);
            assert_eq!(serde_json::from_slice::<serde_json::Value>(&accessibility).expect("accessibility JSON"), serde_json::from_slice::<serde_json::Value>(&expected_accessibility).expect("expected accessibility JSON"), "{}:{} accessibility", scenario.id, checkpoint.id);
            assert_eq!(router.comparison_role_counts().expect("role counts"), checkpoint.expected_visible_role_counts, "{}:{} role counts", scenario.id, checkpoint.id);
         }
         while event_index < events.len()
         {
            router.apply_comparison_event(&events[event_index]).expect("remaining comparison trace event");
            event_index += 1;
         }
      }
   }
}

#[test]
fn semantic_checkpoint_evidence_exposes_missing_mutation_focus_and_visibility()
{
   let workspace = workspace_root();
   let root = spec_root();
   let scenarios = load_apple_pr_scenarios(&workspace).expect("Apple PR scenarios");

   let dashboard = &scenarios[1].1;
   let dashboard_router = prepare(dashboard);
   let (state, _) = dashboard_router.comparison_checkpoint_json("leaf-updated").expect("dashboard checkpoint");
   assert_ne!(serde_json::from_slice::<serde_json::Value>(&state).expect("dashboard state"), serde_json::from_slice::<serde_json::Value>(&fs::read(root.join(&dashboard.parity_checkpoints[2].state.path)).expect("expected dashboard state")).expect("expected dashboard JSON"));

   let chat = &scenarios[3].1;
   let mut chat_router = prepare(chat);
   let focus = load_trace(&root, chat, "select-replace").remove(0);
   chat_router.apply_comparison_event(&focus).expect("chat focus event");
   let (state, accessibility) = chat_router.comparison_checkpoint_json("selection-replaced").expect("chat checkpoint");
   assert_eq!(serde_json::from_slice::<serde_json::Value>(&state).expect("chat state")["model"]["selection_active"], true);
   assert_eq!(serde_json::from_slice::<serde_json::Value>(&accessibility).expect("chat accessibility")["nodes"][1]["focused"], true);

   let startup = &scenarios[0].1;
   let mut startup_router = prepare(startup);
   let background = load_trace(&root, startup, "warm-resume").remove(0);
   startup_router.apply_comparison_event(&background).expect("startup background event");
   let (state, _) = startup_router.comparison_checkpoint_json("warm-resume-ready").expect("startup checkpoint");
   assert_eq!(serde_json::from_slice::<serde_json::Value>(&state).expect("startup state")["model"]["scene_visible"], false);
}

fn image_draws(builder: &DrawListBuilder) -> Vec<(gfx::ImageHandle, gfx::RectF, gfx::RectF)>
{
   builder.drawlist().items.iter().filter_map(|item| match item
   {
      gfx::DrawCmd::Image {tex, dst, src, ..} => Some((*tex, *src, *dst)),
      _ => None,
   }).collect()
}

fn assert_close(actual: f32, expected: f32)
{
   assert!((actual - expected).abs() <= 0.000_5, "expected {expected}, got {actual}");
}

fn assert_inline_draws_are_clipped(builder: &DrawListBuilder, handles: &[gfx::ImageHandle])
{
   let mut clip_depth = 0usize;
   for item in &builder.drawlist().items
   {
      match item
      {
         gfx::DrawCmd::ClipPush {..} => clip_depth += 1,
         gfx::DrawCmd::ClipPop =>
         {
            assert!(clip_depth > 0, "unbalanced comparison clip pop");
            clip_depth -= 1;
         }
         gfx::DrawCmd::Image {tex, ..} if handles.contains(tex) => assert!(clip_depth > 0, "inline-text image escaped its label clip"),
         _ => {}
      }
   }
   assert_eq!(clip_depth, 0, "comparison draw left an open clip");
}

fn assert_inline_draws_are_pixel_aligned(builder: &DrawListBuilder, handles: &[gfx::ImageHandle], scale: f32)
{
   for item in &builder.drawlist().items
   {
      if let gfx::DrawCmd::Image {tex, dst, ..} = item
      {
         if handles.contains(tex)
         {
            assert!((dst.x * scale - (dst.x * scale).round()).abs() < 0.0001, "inline-text x origin was not pixel aligned: {}", dst.x);
            assert!((dst.y * scale - (dst.y * scale).round()).abs() < 0.0001, "inline-text y origin was not pixel aligned: {}", dst.y);
         }
      }
   }
}

fn png_dimensions(bytes: &[u8]) -> (u32, u32)
{
   assert!(bytes.len() >= 24 && &bytes[..8] == b"\x89PNG\r\n\x1a\n" && &bytes[12..16] == b"IHDR");
   (
      u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]),
      u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]),
   )
}
