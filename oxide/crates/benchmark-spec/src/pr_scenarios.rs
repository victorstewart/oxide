use anyhow::{ensure, Context, Result};
use serde::de::DeserializeOwned;
use std::fs;
use std::path::{Path, PathBuf};

use crate::acquisition::APPLE_PR_SCENARIO_IDS;
use crate::pr_fixtures::{
   validate_chat_fixture, validate_dashboard_fixture, validate_feed_fixture,
   validate_endurance_fixture, validate_image_decode_zoom_fixture, validate_navigation_fixture,
   validate_startup_fixture, ChatFixture, DashboardFixture, EnduranceFixture, FeedFixture,
   ImageDecodeZoomFixture, NavigationFixture, StartupFixture,
};
use crate::scenario::{ScenarioSpec, TraceEvent, TraceOperation, TraceValue};
use crate::validate::validate_scenario_artifacts;

pub const PR_VERTICAL_SCENARIO_IDS: &[&str] = &[
   "dashboard.mixed-static",
   "feed.variable-scroll",
   "navigation.modal",
];

struct VerticalScenarioContract
{
   id: &'static str,
   primary_metric: &'static str,
   phases: &'static [&'static str],
}

const VERTICAL_SCENARIO_CONTRACTS: &[VerticalScenarioContract] = &[
   VerticalScenarioContract {
      id: "dashboard.mixed-static",
      primary_metric: "leaf_update_to_attributed_presentation_ms",
      phases: &["setup", "prewarm", "first-mount", "clean-idle", "leaf-updates", "update-10-percent", "teardown"],
   },
   VerticalScenarioContract {
      id: "feed.variable-scroll",
      primary_metric: "frame.missed_display_opportunities_per_1000",
      phases: &["setup", "prewarm", "forward-fling", "reverse-fling", "favorite-one", "prepend-20", "settle", "teardown"],
   },
   VerticalScenarioContract {
      id: "navigation.modal",
      primary_metric: "modal_open_to_attributed_presentation_ms",
      phases: &["setup", "prewarm", "canonical-cycles", "interactive-cancel", "teardown"],
   },
];

const APPLE_PR_PRIMARY_METRICS: &[&str] = &[
   "controller_launch_request_to_first_attributed_present_proxy_ms",
   "leaf_update_to_attributed_presentation_ms",
   "frame.missed_display_opportunities_per_1000",
   "keystroke_to_attributed_presentation_ms",
   "modal_open_to_attributed_presentation_ms",
   "bytes_ready_to_first_visible_ms",
];

const STARTUP_PHASES: &[&str] = &["setup", "terminated-warm-cache", "fresh-install-first-launch", "warm-resume", "teardown"];
const CHAT_PHASES: &[&str] = &["setup", "prewarm", "prepend-50", "append-10hz", "type-100", "paste-10kib", "select-replace", "teardown"];
const IMAGE_PHASES: &[&str] = &["setup", "prewarm", "bytes-ready", "decode", "upload", "first-visible", "pan", "pinch", "teardown"];
const ENDURANCE_PHASES: &[&str] = &["setup", "prewarm", "open-close-heavy-screen", "tab-switch-heavy", "idle-animation", "recovery", "teardown"];

