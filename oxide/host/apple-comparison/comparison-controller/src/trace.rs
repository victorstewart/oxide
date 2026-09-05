use anyhow::{bail, Context, Result};
use roxmltree::Document;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MACOS_PRESENTATION_CORRELATION_AVAILABILITY: &str = "available-correlated-frame-lifetime-calibration-pending";
pub const MACOS_PRESENTATION_CORRELATION_CALIBRATION: &str = "pending-frame-lifetime-endpoint-signpost-to-swap-and-trace-overhead-calibration";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsPresentationCorrelationArtifact
{
   pub schema_version: u32,
   pub availability: String,
   pub calibration_status: String,
   pub correlations: Vec<MacOsPresentationCorrelation>,
   pub uncorrelated_visual_generations: Vec<MacOsUncorrelatedVisualGeneration>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsPresentationCorrelation
{
   pub process: String,
   pub scenario_index: u64,
   pub input_generation: u64,
   pub visual_generation: u64,
   pub display_opportunity_ns: Option<u64>,
   pub input_received_ns: u64,
   pub visual_generation_ns: u64,
   pub update_start_ns: u64,
   pub update_end_ns: u64,
   pub update_selection: String,
   pub display: String,
   pub swap_id: u64,
   pub frame_lifetime_start_ns: u64,
   pub frame_lifetime_end_ns: u64,
   pub candidate_input_to_frame_lifetime_end_ns: u64,
   pub candidate_visual_to_frame_lifetime_end_ns: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsUncorrelatedVisualGeneration
{
   pub process: String,
   pub scenario_index: u64,
   pub input_generation: u64,
   pub visual_generation: u64,
   pub display_opportunity_ns: Option<u64>,
   pub input_received_ns: u64,
   pub visual_generation_ns: u64,
   pub reason: String,
   pub superseded_by_scenario_index: u64,
   pub superseded_by_visual_generation: u64,
   pub superseded_by_visual_generation_ns: u64,
   pub next_exact_process_update_start_ns: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsLaunchClockAnchorInterval
{
   pub id: u32,
   pub before_ticks: u64,
   pub after_ticks: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsLaunchPresentationCorrelation
{
   pub schema_version: u32,
   pub pid: u32,
   pub process: String,
   pub first_visual_generation_trace_ns: u64,
   pub first_attributed_present_proxy_trace_ns: u64,
   pub input_received_trace_ns: u64,
   pub response_visual_generation_trace_ns: u64,
   pub response_attributed_present_proxy_trace_ns: u64,
   pub first_visual_generation_ticks: u64,
   pub first_attributed_present_proxy_ticks: u64,
   pub input_received_ticks: u64,
   pub response_visual_generation_ticks: u64,
   pub response_attributed_present_proxy_ticks: u64,
   pub clock_mapping_max_uncertainty_ticks: u64,
   pub exact_pid_filtered: bool,
   pub calibration_status: String,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TraceCell
{
   raw: Option<String>,
   formatted: Option<String>,
   sentinel: bool,
}

impl TraceCell
{
   pub(crate) fn display(&self) -> Option<&str>
   {
      self.formatted.as_deref().or(self.raw.as_deref())
   }

   pub(crate) fn raw_u64(&self) -> Option<u64>
   {
      self.raw.as_deref()?.parse::<u64>().ok()
   }
}

pub(crate) type TraceRow = BTreeMap<String, TraceCell>;

pub(crate) fn cell_is_sentinel(row: &TraceRow, mnemonic: &str) -> bool
{
   row.get(mnemonic).is_some_and(|cell| cell.sentinel)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MarkerKind
{
   DisplayOpportunity,
   InputReceived,
   VisualGeneration,
}

#[derive(Clone, Debug)]
struct Marker
{
   kind: MarkerKind,
   process: String,
   scenario_index: u64,
   generation: u64,
   time_ns: u64,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Update
{
   process: String,
   display: String,
   swap_id: u64,
   start_ns: u64,
   end_ns: u64,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct FrameLifetime
{
   display: String,
   swap_id: u64,
   start_ns: u64,
   end_ns: u64,
}

enum UpdateSelection<'a>
{
   Correlated(&'a Update, &'static str),
   Superseded(&'a Marker, u64),
}

pub fn correlate_macos_presentation_trace(signposts_xml: &str, updates_xml: &str, frame_lifetimes_xml: &str) -> Result<MacOsPresentationCorrelationArtifact>
{
   let markers = parse_markers(&parse_trace_rows(signposts_xml, "os-signpost")?)?;
   let updates = parse_updates(&parse_trace_rows(updates_xml, "hitches-updates")?)?;
   let frame_lifetimes = parse_frame_lifetimes(&parse_trace_rows(frame_lifetimes_xml, "hitches-frame-lifetimes")?)?;
   let mut correlations = Vec::new();
   let mut uncorrelated_visual_generations = Vec::new();
   let mut visual_keys = BTreeSet::new();
   for visual in markers.iter().filter(|marker| marker.kind == MarkerKind::VisualGeneration)
   {
      if visual.generation == 0
      {
         bail!("VisualGeneration scenario {} in {} has generation zero", visual.scenario_index, visual.process);
      }
      let visual_key = (visual.process.as_str(), visual.scenario_index, visual.generation);
      if !visual_keys.insert(visual_key)
      {
         bail!("duplicate VisualGeneration scenario {} generation {} in {}", visual.scenario_index, visual.generation, visual.process);
      }
      let input_generation = visual.generation - 1;
      let input = unique_marker(
         &markers,
         MarkerKind::InputReceived,
         &visual.process,
         visual.scenario_index,
         input_generation,
      )?;
      if input.time_ns > visual.time_ns
      {
         bail!("InputReceived follows VisualGeneration for scenario {} generation {} in {}", visual.scenario_index, visual.generation, visual.process);
      }
      let display_opportunity = markers.iter()
         .filter(|marker| {
            marker.kind == MarkerKind::DisplayOpportunity
               && marker.process == visual.process
               && marker.scenario_index == visual.scenario_index
               && marker.generation == input_generation
               && marker.time_ns <= input.time_ns
         })
         .max_by_key(|marker| marker.time_ns);
      let update_selection = select_update(&updates, &markers, visual)?;
      let (update, update_selection) = match update_selection
      {
         UpdateSelection::Correlated(update, selection) => (update, selection),
         UpdateSelection::Superseded(superseding_visual, next_update_start_ns) =>
         {
            uncorrelated_visual_generations.push(MacOsUncorrelatedVisualGeneration {
               process: visual.process.clone(),
               scenario_index: visual.scenario_index,
               input_generation,
               visual_generation: visual.generation,
               display_opportunity_ns: display_opportunity.map(|marker| marker.time_ns),
               input_received_ns: input.time_ns,
               visual_generation_ns: visual.time_ns,
               reason: String::from("superseded-before-next-exact-process-update"),
               superseded_by_scenario_index: superseding_visual.scenario_index,
               superseded_by_visual_generation: superseding_visual.generation,
               superseded_by_visual_generation_ns: superseding_visual.time_ns,
               next_exact_process_update_start_ns: next_update_start_ns,
            });
            continue;
         }
      };
      let matching_frame_lifetimes = frame_lifetimes.iter()
         .filter(|frame| frame.swap_id == update.swap_id && frame.display == update.display)
         .collect::<Vec<_>>();
      if matching_frame_lifetimes.len() != 1
      {
         bail!(
            "expected one frame lifetime for display {} swap {}, observed {}",
            update.display,
            update.swap_id,
            matching_frame_lifetimes.len(),
         );
      }
      let frame_lifetime = matching_frame_lifetimes[0];
      if frame_lifetime.end_ns < update.start_ns || frame_lifetime.end_ns < visual.time_ns
      {
         bail!("frame lifetime ends before its correlated visual/update boundary for swap {}", update.swap_id);
      }
      correlations.push(MacOsPresentationCorrelation {
         process: visual.process.clone(),
         scenario_index: visual.scenario_index,
         input_generation,
         visual_generation: visual.generation,
         display_opportunity_ns: display_opportunity.map(|marker| marker.time_ns),
         input_received_ns: input.time_ns,
         visual_generation_ns: visual.time_ns,
         update_start_ns: update.start_ns,
         update_end_ns: update.end_ns,
         update_selection: String::from(update_selection),
         display: update.display.clone(),
         swap_id: update.swap_id,
         frame_lifetime_start_ns: frame_lifetime.start_ns,
         frame_lifetime_end_ns: frame_lifetime.end_ns,
         candidate_input_to_frame_lifetime_end_ns: frame_lifetime.end_ns - input.time_ns,
         candidate_visual_to_frame_lifetime_end_ns: frame_lifetime.end_ns - visual.time_ns,
      });
   }
   if correlations.is_empty()
   {
      bail!("macOS trace has no comparison VisualGeneration markers");
   }
   correlations.sort_by(|left, right| {
      left.visual_generation_ns.cmp(&right.visual_generation_ns)
         .then_with(|| left.process.cmp(&right.process))
         .then_with(|| left.scenario_index.cmp(&right.scenario_index))
         .then_with(|| left.visual_generation.cmp(&right.visual_generation))
   });
   uncorrelated_visual_generations.sort_by(|left, right| {
      left.visual_generation_ns.cmp(&right.visual_generation_ns)
         .then_with(|| left.process.cmp(&right.process))
         .then_with(|| left.scenario_index.cmp(&right.scenario_index))
         .then_with(|| left.visual_generation.cmp(&right.visual_generation))
   });
   Ok(MacOsPresentationCorrelationArtifact {
      schema_version: 1,
      availability: String::from(MACOS_PRESENTATION_CORRELATION_AVAILABILITY),
      calibration_status: String::from(MACOS_PRESENTATION_CORRELATION_CALIBRATION),
      correlations,
      uncorrelated_visual_generations,
   })
}

pub fn correlate_macos_launch_presentation_trace(
   signposts_xml: &str,
   updates_xml: &str,
   frame_lifetimes_xml: &str,
   exact_pid: u32,
   clock_anchors: &[MacOsLaunchClockAnchorInterval],
) -> Result<MacOsLaunchPresentationCorrelation>
{
   if exact_pid == 0 || clock_anchors.len() != 2
   {
      bail!("macOS launch trace correlation requires one exact PID and two controller clock anchors");
   }
   let rows = parse_trace_rows(signposts_xml, "os-signpost")?;
   let markers = parse_markers(&rows)?;
   let updates = parse_updates(&parse_trace_rows(updates_xml, "hitches-updates")?)?;
   let frame_lifetimes = parse_frame_lifetimes(&parse_trace_rows(frame_lifetimes_xml, "hitches-frame-lifetimes")?)?;
   let process_suffix = format!(" ({})", exact_pid);
   let processes = markers.iter()
      .filter(|marker| marker.kind == MarkerKind::VisualGeneration && marker.process.ends_with(&process_suffix))
      .map(|marker| marker.process.as_str())
      .collect::<BTreeSet<_>>();
   if processes.len() != 1
   {
      bail!("macOS launch trace does not contain one exact-PID presentation-marker process");
   }
   let process = *processes.iter().next().context("missing exact-PID launch process")?;
   let first_visual = unique_marker(&markers, MarkerKind::VisualGeneration, process, 0, 1)?;
   let input = unique_marker(&markers, MarkerKind::InputReceived, process, 0, 1)?;
   let response_visual = unique_marker(&markers, MarkerKind::VisualGeneration, process, 0, 2)?;
   if first_visual.time_ns >= input.time_ns || input.time_ns > response_visual.time_ns
   {
      bail!("macOS launch trace markers are not ordered from first UI through trusted input and response generation");
   }
   let first_present = attributed_present_end(&updates, &frame_lifetimes, &markers, first_visual)?;
   let response_present = attributed_present_end(&updates, &frame_lifetimes, &markers, response_visual)?;
   if first_present >= input.time_ns || response_present < response_visual.time_ns
   {
      bail!("macOS launch attributed presentation endpoints do not bracket the trusted interaction");
   }
   let trace_anchors = parse_launch_clock_anchor_trace_times(&rows)?;
   let map = LaunchClockMap::new(clock_anchors, &trace_anchors)?;
   Ok(MacOsLaunchPresentationCorrelation {
      schema_version: 1,
      pid: exact_pid,
      process: String::from(process),
      first_visual_generation_trace_ns: first_visual.time_ns,
      first_attributed_present_proxy_trace_ns: first_present,
      input_received_trace_ns: input.time_ns,
      response_visual_generation_trace_ns: response_visual.time_ns,
      response_attributed_present_proxy_trace_ns: response_present,
      first_visual_generation_ticks: map.ticks(first_visual.time_ns)?,
      first_attributed_present_proxy_ticks: map.ticks(first_present)?,
      input_received_ticks: map.ticks(input.time_ns)?,
      response_visual_generation_ticks: map.ticks(response_visual.time_ns)?,
      response_attributed_present_proxy_ticks: map.ticks(response_present)?,
      clock_mapping_max_uncertainty_ticks: map.max_uncertainty_ticks,
      exact_pid_filtered: true,
      calibration_status: String::from("pending-frame-lifetime-endpoint-and-trace-overhead-calibration"),
   })
}

fn attributed_present_end(updates: &[Update], frame_lifetimes: &[FrameLifetime], markers: &[Marker], visual: &Marker) -> Result<u64>
{
   let update = match select_update(updates, markers, visual)?
   {
      UpdateSelection::Correlated(update, _) => update,
      UpdateSelection::Superseded(_, _) => bail!("macOS launch visual generation was superseded before an exact-process update"),
   };
   let matching = frame_lifetimes.iter().filter(|frame| frame.display == update.display && frame.swap_id == update.swap_id).collect::<Vec<_>>();
   if matching.len() != 1 || matching[0].end_ns < visual.time_ns
   {
      bail!("macOS launch update has no unique attributed frame-lifetime endpoint");
   }
   Ok(matching[0].end_ns)
}

fn parse_launch_clock_anchor_trace_times(rows: &[TraceRow]) -> Result<BTreeMap<u32, u64>>
{
   let mut anchors = BTreeMap::new();
   for row in rows
   {
      if cell_display(row, "subsystem") != Some("com.oxide.comparison")
         || cell_display(row, "category") != Some("ClockMap")
         || cell_display(row, "event-type") != Some("Event")
         || cell_display(row, "name") != Some("ClockAnchor")
      {
         continue;
      }
      let id = u32::try_from(parse_message_field(required_display(row, "message", "os-signpost")?, "anchor")?).context("clock anchor ID exceeds u32")?;
      let trace_ns = required_u64(row, "time", "os-signpost")?;
      if anchors.insert(id, trace_ns).is_some()
      {
         bail!("duplicate macOS launch trace clock anchor {}", id);
      }
   }
   Ok(anchors)
}

struct LaunchClockMap
{
   first_trace_ns: u64,
   first_midpoint_ticks: f64,
   ticks_per_ns: f64,
   max_uncertainty_ticks: u64,
}

impl LaunchClockMap
{
   fn new(intervals: &[MacOsLaunchClockAnchorInterval], trace_times: &BTreeMap<u32, u64>) -> Result<Self>
   {
      let first = &intervals[0];
      let second = &intervals[1];
      if first.id != 1
         || second.id != 2
         || first.before_ticks == 0
         || first.after_ticks < first.before_ticks
         || second.before_ticks <= first.after_ticks
         || second.after_ticks < second.before_ticks
      {
         bail!("macOS launch controller clock-anchor intervals are missing or misordered");
      }
      let first_trace_ns = *trace_times.get(&first.id).context("trace has no first controller clock anchor")?;
      let second_trace_ns = *trace_times.get(&second.id).context("trace has no second controller clock anchor")?;
      if second_trace_ns <= first_trace_ns
      {
         bail!("macOS launch trace clock anchors are not increasing");
      }
      let first_midpoint_ticks = midpoint(first.before_ticks, first.after_ticks)?;
      let second_midpoint_ticks = midpoint(second.before_ticks, second.after_ticks)?;
      if second_midpoint_ticks <= first_midpoint_ticks
      {
         bail!("macOS launch controller clock anchor midpoints are not increasing");
      }
      let ticks_per_ns = (second_midpoint_ticks - first_midpoint_ticks) / (second_trace_ns - first_trace_ns) as f64;
      if !ticks_per_ns.is_finite() || ticks_per_ns <= 0.0
      {
         bail!("macOS launch controller-to-trace clock slope is invalid");
      }
      let first_width = first.after_ticks - first.before_ticks;
      let second_width = second.after_ticks - second.before_ticks;
      Ok(Self {
         first_trace_ns,
         first_midpoint_ticks,
         ticks_per_ns,
         max_uncertainty_ticks: first_width.max(second_width).saturating_add(2),
      })
   }

   fn ticks(&self, trace_ns: u64) -> Result<u64>
   {
      let delta = trace_ns as f64 - self.first_trace_ns as f64;
      let ticks = self.first_midpoint_ticks + delta * self.ticks_per_ns;
      if !ticks.is_finite() || ticks <= 0.0 || ticks > u64::MAX as f64
      {
         bail!("macOS launch trace timestamp cannot be mapped into mach-continuous ticks");
      }
      Ok(ticks.round() as u64)
   }
}

fn midpoint(first: u64, second: u64) -> Result<f64>
{
   let delta = second.checked_sub(first).context("clock anchor interval is reversed")?;
   Ok(first as f64 + delta as f64 * 0.5)
}

fn select_update<'a>(updates: &'a [Update], markers: &'a [Marker], visual: &Marker) -> Result<UpdateSelection<'a>>
{
   let containing = updates.iter()
      .filter(|update| update.process == visual.process && update.start_ns <= visual.time_ns && update.end_ns >= visual.time_ns)
      .collect::<Vec<_>>();
   if containing.len() > 1
   {
      bail!("multiple exact-process updates contain VisualGeneration scenario {} generation {}", visual.scenario_index, visual.generation);
   }
   if let Some(update) = containing.first()
   {
      let next_visual_inside = markers.iter()
         .filter(|marker| marker.kind == MarkerKind::VisualGeneration && marker.process == visual.process && marker.time_ns > visual.time_ns && marker.time_ns <= update.end_ns)
         .min_by_key(|marker| marker.time_ns);
      if let Some(next_visual) = next_visual_inside
      {
         return Ok(UpdateSelection::Superseded(next_visual, update.start_ns));
      }
      return Ok(UpdateSelection::Correlated(update, "contains-visual-generation"));
   }
   let next_start = updates.iter()
      .filter(|update| update.process == visual.process && update.start_ns > visual.time_ns)
      .map(|update| update.start_ns)
      .min()
      .with_context(|| format!(
         "no exact-process update follows VisualGeneration scenario {} generation {} in {}",
         visual.scenario_index,
         visual.generation,
         visual.process,
      ))?;
   if let Some(next_visual) = markers.iter()
      .filter(|marker| marker.kind == MarkerKind::VisualGeneration && marker.process == visual.process && marker.time_ns > visual.time_ns && marker.time_ns <= next_start)
      .min_by_key(|marker| marker.time_ns)
   {
      return Ok(UpdateSelection::Superseded(next_visual, next_start));
   }
   let matching = updates.iter()
      .filter(|update| update.process == visual.process && update.start_ns == next_start)
      .collect::<Vec<_>>();
   if matching.len() != 1
   {
      bail!("multiple exact-process updates share the first start after VisualGeneration scenario {} generation {}", visual.scenario_index, visual.generation);
   }
   Ok(UpdateSelection::Correlated(matching[0], "first-exact-process-update-after-visual-generation"))
}

fn unique_marker<'a>(markers: &'a [Marker], kind: MarkerKind, process: &str, scenario_index: u64, generation: u64) -> Result<&'a Marker>
{
   let matching = markers.iter().filter(|marker| {
      marker.kind == kind
         && marker.process == process
         && marker.scenario_index == scenario_index
         && marker.generation == generation
   }).collect::<Vec<_>>();
   if matching.len() != 1
   {
      bail!(
         "expected one {:?} for scenario {} generation {} in {}, observed {}",
         kind,
         scenario_index,
         generation,
         process,
         matching.len(),
      );
   }
   Ok(matching[0])
}

fn parse_markers(rows: &[TraceRow]) -> Result<Vec<Marker>>
{
   let mut markers = Vec::new();
   for row in rows
   {
      if cell_display(row, "subsystem") != Some("com.oxide.comparison")
         || cell_display(row, "category") != Some("Presentation")
         || cell_display(row, "event-type") != Some("Event")
      {
         continue;
      }
      let kind = match cell_display(row, "name")
      {
         Some("DisplayOpportunity") => MarkerKind::DisplayOpportunity,
         Some("InputReceived") => MarkerKind::InputReceived,
         Some("VisualGeneration") => MarkerKind::VisualGeneration,
         _ => continue,
      };
      let process = required_display(row, "process", "os-signpost")?.to_string();
      let time_ns = required_u64(row, "time", "os-signpost")?;
      let message = required_display(row, "message", "os-signpost")?;
      markers.push(Marker {
         kind,
         process,
         scenario_index: parse_message_field(message, "scenario")?,
         generation: parse_message_field(message, "generation")?,
         time_ns,
      });
   }
   Ok(markers)
}

fn parse_updates(rows: &[TraceRow]) -> Result<Vec<Update>>
{
   let mut updates = rows.iter().map(|row| {
      let start_ns = required_u64(row, "start", "hitches-updates")?;
      let duration_ns = required_u64(row, "duration", "hitches-updates")?;
      if duration_ns == 0
      {
         bail!("hitches-updates row has zero duration");
      }
      Ok(Update {
         process: required_display(row, "process", "hitches-updates")?.to_string(),
         display: required_display(row, "display", "hitches-updates")?.to_string(),
         swap_id: required_u64(row, "swap-id", "hitches-updates")?,
         start_ns,
         end_ns: start_ns.checked_add(duration_ns).context("hitches-updates interval overflow")?,
      })
   }).collect::<Result<Vec<_>>>()?;
   updates.sort();
   updates.dedup();
   Ok(updates)
}

fn parse_frame_lifetimes(rows: &[TraceRow]) -> Result<Vec<FrameLifetime>>
{
   let mut frame_lifetimes = rows.iter().map(|row| {
      let start_ns = required_u64(row, "start", "hitches-frame-lifetimes")?;
      let duration_ns = required_u64(row, "duration", "hitches-frame-lifetimes")?;
      if duration_ns == 0
      {
         bail!("hitches-frame-lifetimes row has zero duration");
      }
      Ok(FrameLifetime {
         display: required_display(row, "display", "hitches-frame-lifetimes")?.to_string(),
         swap_id: required_u64(row, "swap-id", "hitches-frame-lifetimes")?,
         start_ns,
         end_ns: start_ns.checked_add(duration_ns).context("hitches-frame-lifetimes interval overflow")?,
      })
   }).collect::<Result<Vec<_>>>()?;
   frame_lifetimes.sort();
   frame_lifetimes.dedup();
   Ok(frame_lifetimes)
}

pub(crate) fn parse_message_field(message: &str, field: &str) -> Result<u64>
{
   let prefix = format!("{}=", field);
   let start = message.find(&prefix).with_context(|| format!("comparison marker message has no {} field", field))? + prefix.len();
   let tail = message[start..].trim_start();
   let digits = tail.bytes().take_while(u8::is_ascii_digit).count();
   if digits == 0
   {
      bail!("comparison marker message has no numeric {} value", field);
   }
   tail[..digits].parse::<u64>().with_context(|| format!("parsing comparison marker {} value", field))
}

pub(crate) fn required_display<'a>(row: &'a TraceRow, mnemonic: &str, schema: &str) -> Result<&'a str>
{
   cell_display(row, mnemonic).filter(|value| !value.is_empty()).with_context(|| format!("{} row has no {} value", schema, mnemonic))
}

pub(crate) fn required_u64(row: &TraceRow, mnemonic: &str, schema: &str) -> Result<u64>
{
   row.get(mnemonic).and_then(TraceCell::raw_u64).with_context(|| format!("{} row has no numeric {} value", schema, mnemonic))
}

pub(crate) fn cell_display<'a>(row: &'a TraceRow, mnemonic: &str) -> Option<&'a str>
{
   row.get(mnemonic).and_then(TraceCell::display)
}

pub(crate) fn parse_trace_rows(xml: &str, expected_schema: &str) -> Result<Vec<TraceRow>>
{
   let document = Document::parse(xml.trim()).with_context(|| format!("parsing {} xctrace XML", expected_schema))?;
   let mut rows = Vec::new();
   let mut last_columns = None::<Vec<String>>;
   let mut observed_schema = false;
   let mut references = BTreeMap::<String, TraceCell>::new();
   for node in document.descendants().filter(|node| node.has_tag_name("node"))
   {
      let columns = if let Some(schema) = node.children().find(|child| child.is_element() && child.has_tag_name("schema"))
      {
         let schema_name = schema.attribute("name").context("xctrace schema has no name")?;
         if schema_name != expected_schema
         {
            bail!("expected xctrace schema {}, observed {}", expected_schema, schema_name);
         }
         observed_schema = true;
         let columns = schema.children()
            .filter(|child| child.is_element() && child.has_tag_name("col"))
            .map(|column| {
               column.children()
                  .find(|child| child.is_element() && child.has_tag_name("mnemonic"))
                  .and_then(|mnemonic| mnemonic.text())
                  .map(str::to_string)
                  .context("xctrace column has no mnemonic")
            })
            .collect::<Result<Vec<_>>>()?;
         last_columns = Some(columns.clone());
         columns
      }
      else if let Some(columns) = last_columns.clone()
      {
         columns
      }
      else
      {
         continue;
      };
      for row_node in node.children().filter(|child| child.is_element() && child.has_tag_name("row"))
      {
         let mut row = BTreeMap::new();
         for (index, value_node) in row_node.children().filter(|child| child.is_element()).enumerate()
         {
            let mnemonic = columns.get(index).context("xctrace row has more values than its schema")?;
            if value_node.has_tag_name("sentinel")
            {
               row.insert(mnemonic.clone(), TraceCell {raw: None, formatted: None, sentinel: true});
               continue;
            }
            let cell = resolve_trace_cell(&value_node, &references)?;
            if let Some(identifier) = value_node.attribute("id")
            {
               references.insert(identifier.to_string(), cell.clone());
            }
            for descendant in value_node.descendants().filter(|descendant| descendant.is_element()).skip(1)
            {
               if let Some(identifier) = descendant.attribute("id")
               {
                  references.insert(identifier.to_string(), resolve_trace_cell(&descendant, &references)?);
               }
            }
            row.insert(mnemonic.clone(), cell);
         }
         rows.push(row);
      }
   }
   if !observed_schema
   {
      bail!("xctrace export has no {} schema", expected_schema);
   }
   if rows.is_empty()
   {
      bail!("xctrace export has no {} rows", expected_schema);
   }
   Ok(rows)
}

fn resolve_trace_cell(node: &roxmltree::Node<'_, '_>, references: &BTreeMap<String, TraceCell>) -> Result<TraceCell>
{
   let mut cell = TraceCell {
      raw: node.text().map(str::trim).filter(|value| !value.is_empty()).map(str::to_string),
      formatted: node.attribute("fmt").map(str::to_string),
      sentinel: false,
   };
   if let Some(reference) = node.attribute("ref")
   {
      let referenced = references.get(reference).with_context(|| format!("unresolved xctrace reference {}", reference))?;
      if cell.raw.is_none()
      {
         cell.raw = referenced.raw.clone();
      }
      if cell.formatted.is_none()
      {
         cell.formatted = referenced.formatted.clone();
      }
   }
   Ok(cell)
}
