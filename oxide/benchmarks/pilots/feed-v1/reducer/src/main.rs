use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use oxide_feed_v1_reducer::{build_evidence_manifest, reduce, verify_attachment_export, verify_smoke, ReducePaths};

fn main() -> ExitCode
{
   match run()
   {
      Ok(()) => ExitCode::SUCCESS,
      Err(error) =>
      {
         eprintln!("feed-v1 reducer: {error}");
         ExitCode::FAILURE
      }
   }
}

fn run() -> Result<(), String>
{
   let mut args = env::args().skip(1);
   match args.next().as_deref()
   {
      Some("reduce") =>
      {
         let run_root = required_path(&mut args, "run root")?;
         let output_json = required_path(&mut args, "output JSON")?;
         let output_markdown = required_path(&mut args, "output Markdown")?;
         reject_extra(args)?;
         reduce(&ReducePaths { run_root, output_json, output_markdown })
      }
      Some("manifest") =>
      {
         let source_root = required_path(&mut args, "source root")?;
         let repository_root = required_path(&mut args, "repository root")?;
         let uikit_app = required_path(&mut args, "UIKit app")?;
         let oxide_app = required_path(&mut args, "Oxide app")?;
         let controller_runner = required_path(&mut args, "controller runner app")?;
         let controller_xctest = required_path(&mut args, "controller xctest")?;
         let output = required_path(&mut args, "manifest output")?;
         reject_extra(args)?;
         build_evidence_manifest(
            &source_root,
            &repository_root,
            &uikit_app,
            &oxide_app,
            &controller_runner,
            &controller_xctest,
            &output,
         )
      }
      Some("verify-attachments") =>
      {
         let root = required_path(&mut args, "attachment root")?;
         reject_extra(args)?;
         verify_attachment_export(&root)
      }
      Some("verify-smoke") =>
      {
         let root = required_path(&mut args, "smoke result root")?;
         reject_extra(args)?;
         verify_smoke(&root)
      }
      _ => Err("usage: oxide-feed-v1-reducer reduce <run-root> <latest.json> <latest.md> | manifest <source-root> <repository-root> <UIKit.app> <Oxide.app> <FeedV1Controller-Runner.app> <FeedV1Controller.xctest> <output.json> | verify-attachments <root> | verify-smoke <run-root>".to_string()),
   }
}

fn required_path(args: &mut impl Iterator<Item = String>, name: &str) -> Result<PathBuf, String>
{
   args.next().map(PathBuf::from).ok_or_else(|| format!("missing {name}"))
}

fn reject_extra(mut args: impl Iterator<Item = String>) -> Result<(), String>
{
   if let Some(extra) = args.next()
   {
      return Err(format!("unexpected argument {extra}"));
   }
   Ok(())
}
