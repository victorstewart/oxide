use oxide_apple_comparison_controller::{reduce_macos_system_trace, MACOS_SYSTEM_TRACE_AVAILABILITY};

const SIGNPOSTS: &str = include_str!("fixtures/system_trace_signposts.xml");
const THREAD_INFO: &str = include_str!("fixtures/system_trace_thread_info.xml");
const THREAD_STATE: &str = include_str!("fixtures/system_trace_thread_state.xml");
const CONTEXT_SWITCH: &str = include_str!("fixtures/system_trace_context_switch.xml");

#[test]
fn reduces_exact_pid_main_thread_scheduler_evidence_inside_measured_phase()
{
   let artifact = reduce_macos_system_trace(SIGNPOSTS, THREAD_INFO, THREAD_STATE, CONTEXT_SWITCH, 42).expect("System Trace reduction");
   assert_eq!(artifact.schema_version, 1);
   assert_eq!(artifact.availability, MACOS_SYSTEM_TRACE_AVAILABILITY);
   assert_eq!(artifact.pid, 42);
   assert_eq!(artifact.main_thread_tid, 900);
   assert!(artifact.exact_pid_filtered);
   assert!(artifact.exact_main_thread_filtered);
   assert_eq!(artifact.phases.len(), 1);
   let phase = &artifact.phases[0];
   assert_eq!(phase.scenario_index, 2);
   assert_eq!(phase.phase_identifier, 7);
   assert_eq!(phase.begin_ns, 100);
   assert_eq!(phase.end_ns, 300);
   assert_eq!(phase.main_thread_running_ns, 110);
   assert_eq!(phase.runnable_wait_ns, 50);
   assert_eq!(phase.context_switches, 3);
   assert_eq!(phase.wakeups, 2);
}

#[test]
fn rejects_missing_or_ambiguous_phase_boundaries_and_identity_mismatch()
{
   let missing_end = SIGNPOSTS.replace("PhaseEnd", "Unrelated");
   assert!(reduce_macos_system_trace(&missing_end, THREAD_INFO, THREAD_STATE, CONTEXT_SWITCH, 42).is_err());

   let duplicate_begin = SIGNPOSTS.replace("<event-time>300</event-time>", "<event-time>200</event-time>").replace("PhaseEnd", "PhaseBegin");
   assert!(reduce_macos_system_trace(&duplicate_begin, THREAD_INFO, THREAD_STATE, CONTEXT_SWITCH, 42).is_err());

   let wrong_process = SIGNPOSTS.replacen("<pid>42</pid>", "<pid>43</pid>", 1);
   assert!(reduce_macos_system_trace(&wrong_process, THREAD_INFO, THREAD_STATE, CONTEXT_SWITCH, 42).is_err());

   let wrong_thread = SIGNPOSTS.replacen("<tid>900</tid>", "<tid>901</tid>", 1);
   assert!(reduce_macos_system_trace(&wrong_thread, THREAD_INFO, THREAD_STATE, CONTEXT_SWITCH, 42).is_err());
}

#[test]
fn rejects_unsupported_schema_empty_evidence_and_incomplete_state_coverage()
{
   let unsupported = THREAD_STATE.replace("<mnemonic>summary</mnemonic>", "<mnemonic>changed</mnemonic>");
   assert!(reduce_macos_system_trace(SIGNPOSTS, THREAD_INFO, &unsupported, CONTEXT_SWITCH, 42).is_err());

   let empty_context = CONTEXT_SWITCH.replace("<row>", "<discarded>").replace("</row>", "</discarded>");
   assert!(reduce_macos_system_trace(SIGNPOSTS, THREAD_INFO, THREAD_STATE, &empty_context, 42).is_err());

   let gap = THREAD_STATE.replace("<start-time>220</start-time>", "<start-time>225</start-time>");
   assert!(reduce_macos_system_trace(SIGNPOSTS, THREAD_INFO, &gap, CONTEXT_SWITCH, 42).is_err());
}
