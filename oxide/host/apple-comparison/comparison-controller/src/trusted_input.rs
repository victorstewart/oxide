//! Typed public-XCUI input plans and receipt materialization for macOS comparison runs.
//!
//! Multipointer image zoom and navigation cancellation are admitted only when their
//! complete traces match the frozen contracts; both dispatch as real mouse drags.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path};

use anyhow::{bail, ensure, Context, Result};
use oxide_benchmark_spec::{AppleCampaignMeasurementTimingSpec, AppleCampaignPassTimingSpec, AppleCampaignScenarioTimingSpec, ArtifactIdentity, ScenarioSpec, TraceEvent, TraceOperation, TraceValue};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{ComparisonSide, MacOsCampaignPlan, MacOsCampaignSession};

const IMAGE_PINCH_OVERRIDE_EVENT_COUNT: usize = 18;
const NAVIGATION_CANCEL_OVERRIDE_EVENT_COUNT: usize = 5;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "command", rename_all = "kebab-case")]
pub enum MacOsTrustedInputCommand
{
   Click {
      first_event_index: usize,
      last_event_index: usize,
      target: String,
      x_millionths: Option<i32>,
      y_millionths: Option<i32>,
   },
   Wheel {
      event_index: usize,
      target: String,
      delta_x_millionths: i32,
      delta_y_millionths: i32,
   },
   Text {
      event_index: usize,
      target: String,
      value: String,
   },
   SelectReplace {
      first_event_index: usize,
      last_event_index: usize,
      target: String,
      selection_start_utf8: usize,
      selection_end_utf8: usize,
      replacement: String,
   },
   ImagePinchOverride {
      first_event_index: usize,
      last_event_index: usize,
      target: String,
      start_x_millionths: i32,
      start_y_millionths: i32,
      end_x_millionths: i32,
      end_y_millionths: i32,
      duration_us: u64,
   },
   NavigationInteractiveCancelOverride {
      first_event_index: usize,
      last_event_index: usize,
      target: String,
      start_x_millionths: i32,
      start_y_millionths: i32,
      end_x_millionths: i32,
      end_y_millionths: i32,
      duration_us: u64,
   },
   KeyPair {
      key_down_event_index: usize,
      key_up_event_index: usize,
      target: Option<String>,
      key: String,
   },
   SinglePointerDrag {
      first_event_index: usize,
      last_event_index: usize,
      pointer: u32,
      start_x_millionths: i32,
      start_y_millionths: i32,
      end_x_millionths: i32,
      end_y_millionths: i32,
      duration_us: u64,
   },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsDeclaredApplicationStimulus
{
   pub event_index: usize,
   pub operation: TraceOperation,
   pub target: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsTrustedInputPlan
{
   pub commands: Vec<MacOsTrustedInputCommand>,
   pub application_stimuli: Vec<MacOsDeclaredApplicationStimulus>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsTrustedInputScenarioPreflight
{
   pub scenario_id: String,
   pub command_count: usize,
   pub application_stimulus_count: usize,
   pub commands: Vec<MacOsTrustedInputCommand>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsTrustedInputReceiptManifestEntry
{
   pub command_sequence: u64,
   pub scenario_id: String,
   pub request_sha256: String,
   pub controller_receipt_sha256: String,
   pub application_receipt_sha256: String,
   pub first_event_index: usize,
   pub last_event_index: usize,
   pub raw_event_families: Vec<String>,
   pub raw_event_types: Vec<u64>,
   pub state_generation_before: u64,
   pub state_generation_after: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsTrustedInputReceiptManifest
{
   pub schema_version: u32,
   pub run_id: String,
   pub plan_sha256: String,
   pub chunk_id: String,
   pub pass_id: String,
   pub pack_id: String,
   pub pair_index: u32,
   pub side: ComparisonSide,
   pub generation: String,
   pub expected_command_count: u64,
   pub receipts: Vec<MacOsTrustedInputReceiptManifestEntry>,
   pub complete: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum SwiftCommandKind
{
   Click,
   Wheel,
   Text,
   SelectReplace,
   ImagePinchOverride,
   NavigationInteractiveCancelOverride,
   KeyPair,
   SinglePointerDrag,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
struct SwiftCommand
{
   kind: SwiftCommandKind,
   first_event_index: usize,
   last_event_index: usize,
   target: Option<String>,
   pointer: Option<u32>,
   start_x_millionths: Option<i32>,
   start_y_millionths: Option<i32>,
   end_x_millionths: Option<i32>,
   end_y_millionths: Option<i32>,
   delta_x_millionths: Option<i32>,
   delta_y_millionths: Option<i32>,
   duration_us: Option<u64>,
   #[serde(rename = "selectionStartUTF8")]
   selection_start_utf8: Option<usize>,
   #[serde(rename = "selectionEndUTF8")]
   selection_end_utf8: Option<usize>,
   value: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
struct SwiftIdentity
{
   schema_version: u32,
   #[serde(rename = "runID")]
   run_id: String,
   #[serde(rename = "planSHA256")]
   plan_sha256: String,
   #[serde(rename = "chunkID")]
   chunk_id: String,
   #[serde(rename = "passID")]
   pass_id: String,
   #[serde(rename = "packID")]
   pack_id: String,
   pair_index: u32,
   side: ComparisonSide,
   generation: String,
   #[serde(rename = "scenarioID")]
   scenario_id: String,
   command_sequence: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SwiftRequest
{
   identity: SwiftIdentity,
   command: SwiftCommand,
   descriptor_prepared_timestamp: u64,
   durable: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SwiftControllerReceipt
{
   identity: SwiftIdentity,
   #[serde(rename = "requestSHA256")]
   request_sha256: String,
   dispatch_path: String,
   dispatch_started_timestamp: u64,
   dispatch_completed_timestamp: u64,
   application_was_foreground: bool,
   durable: bool,
   complete: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SwiftApplicationReceipt
{
   identity: SwiftIdentity,
   #[serde(rename = "requestSHA256")]
   request_sha256: String,
   #[serde(rename = "controllerReceiptSHA256")]
   controller_receipt_sha256: String,
   first_event_index: usize,
   last_event_index: usize,
   raw_event_families: Vec<String>,
   raw_event_types: Vec<u64>,
   raw_event_timestamp: f64,
   raw_event_location_x: f64,
   raw_event_location_y: f64,
   window_number: i64,
   application_received_timestamp: u64,
   application_was_foreground: bool,
   state_generation_before: u64,
   state_generation_after: u64,
   durable: bool,
   complete: bool,
}

pub fn compile_macos_trusted_input_trace(events: &[TraceEvent]) -> Result<MacOsTrustedInputPlan>
{
   compile_macos_trusted_input_trace_with_policy(events, false)
}

fn compile_macos_trusted_input_trace_with_policy(events: &[TraceEvent], allow_non_claim_lifecycle_stimuli: bool) -> Result<MacOsTrustedInputPlan>
{
   let mut plan = MacOsTrustedInputPlan::default();
   let mut index = 0;
   while index < events.len()
   {
      let event = &events[index];
      match event.op
      {
         TraceOperation::PointerDown =>
         {
            let (command, next) = if events.get(index + 1).is_some_and(|event| event.op == TraceOperation::PointerDown)
            {
               compile_image_pinch_override(events, index)?
            }
            else if events.get(index + 3).is_some_and(|event| event.op == TraceOperation::PointerCancel)
            {
               compile_navigation_cancel_override(events, index)?
            }
            else
            {
               compile_pointer_sequence(events, index)?
            };
            plan.commands.push(command);
            index = next;
         }
         TraceOperation::PointerMove | TraceOperation::PointerUp =>
         {
            bail!("trusted macOS input event {index} is an unpaired {:?}", event.op);
         }
         TraceOperation::PointerCancel =>
         {
            bail!("trusted macOS input event {index} uses pointer-cancel, which public XCUI cannot synthesize faithfully");
         }
         TraceOperation::Wheel =>
         {
            let target = required_target(event, index, "wheel")?;
            let delta_x = event.delta_x_millionths.unwrap_or(0);
            let delta_y = event.delta_y_millionths.unwrap_or(0);
            ensure!(delta_x != 0 || delta_y != 0, "trusted macOS wheel event {index} has no delta");
            plan.commands.push(MacOsTrustedInputCommand::Wheel {
               event_index: index,
               target,
               delta_x_millionths: delta_x,
               delta_y_millionths: delta_y,
            });
            index += 1;
         }
         TraceOperation::CommitText =>
         {
            let target = required_target(event, index, "text")?;
            ensure!(target == "chat:composer", "trusted macOS text event {index} targets {target}; selected replacement has no adjacent exact focus event");
            let value = text_value(event, index, "text")?;
            plan.commands.push(MacOsTrustedInputCommand::Text {event_index: index, target, value});
            index += 1;
         }
         TraceOperation::Focus =>
         {
            if matches!(event.value, Some(TraceValue::Text(ref value)) if value.contains(':'))
            {
               let (command, next) = compile_select_replace(events, index)?;
               plan.commands.push(command);
               index = next;
               continue;
            }
            let target = required_target(event, index, "focus")?;
            plan.commands.push(MacOsTrustedInputCommand::Click {
               first_event_index: index,
               last_event_index: index,
               target,
               x_millionths: event.x_millionths,
               y_millionths: event.y_millionths,
            });
            index += 1;
         }
         TraceOperation::Navigate =>
         {
            let target = required_target(event, index, "navigation")?;
            ensure!(target != "navigation:cancel", "trusted macOS navigation event {index} requests interactive cancellation, which public XCUI cannot synthesize faithfully");
            plan.commands.push(MacOsTrustedInputCommand::Click {
               first_event_index: index,
               last_event_index: index,
               target,
               x_millionths: event.x_millionths,
               y_millionths: event.y_millionths,
            });
            index += 1;
         }
         TraceOperation::Mutate if event.target.as_deref().is_some_and(|target| target.ends_with(":favorite")) =>
         {
            let target = required_target(event, index, "favorite")?;
            plan.commands.push(MacOsTrustedInputCommand::Click {
               first_event_index: index,
               last_event_index: index,
               target,
               x_millionths: event.x_millionths,
               y_millionths: event.y_millionths,
            });
            index += 1;
         }
         TraceOperation::KeyDown =>
         {
            let Some(key_up) = events.get(index + 1) else
            {
               bail!("trusted macOS key-down event {index} has no paired key-up");
            };
            ensure!(key_up.op == TraceOperation::KeyUp, "trusted macOS key-down event {index} is not immediately paired with key-up");
            ensure!(event.target == key_up.target && event.value == key_up.value, "trusted macOS key pair at event {index} changes target or key identity");
            let key = text_value(event, index, "key")?;
            plan.commands.push(MacOsTrustedInputCommand::KeyPair {
               key_down_event_index: index,
               key_up_event_index: index + 1,
               target: event.target.clone(),
               key,
            });
            index += 2;
         }
         TraceOperation::KeyUp =>
         {
            bail!("trusted macOS key-up event {index} has no paired key-down");
         }
         TraceOperation::ImeStart | TraceOperation::ImeUpdate | TraceOperation::ImeEnd =>
         {
            bail!("trusted macOS input event {index} uses IME composition, which public XCUI cannot synthesize faithfully");
         }
         TraceOperation::Background | TraceOperation::Foreground =>
         {
            ensure!(allow_non_claim_lifecycle_stimuli, "trusted macOS lifecycle event {index} uses {:?}; measured background/foreground transitions require controller-owned activation or suspension with receipts", event.op);
            plan.application_stimuli.push(MacOsDeclaredApplicationStimulus {
               event_index: index,
               operation: event.op,
               target: event.target.clone(),
            });
            index += 1;
         }
         TraceOperation::Resize | TraceOperation::Orientation | TraceOperation::Theme | TraceOperation::Scale
         | TraceOperation::Mutate | TraceOperation::ResourceArrival | TraceOperation::Pressure =>
         {
            plan.application_stimuli.push(MacOsDeclaredApplicationStimulus {
               event_index: index,
               operation: event.op,
               target: event.target.clone(),
            });
            index += 1;
         }
      }
   }
   Ok(plan)
}

pub(crate) fn preflight_macos_trusted_input_scenarios(spec_root: &Path, bindings: &[(String, ArtifactIdentity)], timing: Option<&AppleCampaignPassTimingSpec>, allow_non_claim_lifecycle_stimuli: bool) -> Result<Vec<MacOsTrustedInputScenarioPreflight>>
{
   if let Some(timing) = timing
   {
      ensure!(timing.scenarios.len() == bindings.len() && timing.scenarios.iter().zip(bindings).all(|(scenario, binding)| scenario.scenario_id == binding.0), "trusted-input timing selection differs from the session pack");
   }
   let mut preflight = Vec::with_capacity(bindings.len());
   for (scenario_index, (scenario_id, artifact)) in bindings.iter().enumerate()
   {
      let bytes = read_verified_artifact(spec_root, artifact).with_context(|| format!("reading trusted-input scenario {scenario_id}"))?;
      let scenario: ScenarioSpec = serde_json::from_slice(&bytes).with_context(|| format!("decoding trusted-input scenario {scenario_id}"))?;
      ensure!(scenario.id == *scenario_id, "trusted-input scenario binding differs from its manifest: {scenario_id}");
      let scenario_timing = timing.map(|timing| &timing.scenarios[scenario_index]);
      let mut phase_events = Vec::with_capacity(scenario.phases.len());
      for phase in &scenario.phases
      {
         let events = if let Some(trace) = &phase.trace
         {
            let trace_bytes = read_verified_artifact(spec_root, trace).with_context(|| format!("reading trusted-input trace {}", trace.path))?;
            serde_json::from_slice(&trace_bytes).with_context(|| format!("decoding trusted-input trace {}", trace.path))?
         }
         else {Vec::new()};
         let trace_duration_us = events.iter().map(|event: &TraceEvent| event.at_us).max().unwrap_or(0);
         let checkpoint_duration_us = scenario.parity_checkpoints.iter().filter(|checkpoint| checkpoint.phase_id == phase.id).filter_map(|checkpoint| checkpoint.at_us).max().unwrap_or(0);
         let source_duration_us = phase.duration_ms.map(|duration_ms| duration_ms.checked_mul(1_000).context("trusted-input phase duration overflow")).transpose()?.unwrap_or(trace_duration_us.max(checkpoint_duration_us));
         phase_events.push((phase, events, source_duration_us));
      }
      let mut scheduled_events = Vec::new();
      for (phase, events, source_duration_us) in &phase_events
      {
         scheduled_events.extend(expand_timed_trace_events(&phase_events, phase.id.as_str(), events, *source_duration_us, scenario_timing)?);
      }
      let plan = compile_macos_trusted_input_trace_with_policy(&scheduled_events, allow_non_claim_lifecycle_stimuli).with_context(|| format!("preflighting trusted macOS input for {scenario_id}"))?;
      preflight.push(MacOsTrustedInputScenarioPreflight {
         scenario_id: scenario_id.clone(),
         command_count: plan.commands.len(),
         application_stimulus_count: plan.application_stimuli.len(),
         commands: plan.commands,
      });
   }
   Ok(preflight)
}

fn expand_timed_trace_events(phase_events: &[(&oxide_benchmark_spec::ScenarioPhase, Vec<TraceEvent>, u64)], phase_id: &str, events: &[TraceEvent], source_duration_us: u64, timing: Option<&AppleCampaignScenarioTimingSpec>) -> Result<Vec<TraceEvent>>
{
   let Some(AppleCampaignScenarioTimingSpec {measurement: AppleCampaignMeasurementTimingSpec::Iterations {phase_id: iteration_phase_id, source_iteration_count, iteration_count, occupied_seconds}, ..}) = timing else
   {
      return Ok(events.to_vec());
   };
   if iteration_phase_id != phase_id
   {
      return Ok(events.to_vec());
   }
   ensure!(*source_iteration_count > 0 && *iteration_count > 0 && !events.is_empty() && source_duration_us > 0 && events.len() % *source_iteration_count as usize == 0, "trusted-input iteration timing is not representable for phase {phase_id}");
   let fixed_us = phase_events.iter().filter(|(phase, _, _)| phase.measured && phase.id != *iteration_phase_id).try_fold(0_u64, |total, (_, _, duration_us)| total.checked_add(*duration_us).context("trusted-input fixed measured duration overflow"))?;
   let target_total_us = occupied_seconds.checked_mul(1_000_000).context("trusted-input occupied duration overflow")?;
   let target_duration_us = target_total_us.checked_sub(fixed_us).filter(|duration| *duration > 0).context("trusted-input iteration timing has no target duration")?;
   let events_per_iteration = events.len() / *source_iteration_count as usize;
   let source_stride_us = source_duration_us / u64::from(*source_iteration_count);
   let target_stride_us = target_duration_us / u64::from(*iteration_count);
   ensure!(events_per_iteration > 0 && source_stride_us > 0 && target_stride_us > 0, "trusted-input iteration timing has an empty stride for phase {phase_id}");
   let mut expanded = Vec::with_capacity(events_per_iteration * *iteration_count as usize);
   for index in 0..*iteration_count
   {
      let source_index = index % *source_iteration_count;
      let source_start_us = u64::from(source_index).checked_mul(source_stride_us).context("trusted-input source iteration offset overflow")?;
      let target_start_us = u64::from(index).checked_mul(target_stride_us).context("trusted-input target iteration offset overflow")?;
      let first = source_index as usize * events_per_iteration;
      for event in &events[first..first + events_per_iteration]
      {
         let relative_us = event.at_us.checked_sub(source_start_us).with_context(|| format!("trusted-input event precedes its source iteration in phase {phase_id}"))?;
         let scaled_us = relative_us.checked_mul(target_stride_us).context("trusted-input scaled timestamp overflow")? / source_stride_us;
         let mut expanded_event = event.clone();
         expanded_event.at_us = target_start_us.checked_add(scaled_us).context("trusted-input expanded timestamp overflow")?;
         expanded.push(expanded_event);
      }
   }
   Ok(expanded)
}

pub(crate) fn materialize_macos_trusted_input_receipt_manifest(directory: &Path, plan: &MacOsCampaignPlan, session: &MacOsCampaignSession, generation: &str, preflight: &[MacOsTrustedInputScenarioPreflight]) -> Result<ArtifactIdentity>
{
   let side = match session.side {ComparisonSide::Native => "native", ComparisonSide::Oxide => "oxide"};
   let expected = preflight.iter().flat_map(|scenario| scenario.commands.iter().map(move |command| (&scenario.scenario_id, command))).collect::<Vec<_>>();
   ensure!(preflight.iter().all(|scenario| scenario.command_count == scenario.commands.len()), "trusted-input preflight command details differ from their counts");
   let expected_command_count = u64::try_from(expected.len()).context("trusted-input expected command count exceeds u64")?;
   let prefix = format!("{}.trusted-input.", side);
   let manifest_name = format!("{}.trusted-input.manifest.json", side);
   let mut expected_names = BTreeSet::new();
   for sequence in 0..expected_command_count
   {
      for suffix in ["request.json", "controller.json", "application.json"]
      {
         expected_names.insert(format!("{}{}.{}", prefix, sequence, suffix));
      }
   }
   for entry in fs::read_dir(directory).with_context(|| format!("reading trusted-input session directory {}", directory.display()))?
   {
      let name = entry?.file_name().to_string_lossy().into_owned();
      if name.starts_with(&prefix) && name != manifest_name && !expected_names.remove(&name)
      {
         bail!("trusted-input session contains an extra or duplicate artifact: {}", name);
      }
   }
   ensure!(expected_names.is_empty(), "trusted-input session is missing expected artifacts: {:?}", expected_names);

   let mut receipts = Vec::with_capacity(expected.len());
   let mut active_scenario = None;
   let mut prior_scenario_generation = None;
   let mut completed_scenarios = BTreeSet::new();
   for (sequence, (scenario_id, command)) in expected.into_iter().enumerate()
   {
      let sequence = u64::try_from(sequence).context("trusted-input command sequence exceeds u64")?;
      let expected_identity = SwiftIdentity {
         schema_version: 1,
         run_id: plan.run_id.clone(),
         plan_sha256: plan.plan_sha256.clone(),
         chunk_id: session.chunk_id.clone(),
         pass_id: session.pass_id.clone(),
         pack_id: session.pack_id.clone(),
         pair_index: session.pair_index,
         side: session.side,
         generation: String::from(generation),
         scenario_id: (*scenario_id).clone(),
         command_sequence: sequence,
      };
      let request_path = directory.join(format!("{}{}.request.json", prefix, sequence));
      let controller_path = directory.join(format!("{}{}.controller.json", prefix, sequence));
      let application_path = directory.join(format!("{}{}.application.json", prefix, sequence));
      let (request, request_bytes) = read_canonical_json::<SwiftRequest>(&request_path)?;
      let (controller, controller_bytes) = read_canonical_json::<SwiftControllerReceipt>(&controller_path)?;
      let (application, application_bytes) = read_canonical_json::<SwiftApplicationReceipt>(&application_path)?;
      let expected_command = swift_command(command);
      ensure!(request.identity == expected_identity && request.command == expected_command && request.descriptor_prepared_timestamp > 0 && request.durable, "trusted-input prepared descriptor differs from preflight identity or command at sequence {}", sequence);
      let request_sha256 = sha256(&request_bytes);
      let controller_receipt_sha256 = sha256(&controller_bytes);
      ensure!(controller.identity == expected_identity && controller.request_sha256 == request_sha256 && controller.dispatch_path == dispatch_path(command) && controller.dispatch_started_timestamp > 0 && controller.dispatch_completed_timestamp >= controller.dispatch_started_timestamp && controller.application_was_foreground && controller.durable && controller.complete, "trusted-input controller receipt is incomplete or unbound at sequence {}", sequence);
      let (first_event_index, last_event_index) = event_range(command);
      let (raw_event_families, raw_event_types) = raw_event_identity(command);
      ensure!(application.identity == expected_identity && application.request_sha256 == request_sha256 && application.controller_receipt_sha256 == controller_receipt_sha256 && application.first_event_index == first_event_index && application.last_event_index == last_event_index && application.raw_event_families.iter().map(String::as_str).eq(raw_event_families.iter().copied()) && application.raw_event_types == raw_event_types && application.raw_event_timestamp.is_finite() && application.raw_event_timestamp >= 0.0 && application.raw_event_location_x.is_finite() && application.raw_event_location_y.is_finite() && application.window_number > 0 && application.application_received_timestamp > 0 && application.application_was_foreground && application.state_generation_after > application.state_generation_before && application.durable && application.complete, "trusted-input application receipt is incomplete or unbound at sequence {}", sequence);
      ensure!(request.descriptor_prepared_timestamp <= controller.dispatch_started_timestamp && controller.dispatch_started_timestamp <= application.application_received_timestamp && application.application_received_timestamp <= controller.dispatch_completed_timestamp, "trusted-input descriptor preparation, live dispatch, and application receipt timestamps are misordered at sequence {}", sequence);
      if active_scenario.as_deref() != Some(scenario_id.as_str())
      {
         if let Some(previous) = active_scenario.replace(scenario_id.clone())
         {
            completed_scenarios.insert(previous);
         }
         ensure!(!completed_scenarios.contains(scenario_id), "trusted-input receipt sequence re-enters completed scenario {}", scenario_id);
         prior_scenario_generation = None;
      }
      if let Some(previous) = prior_scenario_generation
      {
         ensure!(application.state_generation_before >= previous, "trusted-input state generation moved backward within scenario {}", scenario_id);
      }
      prior_scenario_generation = Some(application.state_generation_after);
      receipts.push(MacOsTrustedInputReceiptManifestEntry {
         command_sequence: sequence,
         scenario_id: (*scenario_id).clone(),
         request_sha256,
         controller_receipt_sha256,
         application_receipt_sha256: sha256(&application_bytes),
         first_event_index,
         last_event_index,
         raw_event_families: raw_event_families.iter().map(|family| String::from(*family)).collect(),
         raw_event_types: raw_event_types.to_vec(),
         state_generation_before: application.state_generation_before,
         state_generation_after: application.state_generation_after,
      });
   }
   let manifest = MacOsTrustedInputReceiptManifest {
      schema_version: 1,
      run_id: plan.run_id.clone(),
      plan_sha256: plan.plan_sha256.clone(),
      chunk_id: session.chunk_id.clone(),
      pass_id: session.pass_id.clone(),
      pack_id: session.pack_id.clone(),
      pair_index: session.pair_index,
      side: session.side,
      generation: String::from(generation),
      expected_command_count,
      receipts,
      complete: true,
   };
   let path = directory.join(&manifest_name);
   let mut expected_bytes = serde_json::to_vec_pretty(&manifest).context("encoding trusted-input receipt manifest")?;
   expected_bytes.push(b'\n');
   if path.exists()
   {
      let stored = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
      ensure!(stored == expected_bytes, "stored trusted-input receipt manifest differs from current raw evidence");
   }
   else
   {
      super::durable_json(&manifest, &path)?;
   }
   let relative_path = Path::new("Runs")
      .join(&plan.run_id)
      .join(&session.chunk_id)
      .join(&session.pass_id)
      .join(&session.pack_id)
      .join(session.pair_index.to_string())
      .join(manifest_name);
   Ok(ArtifactIdentity {path: relative_path.to_string_lossy().into_owned(), sha256: sha256(&expected_bytes)})
}

fn read_canonical_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<(T, Vec<u8>)>
{
   let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
   let value: serde_json::Value = serde_json::from_slice(&bytes).with_context(|| format!("decoding canonical JSON value {}", path.display()))?;
   ensure!(serde_json::to_vec(&value).context("encoding canonical trusted-input JSON")? == bytes, "trusted-input artifact is not canonical JSON: {}", path.display());
   let decoded = serde_json::from_value(value).with_context(|| format!("decoding typed trusted-input artifact {}", path.display()))?;
   Ok((decoded, bytes))
}

fn swift_command(command: &MacOsTrustedInputCommand) -> SwiftCommand
{
   let mut value = SwiftCommand {
      kind: SwiftCommandKind::Click,
      first_event_index: 0,
      last_event_index: 0,
      target: None,
      pointer: None,
      start_x_millionths: None,
      start_y_millionths: None,
      end_x_millionths: None,
      end_y_millionths: None,
      delta_x_millionths: None,
      delta_y_millionths: None,
      duration_us: None,
      selection_start_utf8: None,
      selection_end_utf8: None,
      value: None,
   };
   match command
   {
      MacOsTrustedInputCommand::Click {first_event_index, last_event_index, target, x_millionths, y_millionths} =>
      {
         value.first_event_index = *first_event_index;
         value.last_event_index = *last_event_index;
         value.target = Some(target.clone());
         value.start_x_millionths = *x_millionths;
         value.start_y_millionths = *y_millionths;
      }
      MacOsTrustedInputCommand::Wheel {event_index, target, delta_x_millionths, delta_y_millionths} =>
      {
         value.kind = SwiftCommandKind::Wheel;
         value.first_event_index = *event_index;
         value.last_event_index = *event_index;
         value.target = Some(target.clone());
         value.delta_x_millionths = Some(*delta_x_millionths);
         value.delta_y_millionths = Some(*delta_y_millionths);
      }
      MacOsTrustedInputCommand::Text {event_index, target, value: text} =>
      {
         value.kind = SwiftCommandKind::Text;
         value.first_event_index = *event_index;
         value.last_event_index = *event_index;
         value.target = Some(target.clone());
         value.value = Some(text.clone());
      }
      MacOsTrustedInputCommand::SelectReplace {first_event_index, last_event_index, target, selection_start_utf8, selection_end_utf8, replacement} =>
      {
         value.kind = SwiftCommandKind::SelectReplace;
         value.first_event_index = *first_event_index;
         value.last_event_index = *last_event_index;
         value.target = Some(target.clone());
         value.selection_start_utf8 = Some(*selection_start_utf8);
         value.selection_end_utf8 = Some(*selection_end_utf8);
         value.value = Some(replacement.clone());
      }
      MacOsTrustedInputCommand::ImagePinchOverride {first_event_index, last_event_index, target, start_x_millionths, start_y_millionths, end_x_millionths, end_y_millionths, duration_us} =>
      {
         value.kind = SwiftCommandKind::ImagePinchOverride;
         value.first_event_index = *first_event_index;
         value.last_event_index = *last_event_index;
         value.target = Some(target.clone());
         value.start_x_millionths = Some(*start_x_millionths);
         value.start_y_millionths = Some(*start_y_millionths);
         value.end_x_millionths = Some(*end_x_millionths);
         value.end_y_millionths = Some(*end_y_millionths);
         value.duration_us = Some(*duration_us);
      }
      MacOsTrustedInputCommand::NavigationInteractiveCancelOverride {first_event_index, last_event_index, target, start_x_millionths, start_y_millionths, end_x_millionths, end_y_millionths, duration_us} =>
      {
         value.kind = SwiftCommandKind::NavigationInteractiveCancelOverride;
         value.first_event_index = *first_event_index;
         value.last_event_index = *last_event_index;
         value.target = Some(target.clone());
         value.start_x_millionths = Some(*start_x_millionths);
         value.start_y_millionths = Some(*start_y_millionths);
         value.end_x_millionths = Some(*end_x_millionths);
         value.end_y_millionths = Some(*end_y_millionths);
         value.duration_us = Some(*duration_us);
      }
      MacOsTrustedInputCommand::KeyPair {key_down_event_index, key_up_event_index, target, key} =>
      {
         value.kind = SwiftCommandKind::KeyPair;
         value.first_event_index = *key_down_event_index;
         value.last_event_index = *key_up_event_index;
         value.target = target.clone();
         value.value = Some(key.clone());
      }
      MacOsTrustedInputCommand::SinglePointerDrag {first_event_index, last_event_index, pointer, start_x_millionths, start_y_millionths, end_x_millionths, end_y_millionths, duration_us} =>
      {
         value.kind = SwiftCommandKind::SinglePointerDrag;
         value.first_event_index = *first_event_index;
         value.last_event_index = *last_event_index;
         value.pointer = Some(*pointer);
         value.start_x_millionths = Some(*start_x_millionths);
         value.start_y_millionths = Some(*start_y_millionths);
         value.end_x_millionths = Some(*end_x_millionths);
         value.end_y_millionths = Some(*end_y_millionths);
         value.duration_us = Some(*duration_us);
      }
   }
   value
}

fn dispatch_path(command: &MacOsTrustedInputCommand) -> &'static str
{
   match command
   {
      MacOsTrustedInputCommand::Click {..} => "XCUIElement.click",
      MacOsTrustedInputCommand::Wheel {..} => "XCUIElement.scroll",
      MacOsTrustedInputCommand::Text {..} => "XCUIElement.typeText",
      MacOsTrustedInputCommand::SelectReplace {..} => "XCUIElement.click-typeKey-typeText",
      MacOsTrustedInputCommand::ImagePinchOverride {..} => "XCUICoordinate.click-drag",
      MacOsTrustedInputCommand::NavigationInteractiveCancelOverride {..} => "XCUICoordinate.click-drag-mouseUp-cancel",
      MacOsTrustedInputCommand::KeyPair {..} => "XCUIElement.typeKey",
      MacOsTrustedInputCommand::SinglePointerDrag {..} => "XCUICoordinate.click-drag",
   }
}

fn event_range(command: &MacOsTrustedInputCommand) -> (usize, usize)
{
   match command
   {
      MacOsTrustedInputCommand::Click {first_event_index, last_event_index, ..}
      | MacOsTrustedInputCommand::SelectReplace {first_event_index, last_event_index, ..}
      | MacOsTrustedInputCommand::ImagePinchOverride {first_event_index, last_event_index, ..}
      | MacOsTrustedInputCommand::NavigationInteractiveCancelOverride {first_event_index, last_event_index, ..}
      | MacOsTrustedInputCommand::SinglePointerDrag {first_event_index, last_event_index, ..} => (*first_event_index, *last_event_index),
      MacOsTrustedInputCommand::Wheel {event_index, ..} | MacOsTrustedInputCommand::Text {event_index, ..} => (*event_index, *event_index),
      MacOsTrustedInputCommand::KeyPair {key_down_event_index, key_up_event_index, ..} => (*key_down_event_index, *key_up_event_index),
   }
}

fn raw_event_identity(command: &MacOsTrustedInputCommand) -> (&'static [&'static str], &'static [u64])
{
   match command
   {
      MacOsTrustedInputCommand::Click {..} => (&["mouse"], &[1]),
      MacOsTrustedInputCommand::ImagePinchOverride {..}
      | MacOsTrustedInputCommand::NavigationInteractiveCancelOverride {..}
      | MacOsTrustedInputCommand::SinglePointerDrag {..} => (&["mouse"], &[6]),
      MacOsTrustedInputCommand::Wheel {..} => (&["scroll"], &[22]),
      MacOsTrustedInputCommand::Text {..} | MacOsTrustedInputCommand::KeyPair {..} => (&["key"], &[10]),
      MacOsTrustedInputCommand::SelectReplace {..} => (&["mouse", "key"], &[1, 10]),
   }
}

fn sha256(bytes: &[u8]) -> String
{
   format!("{:x}", Sha256::digest(bytes))
}

fn compile_select_replace(events: &[TraceEvent], first: usize) -> Result<(MacOsTrustedInputCommand, usize)>
{
   let focus = &events[first];
   let target = required_target(focus, first, "selection")?;
   ensure!(target == "chat:append:16" && matches!(focus.value, Some(TraceValue::Text(ref value)) if value == "0:6"), "trusted macOS selection event {first} is not the frozen public-XCUI-safe chat:append:16 UTF-8 range 0:6");
   let replacement = events.get(first + 1).with_context(|| format!("trusted macOS selection event {first} has no adjacent replacement"))?;
   ensure!(replacement.op == TraceOperation::CommitText && replacement.target.as_deref() == Some(target.as_str()) && matches!(replacement.value, Some(TraceValue::Text(ref value)) if value == "Oxide"), "trusted macOS selection event {first} is not followed by the frozen adjacent Oxide replacement");
   Ok((MacOsTrustedInputCommand::SelectReplace {
      first_event_index: first,
      last_event_index: first + 1,
      target,
      selection_start_utf8: 0,
      selection_end_utf8: 6,
      replacement: String::from("Oxide"),
   }, first + 2))
}

fn compile_image_pinch_override(events: &[TraceEvent], first: usize) -> Result<(MacOsTrustedInputCommand, usize)>
{
   let end = first.checked_add(IMAGE_PINCH_OVERRIDE_EVENT_COUNT).context("trusted macOS image pinch override range overflow")?;
   let frozen = events.get(first..end).context("trusted macOS image pinch/multipointer override is truncated")?;
   let base_us = frozen[0].at_us;
   for step in 0..=8_usize
   {
      let at_us = base_us + step as u64 * 250_000;
      let operation = if step == 0 {TraceOperation::PointerDown} else if step == 8 {TraceOperation::PointerUp} else {TraceOperation::PointerMove};
      let first_x = 375_000 - step as i32 * 31_250;
      let second_x = 625_000 + step as i32 * 31_250;
      ensure!(exact_pointer_event(&frozen[step * 2], at_us, operation, 1, first_x, 500_000), "trusted macOS image pinch override differs from frozen event {}", first + step * 2);
      ensure!(exact_pointer_event(&frozen[step * 2 + 1], at_us, operation, 2, second_x, 500_000), "trusted macOS image pinch override differs from frozen event {}", first + step * 2 + 1);
   }
   Ok((MacOsTrustedInputCommand::ImagePinchOverride {
      first_event_index: first,
      last_event_index: first + IMAGE_PINCH_OVERRIDE_EVENT_COUNT - 1,
      target: String::from("image.zoom"),
      start_x_millionths: 0,
      start_y_millionths: 500_000,
      end_x_millionths: 1_000_000,
      end_y_millionths: 500_000,
      duration_us: 2_000_000,
   }, end))
}

fn compile_navigation_cancel_override(events: &[TraceEvent], first: usize) -> Result<(MacOsTrustedInputCommand, usize)>
{
   let end = first.checked_add(NAVIGATION_CANCEL_OVERRIDE_EVENT_COUNT).context("trusted macOS navigation cancellation override range overflow")?;
   let frozen = events.get(first..end).context("trusted macOS navigation cancellation override is truncated")?;
   let base_us = frozen[0].at_us;
   ensure!(exact_pointer_event(&frozen[0], base_us, TraceOperation::PointerDown, 1, 950_000, 500_000), "trusted macOS navigation cancellation override differs from frozen event {first}");
   ensure!(exact_pointer_event(&frozen[1], base_us + 250_000, TraceOperation::PointerMove, 1, 750_000, 500_000), "trusted macOS navigation cancellation override differs from frozen event {}", first + 1);
   ensure!(exact_pointer_event(&frozen[2], base_us + 500_000, TraceOperation::PointerMove, 1, 500_000, 500_000), "trusted macOS navigation cancellation override differs from frozen event {}", first + 2);
   ensure!(exact_pointer_event(&frozen[3], base_us + 750_000, TraceOperation::PointerCancel, 1, 500_000, 500_000), "trusted macOS navigation cancellation override differs from frozen event {}", first + 3);
   let consequence = &frozen[4];
   ensure!(consequence.at_us == base_us + 1_000_000
      && consequence.op == TraceOperation::Navigate
      && consequence.pointer.is_none()
      && consequence.x_millionths.is_none()
      && consequence.y_millionths.is_none()
      && consequence.delta_x_millionths.is_none()
      && consequence.delta_y_millionths.is_none()
      && consequence.target.as_deref() == Some("navigation:cancel")
      && consequence.value.is_none()
      && consequence.state_id.as_deref() == Some("navigation:list-restored"), "trusted macOS navigation cancellation override differs from frozen event {}", first + 4);
   Ok((MacOsTrustedInputCommand::NavigationInteractiveCancelOverride {
      first_event_index: first,
      last_event_index: first + NAVIGATION_CANCEL_OVERRIDE_EVENT_COUNT - 1,
      target: String::from("navigation.table"),
      start_x_millionths: 950_000,
      start_y_millionths: 500_000,
      end_x_millionths: 500_000,
      end_y_millionths: 500_000,
      duration_us: 750_000,
   }, end))
}

fn exact_pointer_event(event: &TraceEvent, at_us: u64, operation: TraceOperation, pointer: u32, x_millionths: i32, y_millionths: i32) -> bool
{
   event.at_us == at_us
      && event.op == operation
      && event.pointer == Some(pointer)
      && event.x_millionths == Some(x_millionths)
      && event.y_millionths == Some(y_millionths)
      && event.delta_x_millionths.is_none()
      && event.delta_y_millionths.is_none()
      && event.target.is_none()
      && event.value.is_none()
      && event.state_id.is_none()
}

fn compile_pointer_sequence(events: &[TraceEvent], first: usize) -> Result<(MacOsTrustedInputCommand, usize)>
{
   let down = &events[first];
   let pointer = down.pointer.with_context(|| format!("trusted macOS pointer-down event {first} has no pointer identity"))?;
   let start_x = down.x_millionths.with_context(|| format!("trusted macOS pointer-down event {first} has no x coordinate"))?;
   let start_y = down.y_millionths.with_context(|| format!("trusted macOS pointer-down event {first} has no y coordinate"))?;
   let mut moved = false;
   let mut index = first + 1;
   loop
   {
      let Some(event) = events.get(index) else
      {
         bail!("trusted macOS pointer-down event {first} has no paired pointer-up");
      };
      match event.op
      {
         TraceOperation::PointerDown =>
         {
            bail!("trusted macOS pointer sequence at event {first} activates multiple pointers; pinch/multipointer input is unsupported by public macOS XCUI");
         }
         TraceOperation::PointerCancel =>
         {
            bail!("trusted macOS pointer sequence at event {first} uses pointer-cancel, which public XCUI cannot synthesize faithfully");
         }
         TraceOperation::PointerMove | TraceOperation::PointerUp =>
         {
            ensure!(event.pointer == Some(pointer), "trusted macOS pointer sequence at event {first} changes pointer identity");
            if event.op == TraceOperation::PointerMove
            {
               moved = true;
               index += 1;
               continue;
            }
            let end_x = event.x_millionths.with_context(|| format!("trusted macOS pointer-up event {index} has no x coordinate"))?;
            let end_y = event.y_millionths.with_context(|| format!("trusted macOS pointer-up event {index} has no y coordinate"))?;
            let mut next = index + 1;
            if !moved && events.get(next).is_some_and(|event| event.op == TraceOperation::Navigate)
            {
               let consequence = &events[next];
               ensure!(consequence.target.as_deref() != Some("navigation:cancel"), "trusted macOS pointer click at event {first} produces interactive cancellation, which public XCUI cannot synthesize faithfully");
               next += 1;
            }
            if !moved && start_x == end_x && start_y == end_y
            {
               let target = down.target.clone().or_else(|| event.target.clone()).with_context(|| format!("trusted macOS pointer click at event {first} has no target"))?;
               return Ok((MacOsTrustedInputCommand::Click {
                  first_event_index: first,
                  last_event_index: next - 1,
                  target,
                  x_millionths: Some(start_x),
                  y_millionths: Some(start_y),
               }, next));
            }
            let duration_us = event.at_us.checked_sub(down.at_us).with_context(|| format!("trusted macOS pointer sequence at event {first} moves backward in time"))?;
            ensure!(duration_us > 0, "trusted macOS pointer drag at event {first} has zero duration");
            return Ok((MacOsTrustedInputCommand::SinglePointerDrag {
               first_event_index: first,
               last_event_index: index,
               pointer,
               start_x_millionths: start_x,
               start_y_millionths: start_y,
               end_x_millionths: end_x,
               end_y_millionths: end_y,
               duration_us,
            }, index + 1));
         }
         _ => bail!("trusted macOS pointer sequence at event {first} is interrupted before pointer-up by {:?}", event.op),
      }
   }
}

fn required_target(event: &TraceEvent, index: usize, label: &str) -> Result<String>
{
   event.target.clone().filter(|target| !target.is_empty()).with_context(|| format!("trusted macOS {label} event {index} has no target"))
}

fn text_value(event: &TraceEvent, index: usize, label: &str) -> Result<String>
{
   match &event.value
   {
      Some(TraceValue::Text(value)) if !value.is_empty() => Ok(value.clone()),
      _ => bail!("trusted macOS {label} event {index} has no nonempty text value"),
   }
}

fn read_verified_artifact(root: &Path, identity: &ArtifactIdentity) -> Result<Vec<u8>>
{
   ensure!(!identity.path.is_empty() && Path::new(&identity.path).components().all(|component| matches!(component, Component::Normal(_))), "trusted-input artifact path is unsafe: {}", identity.path);
   let path = root.join(&identity.path);
   let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
   ensure!(format!("{:x}", Sha256::digest(&bytes)) == identity.sha256, "trusted-input artifact hash mismatch: {}", identity.path);
   Ok(bytes)
}

#[cfg(test)]
mod tests
{
   use oxide_benchmark_spec::{AppleCampaignEvidenceRole, AppleCampaignPassRole, ComparisonOrder, DecimalU64, ScenarioPhase};
   use serde_json::json;

   use super::*;

   #[test]
   fn canonical_receipt_triplet_materializes_relative_manifest()
   {
      let fixture = Fixture::new();
      let identity = materialize_macos_trusted_input_receipt_manifest(&fixture.directory, &fixture.plan, &fixture.session, &fixture.generation, &fixture.preflight).expect("trusted-input receipt manifest");
      assert_eq!(identity.path, "Runs/run/chunk/pass/pack/0/native.trusted-input.manifest.json");
      let bytes = fs::read(fixture.root.path().join(&identity.path)).expect("manifest bytes");
      assert_eq!(identity.sha256, sha256(&bytes));
      let manifest: MacOsTrustedInputReceiptManifest = serde_json::from_slice(&bytes).expect("manifest JSON");
      assert_eq!(manifest.expected_command_count, 1);
      assert_eq!(manifest.receipts[0].state_generation_before, 4);
      assert_eq!(manifest.receipts[0].state_generation_after, 6);
   }

   #[test]
   fn missing_or_extra_receipt_artifacts_fail_closed()
   {
      let missing = Fixture::new();
      fs::remove_file(missing.directory.join("native.trusted-input.0.application.json")).expect("remove application receipt");
      assert!(materialize_macos_trusted_input_receipt_manifest(&missing.directory, &missing.plan, &missing.session, &missing.generation, &missing.preflight).expect_err("missing receipt must fail").to_string().contains("missing expected artifacts"));

      let extra = Fixture::new();
      fs::write(extra.directory.join("native.trusted-input.1.request.json"), b"{}").expect("extra receipt");
      assert!(materialize_macos_trusted_input_receipt_manifest(&extra.directory, &extra.plan, &extra.session, &extra.generation, &extra.preflight).expect_err("extra receipt must fail").to_string().contains("extra or duplicate"));
   }

   #[test]
   fn broken_hash_link_and_generation_transition_fail_closed()
   {
      let broken_link = Fixture::new();
      broken_link.write_application("0".repeat(64), 4, 6, 30);
      assert!(materialize_macos_trusted_input_receipt_manifest(&broken_link.directory, &broken_link.plan, &broken_link.session, &broken_link.generation, &broken_link.preflight).expect_err("broken controller link must fail").to_string().contains("application receipt"));

      let broken_generation = Fixture::new();
      let controller_sha256 = sha256(&fs::read(broken_generation.directory.join("native.trusted-input.0.controller.json")).expect("controller receipt"));
      broken_generation.write_application(controller_sha256, 4, 4, 30);
      assert!(materialize_macos_trusted_input_receipt_manifest(&broken_generation.directory, &broken_generation.plan, &broken_generation.session, &broken_generation.generation, &broken_generation.preflight).expect_err("non-advancing generation must fail").to_string().contains("application receipt"));
   }

   #[test]
   fn mismatched_raw_event_range_and_family_fail_closed()
   {
      let broken_range = Fixture::new();
      let controller_sha256 = sha256(&fs::read(broken_range.directory.join("native.trusted-input.0.controller.json")).expect("controller receipt"));
      broken_range.write_application_evidence(controller_sha256, 1, 1, &["mouse"], &[1]);
      assert!(materialize_macos_trusted_input_receipt_manifest(&broken_range.directory, &broken_range.plan, &broken_range.session, &broken_range.generation, &broken_range.preflight).expect_err("wrong event range must fail").to_string().contains("application receipt"));

      let broken_family = Fixture::new();
      let controller_sha256 = sha256(&fs::read(broken_family.directory.join("native.trusted-input.0.controller.json")).expect("controller receipt"));
      broken_family.write_application_evidence(controller_sha256, 0, 0, &["key"], &[10]);
      assert!(materialize_macos_trusted_input_receipt_manifest(&broken_family.directory, &broken_family.plan, &broken_family.session, &broken_family.generation, &broken_family.preflight).expect_err("wrong raw event family must fail").to_string().contains("application receipt"));
   }

   #[test]
   fn misordered_cross_process_timestamps_fail_closed()
   {
      let fixture = Fixture::new();
      let controller_sha256 = sha256(&fs::read(fixture.directory.join("native.trusted-input.0.controller.json")).expect("controller receipt"));
      fixture.write_application(controller_sha256, 4, 6, 50);
      assert!(materialize_macos_trusted_input_receipt_manifest(&fixture.directory, &fixture.plan, &fixture.session, &fixture.generation, &fixture.preflight).expect_err("late application receipt must fail").to_string().contains("timestamps are misordered"));
   }

   #[test]
   fn iteration_timing_expands_the_exact_session_command_stream()
   {
      let phase = ScenarioPhase {id: String::from("cycles"), measured: true, duration_ms: Some(200), trace: None};
      let event = |at_us| TraceEvent {
         at_us,
         op: TraceOperation::Wheel,
         pointer: None,
         x_millionths: None,
         y_millionths: None,
         delta_x_millionths: Some(0),
         delta_y_millionths: Some(1),
         target: Some(String::from("list")),
         value: None,
         state_id: None,
      };
      let events = vec![event(0), event(100_000)];
      let phases = vec![(&phase, events.clone(), 200_000)];
      let timing = AppleCampaignScenarioTimingSpec {
         scenario_id: String::from("scenario"),
         setup_seconds: 1,
         warmup_seconds: 1,
         measurement: AppleCampaignMeasurementTimingSpec::Iterations {
            phase_id: String::from("cycles"),
            source_iteration_count: 2,
            iteration_count: 4,
            occupied_seconds: 1,
         },
      };
      let expanded = expand_timed_trace_events(&phases, "cycles", &events, 200_000, Some(&timing)).expect("expanded events");
      assert_eq!(expanded.iter().map(|event| event.at_us).collect::<Vec<_>>(), [0, 250_000, 500_000, 750_000]);
      let plan = compile_macos_trusted_input_trace(&expanded).expect("expanded trusted-input plan");
      assert_eq!(plan.commands.len(), 4);
      assert!(matches!(plan.commands[3], MacOsTrustedInputCommand::Wheel {event_index: 3, ..}));
   }

   #[test]
   fn current_chat_scenario_preflight_includes_the_typed_select_replace_command()
   {
      let spec_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks/comparative/specs/v1");
      let path = Path::new("scenarios/chat.live-update.json");
      let bytes = fs::read(spec_root.join(path)).expect("chat scenario");
      let bindings = vec![(String::from("chat.live-update"), ArtifactIdentity {
         path: path.to_string_lossy().into_owned(),
         sha256: sha256(&bytes),
      })];
      let preflight = preflight_macos_trusted_input_scenarios(&spec_root, &bindings, None, false).expect("chat trusted-input preflight");
      assert!(matches!(preflight.as_slice(), [scenario] if scenario.commands.iter().any(|command| matches!(command, MacOsTrustedInputCommand::SelectReplace {first_event_index, last_event_index, ..} if *last_event_index == *first_event_index + 1))));
   }

   #[test]
   fn non_claim_qualification_policy_classifies_lifecycle_as_an_application_stimulus()
   {
      let event = TraceEvent {
         at_us: 0,
         op: TraceOperation::Foreground,
         pointer: None,
         x_millionths: None,
         y_millionths: None,
         delta_x_millionths: None,
         delta_y_millionths: None,
         target: Some(String::from("application:lifecycle")),
         value: None,
         state_id: None,
      };
      let plan = compile_macos_trusted_input_trace_with_policy(&[event], true).expect("non-claim lifecycle stimulus");
      assert!(plan.commands.is_empty());
      assert!(matches!(plan.application_stimuli.as_slice(), [stimulus] if stimulus.event_index == 0 && stimulus.operation == TraceOperation::Foreground));
   }

   #[test]
   fn current_override_scenarios_preflight_with_full_trace_ranges()
   {
      let spec_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks/comparative/specs/v1");
      for (scenario_id, file, range) in [("image.decode-zoom", "image.decode-zoom.json", 17), ("navigation.modal", "navigation.modal.json", 4)]
      {
         let path = Path::new("scenarios").join(file);
         let bytes = fs::read(spec_root.join(&path)).expect("override scenario");
         let bindings = vec![(String::from(scenario_id), ArtifactIdentity {path: path.to_string_lossy().into_owned(), sha256: sha256(&bytes)})];
         let preflight = preflight_macos_trusted_input_scenarios(&spec_root, &bindings, None, false).expect("override trusted-input preflight");
         assert!(matches!(preflight.as_slice(), [scenario] if scenario.commands.iter().any(|command| match command
         {
            MacOsTrustedInputCommand::ImagePinchOverride {first_event_index, last_event_index, ..}
            | MacOsTrustedInputCommand::NavigationInteractiveCancelOverride {first_event_index, last_event_index, ..} => *last_event_index == *first_event_index + range,
            _ => false,
         })));
      }
   }

   #[test]
   fn override_commands_materialize_to_exact_swift_mirrors_and_mouse_drag_evidence()
   {
      let command = MacOsTrustedInputCommand::ImagePinchOverride {
         first_event_index: 0,
         last_event_index: 17,
         target: String::from("image.zoom"),
         start_x_millionths: 0,
         start_y_millionths: 500_000,
         end_x_millionths: 1_000_000,
         end_y_millionths: 500_000,
         duration_us: 2_000_000,
      };
      let mirror = swift_command(&command);
      assert_eq!(mirror.kind, SwiftCommandKind::ImagePinchOverride);
      assert_eq!(mirror.target.as_deref(), Some("image.zoom"));
      assert_eq!((mirror.first_event_index, mirror.last_event_index), (0, 17));
      assert_eq!((mirror.start_x_millionths, mirror.end_x_millionths), (Some(0), Some(1_000_000)));
      assert_eq!(dispatch_path(&command), "XCUICoordinate.click-drag");
      assert_eq!(raw_event_identity(&command), (&["mouse"][..], &[6][..]));

      let navigation = MacOsTrustedInputCommand::NavigationInteractiveCancelOverride {
         first_event_index: 0,
         last_event_index: 4,
         target: String::from("navigation.table"),
         start_x_millionths: 950_000,
         start_y_millionths: 500_000,
         end_x_millionths: 500_000,
         end_y_millionths: 500_000,
         duration_us: 750_000,
      };
      let mirror = swift_command(&navigation);
      assert_eq!(mirror.kind, SwiftCommandKind::NavigationInteractiveCancelOverride);
      assert_eq!((mirror.first_event_index, mirror.last_event_index), (0, 4));
      assert_eq!(dispatch_path(&navigation), "XCUICoordinate.click-drag-mouseUp-cancel");
      assert_eq!(raw_event_identity(&navigation), (&["mouse"][..], &[6][..]));
   }

   struct Fixture
   {
      root: tempfile::TempDir,
      directory: std::path::PathBuf,
      plan: MacOsCampaignPlan,
      session: MacOsCampaignSession,
      generation: String,
      preflight: Vec<MacOsTrustedInputScenarioPreflight>,
      identity: serde_json::Value,
      request_sha256: String,
   }

   impl Fixture
   {
      fn new() -> Self
      {
         let root = tempfile::tempdir().expect("trusted-input fixture root");
         let generation = sha256(b"generation");
         let session = MacOsCampaignSession {
            chunk_id: String::from("chunk"),
            pass_id: String::from("pass"),
            pack_id: String::from("pack"),
            pass_role: AppleCampaignPassRole::Primary,
            evidence_role: AppleCampaignEvidenceRole::ClaimBearing,
            collector: None,
            launch_class: None,
            timing: None,
            pair_index: 0,
            order: ComparisonOrder::Ab,
            side: ComparisonSide::Native,
            max_occupied_seconds: 1,
            scale_overlay: None,
         };
         let plan = MacOsCampaignPlan {
            schema_version: 2,
            run_id: String::from("run"),
            plan_sha256: sha256(b"plan"),
            seed: DecimalU64(1),
            plan_resource_path: None,
            sessions: vec![session.clone()],
         };
         let directory = root.path().join("Runs/run/chunk/pass/pack/0");
         fs::create_dir_all(&directory).expect("trusted-input directory");
         let identity = json!({
            "schemaVersion": 1,
            "runID": plan.run_id,
            "planSHA256": plan.plan_sha256,
            "chunkID": session.chunk_id,
            "passID": session.pass_id,
            "packID": session.pack_id,
            "pairIndex": 0,
            "side": "native",
            "generation": generation,
            "scenarioID": "scenario",
            "commandSequence": 0
         });
         let command = MacOsTrustedInputCommand::Click {
            first_event_index: 0,
            last_event_index: 0,
            target: String::from("target"),
            x_millionths: Some(500_000),
            y_millionths: Some(500_000),
         };
         let request = json!({
            "identity": identity,
            "command": {
               "kind": "click",
               "firstEventIndex": 0,
               "lastEventIndex": 0,
               "target": "target",
               "startXMillionths": 500000,
               "startYMillionths": 500000
            },
            "descriptorPreparedTimestamp": 10,
            "durable": true
         });
         let request_bytes = canonical(&request);
         fs::write(directory.join("native.trusted-input.0.request.json"), &request_bytes).expect("request");
         let request_sha256 = sha256(&request_bytes);
         let controller = json!({
            "identity": identity,
            "requestSHA256": request_sha256,
            "dispatchPath": "XCUIElement.click",
            "dispatchStartedTimestamp": 20,
            "dispatchCompletedTimestamp": 40,
            "applicationWasForeground": true,
            "durable": true,
            "complete": true
         });
         let controller_bytes = canonical(&controller);
         fs::write(directory.join("native.trusted-input.0.controller.json"), &controller_bytes).expect("controller receipt");
         let preflight = vec![MacOsTrustedInputScenarioPreflight {
            scenario_id: String::from("scenario"),
            command_count: 1,
            application_stimulus_count: 0,
            commands: vec![command],
         }];
         let fixture = Self {root, directory, plan, session, generation, preflight, identity, request_sha256};
         fixture.write_application(sha256(&controller_bytes), 4, 6, 30);
         fixture
      }

      fn write_application(&self, controller_receipt_sha256: String, state_generation_before: u64, state_generation_after: u64, application_received_timestamp: u64)
      {
         self.write_application_receipt(controller_receipt_sha256, state_generation_before, state_generation_after, application_received_timestamp, 0, 0, &["mouse"], &[1]);
      }

      fn write_application_evidence(&self, controller_receipt_sha256: String, first_event_index: usize, last_event_index: usize, raw_event_families: &[&str], raw_event_types: &[u64])
      {
         self.write_application_receipt(controller_receipt_sha256, 4, 6, 30, first_event_index, last_event_index, raw_event_families, raw_event_types);
      }

      fn write_application_receipt(&self, controller_receipt_sha256: String, state_generation_before: u64, state_generation_after: u64, application_received_timestamp: u64, first_event_index: usize, last_event_index: usize, raw_event_families: &[&str], raw_event_types: &[u64])
      {
         let application = json!({
            "identity": self.identity,
            "requestSHA256": self.request_sha256,
            "controllerReceiptSHA256": controller_receipt_sha256,
            "firstEventIndex": first_event_index,
            "lastEventIndex": last_event_index,
            "rawEventFamilies": raw_event_families,
            "rawEventTypes": raw_event_types,
            "rawEventTimestamp": 1.5,
            "rawEventLocationX": 10.0,
            "rawEventLocationY": 20.0,
            "windowNumber": 1,
            "applicationReceivedTimestamp": application_received_timestamp,
            "applicationWasForeground": true,
            "stateGenerationBefore": state_generation_before,
            "stateGenerationAfter": state_generation_after,
            "durable": true,
            "complete": true
         });
         fs::write(self.directory.join("native.trusted-input.0.application.json"), canonical(&application)).expect("application receipt");
      }
   }

   fn canonical(value: &serde_json::Value) -> Vec<u8>
   {
      serde_json::to_vec(value).expect("canonical fixture JSON")
   }
}
