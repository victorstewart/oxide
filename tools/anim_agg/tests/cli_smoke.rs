use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn aggregates_sample_anim_log() -> Result<(), Box<dyn std::error::Error>>
{
   let dir = tempdir()?;
   let input = dir.path().join("anim_log.txt");
   let json = dir.path().join("summary.json");
   let log = "summary suite=anim component=Button variant=primary state=default time_ms=0 pixdiff=0 max_err=0 mse=0.0\nsummary suite=anim component=Button variant=primary state=pressed time_ms=33 pixdiff=4 max_err=2 mse=1.0\n";
   fs::write(&input, log)?;

   let binary = assert_cmd::cargo::cargo_bin!("anim_agg");
   Command::new(binary)
      .args([
         "--input",
         input.to_str().unwrap(),
         "--json",
         json.to_str().unwrap()
      ])
      .assert()
      .success()
      .stdout(predicate::str::contains("frames=2 failures=1"));

   let summary: serde_json::Value = serde_json::from_slice(&fs::read(&json)?)?;
   assert_eq!(summary["frames"], 2);
   assert_eq!(summary["failures"], 1);
   assert_eq!(summary["by_component"]["Button"], 2);
   Ok(())
}
