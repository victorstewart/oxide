use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn aggregates_sweep_log() -> Result<(), Box<dyn std::error::Error>>
{
   let dir = tempdir()?;
   let input = dir.path().join("sweep_log.txt");
   let json = dir.path().join("summary.json");
   let log = "## RUN use=0.10 prefilter=0.20\nfps=58.0\nenc_ms=1.6\nframe_ms=1.7\n## END use=0.10 prefilter=0.20\n## RUN use=0.12 prefilter=0.30\nfps=59.5\np95_ms=1.4\nenc_ms=1.5\n## END use=0.12 prefilter=0.30\n";
   fs::write(&input, log)?;

   let binary = assert_cmd::cargo::cargo_bin!("sweep_agg");
   Command::new(binary)
      .args([
         "--input",
         input.to_str().unwrap(),
         "--json",
         json.to_str().unwrap()
      ])
      .assert()
      .success()
      .stdout(predicate::str::contains("best_use="));

   let summary: serde_json::Value = serde_json::from_slice(&fs::read(&json)?)?;
   assert_eq!(summary["best_use"], 0.12);
   assert_eq!(summary["best_prefilter"], 0.30);
   assert_eq!(summary["basis"], "max_avg_fps");
   Ok(())
}
