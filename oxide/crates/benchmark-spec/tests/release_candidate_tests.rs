use std::fs;
use std::path::{Path, PathBuf};

use oxide_benchmark_spec::{load_release_candidate_for_capture, RELEASE_CANDIDATE_IDS};
use serde_json::{json, Value};

#[test]
fn capture_loader_accepts_all_five_frozen_release_candidates()
{
   let root = spec_root();
   for id in RELEASE_CANDIDATE_IDS
   {
      let candidate = load_release_candidate_for_capture(&root, id).expect("validated release candidate");
      assert_eq!(candidate.id, id);
      assert_eq!(candidate.candidate_sha256.len(), 64);
      assert!(!candidate.checkpoint_ids.is_empty());
      assert!(!candidate.phases.is_empty());
      assert!(!candidate.viewport_classes.is_empty());
      assert!(!root.join("scenarios").join(format!("{id}.json")).exists());
   }
}

#[test]
fn capture_loader_rejects_unknown_identity_and_every_capture_admission_field()
{
   let temporary = tempfile::tempdir().expect("temporary spec root");
   copy_tree(&spec_root(), temporary.path());
   let id = "grid.large-scroll";
   assert!(load_release_candidate_for_capture(temporary.path(), "unknown.candidate").expect_err("unknown id must fail").to_string().contains("unsupported release candidate"));
   let path = temporary.path().join(format!("release-candidates/{id}.candidate.json"));
   let original = fs::read(&path).expect("candidate bytes");
   for (field, replacement, expected) in [
      ("id", json!("effects.layers"), "identity is invalid"),
      ("candidate_status", json!("ready"), "invalid status"),
      ("owning_pass", json!("attribution"), "invalid owning pass"),
   ]
   {
      rewrite(&path, &original, |candidate| candidate[field] = replacement.clone());
      assert!(load_release_candidate_for_capture(temporary.path(), id).expect_err("admission field must fail").to_string().contains(expected));
   }
   rewrite(&path, &original, |candidate| candidate["screenshot_materialization"]["scenario_manifest_path"] = json!("scenarios/other.json"));
   assert!(load_release_candidate_for_capture(temporary.path(), id).expect_err("destination must fail").to_string().contains("noncanonical manifest destination"));
   fs::write(&path, [original.as_slice(), b"\n"].concat()).expect("noncanonical candidate");
   assert!(load_release_candidate_for_capture(temporary.path(), id).expect_err("noncanonical JSON must fail").to_string().contains("not canonical compact JSON"));
}

#[test]
fn capture_loader_rejects_declared_and_observed_artifact_hash_drift()
{
   let temporary = tempfile::tempdir().expect("temporary spec root");
   copy_tree(&spec_root(), temporary.path());
   let id = "grid.large-scroll";
   let candidate_path = temporary.path().join(format!("release-candidates/{id}.candidate.json"));
   let original = fs::read(&candidate_path).expect("candidate bytes");
   rewrite(&candidate_path, &original, |candidate| candidate["fixture"]["sha256"] = json!("0".repeat(64)));
   assert!(load_release_candidate_for_capture(temporary.path(), id).expect_err("declared hash drift must fail").to_string().contains("SHA-256 mismatch"));
   fs::write(&candidate_path, &original).expect("restore candidate");
   fs::write(temporary.path().join("fixtures/grid.large-scroll.json"), b"{}\n").expect("tamper fixture");
   assert!(load_release_candidate_for_capture(temporary.path(), id).expect_err("artifact bytes drift must fail").to_string().contains("SHA-256 mismatch"));
}

fn spec_root() -> PathBuf
{
   Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join("benchmarks/comparative/specs/v1")
}

fn rewrite(path: &Path, original: &[u8], mutation: impl FnOnce(&mut Value))
{
   let mut value = serde_json::from_slice::<Value>(original).expect("candidate JSON");
   mutation(&mut value);
   let mut bytes = serde_json::to_vec(&value).expect("canonical candidate JSON");
   bytes.push(b'\n');
   fs::write(path, bytes).expect("mutated candidate");
}

fn copy_tree(source: &Path, destination: &Path)
{
   for entry in fs::read_dir(source).expect("source directory")
   {
      let entry = entry.expect("source entry");
      let destination = destination.join(entry.file_name());
      if entry.file_type().expect("source type").is_dir()
      {
         fs::create_dir(&destination).expect("destination directory");
         copy_tree(&entry.path(), &destination);
      }
      else
      {
         fs::copy(entry.path(), destination).expect("copy fixture");
      }
   }
}