pub fn validate_apple_pr_scenario_set(spec_root: &Path, scenarios: &[(PathBuf, ScenarioSpec)]) -> Result<()>
{
   ensure!(scenarios.len() == APPLE_PR_SCENARIO_IDS.len(), "Apple PR has {} scenarios, expected {}", scenarios.len(), APPLE_PR_SCENARIO_IDS.len());
   for (((path, scenario), expected_id), primary_metric) in scenarios.iter().zip(APPLE_PR_SCENARIO_IDS).zip(APPLE_PR_PRIMARY_METRICS)
   {
      ensure!(scenario.id == *expected_id, "Apple PR scenario order/id differs: expected {}, observed {}", expected_id, scenario.id);
      ensure!(path.file_stem().and_then(|stem| stem.to_str()) == Some(scenario.id.as_str()), "Apple PR scenario {} filename does not match its id", scenario.id);
      ensure!(scenario.primary_metric == *primary_metric, "Apple PR scenario {} primary metric differs from the frozen workload matrix", scenario.id);
      ensure!(scenario.viewport_class == "phone-portrait" && scenario.fairness_contract.logical_viewport_width == 390 && scenario.fairness_contract.logical_viewport_height == 844, "Apple PR scenario {} does not use the frozen phone portrait viewport", scenario.id);
      ensure!(scenario.font_pack.id == "oxide-bench-fonts-v1", "Apple PR scenario {} does not use the pinned font pack", scenario.id);
      validate_scenario_artifacts(spec_root, scenario).with_context(|| format!("validating Apple PR scenario {}", scenario.id))?;
      validate_typed_fixture(spec_root, scenario)?;
      match scenario.id.as_str()
      {
         "startup.first-screen" => ensure!(phase_ids(scenario).eq(STARTUP_PHASES.iter().copied()), "startup scenario phases differ from the frozen contract"),
         "chat.live-update" => validate_dynamic_contract(scenario, CHAT_PHASES)?,
         "image.decode-zoom" => validate_dynamic_contract(scenario, IMAGE_PHASES)?,
         _ => {}
      }
   }
   Ok(())
}

pub fn validate_pr_vertical_slice(spec_root: &Path, scenarios: &[(PathBuf, ScenarioSpec)]) -> Result<()>
{
   ensure!(scenarios.len() == VERTICAL_SCENARIO_CONTRACTS.len(), "PR vertical slice has {} scenarios, expected {}", scenarios.len(), VERTICAL_SCENARIO_CONTRACTS.len());
   for ((path, scenario), contract) in scenarios.iter().zip(VERTICAL_SCENARIO_CONTRACTS)
   {
      ensure!(scenario.id == contract.id, "PR vertical scenario order/id differs: expected {}, observed {}", contract.id, scenario.id);
      ensure!(path.file_stem().and_then(|stem| stem.to_str()) == Some(scenario.id.as_str()), "PR vertical scenario {} filename does not match its id", scenario.id);
      validate_scenario_artifacts(spec_root, scenario).with_context(|| format!("validating PR vertical scenario {}", scenario.id))?;
      validate_typed_fixture(spec_root, scenario)?;
      ensure!(scenario.primary_metric == contract.primary_metric, "PR vertical scenario {} primary metric differs from the frozen contract", scenario.id);
      ensure!(scenario.phases.iter().map(|phase| phase.id.as_str()).eq(contract.phases.iter().copied()), "PR vertical scenario {} phases differ from the frozen contract", scenario.id);
      ensure!(scenario.viewport_class == "phone-portrait", "PR vertical scenario {} does not use phone portrait", scenario.id);
      ensure!(scenario.font_pack.id == "oxide-bench-fonts-v1", "PR vertical scenario {} does not use the pinned font pack", scenario.id);
      ensure!(scenario.fairness_contract.logical_viewport_width == 390 && scenario.fairness_contract.logical_viewport_height == 844, "PR vertical scenario {} viewport differs from 390x844", scenario.id);
      ensure!(scenario.fairness_contract.locale == "en_US_POSIX" && scenario.fairness_contract.timezone == "UTC", "PR vertical scenario {} locale/timezone differs from the frozen contract", scenario.id);
      ensure!(scenario.fairness_contract.schedule_tolerance_us == 1_000 && scenario.fairness_contract.coordinate_tolerance_microunits == 1_000, "PR vertical scenario {} trace tolerances differ from the frozen contract", scenario.id);
      let prewarm = scenario.phases.iter().find(|phase| phase.id == "prewarm").with_context(|| format!("PR vertical scenario {} has no prewarm phase", scenario.id))?;
      ensure!(!prewarm.measured && prewarm.duration_ms == Some(2_000), "PR vertical scenario {} prewarm differs from two seconds", scenario.id);
      let measured_ms = scenario.phases.iter().filter(|phase| phase.measured).try_fold(0_u64, |total, phase| {
         let duration_ms = phase.duration_ms.with_context(|| format!("PR vertical scenario {} measured phase {} has no duration", scenario.id, phase.id))?;
         total.checked_add(duration_ms).context("PR vertical measured duration overflows u64")
      })?;
      ensure!(measured_ms == 6_000, "PR vertical scenario {} measured duration is {}ms, expected 6000ms", scenario.id, measured_ms);
   }
   Ok(())
}

