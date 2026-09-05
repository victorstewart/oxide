use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;

use tempfile::tempdir;

use oxide_benchmark_spec::load_apple_pr_acquisition;
use xtask::apple_comparison::{apple_pr_dry_run, finalize_per_app_transport_pair, validate_benchmark_telemetry, ExpectedAppleTransportPair};

fn hash(bytes: &[u8]) -> String
{
   format!("{:x}", Sha256::digest(bytes))
}

fn expected_pair() -> ExpectedAppleTransportPair
{
   ExpectedAppleTransportPair {
      run_id: String::from("run"),
      plan_sha256: String::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
      pass_id: String::from("pass"),
      pair_index: 2,
      generation: String::from("current"),
   }
}

fn workspace_root() -> std::path::PathBuf
{
   std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace root").to_path_buf()
}

#[test]
fn finalizer_rejects_stale_generation_without_a_checkpoint()
{
   let root = tempdir().expect("tempdir");
   let expected = expected_pair();
   let oxide_path = root.path().join("oxide.json");
   let oxide_ack_path = root.path().join("oxide.ack.json");
   let native_path = root.path().join("native.json");
   let native_ack_path = root.path().join("native.ack.json");
   let output_path = root.path().join("pair.complete.json");
   let oxide = json!({
      "generation": "stale",
      "pairIndex": 2,
      "passID": "pass",
      "payloadSHA256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      "planSHA256": expected.plan_sha256,
      "runID": "run",
      "schemaVersion": 1,
      "side": "oxide"
   });
   fs::write(&oxide_path, serde_json::to_vec(&oxide).expect("oxide JSON")).expect("oxide");
   fs::write(&oxide_ack_path, b"{}").expect("oxide ack");
   fs::write(&native_path, b"{}").expect("native");
   fs::write(&native_ack_path, b"{}").expect("native ack");
   assert!(finalize_per_app_transport_pair(&expected, &oxide_path, &oxide_ack_path, &native_path, &native_ack_path, &output_path).is_err());
   assert!(!output_path.exists());
}

#[test]
fn finalizer_atomically_commits_a_valid_pulled_chain()
{
   let root = tempdir().expect("tempdir");
   let expected = expected_pair();
   let oxide_path = root.path().join("oxide.json");
   let oxide_ack_path = root.path().join("oxide.ack.json");
   let native_path = root.path().join("native.json");
   let native_ack_path = root.path().join("native.ack.json");
   let output_path = root.path().join("pair.complete.json");
   let oxide = serde_json::to_vec(&json!({
      "generation": "current",
      "pairIndex": 2,
      "passID": "pass",
      "payloadSHA256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      "planSHA256": expected.plan_sha256,
      "runID": "run",
      "schemaVersion": 1,
      "side": "oxide"
   })).expect("oxide JSON");
   let oxide_sha256 = hash(&oxide);
   let native = serde_json::to_vec(&json!({
      "generation": "current",
      "pairIndex": 2,
      "passID": "pass",
      "payloadSHA256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
      "planSHA256": expected.plan_sha256,
      "predecessorSHA256": oxide_sha256,
      "runID": "run",
      "schemaVersion": 1,
      "side": "native"
   })).expect("native JSON");
   let oxide_ack = serde_json::to_vec(&json!({
      "artifactSHA256": hash(&oxide),
      "durable": true,
      "generation": "current",
      "schemaVersion": 1
   })).expect("oxide ack JSON");
   let native_ack = serde_json::to_vec(&json!({
      "artifactSHA256": hash(&native),
      "durable": true,
      "generation": "current",
      "schemaVersion": 1
   })).expect("native ack JSON");
   fs::write(&oxide_path, oxide).expect("oxide");
   fs::write(&oxide_ack_path, oxide_ack).expect("oxide ack");
   fs::write(&native_path, native).expect("native");
   fs::write(&native_ack_path, native_ack).expect("native ack");

   let checkpoint = finalize_per_app_transport_pair(&expected, &oxide_path, &oxide_ack_path, &native_path, &native_ack_path, &output_path).expect("finalize");
   assert!(checkpoint.complete);
   assert!(output_path.is_file());
   assert!(fs::read_dir(root.path()).expect("read root").all(|entry| !entry.expect("entry").file_name().to_string_lossy().ends_with(".tmp")));
}

