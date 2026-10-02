use xtask::run_cli;

#[test]
fn retained_commands_are_dispatched()
{
   assert!(run_cli(&[String::from("experiments"), String::from("check"), String::from("--help")]).is_ok());
   let missing_manifest = run_cli(&[String::from("experiments"), String::from("check")]).unwrap_err();
   assert!(missing_manifest.to_string().contains("requires --manifest PATH"));
   assert!(run_cli(&[String::from("ios"), String::from("time-profiler-summary")]).is_err());
}