pub fn validate_nightly_endurance_scenario(spec_root: &Path, scenario: &ScenarioSpec) -> Result<()>
{
   ensure!(scenario.id == "endurance.churn", "nightly endurance scenario has unexpected id {}", scenario.id);
   ensure!(scenario.primary_metric == "memory.retained_slope_bytes_per_min", "nightly endurance primary metric differs from the retained-footprint contract");
   for metric in ["memory.resident_drift_bytes", "memory.recovery_ms"]
   {
      ensure!(scenario.required_metrics.iter().any(|candidate| candidate == metric), "nightly endurance is missing required metric {}", metric);
   }
   ensure!(scenario.viewport_class == "phone-portrait" && scenario.fairness_contract.logical_viewport_width == 390 && scenario.fairness_contract.logical_viewport_height == 844, "nightly endurance does not use the frozen phone portrait viewport");
   ensure!(scenario.font_pack.id == "oxide-bench-fonts-v1", "nightly endurance does not use the pinned font pack");
   ensure!(phase_ids(scenario).eq(ENDURANCE_PHASES.iter().copied()), "nightly endurance phases differ from the frozen contract");
   validate_scenario_artifacts(spec_root, scenario).context("validating nightly endurance artifacts")?;
   let fixture = load_fixture::<EnduranceFixture>(spec_root, scenario)?;
   validate_endurance_fixture(&fixture)?;
   let expected_roles = [("endurance", 1), ("label", 176), ("icon-image", 64), ("rounded-card", 32), ("control", 24), ("backdrop-region", 4)];
   ensure!(scenario.scene.roles.iter().map(String::as_str).eq(expected_roles.iter().map(|(role, _)| *role)), "nightly endurance scene roles differ from the frozen dashboard-derived contract");
   ensure!(scenario.fairness_contract.expected_visible_role_counts.iter().map(|count| (count.role.as_str(), count.count)).eq(expected_roles.iter().copied()), "nightly endurance role counts differ from the frozen dashboard-derived contract");
   let prewarm = scenario.phases.iter().find(|phase| phase.id == "prewarm").context("nightly endurance has no prewarm phase")?;
   ensure!(!prewarm.measured && prewarm.duration_ms == Some(2_000) && prewarm.trace.is_none(), "nightly endurance prewarm differs from the unmeasured two-second contract");
   let measured_ms = scenario.phases.iter().filter(|phase| phase.measured).try_fold(0_u64, |total, phase| {
      let duration_ms = phase.duration_ms.with_context(|| format!("nightly endurance measured phase {} has no duration", phase.id))?;
      total.checked_add(duration_ms).context("nightly endurance measured duration overflows u64")
   })?;
   ensure!(measured_ms == 300_000, "nightly endurance measured duration is {}ms, expected 300000ms", measured_ms);
   ensure!(phase_duration(scenario, "open-close-heavy-screen") == Some(120_000), "nightly endurance open/close phase must be 120000ms");
   ensure!(phase_duration(scenario, "tab-switch-heavy") == Some(165_000), "nightly endurance tab-switch phase must be 165000ms");
   ensure!(phase_duration(scenario, "idle-animation") == Some(10_000), "nightly endurance animation phase must be 10000ms");
   ensure!(phase_duration(scenario, "recovery") == Some(5_000), "nightly endurance recovery phase must be 5000ms");
   validate_endurance_heavy_trace(&load_trace(spec_root, scenario, "open-close-heavy-screen")?, &fixture)?;
   validate_endurance_tab_trace(&load_trace(spec_root, scenario, "tab-switch-heavy")?, &fixture)?;
   validate_endurance_animation_trace(&load_trace(spec_root, scenario, "idle-animation")?, &fixture)?;
   ensure!(scenario.phases.iter().find(|phase| phase.id == "recovery").is_some_and(|phase| phase.trace.is_none()), "nightly endurance recovery phase must be event-free");
   let checkpoint_contract = [
      ("heavy-screen-recovered", "open-close-heavy-screen", 119_400_000),
      ("tab-restored", "tab-switch-heavy", 164_670_000),
      ("animation-settled", "idle-animation", 9_983_333),
      ("recovered", "recovery", 5_000_000),
   ];
   ensure!(scenario.parity_checkpoints.len() == checkpoint_contract.len(), "nightly endurance has {} checkpoints, expected {}", scenario.parity_checkpoints.len(), checkpoint_contract.len());
   for (checkpoint, expected) in scenario.parity_checkpoints.iter().zip(checkpoint_contract)
   {
      ensure!(checkpoint.id == expected.0 && checkpoint.phase_id == expected.1 && checkpoint.at_us == Some(expected.2), "nightly endurance checkpoint {} differs from the frozen phase/time contract", checkpoint.id);
      ensure!(checkpoint.screenshot.path == "checkpoints/dashboard.mixed-static/idle/screenshot.png", "nightly endurance checkpoint {} does not reuse the exact dashboard idle screenshot", checkpoint.id);
   }
   Ok(())
}

