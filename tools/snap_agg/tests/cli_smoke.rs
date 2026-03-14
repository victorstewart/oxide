use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn aggregates_snapshot_log() -> Result<(), Box<dyn std::error::Error>>
{
   let dir = tempdir()?;
   let input = dir.path().join("snap_log.txt");
   let json = dir.path().join("rows.json");
   let log = "summary suite=static component=scene_controls variant=default state=default pixdiff=0 max_err=0 mse=0.0\nsummary suite=static component=scene_controls variant=default state=hover pixdiff=5 max_err=2 mse=0.1\n";
   fs::write(&input, log)?;

   let binary = assert_cmd::cargo::cargo_bin!("snap_agg");
   Command::new(binary)
      .args([
         "--input",
         input.to_str().unwrap(),
         "--json",
         json.to_str().unwrap()
      ])
      .assert()
      .success()
      .stdout(predicate::str::contains("failures=1 total=2"));

   let rows: Vec<serde_json::Value> = serde_json::from_slice(&fs::read(&json)?)?;
   assert_eq!(rows.len(), 2);
   assert_eq!(rows[0]["component"], "scene_controls");
   assert_eq!(rows[1]["state"], "hover");
   Ok(())
}
