use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use super::{validate_run_id, validate_sha256, ComparisonSide};

pub const MACOS_LAUNCH_PRESENTATION_CALIBRATION_PENDING: &str = "pending-external-sensor-endpoint-and-trace-overhead-calibration";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MacOsLaunchClass
{
   TerminatedProcessWarmSystemCache,
   FreshInstallFirstLaunch,
   WarmResume,
}

impl MacOsLaunchClass
{
   pub fn as_str(self) -> &'static str
   {
      match self
      {
         Self::TerminatedProcessWarmSystemCache => "terminated-warm-system-cache",
         Self::FreshInstallFirstLaunch => "fresh-install-first-launch",
         Self::WarmResume => "warm-resume",
      }
   }

   fn checkpoint_id(self) -> &'static str
   {
      match self
      {
         Self::TerminatedProcessWarmSystemCache => "terminated-ready",
         Self::FreshInstallFirstLaunch => "fresh-install-ready",
         Self::WarmResume => "warm-resume-ready",
      }
   }

   fn cache_class(self) -> &'static str
   {
      match self
      {
         Self::TerminatedProcessWarmSystemCache => "warm-system-cache",
         Self::FreshInstallFirstLaunch => "unclassified-system-cache",
         Self::WarmResume => "resident-process",
      }
   }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacOsLaunchExpectation
{
   pub run_id: String,
   pub plan_sha256: String,
   pub chunk_id: String,
   pub pack_id: String,
   pub pair_index: u32,
   pub generation: String,
   pub executable_sha256: String,
   pub side: ComparisonSide,
   pub launch_class: MacOsLaunchClass,
   pub installed_bundle_path: String,
   pub data_container_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MacOsLaunchApplicationIdentity
{
   pub schema_version: u32,
   #[serde(rename = "runID")]
   pub run_id: String,
   #[serde(rename = "planSHA256")]
   pub plan_sha256: String,
   #[serde(rename = "chunkID")]
   pub chunk_id: String,
   #[serde(rename = "passID")]
   pub pass_id: String,
   #[serde(rename = "packID")]
   pub pack_id: String,
   pub pair_index: u32,
   pub side: ComparisonSide,
   pub generation: String,
   pub scenario_id: String,
   pub launch_class: String,
   pub cache_class: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MacOsLaunchProbeDescriptor
{
   pub schema_version: u32,
   pub probe_id: String,
   pub dispatch_path: String,
   pub target_identity: String,
   pub action_identity: String,
   pub window_number: i64,
   pub x_points: f64,
   pub y_points: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MacOsLaunchReadinessReceipt
{
   pub identity: MacOsLaunchApplicationIdentity,
   pub readiness_timestamp: u64,
   pub trusted_input_offset_ns: u64,
   pub trusted_input_deadline_ns: u64,
   pub probe: MacOsLaunchProbeDescriptor,
   pub initial_state_generation: u64,
   pub durable: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MacOsLaunchApplicationDidFinishReceipt
{
   pub identity: MacOsLaunchApplicationIdentity,
   pub application_did_finish_timestamp: u64,
   pub validation: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MacOsLaunchFirstCompleteUIReceipt
{
   pub identity: MacOsLaunchApplicationIdentity,
   pub visual_generation: u64,
   pub generation_marker_timestamp: u64,
   pub first_complete_ui_timestamp: u64,
   pub checkpoint_id: String,
   pub validation: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MacOsLaunchCompleteReceipt
{
   pub identity: MacOsLaunchApplicationIdentity,
   pub application_did_finish_timestamp: u64,
   pub first_complete_ui_timestamp: u64,
   pub readiness_timestamp: u64,
   pub trusted_input_received_timestamp: u64,
   pub response_generation_timestamp: u64,
   pub response_complete_ui_timestamp: u64,
   pub initial_state_generation: u64,
   pub response_state_generation: u64,
   pub response_visual_generation: u64,
   pub probe: MacOsLaunchProbeDescriptor,
   pub validation: String,
   pub complete: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MacOsLaunchClockAnchor
{
   pub id: u32,
   pub before_ticks: u64,
   pub after_ticks: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MacOsLaunchUIControllerReceipt
{
   pub schema_version: u32,
   #[serde(rename = "runID")]
   pub run_id: String,
   #[serde(rename = "planSHA256")]
   pub plan_sha256: String,
   #[serde(rename = "chunkID")]
   pub chunk_id: String,
   #[serde(rename = "passID")]
   pub pass_id: String,
   #[serde(rename = "packID")]
   pub pack_id: String,
   pub pair_index: u32,
   pub side: ComparisonSide,
   pub generation: String,
   pub launch_class: String,
   pub mode: String,
   pub launch_source: String,
   pub input_source: Option<String>,
   pub launch_request_ticks: u64,
   pub ready_observed_ticks: u64,
   pub input_request_ticks: Option<u64>,
   pub complete_observed_ticks: Option<u64>,
   pub clock_anchors: Vec<MacOsLaunchClockAnchor>,
   pub installed_bundle_path: String,
   pub data_container_path: Option<String>,
   pub data_container_was_absent: Option<bool>,
   pub install_identity_claimed: Option<bool>,
   pub initial_process_identifier: i32,
   pub background_observed_ticks: Option<u64>,
   pub suspended_observed_ticks: Option<u64>,
   pub resumed_process_identifier: Option<i32>,
   pub resumed_same_process: Option<bool>,
   pub app_was_terminated: bool,
   pub complete: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MacOsLaunchUIControllerStartReceipt
{
   pub schema_version: u32,
   #[serde(rename = "runID")]
   pub run_id: String,
   #[serde(rename = "planSHA256")]
   pub plan_sha256: String,
   #[serde(rename = "chunkID")]
   pub chunk_id: String,
   #[serde(rename = "passID")]
   pub pass_id: String,
   #[serde(rename = "packID")]
   pub pack_id: String,
   pub pair_index: u32,
   pub side: ComparisonSide,
   pub generation: String,
   pub launch_class: String,
   pub launch_source: String,
   pub launch_request_ticks: u64,
   pub ready_observed_ticks: u64,
   pub first_clock_anchor: MacOsLaunchClockAnchor,
   pub complete: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MacOsLaunchCachePrimerReceipt
{
   pub schema_version: u32,
   #[serde(rename = "runID")]
   pub run_id: String,
   #[serde(rename = "planSHA256")]
   pub plan_sha256: String,
   pub generation: String,
   pub side: ComparisonSide,
   #[serde(rename = "executableSHA256")]
   pub executable_sha256: String,
   #[serde(rename = "readySHA256")]
   pub ready_sha256: String,
   #[serde(rename = "uiControllerSHA256")]
   pub ui_controller_sha256: String,
   pub completed_ticks: u64,
   pub launch_source: String,
   pub app_was_terminated: bool,
   pub complete: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsLaunchPreparationReceipt
{
   pub schema_version: u32,
   #[serde(rename = "runID")]
   pub run_id: String,
   #[serde(rename = "planSHA256")]
   pub plan_sha256: String,
   pub generation: String,
   pub side: ComparisonSide,
   pub launch_class: MacOsLaunchClass,
   #[serde(rename = "executableSHA256")]
   pub executable_sha256: String,
   pub installed_bundle_path: String,
   pub data_container_path: Option<String>,
   pub cache_primed: bool,
   pub installed_bundle_was_absent: Option<bool>,
   pub data_container_was_absent: Option<bool>,
   pub completed_ticks: u64,
   pub complete: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsLaunchEvidence
{
   pub schema_version: u32,
   pub run_id: String,
   #[serde(rename = "planSHA256")]
   pub plan_sha256: String,
   pub generation: String,
   #[serde(rename = "executableSHA256")]
   pub executable_sha256: String,
   pub side: ComparisonSide,
   pub launch_class: MacOsLaunchClass,
   pub pid: u32,
   pub preparation_completed_ticks: u64,
   pub launch_request_ticks: u64,
   pub process_start_ticks: u64,
   pub process_observed_ticks: u64,
   pub application_did_finish_launching_ticks: u64,
   pub first_complete_ui_generation_ticks: u64,
   pub first_attributed_present_proxy_ticks: u64,
   pub first_interactive_input_request_ticks: u64,
   pub first_interactive_input_received_ticks: u64,
   pub first_interactive_response_generation_ticks: u64,
   pub first_interactive_response_present_proxy_ticks: u64,
   pub trace_started_before_launch_request: bool,
   pub trace_all_processes: bool,
   pub exact_pid_filtered: bool,
   #[serde(rename = "preparationReceiptSHA256")]
   pub preparation_receipt_sha256: String,
   pub presentation_calibration_status: String,
   pub complete: bool,
}

pub fn validate_macos_launch_evidence(evidence: &MacOsLaunchEvidence, expected: &MacOsLaunchExpectation) -> Result<()>
{
   if evidence.schema_version != 4
   {
      bail!("macOS launch evidence has unsupported schema version {}", evidence.schema_version);
   }
   validate_run_id(&evidence.run_id)?;
   validate_sha256(&evidence.plan_sha256)?;
   validate_sha256(&evidence.generation)?;
   validate_sha256(&evidence.executable_sha256)?;
   validate_sha256(&evidence.preparation_receipt_sha256)?;
   if evidence.run_id != expected.run_id
      || evidence.plan_sha256 != expected.plan_sha256
      || evidence.generation != expected.generation
      || evidence.executable_sha256 != expected.executable_sha256
      || evidence.side != expected.side
      || evidence.launch_class != expected.launch_class
   {
      bail!("macOS launch evidence identity differs from its frozen session");
   }
   if evidence.pid == 0 || !evidence.complete
   {
      bail!("macOS launch evidence is incomplete");
   }
   if !evidence.trace_started_before_launch_request || !evidence.trace_all_processes || !evidence.exact_pid_filtered
   {
      bail!("macOS launch trace does not start before launch and reduce to the exact launched PID");
   }
   if evidence.presentation_calibration_status != MACOS_LAUNCH_PRESENTATION_CALIBRATION_PENDING
   {
      bail!("macOS launch evidence has an unknown presentation calibration status");
   }
   let ordered = match evidence.launch_class
   {
      MacOsLaunchClass::TerminatedProcessWarmSystemCache | MacOsLaunchClass::FreshInstallFirstLaunch => vec![
         evidence.preparation_completed_ticks,
         evidence.launch_request_ticks,
         evidence.process_start_ticks,
         evidence.application_did_finish_launching_ticks,
         evidence.first_complete_ui_generation_ticks,
         evidence.first_attributed_present_proxy_ticks,
         evidence.first_interactive_input_request_ticks,
         evidence.first_interactive_input_received_ticks,
         evidence.first_interactive_response_generation_ticks,
         evidence.first_interactive_response_present_proxy_ticks,
      ],
      MacOsLaunchClass::WarmResume => vec![
         evidence.process_start_ticks,
         evidence.application_did_finish_launching_ticks,
         evidence.preparation_completed_ticks,
         evidence.launch_request_ticks,
         evidence.first_complete_ui_generation_ticks,
         evidence.first_attributed_present_proxy_ticks,
         evidence.first_interactive_input_request_ticks,
         evidence.first_interactive_input_received_ticks,
         evidence.first_interactive_response_generation_ticks,
         evidence.first_interactive_response_present_proxy_ticks,
      ],
   };
   if ordered[0] == 0 || ordered.windows(2).any(|pair| pair[0] >= pair[1])
   {
      bail!("macOS launch milestones are missing or not strictly ordered");
   }
   if evidence.process_observed_ticks < evidence.process_start_ticks
      || evidence.process_observed_ticks >= evidence.first_interactive_response_present_proxy_ticks
   {
      bail!("macOS exact process observation is outside the launched process lifetime evidence window");
   }
   Ok(())
}

pub(crate) fn validate_macos_launch_application_receipts(
   application_did_finish: &MacOsLaunchApplicationDidFinishReceipt,
   first_complete_ui: &MacOsLaunchFirstCompleteUIReceipt,
   ready: &MacOsLaunchReadinessReceipt,
   complete: &MacOsLaunchCompleteReceipt,
   expected: &MacOsLaunchExpectation,
) -> Result<()>
{
   for identity in [
      &application_did_finish.identity,
      &first_complete_ui.identity,
      &ready.identity,
      &complete.identity,
   ]
   {
      validate_macos_launch_application_identity(identity, expected)?;
   }
   if application_did_finish.validation != "application-delegate-did-finish-launching-mach-continuous-time"
      || first_complete_ui.visual_generation != 1
      || first_complete_ui.checkpoint_id != expected.launch_class.checkpoint_id()
      || first_complete_ui.validation != format!("frozen-{}-state-accessibility-role-counts-and-first-complete-ui", expected.launch_class.checkpoint_id())
      || !ready.durable
      || ready.trusted_input_offset_ns != 50_000_000
      || ready.trusted_input_deadline_ns != 2_000_000_000
      || !complete.complete
      || complete.response_visual_generation != 2
      || complete.validation != format!("{}-real-target-action-state-transition-and-response-generation", expected.launch_class.as_str())
   {
      bail!("macOS launch application receipts do not satisfy the frozen lifecycle and probe contract");
   }
   validate_macos_launch_probe(&ready.probe)?;
   validate_macos_launch_probe(&complete.probe)?;
   if ready.probe != complete.probe
      || application_did_finish.application_did_finish_timestamp != complete.application_did_finish_timestamp
      || first_complete_ui.first_complete_ui_timestamp != complete.first_complete_ui_timestamp
      || ready.readiness_timestamp != complete.readiness_timestamp
      || ready.initial_state_generation != complete.initial_state_generation
      || complete.response_state_generation <= complete.initial_state_generation
   {
      bail!("macOS launch application receipts disagree across durable barriers");
   }
   let ordered = [
      application_did_finish.application_did_finish_timestamp,
      first_complete_ui.generation_marker_timestamp,
      first_complete_ui.first_complete_ui_timestamp,
      ready.readiness_timestamp,
      complete.trusted_input_received_timestamp,
      complete.response_generation_timestamp,
      complete.response_complete_ui_timestamp,
   ];
   if ordered[0] == 0 || ordered.windows(2).any(|pair| pair[0] >= pair[1])
   {
      bail!("macOS launch application receipt timestamps are missing or misordered");
   }
   Ok(())
}

pub(crate) fn validate_macos_launch_ui_controller_receipt(receipt: &MacOsLaunchUIControllerReceipt, expected: &MacOsLaunchExpectation, mode: &str) -> Result<()>
{
   if receipt.schema_version != 1
      || receipt.run_id != expected.run_id
      || receipt.plan_sha256 != expected.plan_sha256
      || receipt.chunk_id != expected.chunk_id
      || receipt.pass_id != "canonical-launch"
      || receipt.pack_id != expected.pack_id
      || receipt.pair_index != expected.pair_index
      || receipt.side != expected.side
      || receipt.generation != expected.generation
      || receipt.launch_class != expected.launch_class.as_str()
      || receipt.mode != mode
      || receipt.launch_source != "XCUIApplication(url:).launch"
      || receipt.launch_request_ticks == 0
      || receipt.ready_observed_ticks <= receipt.launch_request_ticks
      || receipt.installed_bundle_path != expected.installed_bundle_path
      || receipt.data_container_path != expected.data_container_path
      || receipt.initial_process_identifier <= 0
      || !receipt.app_was_terminated
      || !receipt.complete
   {
      bail!("macOS launch UI-controller receipt differs from its frozen session or lifecycle source");
   }
   match expected.launch_class
   {
      MacOsLaunchClass::TerminatedProcessWarmSystemCache if receipt.data_container_was_absent.is_none()
         && receipt.install_identity_claimed.is_none()
         && receipt.background_observed_ticks.is_none()
         && receipt.suspended_observed_ticks.is_none()
         && receipt.resumed_process_identifier.is_none()
         && receipt.resumed_same_process.is_none() => (),
      MacOsLaunchClass::FreshInstallFirstLaunch if receipt.data_container_was_absent == Some(true)
         && receipt.install_identity_claimed == Some(true)
         && receipt.background_observed_ticks.is_none()
         && receipt.suspended_observed_ticks.is_none()
         && receipt.resumed_process_identifier.is_none()
         && receipt.resumed_same_process.is_none() => (),
      MacOsLaunchClass::WarmResume if receipt.data_container_was_absent.is_none()
         && receipt.install_identity_claimed.is_none()
         && receipt.background_observed_ticks.is_some_and(|ticks| ticks > 0 && ticks < receipt.suspended_observed_ticks.unwrap_or(0))
         && receipt.suspended_observed_ticks.is_some_and(|ticks| ticks < receipt.launch_request_ticks)
         && receipt.resumed_process_identifier == Some(receipt.initial_process_identifier)
         && receipt.resumed_same_process == Some(true) => (),
      _ => bail!("macOS launch UI-controller lifecycle proof does not match its launch class"),
   }
   match mode
   {
      "cache-primer" if receipt.input_source.is_none()
         && receipt.input_request_ticks.is_none()
         && receipt.complete_observed_ticks.is_none()
         && receipt.clock_anchors.is_empty() => (),
      "measure" if receipt.input_source.as_deref() == Some("XCUIElement.click")
         && receipt.input_request_ticks.is_some_and(|ticks| ticks > receipt.ready_observed_ticks)
         && receipt.complete_observed_ticks.is_some_and(|ticks| ticks > receipt.input_request_ticks.unwrap_or(0))
         && receipt.clock_anchors.len() == 2 =>
      {
         let first = &receipt.clock_anchors[0];
         let second = &receipt.clock_anchors[1];
         if first.id != 1
            || second.id != 2
            || first.before_ticks == 0
            || first.after_ticks < first.before_ticks
            || second.before_ticks <= first.after_ticks
            || second.after_ticks < second.before_ticks
         {
            bail!("macOS launch controller clock anchors are missing or misordered");
         }
      }
      _ => bail!("macOS launch UI-controller mode has incompatible input or clock evidence"),
   }
   Ok(())
}

pub(crate) fn validate_macos_launch_ui_controller_start_receipt(receipt: &MacOsLaunchUIControllerStartReceipt, expected: &MacOsLaunchExpectation) -> Result<()>
{
   if receipt.schema_version != 1
      || receipt.run_id != expected.run_id
      || receipt.plan_sha256 != expected.plan_sha256
      || receipt.chunk_id != expected.chunk_id
      || receipt.pass_id != "canonical-launch"
      || receipt.pack_id != expected.pack_id
      || receipt.pair_index != expected.pair_index
      || receipt.side != expected.side
      || receipt.generation != expected.generation
      || receipt.launch_class != expected.launch_class.as_str()
      || receipt.launch_source != "XCUIApplication(url:).launch"
      || receipt.launch_request_ticks == 0
      || receipt.ready_observed_ticks <= receipt.launch_request_ticks
      || receipt.first_clock_anchor.id != 1
      || receipt.first_clock_anchor.before_ticks == 0
      || receipt.first_clock_anchor.after_ticks < receipt.first_clock_anchor.before_ticks
      || !receipt.complete
   {
      bail!("macOS launch UI-controller start receipt differs from its frozen launch request");
   }
   Ok(())
}

pub(crate) fn validate_macos_launch_cache_primer_receipts(ready: &MacOsLaunchReadinessReceipt, controller: &MacOsLaunchUIControllerReceipt, expected: &MacOsLaunchExpectation) -> Result<()>
{
   validate_macos_launch_application_identity(&ready.identity, expected)?;
   validate_macos_launch_ui_controller_receipt(controller, expected, "cache-primer")?;
   validate_macos_launch_probe(&ready.probe)?;
   if !ready.durable
      || ready.readiness_timestamp == 0
      || ready.trusted_input_offset_ns != 50_000_000
      || ready.trusted_input_deadline_ns != 2_000_000_000
   {
      bail!("macOS launch cache primer did not reach the frozen durable ready barrier");
   }
   Ok(())
}

fn validate_macos_launch_application_identity(identity: &MacOsLaunchApplicationIdentity, expected: &MacOsLaunchExpectation) -> Result<()>
{
   if identity.schema_version != 1
      || identity.run_id != expected.run_id
      || identity.plan_sha256 != expected.plan_sha256
      || identity.chunk_id != expected.chunk_id
      || identity.pass_id != "canonical-launch"
      || identity.pack_id != expected.pack_id
      || identity.pair_index != expected.pair_index
      || identity.side != expected.side
      || identity.generation != expected.generation
      || identity.scenario_id != "startup.first-screen"
      || identity.launch_class != expected.launch_class.as_str()
      || identity.cache_class != expected.launch_class.cache_class()
   {
      bail!("macOS launch application identity differs from its frozen session");
   }
   Ok(())
}

pub fn validate_macos_launch_preparation_receipt(receipt: &MacOsLaunchPreparationReceipt, expected: &MacOsLaunchExpectation) -> Result<()>
{
   if receipt.schema_version != 1
      || receipt.run_id != expected.run_id
      || receipt.plan_sha256 != expected.plan_sha256
      || receipt.generation != expected.generation
      || receipt.side != expected.side
      || receipt.launch_class != expected.launch_class
      || receipt.executable_sha256 != expected.executable_sha256
      || receipt.installed_bundle_path != expected.installed_bundle_path
      || receipt.data_container_path != expected.data_container_path
      || receipt.completed_ticks == 0
      || !receipt.complete
   {
      bail!("macOS launch preparation receipt differs from its frozen session");
   }
   match receipt.launch_class
   {
      MacOsLaunchClass::TerminatedProcessWarmSystemCache if receipt.cache_primed
         && receipt.installed_bundle_was_absent.is_none()
         && receipt.data_container_was_absent.is_none() => Ok(()),
      MacOsLaunchClass::FreshInstallFirstLaunch if !receipt.cache_primed
         && receipt.installed_bundle_was_absent == Some(true)
         && receipt.data_container_was_absent == Some(true) => Ok(()),
      MacOsLaunchClass::WarmResume if !receipt.cache_primed
         && receipt.installed_bundle_was_absent.is_none()
         && receipt.data_container_was_absent.is_none() => Ok(()),
      _ => bail!("macOS launch preparation proof does not match its launch class"),
   }
}

fn validate_macos_launch_probe(probe: &MacOsLaunchProbeDescriptor) -> Result<()>
{
   if probe.schema_version != 1
      || probe.probe_id != "startup-primary-control"
      || probe.dispatch_path != "trusted-os-input-target-action"
      || probe.target_identity.is_empty()
      || probe.action_identity.is_empty()
      || probe.window_number <= 0
      || !probe.x_points.is_finite()
      || !probe.y_points.is_finite()
   {
      bail!("macOS launch trusted-input probe is incomplete");
   }
   Ok(())
}