fn validate_typed_fixture(spec_root: &Path, scenario: &ScenarioSpec) -> Result<()>
{
   match scenario.id.as_str()
   {
      "startup.first-screen" => validate_startup_fixture(&load_fixture::<StartupFixture>(spec_root, scenario)?)?,
      "dashboard.mixed-static" => validate_dashboard_fixture(&load_fixture::<DashboardFixture>(spec_root, scenario)?)?,
      "feed.variable-scroll" => validate_feed_fixture(&load_fixture::<FeedFixture>(spec_root, scenario)?)?,
      "chat.live-update" => validate_chat_fixture(&load_fixture::<ChatFixture>(spec_root, scenario)?)?,
      "navigation.modal" => validate_navigation_fixture(&load_fixture::<NavigationFixture>(spec_root, scenario)?)?,
      "image.decode-zoom" => validate_image_decode_zoom_fixture(spec_root, &load_fixture::<ImageDecodeZoomFixture>(spec_root, scenario)?)?,
      id => anyhow::bail!("unsupported Apple PR fixture {}", id),
   }
   Ok(())
}

fn phase_ids(scenario: &ScenarioSpec) -> impl Iterator<Item = &str>
{
   scenario.phases.iter().map(|phase| phase.id.as_str())
}

fn phase_duration(scenario: &ScenarioSpec, id: &str) -> Option<u64>
{
   scenario.phases.iter().find(|phase| phase.id == id).and_then(|phase| phase.duration_ms)
}

fn load_trace(spec_root: &Path, scenario: &ScenarioSpec, phase_id: &str) -> Result<Vec<TraceEvent>>
{
   let phase = scenario.phases.iter().find(|phase| phase.id == phase_id).with_context(|| format!("scenario {} has no phase {}", scenario.id, phase_id))?;
   let trace = phase.trace.as_ref().with_context(|| format!("scenario {} phase {} has no trace", scenario.id, phase_id))?;
   let bytes = fs::read(spec_root.join(&trace.path)).with_context(|| format!("reading scenario {} trace {}", scenario.id, trace.path))?;
   serde_json::from_slice(&bytes).with_context(|| format!("parsing scenario {} trace {}", scenario.id, trace.path))
}

fn validate_endurance_heavy_trace(events: &[TraceEvent], fixture: &EnduranceFixture) -> Result<()>
{
   ensure!(events.len() == fixture.heavy_screen_cycle_count as usize * 2, "nightly endurance heavy-screen trace has {} events, expected {}", events.len(), fixture.heavy_screen_cycle_count * 2);
   for (index, event) in events.iter().enumerate()
   {
      let cycle = index as u64 / 2;
      let visible = index % 2 == 1;
      let expected_at = cycle * 1_200_000 + if visible {600_000} else {0};
      let expected_state = format!("endurance:heavy-screen:{}:{:03}", if visible {"open"} else {"closed"}, cycle + 1);
      ensure!(event.at_us == expected_at && event.op == TraceOperation::Mutate, "nightly endurance heavy-screen event {} has unexpected timing or operation", index);
      ensure!(event.target.as_deref() == Some(fixture.heavy_screen_target_id.as_str()) && event.value == Some(TraceValue::Boolean(visible)) && event.state_id.as_deref() == Some(expected_state.as_str()), "nightly endurance heavy-screen event {} differs from the frozen transition", index);
   }
   Ok(())
}

