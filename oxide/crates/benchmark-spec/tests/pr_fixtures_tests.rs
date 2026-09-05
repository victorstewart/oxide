use oxide_benchmark_spec::{load_apple_pr_scenarios, load_pr_vertical_scenarios, validate_apple_pr_scenario_set, validate_pr_vertical_slice, ChatFixture, DashboardFixture, EnduranceFixture, FeedFixture, ImageDecodeZoomFixture, NavigationFixture, StartupFixture};
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn workspace_root() -> PathBuf
{
   Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn committed_vertical_fixtures_deserialize_through_exported_types()
{
   let root = workspace_root().join("benchmarks/comparative/specs/v1/fixtures");
   let dashboard = read_json::<DashboardFixture>(&root.join("dashboard.mixed-static.json"));
   let feed = read_json::<FeedFixture>(&root.join("feed.variable-scroll.json"));
   let navigation = read_json::<NavigationFixture>(&root.join("navigation.modal.json"));

   assert_eq!(dashboard.visible_node_count, 300);
   assert_eq!(dashboard.categories.label, 176);
   assert_eq!(dashboard.leaf_update_sequence.len(), 20);
   assert_eq!(dashboard.update_10_percent_ids.len(), 30);
   assert_eq!(feed.row_count, 2_000);
   assert_eq!(feed.rows.len(), 2_000);
   assert_eq!(feed.thumbnail_count, 128);
   assert_eq!(feed.prepend_rows.len(), 20);
   assert_eq!(navigation.list_item_count, 12);
   assert_eq!(navigation.cycle_count, 4);
   assert_eq!(navigation.transition.duration_ms, 300);
   assert_eq!(navigation.interactive_cancel_fraction, 0.5);
}

#[test]
fn committed_remaining_pr_fixtures_deserialize_through_exported_types()
{
   let root = workspace_root().join("benchmarks/comparative/specs/v1/fixtures");
   let startup = read_json::<StartupFixture>(&root.join("startup.first-screen.json"));
   let chat = read_json::<ChatFixture>(&root.join("chat.live-update.json"));
   let image = read_json::<ImageDecodeZoomFixture>(&root.join("image.decode-zoom.json"));

   assert_eq!(startup.cards.len(), 24);
   assert_eq!(startup.cards.iter().filter(|card| card.initially_visible).count(), 6);
   assert_eq!(startup.data.as_bytes().len(), 24 * 1_024);
   assert_eq!(startup.initial_image_indices, [0, 1, 2, 3, 4, 5]);
   assert_eq!(chat.messages.len(), 5_000);
   assert_eq!(chat.avatar_count, 64);
   assert_eq!(chat.prepend_messages.len(), 50);
   assert_eq!(chat.typed_text.chars().count(), 100);
   assert_eq!(chat.pasted_text.as_bytes().len(), 10 * 1_024);
   assert_eq!((image.source.width, image.source.height), (4_096, 3_072));
   assert_eq!((image.thumbnail.width, image.thumbnail.height), (384, 288));
}

#[test]
fn committed_endurance_fixture_deserializes_through_its_exported_type()
{
   let path = workspace_root().join("benchmarks/comparative/specs/v1/fixtures/endurance.churn.json");
   let fixture = read_json::<EnduranceFixture>(&path);

   assert_eq!(fixture.id, "endurance.churn");
   assert_eq!(fixture.visible_node_count, 300);
   assert_eq!(fixture.heavy_screen_cycle_count, 100);
   assert_eq!(fixture.tab_switch_count, 500);
   assert_eq!(fixture.animation_frame_count, 600);
   assert_eq!(fixture.initial_tab_index, 0);
}

#[test]
fn typed_vertical_fixtures_reject_unknown_contract_fields()
{
   let path = workspace_root().join("benchmarks/comparative/specs/v1/fixtures/dashboard.mixed-static.json");
   let mut value = read_json::<serde_json::Value>(&path);
   value.as_object_mut().expect("dashboard fixture object").insert(String::from("undeclared_work"), serde_json::json!(1));
   assert!(serde_json::from_value::<DashboardFixture>(value).is_err());

   let path = workspace_root().join("benchmarks/comparative/specs/v1/fixtures/endurance.churn.json");
   let mut value = read_json::<serde_json::Value>(&path);
   value.as_object_mut().expect("endurance fixture object").insert(String::from("undeclared_work"), serde_json::json!(1));
   assert!(serde_json::from_value::<EnduranceFixture>(value).is_err());
}

#[test]
fn vertical_slice_validation_rejects_typed_fixture_drift_with_a_matching_hash()
{
   let source_workspace = workspace_root();
   let source_spec = source_workspace.join("benchmarks/comparative/specs/v1");
   let temporary = TestRoot::new();
   copy_tree(&source_spec, &temporary.0);

   let mut scenarios = load_pr_vertical_scenarios(&source_workspace).expect("load committed PR vertical scenarios");
   let fixture_path = temporary.0.join(&scenarios[1].1.fixture.path);
   let mut feed = read_json::<FeedFixture>(&fixture_path);
   feed.rows[731].favorite = true;
   let mut bytes = serde_json::to_vec_pretty(&feed).expect("serialize changed feed fixture");
   bytes.push(b'\n');
   fs::write(&fixture_path, &bytes).expect("write changed feed fixture");
   scenarios[1].1.fixture.sha256 = format!("{:x}", Sha256::digest(&bytes));

   let error = validate_pr_vertical_slice(&temporary.0, &scenarios).expect_err("typed feed drift must fail");
   assert!(error.to_string().contains("feed row 731 must begin unfavorited"));
}

#[test]
fn apple_pr_validation_rejects_chat_semantic_drift_with_a_matching_hash()
{
   let source_workspace = workspace_root();
   let source_spec = source_workspace.join("benchmarks/comparative/specs/v1");
   let temporary = TestRoot::new();
   copy_tree(&source_spec, &temporary.0);

   let mut scenarios = load_apple_pr_scenarios(&source_workspace).expect("load committed Apple PR scenarios");
   let fixture_path = temporary.0.join(&scenarios[3].1.fixture.path);
   let mut chat = read_json::<ChatFixture>(&fixture_path);
   chat.append_rate_hz = 9;
   let mut bytes = serde_json::to_vec_pretty(&chat).expect("serialize changed chat fixture");
   bytes.push(b'\n');
   fs::write(&fixture_path, &bytes).expect("write changed chat fixture");
   scenarios[3].1.fixture.sha256 = format!("{:x}", Sha256::digest(&bytes));

   let error = validate_apple_pr_scenario_set(&temporary.0, &scenarios).expect_err("typed chat drift must fail");
   assert!(error.to_string().contains("chat fixture append rate is 9Hz"));
}

fn read_json<T: DeserializeOwned>(path: &Path) -> T
{
   serde_json::from_slice(&fs::read(path).expect("read fixture JSON")).expect("parse fixture JSON")
}

fn copy_tree(source: &Path, destination: &Path)
{
   fs::create_dir_all(destination).expect("create copied spec directory");
   for entry in fs::read_dir(source).expect("read source spec directory")
   {
      let entry = entry.expect("read source spec entry");
      let source_path = entry.path();
      let destination_path = destination.join(entry.file_name());
      if entry.file_type().expect("read source spec entry type").is_dir()
      {
         copy_tree(&source_path, &destination_path);
      }
      else
      {
         fs::copy(&source_path, &destination_path).expect("copy spec artifact");
      }
   }
}

struct TestRoot(PathBuf);

impl TestRoot
{
   fn new() -> Self
   {
      let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("system time after epoch").as_nanos();
      let path = std::env::temp_dir().join(format!("oxide-pr-fixtures-{}-{nonce}", std::process::id()));
      fs::create_dir_all(&path).expect("create temporary spec root");
      Self(path)
   }
}

impl Drop for TestRoot
{
   fn drop(&mut self)
   {
      let _ = fs::remove_dir_all(&self.0);
   }
}
