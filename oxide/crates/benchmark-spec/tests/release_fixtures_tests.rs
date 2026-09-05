use oxide_benchmark_spec::{EffectsFixture, GridFixture, MutationFixture, ResizeFixture, TextFixture};
use serde::de::DeserializeOwned;
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn release_fixtures_decode_through_strict_public_types()
{
   let root = spec_root();
   let grid: GridFixture = load(&root, "grid.large-scroll");
   let effects: EffectsFixture = load(&root, "effects.layers");
   let mutation: MutationFixture = load(&root, "mutation.damage");
   let text: TextFixture = load(&root, "text.multilingual");
   let resize: ResizeFixture = load(&root, "resize.theme");

   assert_eq!((grid.tile_count, grid.thumbnail_count, grid.detail_tile_index), (10_000, 256, 7_500));
   assert_eq!((effects.card_count, effects.clip_count, effects.shadow_count, effects.backdrop_blur_count), (100, 100, 32, 8));
   assert_eq!(mutation.mutation_classes.iter().map(|class| class.changed_node_count).collect::<Vec<_>>(), [100, 1_000, 10_000]);
   assert_eq!(text.categories.iter().map(|category| category.count).sum::<u32>(), text.label_count);
   assert_eq!(resize.changes.len(), resize.change_count as usize);
}

#[test]
fn release_fixture_types_reject_unknown_work()
{
   let root = spec_root();
   let mut value = serde_json::from_slice::<serde_json::Value>(&fs::read(root.join("grid.large-scroll.json")).expect("read grid fixture")).expect("parse grid fixture");
   value.as_object_mut().expect("grid fixture object").insert(String::from("undeclared_work"), serde_json::json!(1));
   assert!(serde_json::from_value::<GridFixture>(value).is_err());
}

fn spec_root() -> PathBuf
{
   Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join("benchmarks/comparative/specs/v1/fixtures")
}

fn load<T: DeserializeOwned>(root: &Path, id: &str) -> T
{
   serde_json::from_slice(&fs::read(root.join(format!("{id}.json"))).expect("read release fixture")).expect("parse release fixture")
}