fn validate_endurance_tab_trace(events: &[TraceEvent], fixture: &EnduranceFixture) -> Result<()>
{
   ensure!(events.len() == fixture.tab_switch_count as usize, "nightly endurance tab trace has {} events, expected {}", events.len(), fixture.tab_switch_count);
   for (index, event) in events.iter().enumerate()
   {
      let tab = (index as u32 + 1) % fixture.tab_count;
      let expected_state = format!("endurance:tab:{tab}:switch:{:03}", index + 1);
      ensure!(event.at_us == index as u64 * 330_000 && event.op == TraceOperation::Mutate, "nightly endurance tab event {} has unexpected timing or operation", index);
      ensure!(event.target.as_deref() == Some(fixture.active_tab_target_id.as_str()) && event.value == Some(TraceValue::Integer(i64::from(tab))) && event.state_id.as_deref() == Some(expected_state.as_str()), "nightly endurance tab event {} differs from the frozen transition", index);
   }
   Ok(())
}

fn validate_endurance_animation_trace(events: &[TraceEvent], fixture: &EnduranceFixture) -> Result<()>
{
   ensure!(events.len() == fixture.animation_frame_count as usize, "nightly endurance animation trace has {} events, expected {}", events.len(), fixture.animation_frame_count);
   for (index, event) in events.iter().enumerate()
   {
      let frame = index as u32 + 1;
      let expected_state = format!("endurance:animation-frame:{frame:03}");
      ensure!(event.at_us == index as u64 * 50_000 / 3 && event.op == TraceOperation::Mutate, "nightly endurance animation event {} has unexpected timing or operation", index);
      ensure!(event.target.as_deref() == Some(fixture.animation_frame_target_id.as_str()) && event.value == Some(TraceValue::Integer(i64::from(frame))) && event.state_id.as_deref() == Some(expected_state.as_str()), "nightly endurance animation event {} differs from the frozen transition", index);
   }
   Ok(())
}

fn validate_dynamic_contract(scenario: &ScenarioSpec, phases: &[&str]) -> Result<()>
{
   ensure!(phase_ids(scenario).eq(phases.iter().copied()), "Apple PR scenario {} phases differ from the frozen contract", scenario.id);
   let prewarm = scenario.phases.iter().find(|phase| phase.id == "prewarm").with_context(|| format!("Apple PR scenario {} has no prewarm phase", scenario.id))?;
   ensure!(!prewarm.measured && prewarm.duration_ms == Some(2_000), "Apple PR scenario {} prewarm differs from two seconds", scenario.id);
   let measured_ms = scenario.phases.iter().filter(|phase| phase.measured).try_fold(0_u64, |total, phase| {
      let duration_ms = phase.duration_ms.with_context(|| format!("Apple PR scenario {} measured phase {} has no duration", scenario.id, phase.id))?;
      total.checked_add(duration_ms).context("Apple PR measured duration overflows u64")
   })?;
   ensure!(measured_ms == 6_000, "Apple PR scenario {} measured duration is {}ms, expected 6000ms", scenario.id, measured_ms);
   Ok(())
}

fn load_fixture<T: DeserializeOwned>(spec_root: &Path, scenario: &ScenarioSpec) -> Result<T>
{
   let path = spec_root.join(&scenario.fixture.path);
   let bytes = fs::read(&path).with_context(|| format!("reading typed fixture for scenario {} from {}", scenario.id, path.display()))?;
   serde_json::from_slice(&bytes).with_context(|| format!("parsing typed fixture for scenario {} from {}", scenario.id, path.display()))
}
