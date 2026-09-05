use xtask::run_cli;

#[test]
fn retained_commands_are_dispatched()
{
   assert!(run_cli(&[String::from("experiments"), String::from("check"), String::from("--help")]).is_ok());
   assert!(run_cli(&[String::from("ios"), String::from("time-profiler-summary")]).is_err());
}
