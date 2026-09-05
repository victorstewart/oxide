use std::env;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use oxide_benchmark_spec::promote_release_candidates;

fn main() -> Result<()>
{
   let mut spec_root = None;
   let mut evidence_root = None;
   let mut output_root = None;
   let mut canonical_scale = None;
   let args = env::args().skip(1).collect::<Vec<_>>();
   let mut index = 0;
   while index < args.len()
   {
      match args[index].as_str()
      {
         "--spec-root" => spec_root = Some(PathBuf::from(value(&args, &mut index, "--spec-root")?)),
         "--evidence-root" => evidence_root = Some(PathBuf::from(value(&args, &mut index, "--evidence-root")?)),
         "--output-root" => output_root = Some(PathBuf::from(value(&args, &mut index, "--output-root")?)),
         "--canonical-scale" => canonical_scale = Some(value(&args, &mut index, "--canonical-scale")?.parse::<u32>().context("parsing --canonical-scale")?),
         other => bail!("unknown argument `{other}`"),
      }
      index += 1;
   }
   let report = promote_release_candidates(
      &spec_root.context("missing --spec-root")?,
      &evidence_root.context("missing --evidence-root")?,
      &output_root.context("missing --output-root")?,
      canonical_scale.context("missing --canonical-scale")?,
   )?;
   println!("promoted {} release scenarios across {} canonical checkpoints", report.scenarios.len(), report.scenarios.iter().map(|scenario| scenario.checkpoints.len()).sum::<usize>());
   Ok(())
}

fn value<'a>(args: &'a [String], index: &mut usize, flag: &str) -> Result<&'a str>
{
   *index += 1;
   args.get(*index).map(String::as_str).with_context(|| format!("{flag} requires a value"))
}