#[test]
fn apple_pr_dry_run_has_four_acquisitions_and_exact_session_coverage()
{
   let (_, spec) = load_apple_pr_acquisition(&workspace_root()).expect("Apple PR acquisition");
   let dry_run = apple_pr_dry_run(&spec).expect("Apple PR dry run");
   assert_eq!(dry_run.build_for_testing_count, 1);
   assert_eq!(dry_run.acquisitions.len(), 4);
   assert_eq!(dry_run.session_slots.iter().filter(|slot| slot.pass_id == "minimal-presentation").count(), 40);
   assert_eq!(dry_run.session_slots.iter().filter(|slot| slot.pass_id == "canonical-launch").count(), 8);
   assert!(!dry_run.acquisitions.iter().any(|chunk| chunk.pass_id == "lean"));
}

#[test]
fn apple_pr_dry_run_rejects_missing_pair_and_lean_replay()
{
   let (_, mut spec) = load_apple_pr_acquisition(&workspace_root()).expect("Apple PR acquisition");
   spec.controller_chunks[1].ordered_pair_indices = vec![0];
   assert!(apple_pr_dry_run(&spec).is_err());

   let (_, mut spec) = load_apple_pr_acquisition(&workspace_root()).expect("Apple PR acquisition");
   spec.controller_chunks[1].pass_id = String::from("lean");
   assert!(apple_pr_dry_run(&spec).is_err());
}

#[test]
fn binary_telemetry_validator_accepts_complete_identity_and_rejects_corruption()
{
   let plan = String::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
   let telemetry = telemetry_fixture(&plan, "generation", "chunk", "pass", "session", 10, 20);
   assert_eq!(validate_benchmark_telemetry(&telemetry, &plan, "generation", "chunk", "pass", "session").expect("valid telemetry"), 2);

   let mut corrupt = telemetry.clone();
   *corrupt.last_mut().expect("footer") ^= 1;
   assert!(validate_benchmark_telemetry(&corrupt, &plan, "generation", "chunk", "pass", "session").is_err());
   assert!(validate_benchmark_telemetry(&telemetry[..telemetry.len() - 1], &plan, "generation", "chunk", "pass", "session").is_err());

   let regressed = telemetry_fixture(&plan, "generation", "chunk", "pass", "session", 20, 10);
   assert!(validate_benchmark_telemetry(&regressed, &plan, "generation", "chunk", "pass", "session").is_err());
}

fn telemetry_fixture(plan: &str, generation: &str, chunk: &str, pass: &str, session: &str, begin_timestamp: u64, end_timestamp: u64) -> Vec<u8>
{
   let mut bytes = Vec::new();
   bytes.extend_from_slice(b"OXBTEL02");
   push_u32(&mut bytes, 2);
   push_u32(&mut bytes, 136);
   push_u32(&mut bytes, 44);
   push_u32(&mut bytes, 1);
   push_u64(&mut bytes, 2);
   push_u64(&mut bytes, 2);
   for index in 0..32
   {
      bytes.push(u8::from_str_radix(&plan[index * 2..index * 2 + 2], 16).expect("plan hash"));
   }
   bytes.extend_from_slice(&Sha256::digest(generation.as_bytes()));
   push_u64(&mut bytes, stable_id(chunk));
   push_u64(&mut bytes, stable_id(pass));
   push_u64(&mut bytes, stable_id(session));
   push_u32(&mut bytes, 1);
   push_u32(&mut bytes, 1);
   telemetry_record(&mut bytes, 0, begin_timestamp, 1, 77);
   telemetry_record(&mut bytes, 1, end_timestamp, 2, 77);
   let footer = Sha256::digest(&bytes);
   bytes.extend_from_slice(&footer);
   bytes
}

fn telemetry_record(bytes: &mut Vec<u8>, sequence: u64, timestamp: u64, kind: u16, identifier: u64)
{
   push_u64(bytes, sequence);
   push_u64(bytes, timestamp);
   bytes.extend_from_slice(&kind.to_le_bytes());
   bytes.extend_from_slice(&0_u16.to_le_bytes());
   push_u64(bytes, identifier);
   push_u64(bytes, 0);
   push_u64(bytes, 0);
}

fn push_u32(bytes: &mut Vec<u8>, value: u32)
{
   bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(bytes: &mut Vec<u8>, value: u64)
{
   bytes.extend_from_slice(&value.to_le_bytes());
}

fn stable_id(value: &str) -> u64
{
   value.as_bytes().iter().fold(0xcbf29ce484222325, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3))
}
