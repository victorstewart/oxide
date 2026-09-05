use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use oxide_benchmark_spec::{
   validate_comparison_plan, ComparisonCell, ComparisonOrder, ComparisonPlan,
   ComparisonSession, DecisionAlternative as SpecDecisionAlternative, DecisionClaimKind,
   DecimalU64, EvidenceRole, MetricDefinition, MetricDirection as SpecMetricDirection,
   Platform, RawObservationRow, RawObservationValue, Tier,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::comparative::{
   classify_comparison, decision_estimand, exact_sign_test,
   hierarchical_block_bootstrap_ci, holm_adjust, normalized_pair_effect,
   ClassificationEvidence, ComparisonClassification, DecisionAlternative, ExactSignTest,
   HolmMember, MetricDirection, PairEffectKind, PairedSessionSamples,
   WithinSessionEstimator,
};
use crate::paired_statistics::{balanced_pair_order, median, percentile, PairOrder};

pub const COMPARISON_REPORT_SCHEMA_VERSION: u32 = 1;
pub const COMPARISON_ANALYZER_VERSION: &str = "oxide-comparison-analysis-v1";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ComparisonReport
{
   pub schema_version: u32,
   pub analyzer_version: String,
   pub suite_id: String,
   pub plan_id: String,
   pub plan_sha256: String,
   pub platform: Platform,
   pub tier: Tier,
   pub reference_id: String,
   pub contender_id: String,
   pub superiority_ledger: Vec<SuperiorityLedgerRow>,
   pub optimization_backlog: Vec<OptimizationBacklogRow>,
   pub cells: Vec<ComparisonCellReport>,
   pub decision_families: Vec<DecisionFamilyReport>,
   pub evidence: ComparisonEvidenceManifest,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SuperiorityLedgerRow
{
   pub comparison_cell_id: String,
   pub scenario_id: String,
   pub primary_metric_id: String,
   pub evidence_role: EvidenceRole,
   pub valid_pairs: DecimalU64,
   pub classification: ComparisonClassification,
   pub required_guardrails_established: bool,
   pub publishable_oxide_superiority: bool,
   pub blocking_reasons: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OptimizationBacklogRow
{
   pub rank: DecimalU64,
   pub id: String,
   pub comparison_cell_id: String,
   pub scenario_id: String,
   pub metric_id: String,
   pub subsystem: String,
   pub reason: String,
   pub evidence_artifact_paths: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ComparisonCellReport
{
   pub comparison_cell_id: String,
   pub scenario_id: String,
   pub validation: String,
   pub terminal_hard_outcome: Option<String>,
   pub valid_pairs: DecimalU64,
   pub exogenous_invalid_pairs: DecimalU64,
   pub expected_pairs: DecimalU64,
   pub actual_order_matches_plan: bool,
   pub classification: ComparisonClassification,
   pub primary_metric: MetricAnalysisReport,
   pub guardrails: Vec<GuardrailReport>,
   pub evidence_artifact_paths: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MetricAnalysisReport
{
   pub metric_id: String,
   pub unit: String,
   pub within_session_estimator: String,
   pub pair_effect: String,
   pub sufficient: bool,
   pub insufficiency_reasons: Vec<String>,
   pub oxide_distribution: Option<AbsoluteDistribution>,
   pub reference_distribution: Option<AbsoluteDistribution>,
   pub effect: Option<EffectSummary>,
   pub paired_sessions: Vec<PairSummary>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AbsoluteDistribution
{
   pub session_estimators: Vec<f64>,
   pub p50: f64,
   pub p95: f64,
   pub p99: f64,
   pub peak: f64,
   pub mad: f64,
   pub coefficient_of_variation: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EffectSummary
{
   pub direction_normalized_median: f64,
   pub reported_badness_ratio: Option<f64>,
   pub confidence_interval_95: [f64; 2],
   pub pair_win_rate: f64,
   pub first_to_last_drift: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PairSummary
{
   pub pair_index: DecimalU64,
   pub order: ComparisonOrder,
   pub oxide_sample_count: DecimalU64,
   pub reference_sample_count: DecimalU64,
   pub oxide_estimator: f64,
   pub reference_estimator: f64,
   pub direction_normalized_effect: f64,
   pub oxide_session_id: String,
   pub reference_session_id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DecisionFamilyReport
{
   pub family_id: String,
   pub claim_kind: DecisionClaimKind,
   pub alpha: f64,
   pub complete: bool,
   pub members: Vec<HypothesisReport>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct HypothesisReport
{
   pub comparison_cell_id: String,
   pub metric_id: String,
   pub boundary_id: String,
   pub boundary: f64,
   pub alternative: SpecDecisionAlternative,
   pub exact_sign_test: Option<ExactSignTest>,
   pub adjusted_p_value: Option<f64>,
   pub passes: bool,
   pub unavailable_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GuardrailStatus
{
   Established,
   Failed,
   Unavailable,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GuardrailReport
{
   pub metric_id: String,
   pub family_id: Option<String>,
   pub status: GuardrailStatus,
   pub adjusted_p_value: Option<f64>,
   pub reason: Option<String>,
   pub analysis: MetricAnalysisReport,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ComparisonEvidenceManifest
{
   pub input_root: String,
   pub plan_path: String,
   pub plan_artifact_sha256: String,
   pub sessions_path: String,
   pub sessions_artifact_sha256: String,
   pub raw_artifact_sha256: BTreeMap<String, String>,
}

#[derive(Clone)]
struct LoadedSession
{
   session: ComparisonSession,
   session_id: String,
   observations: Vec<RawObservationRow>,
   raw_path: String,
}

pub fn analyze_comparison_bundle(input_root: &Path) -> Result<ComparisonReport>
{
   ensure!(input_root.is_dir(), "comparison analysis input must be a directory: {}", input_root.display());
   let plan_path = input_root.join("plan.json");
   let sessions_path = input_root.join("sessions.jsonl");
   let plan_bytes = fs::read(&plan_path).with_context(|| format!("reading {}", plan_path.display()))?;
   let plan: ComparisonPlan = serde_json::from_slice(&plan_bytes).with_context(|| format!("decoding {}", plan_path.display()))?;
   validate_comparison_plan(&plan).context("validating comparison analysis plan")?;
   let sessions_bytes = fs::read(&sessions_path).with_context(|| format!("reading {}", sessions_path.display()))?;
   let sessions = decode_json_lines::<ComparisonSession>(&sessions_bytes, &sessions_path)?;
   ensure!(!sessions.is_empty(), "comparison analysis has no sessions");

   let mut loaded = Vec::with_capacity(sessions.len());
   let mut session_ids = BTreeSet::new();
   let mut raw_hashes = BTreeMap::new();
   for session in sessions
   {
      let relative = safe_relative_path(&session.raw_sample_artifact.path)?;
      let raw_path = input_root.join(&relative);
      ensure!(raw_path.extension().and_then(|value| value.to_str()) != Some("zst"), "compressed raw evidence is not supported by analyzer v1: {}", raw_path.display());
      let raw_bytes = fs::read(&raw_path).with_context(|| format!("reading {}", raw_path.display()))?;
      let actual_hash = sha256(&raw_bytes);
      ensure!(actual_hash == session.raw_sample_artifact.sha256, "raw artifact hash mismatch for {}", raw_path.display());
      let observations = decode_json_lines::<RawObservationRow>(&raw_bytes, &raw_path)?;
      ensure!(!observations.is_empty(), "raw session artifact is empty: {}", raw_path.display());
      let raw_session_ids = observations.iter().map(|row| row.session_id.as_str()).collect::<BTreeSet<_>>();
      ensure!(raw_session_ids.len() == 1, "raw artifact contains more than one session id: {}", raw_path.display());
      let session_id = String::from(*raw_session_ids.iter().next().expect("nonempty raw session id set"));
      ensure!(!session_id.is_empty() && session_ids.insert(session_id.clone()), "raw session id is empty or repeated: {}", session_id);
      ensure!(observations.iter().all(|row| row.measurement_pass_id == session.measurement_pass_id), "raw observation pass differs from its session: {}", raw_path.display());
      raw_hashes.insert(relative.to_string_lossy().into_owned(), actual_hash);
      loaded.push(LoadedSession {
         session,
         session_id,
         observations,
         raw_path: relative.to_string_lossy().into_owned(),
      });
   }

   let bootstrap_resamples = match plan.tier
   {
      Tier::Pr => 10_000,
      Tier::Nightly | Tier::ReleaseCore | Tier::ClaimComplete | Tier::Extended | Tier::FullAttribution => 100_000,
   };
   let mut cells = Vec::with_capacity(plan.comparison_cells.len());
   for cell in &plan.comparison_cells
   {
      cells.push(analyze_cell(&plan, cell, &loaded, bootstrap_resamples)?);
   }
   let decision_families = analyze_decision_families(&plan, &cells)?;
   finalize_cells(&plan, &decision_families, &mut cells)?;
   let superiority_ledger = build_ledger(&plan, &cells);
   let optimization_backlog = build_optimization_backlog(&cells);

   Ok(ComparisonReport {
      schema_version: COMPARISON_REPORT_SCHEMA_VERSION,
      analyzer_version: String::from(COMPARISON_ANALYZER_VERSION),
      suite_id: plan.suite_id.clone(),
      plan_id: plan.plan_id.clone(),
      plan_sha256: plan.plan_sha256.clone(),
      platform: plan.platform,
      tier: plan.tier,
      reference_id: plan.reference.id.clone(),
      contender_id: plan.contender.id.clone(),
      superiority_ledger,
      optimization_backlog,
      cells,
      decision_families,
      evidence: ComparisonEvidenceManifest {
         input_root: input_root.to_string_lossy().into_owned(),
         plan_path: String::from("plan.json"),
         plan_artifact_sha256: sha256(&plan_bytes),
         sessions_path: String::from("sessions.jsonl"),
         sessions_artifact_sha256: sha256(&sessions_bytes),
         raw_artifact_sha256: raw_hashes,
      },
   })
}

pub fn render_comparison_report_markdown(report: &ComparisonReport) -> String
{
   let mut body = String::new();
   body.push_str(&format!("# Oxide production comparison: {}\n\n", report.plan_id));
   body.push_str(&format!("- Analyzer: `{}`\n", report.analyzer_version));
   body.push_str(&format!("- Platform/tier: `{:?}` / `{:?}`\n", report.platform, report.tier));
   body.push_str(&format!("- Reference: `{}`\n", report.reference_id));
   body.push_str(&format!("- Contender: `{}`\n", report.contender_id));
   body.push_str(&format!("- Plan SHA-256: `{}`\n\n", report.plan_sha256));

   body.push_str("## Superiority ledger\n\n");
   body.push_str("| Cell | Scenario | Metric | Pairs | Classification | Guardrails | Publishable Oxide superiority |\n");
   body.push_str("| --- | --- | --- | ---: | --- | --- | --- |\n");
   for row in &report.superiority_ledger
   {
      body.push_str(&format!("| `{}` | `{}` | `{}` | {} | {} | {} | {} |\n",
         row.comparison_cell_id, row.scenario_id, row.primary_metric_id, row.valid_pairs.0,
         classification_label(row.classification), yes_no(row.required_guardrails_established),
         yes_no(row.publishable_oxide_superiority)));
   }

   body.push_str("\n## Ranked optimization backlog\n\n");
   if report.optimization_backlog.is_empty()
   {
      body.push_str("None.\n");
   }
   else
   {
      body.push_str("| Rank | Cell | Metric | Subsystem | Reason |\n");
      body.push_str("| ---: | --- | --- | --- | --- |\n");
      for row in &report.optimization_backlog
      {
         body.push_str(&format!("| {} | `{}` | `{}` | `{}` | {} |\n", row.rank.0, row.comparison_cell_id, row.metric_id, row.subsystem, row.reason));
      }
   }

   body.push_str("\n## Cell evidence\n\n");
   for cell in &report.cells
   {
      body.push_str(&format!("### `{}`\n\n", cell.comparison_cell_id));
      body.push_str(&format!("Validation: `{}`. Classification: **{}**. Valid pairs: {}/{}.\n\n",
         cell.validation, classification_label(cell.classification), cell.valid_pairs.0, cell.expected_pairs.0));
      if let Some(outcome) = cell.terminal_hard_outcome.as_ref()
      {
         body.push_str(&format!("Terminal hard outcome: `{}`. No performance winner or synthetic effect is reported.\n\n", outcome));
      }
      if let Some(effect) = cell.primary_metric.effect.as_ref()
      {
         let ratio = effect.reported_badness_ratio.map(|value| format!("{value:.6}")).unwrap_or_else(|| String::from("n/a"));
         body.push_str(&format!("Primary direction-normalized median effect: `{:.9}`; badness ratio: `{}`; descriptive 95% CI: `[{:.9}, {:.9}]`; pair-win rate: `{:.3}`.\n\n",
            effect.direction_normalized_median, ratio, effect.confidence_interval_95[0], effect.confidence_interval_95[1], effect.pair_win_rate));
      }
      if !cell.primary_metric.insufficiency_reasons.is_empty()
      {
         body.push_str(&format!("Insufficiency: {}.\n\n", cell.primary_metric.insufficiency_reasons.join("; ")));
      }
   }

   body.push_str("## Evidence identity\n\n");
   body.push_str(&format!("- Plan artifact: `{}` (`{}`)\n", report.evidence.plan_path, report.evidence.plan_artifact_sha256));
   body.push_str(&format!("- Sessions artifact: `{}` (`{}`)\n", report.evidence.sessions_path, report.evidence.sessions_artifact_sha256));
   for (path, hash) in &report.evidence.raw_artifact_sha256
   {
      body.push_str(&format!("- Raw artifact: `{}` (`{}`)\n", path, hash));
   }
   body
}

fn analyze_cell(plan: &ComparisonPlan, cell: &ComparisonCell, sessions: &[LoadedSession], bootstrap_resamples: usize) -> Result<ComparisonCellReport>
{
   let primary = plan.metric_definitions.iter().find(|metric| metric.id == cell.primary_metric_id).context("validated primary metric disappeared")?;
   let mut terminal = sessions.iter()
      .filter(|loaded| loaded.session.measurement_pass_id == cell.owning_pass_id)
      .filter_map(|loaded| loaded.session.terminal_hard_outcome.clone())
      .next();
   if plan.reference.comparator_acceptance_status != "accepted"
   {
      terminal = Some(format!("comparator-acceptance-{}", plan.reference.comparator_acceptance_status));
   }

   let (primary_metric, invalid_pairs, order_matches) = analyze_metric(plan, cell, primary, sessions, terminal.is_some(), bootstrap_resamples)?;
   let mut guardrails = Vec::with_capacity(cell.required_guardrail_metric_ids.len());
   for metric_id in &cell.required_guardrail_metric_ids
   {
      let metric = plan.metric_definitions.iter().find(|metric| metric.id == *metric_id).context("validated guardrail metric disappeared")?;
      let (analysis, _, _) = analyze_metric(plan, cell, metric, sessions, terminal.is_some(), bootstrap_resamples)?;
      guardrails.push(GuardrailReport {
         metric_id: metric_id.clone(),
         family_id: cell.required_guardrail_family_id.clone(),
         status: GuardrailStatus::Unavailable,
         adjusted_p_value: None,
         reason: analysis.insufficiency_reasons.first().cloned().or_else(|| Some(String::from("decision family has not been applied"))),
         analysis,
      });
   }
   let evidence_artifact_paths = sessions.iter()
      .filter(|loaded| loaded.session.measurement_pass_id == cell.owning_pass_id)
      .filter(|loaded| loaded.observations.iter().any(|row| row.scenario_id == cell.scenario_id))
      .map(|loaded| loaded.raw_path.clone())
      .collect::<BTreeSet<_>>()
      .into_iter()
      .collect::<Vec<_>>();
   let valid_pairs = DecimalU64(primary_metric.paired_sessions.len() as u64);
   let classification = if terminal.is_some()
   {
      ComparisonClassification::HardFailureNoPerformanceClaim
   }
   else
   {
      ComparisonClassification::Inconclusive
   };
   Ok(ComparisonCellReport {
      comparison_cell_id: cell.id.clone(),
      scenario_id: cell.scenario_id.clone(),
      validation: if terminal.is_some() { String::from("hard-failure") } else if primary_metric.sufficient { String::from("valid") } else { String::from("insufficient") },
      terminal_hard_outcome: terminal,
      valid_pairs,
      exogenous_invalid_pairs: DecimalU64(invalid_pairs as u64),
      expected_pairs: plan.pass_pair_count,
      actual_order_matches_plan: order_matches,
      classification,
      primary_metric,
      guardrails,
      evidence_artifact_paths,
   })
}

fn analyze_metric(plan: &ComparisonPlan, cell: &ComparisonCell, metric: &MetricDefinition, sessions: &[LoadedSession], hard_outcome: bool, bootstrap_resamples: usize) -> Result<(MetricAnalysisReport, usize, bool)>
{
   let estimator = parse_estimator(&metric.within_session_estimator)?;
   let effect_kind = parse_effect_kind(&metric.pair_effect)?;
   let direction = convert_direction(metric.direction);
   let minimum_pairs = minimum_from_rule(&cell.sufficiency_rule, "pairs").unwrap_or(metric.exact_test_resolution_floor.0 as usize);
   let minimum_samples = minimum_sample_count(&cell.sufficiency_rule, estimator);
   let expected_orders = balanced_pair_order(plan.seed.0, plan.pass_pair_count.0 as usize);
   let mut paired = Vec::new();
   let mut bootstrap_pairs = Vec::new();
   let mut invalid_pairs = 0;
   let mut order_matches = true;

   for pair_index in 0..plan.pass_pair_count.0
   {
      let reference = unique_session(sessions, pair_index, &plan.reference.id, &cell.owning_pass_id)?;
      let oxide = unique_session(sessions, pair_index, &plan.contender.id, &cell.owning_pass_id)?;
      let (reference, oxide) = match (reference, oxide)
      {
         (Some(reference), Some(oxide)) => (reference, oxide),
         _ =>
         {
            invalid_pairs += 1;
            continue;
         }
      };
      let expected = expected_orders[pair_index as usize];
      let expected_spec = match expected { PairOrder::Ab => ComparisonOrder::Ab, PairOrder::Ba => ComparisonOrder::Ba };
      let pair_order_valid = reference.session.order == expected_spec
         && oxide.session.order == expected_spec
         && starts_in_declared_order(reference, oxide, expected_spec);
      if !pair_order_valid
      {
         order_matches = false;
         invalid_pairs += 1;
         continue;
      }
      if !session_is_valid(&reference.session) || !session_is_valid(&oxide.session)
      {
         invalid_pairs += 1;
         continue;
      }
      let reference_samples = metric_samples(reference, cell, metric)?;
      let oxide_samples = metric_samples(oxide, cell, metric)?;
      if reference_samples.len() < minimum_samples || oxide_samples.len() < minimum_samples
      {
         invalid_pairs += 1;
         continue;
      }
      let reference_estimator = estimate(&reference_samples, estimator);
      let oxide_estimator = estimate(&oxide_samples, estimator);
      let effect = normalized_pair_effect(oxide_estimator, reference_estimator, direction, effect_kind)?;
      paired.push(PairSummary {
         pair_index: DecimalU64(pair_index),
         order: expected_spec,
         oxide_sample_count: DecimalU64(oxide_samples.len() as u64),
         reference_sample_count: DecimalU64(reference_samples.len() as u64),
         oxide_estimator,
         reference_estimator,
         direction_normalized_effect: effect,
         oxide_session_id: oxide.session_id.clone(),
         reference_session_id: reference.session_id.clone(),
      });
      bootstrap_pairs.push(PairedSessionSamples { oxide: oxide_samples, reference: reference_samples });
   }

   let mut insufficiency_reasons = Vec::new();
   if hard_outcome
   {
      insufficiency_reasons.push(String::from("terminal hard outcome suppresses performance analysis"));
   }
   if paired.len() < minimum_pairs
   {
      insufficiency_reasons.push(format!("valid pairs {} below required {}", paired.len(), minimum_pairs));
   }
   if !order_matches
   {
      insufficiency_reasons.push(String::from("actual pair order differs from the seeded balanced plan"));
   }
   let oxide_values = paired.iter().map(|pair| pair.oxide_estimator).collect::<Vec<_>>();
   let reference_values = paired.iter().map(|pair| pair.reference_estimator).collect::<Vec<_>>();
   let effects = paired.iter().map(|pair| pair.direction_normalized_effect).collect::<Vec<_>>();
   let effect = if !hard_outcome && !effects.is_empty()
   {
      let estimand = decision_estimand(&effects)?;
      let block_len = block_length_samples(&metric.block_duration).max(1);
      let interval = hierarchical_block_bootstrap_ci(&bootstrap_pairs, direction, effect_kind, estimator, block_len, plan.seed.0 ^ stable_seed(&cell.id, &metric.id), bootstrap_resamples)?;
      if let Some(maximum) = metric.max_interval_width.as_deref()
      {
         let maximum = parse_boundary(maximum)?.abs();
         let width = interval[1] - interval[0];
         if width > maximum
         {
            insufficiency_reasons.push(format!("effect interval width {:.9} exceeds maximum {:.9}", width, maximum));
         }
      }
      Some(EffectSummary {
         direction_normalized_median: estimand,
         reported_badness_ratio: (effect_kind == PairEffectKind::StrictlyPositiveRatio).then(|| estimand.exp()),
         confidence_interval_95: interval,
         pair_win_rate: effects.iter().filter(|effect| **effect < 0.0).count() as f64 / effects.len() as f64,
         first_to_last_drift: effects.last().copied().unwrap_or(0.0) - effects.first().copied().unwrap_or(0.0),
      })
   }
   else
   {
      None
   };
   let sufficient = insufficiency_reasons.is_empty();
   Ok((MetricAnalysisReport {
      metric_id: metric.id.clone(),
      unit: metric.unit.clone(),
      within_session_estimator: metric.within_session_estimator.clone(),
      pair_effect: metric.pair_effect.clone(),
      sufficient,
      insufficiency_reasons,
      oxide_distribution: (!oxide_values.is_empty() && !hard_outcome).then(|| distribution(&oxide_values)),
      reference_distribution: (!reference_values.is_empty() && !hard_outcome).then(|| distribution(&reference_values)),
      effect,
      paired_sessions: if hard_outcome { Vec::new() } else { paired },
   }, invalid_pairs, order_matches))
}

fn analyze_decision_families(plan: &ComparisonPlan, cells: &[ComparisonCellReport]) -> Result<Vec<DecisionFamilyReport>>
{
   let mut reports = Vec::with_capacity(plan.decision_families.len());
   for family in &plan.decision_families
   {
      let mut hypotheses = Vec::with_capacity(family.ordered_members.len());
      let mut holm_members = Vec::with_capacity(family.ordered_members.len());
      for member in &family.ordered_members
      {
         let cell_spec = plan.comparison_cells.iter().find(|cell| cell.id == member.comparison_cell_id).context("validated family cell disappeared")?;
         let metric_spec = plan.metric_definitions.iter().find(|metric| metric.id == member.metric_id).context("validated family metric disappeared")?;
         let cell = cells.iter().find(|cell| cell.comparison_cell_id == member.comparison_cell_id).context("analyzed family cell disappeared")?;
         let metric = metric_report(cell, &member.metric_id);
         let boundary = decision_boundary(family.claim_kind, cell_spec, metric_spec)?;
         let minimum_pairs = minimum_from_rule(&cell_spec.sufficiency_rule, "pairs").unwrap_or(0)
            .max(family.exact_test_resolution_floor.0 as usize)
            .max(metric_spec.exact_test_resolution_floor.0 as usize);
         let unavailable_reason = if cell.terminal_hard_outcome.is_some()
         {
            Some(String::from("terminal hard outcome"))
         }
         else if metric.is_none()
         {
            Some(String::from("metric is unavailable for the cell"))
         }
         else if metric.is_some_and(|metric| metric.paired_sessions.len() < minimum_pairs)
         {
            Some(format!("valid pairs below exact-test floor {}", minimum_pairs))
         }
         else
         {
            None
         };
         let test = if unavailable_reason.is_none()
         {
            let effects = metric.expect("availability checked").paired_sessions.iter().map(|pair| pair.direction_normalized_effect).collect::<Vec<_>>();
            let alternative = convert_alternative(member.alternative);
            let test = exact_sign_test(&effects, boundary, alternative)?;
            (test.effective_n >= minimum_pairs).then_some(test)
         }
         else
         {
            None
         };
         let unavailable_reason = if unavailable_reason.is_none() && test.is_none()
         {
            Some(format!("non-tied pair count below exact-test floor {}", minimum_pairs))
         }
         else
         {
            unavailable_reason
         };
         holm_members.push(HolmMember {
            comparison_cell_id: member.comparison_cell_id.clone(),
            metric_id: member.metric_id.clone(),
            boundary_id: member.boundary_id.clone(),
            p_value: test.as_ref().map(|test| test.p_value).unwrap_or(1.0),
         });
         hypotheses.push(HypothesisReport {
            comparison_cell_id: member.comparison_cell_id.clone(),
            metric_id: member.metric_id.clone(),
            boundary_id: member.boundary_id.clone(),
            boundary,
            alternative: member.alternative,
            exact_sign_test: test,
            adjusted_p_value: None,
            passes: false,
            unavailable_reason,
         });
      }
      let adjusted = holm_adjust(&holm_members)?;
      for hypothesis in &mut hypotheses
      {
         if hypothesis.exact_sign_test.is_none()
         {
            continue;
         }
         let decision = adjusted.iter().find(|decision| decision.comparison_cell_id == hypothesis.comparison_cell_id && decision.metric_id == hypothesis.metric_id && decision.boundary_id == hypothesis.boundary_id).context("Holm decision member disappeared")?;
         hypothesis.adjusted_p_value = Some(decision.adjusted_p_value);
         hypothesis.passes = decision.adjusted_p_value <= family.alpha;
      }
      reports.push(DecisionFamilyReport {
         family_id: family.id.clone(),
         claim_kind: family.claim_kind,
         alpha: family.alpha,
         complete: hypotheses.iter().all(|hypothesis| hypothesis.exact_sign_test.is_some()),
         members: hypotheses,
      });
   }
   Ok(reports)
}

fn finalize_cells(plan: &ComparisonPlan, families: &[DecisionFamilyReport], cells: &mut [ComparisonCellReport]) -> Result<()>
{
   for cell in cells
   {
      let spec = plan.comparison_cells.iter().find(|candidate| candidate.id == cell.comparison_cell_id).context("analyzed cell disappeared from plan")?;
      let alpha = plan.metric_definitions.iter().find(|metric| metric.id == spec.primary_metric_id).context("primary metric disappeared")?.decision_alpha;
      let evidence = ClassificationEvidence {
         oxide_superiority_adjusted_p: adjusted_p(families, spec.oxide_superiority_family_id.as_deref(), &spec.id, &spec.primary_metric_id),
         reference_superiority_adjusted_p: adjusted_p(families, spec.reference_superiority_family_id.as_deref(), &spec.id, &spec.primary_metric_id),
         equivalence_lower_adjusted_p: adjusted_p(families, spec.equivalence_lower_family_id.as_deref(), &spec.id, &spec.primary_metric_id),
         equivalence_upper_adjusted_p: adjusted_p(families, spec.equivalence_upper_family_id.as_deref(), &spec.id, &spec.primary_metric_id),
      };
      cell.classification = classify_comparison(cell.primary_metric.sufficient, cell.terminal_hard_outcome.is_some(), alpha, evidence)?;
      for guardrail in &mut cell.guardrails
      {
         let family_id = guardrail.family_id.as_deref();
         let hypothesis = find_hypothesis(families, family_id, &spec.id, &guardrail.metric_id);
         match hypothesis
         {
            Some(hypothesis) if hypothesis.passes =>
            {
               guardrail.status = GuardrailStatus::Established;
               guardrail.adjusted_p_value = hypothesis.adjusted_p_value;
               guardrail.reason = None;
            }
            Some(hypothesis) if hypothesis.adjusted_p_value.is_some() =>
            {
               guardrail.status = GuardrailStatus::Failed;
               guardrail.adjusted_p_value = hypothesis.adjusted_p_value;
               guardrail.reason = Some(String::from("adjusted non-inferiority decision did not pass"));
            }
            Some(hypothesis) =>
            {
               guardrail.status = GuardrailStatus::Unavailable;
               guardrail.reason = hypothesis.unavailable_reason.clone();
            }
            None =>
            {
               guardrail.status = GuardrailStatus::Unavailable;
               guardrail.reason = Some(String::from("required guardrail has no decision-family member"));
            }
         }
      }
      cell.validation = if cell.terminal_hard_outcome.is_some()
      {
         String::from("hard-failure")
      }
      else if cell.primary_metric.sufficient
      {
         String::from("valid")
      }
      else
      {
         String::from("insufficient")
      };
   }
   Ok(())
}

fn build_ledger(plan: &ComparisonPlan, cells: &[ComparisonCellReport]) -> Vec<SuperiorityLedgerRow>
{
   cells.iter().map(|cell| {
      let spec = plan.comparison_cells.iter().find(|candidate| candidate.id == cell.comparison_cell_id).expect("analyzed cell must remain in plan");
      let guardrails = cell.guardrails.iter().all(|guardrail| guardrail.status == GuardrailStatus::Established);
      let mut blocking_reasons = Vec::new();
      if cell.classification != ComparisonClassification::OxideFaster
      {
         blocking_reasons.push(format!("classification is {}", classification_label(cell.classification)));
      }
      if !guardrails
      {
         blocking_reasons.push(String::from("required guardrail non-inferiority is not established"));
      }
      if cell.terminal_hard_outcome.is_some()
      {
         blocking_reasons.push(String::from("cell has a terminal hard outcome"));
      }
      if !matches!(plan.tier, Tier::ReleaseCore | Tier::ClaimComplete)
      {
         blocking_reasons.push(String::from("tier is descriptive and cannot publish a framework claim"));
      }
      SuperiorityLedgerRow {
         comparison_cell_id: cell.comparison_cell_id.clone(),
         scenario_id: cell.scenario_id.clone(),
         primary_metric_id: spec.primary_metric_id.clone(),
         evidence_role: spec.evidence_role,
         valid_pairs: cell.valid_pairs,
         classification: cell.classification,
         required_guardrails_established: guardrails,
         publishable_oxide_superiority: spec.evidence_role == EvidenceRole::RequiredClaim
            && matches!(plan.tier, Tier::ReleaseCore | Tier::ClaimComplete)
            && cell.classification == ComparisonClassification::OxideFaster
            && guardrails
            && cell.terminal_hard_outcome.is_none(),
         blocking_reasons,
      }
   }).collect()
}

fn build_optimization_backlog(cells: &[ComparisonCellReport]) -> Vec<OptimizationBacklogRow>
{
   let mut deduplicated = BTreeMap::new();
   for cell in cells
   {
      let reason = match cell.classification
      {
         ComparisonClassification::ReferenceFaster => String::from("reference is materially faster"),
         ComparisonClassification::HardFailureNoPerformanceClaim => String::from("hard outcome blocks a performance claim"),
         ComparisonClassification::Inconclusive => String::from("required comparison is inconclusive"),
         ComparisonClassification::OxideFaster | ComparisonClassification::EquivalentWithinMaterialityRegion => continue,
      };
      let subsystem = metric_subsystem(&cell.primary_metric.metric_id);
      let key = format!("{}:{}", cell.comparison_cell_id, subsystem);
      deduplicated.entry(key.clone()).or_insert_with(|| (key, cell, subsystem, reason));
   }
   let mut entries = deduplicated.into_values().collect::<Vec<_>>();
   entries.sort_by(|left, right| backlog_priority(left.1.classification).cmp(&backlog_priority(right.1.classification)).then_with(|| left.0.cmp(&right.0)));
   entries.into_iter().enumerate().map(|(index, (id, cell, subsystem, reason))| OptimizationBacklogRow {
      rank: DecimalU64((index + 1) as u64),
      id,
      comparison_cell_id: cell.comparison_cell_id.clone(),
      scenario_id: cell.scenario_id.clone(),
      metric_id: cell.primary_metric.metric_id.clone(),
      subsystem: String::from(subsystem),
      reason,
      evidence_artifact_paths: cell.evidence_artifact_paths.clone(),
   }).collect()
}

fn unique_session<'a>(sessions: &'a [LoadedSession], pair_index: u64, implementation_id: &str, pass_id: &str) -> Result<Option<&'a LoadedSession>>
{
   let mut matches = sessions.iter().filter(|loaded| loaded.session.pair_index.0 == pair_index && loaded.session.implementation_id == implementation_id && loaded.session.measurement_pass_id == pass_id);
   let first = matches.next();
   ensure!(matches.next().is_none(), "pair {} has duplicate {} sessions for pass {}", pair_index, implementation_id, pass_id);
   Ok(first)
}

fn metric_samples(loaded: &LoadedSession, cell: &ComparisonCell, metric: &MetricDefinition) -> Result<Vec<f64>>
{
   let mut samples = Vec::new();
   for row in loaded.observations.iter().filter(|row| row.scenario_id == cell.scenario_id && row.metric_id == metric.id)
   {
      if row.quality_flags.iter().any(|flag| flag == "warmup_excluded" || flag.starts_with("invalid"))
      {
         continue;
      }
      let value = match &row.value
      {
         RawObservationValue::FiniteF64(value) => *value,
         RawObservationValue::DecimalU64(value) =>
         {
            ensure!(value.0 <= (1_u64 << 53), "stochastic metric {} exceeds exact f64 integer range", metric.id);
            value.0 as f64
         }
         RawObservationValue::SignedI64(value) =>
         {
            ensure!(value.unsigned_abs() <= (1_u64 << 53), "stochastic metric {} exceeds exact f64 integer range", metric.id);
            *value as f64
         }
         RawObservationValue::Boolean(_) | RawObservationValue::Text(_) => bail!("metric {} has a non-numeric raw value", metric.id),
      };
      ensure!(value.is_finite(), "metric {} contains a nonfinite raw value", metric.id);
      samples.push(value);
   }
   Ok(samples)
}

fn starts_in_declared_order(reference: &LoadedSession, oxide: &LoadedSession, order: ComparisonOrder) -> bool
{
   match order
   {
      ComparisonOrder::Ab => reference.session.monotonic_start_ns.0 <= oxide.session.monotonic_start_ns.0,
      ComparisonOrder::Ba => oxide.session.monotonic_start_ns.0 <= reference.session.monotonic_start_ns.0,
   }
}

fn session_is_valid(session: &ComparisonSession) -> bool
{
   session.validation == "valid" && session.invalid_reason.is_none() && session.terminal_hard_outcome.is_none()
}

fn parse_estimator(value: &str) -> Result<WithinSessionEstimator>
{
   match value
   {
      "median" | "p50" => Ok(WithinSessionEstimator::Median),
      "p95" => Ok(WithinSessionEstimator::P95),
      "p99" => Ok(WithinSessionEstimator::P99),
      "maximum" | "max" | "peak" => Ok(WithinSessionEstimator::Maximum),
      other => bail!("unsupported within-session estimator `{}`", other),
   }
}

fn parse_effect_kind(value: &str) -> Result<PairEffectKind>
{
   match value
   {
      "badness-ratio" | "strictly-positive-ratio" => Ok(PairEffectKind::StrictlyPositiveRatio),
      "absolute-difference" | "risk-difference" | "zero-capable-difference" => Ok(PairEffectKind::ZeroCapableDifference),
      other => bail!("unsupported pair effect `{}`", other),
   }
}

fn convert_direction(direction: SpecMetricDirection) -> MetricDirection
{
   match direction
   {
      SpecMetricDirection::LowerIsBetter => MetricDirection::LowerIsBetter,
      SpecMetricDirection::HigherIsBetter => MetricDirection::HigherIsBetter,
   }
}

fn convert_alternative(alternative: SpecDecisionAlternative) -> DecisionAlternative
{
   match alternative
   {
      SpecDecisionAlternative::Lower => DecisionAlternative::Lower,
      SpecDecisionAlternative::Upper => DecisionAlternative::Upper,
   }
}

fn estimate(samples: &[f64], estimator: WithinSessionEstimator) -> f64
{
   match estimator
   {
      WithinSessionEstimator::Median => median(samples),
      WithinSessionEstimator::P95 => percentile(samples, 0.95),
      WithinSessionEstimator::P99 => percentile(samples, 0.99),
      WithinSessionEstimator::Maximum => samples.iter().copied().max_by(f64::total_cmp).unwrap_or(0.0),
   }
}

fn distribution(values: &[f64]) -> AbsoluteDistribution
{
   let center = median(values);
   let deviations = values.iter().map(|value| (value - center).abs()).collect::<Vec<_>>();
   let mean = values.iter().sum::<f64>() / values.len() as f64;
   let variance = values.iter().map(|value| (value - mean).powi(2)).sum::<f64>() / values.len() as f64;
   AbsoluteDistribution {
      session_estimators: values.to_vec(),
      p50: center,
      p95: percentile(values, 0.95),
      p99: percentile(values, 0.99),
      peak: values.iter().copied().max_by(f64::total_cmp).unwrap_or(0.0),
      mad: median(&deviations),
      coefficient_of_variation: (mean != 0.0).then(|| variance.sqrt() / mean.abs()),
   }
}

fn minimum_sample_count(rule: &str, estimator: WithinSessionEstimator) -> usize
{
   for key in ["frames", "observations", "samples", "events"]
   {
      if let Some(value) = minimum_from_rule(rule, key)
      {
         return value;
      }
   }
   match estimator
   {
      WithinSessionEstimator::P95 => 100,
      WithinSessionEstimator::P99 => 1_000,
      WithinSessionEstimator::Median | WithinSessionEstimator::Maximum => 1,
   }
}

fn minimum_from_rule(rule: &str, key: &str) -> Option<usize>
{
   rule.split([',', ';']).find_map(|clause| {
      let compact = clause.trim().replace(' ', "");
      let (name, value) = compact.split_once(">=")?;
      (name == key).then(|| value.parse::<usize>().ok()).flatten()
   })
}

fn block_length_samples(value: &str) -> usize
{
   let digits = value.bytes().filter(u8::is_ascii_digit).collect::<Vec<_>>();
   String::from_utf8(digits).ok().and_then(|value| value.parse().ok()).unwrap_or(1)
}

fn decision_boundary(kind: DecisionClaimKind, cell: &ComparisonCell, metric: &MetricDefinition) -> Result<f64>
{
   let materiality = parse_boundary(&cell.materiality_boundary).or_else(|_| parse_boundary(&metric.materiality_boundary))?.abs();
   match kind
   {
      DecisionClaimKind::OxideSuperiority => Ok(-materiality),
      DecisionClaimKind::ReferenceSuperiority => Ok(materiality),
      DecisionClaimKind::EquivalenceLower => Ok(-materiality),
      DecisionClaimKind::EquivalenceUpper => Ok(materiality),
      DecisionClaimKind::RequiredGuardrailNoninferiority => metric.guardrail_boundary.as_deref().context("required guardrail metric has no guardrail boundary").and_then(parse_boundary),
   }
}

fn parse_boundary(value: &str) -> Result<f64>
{
   let number = value.rsplit_once('=').map(|(_, number)| number).unwrap_or(value).trim();
   let parsed = number.parse::<f64>().with_context(|| format!("parsing decision boundary `{}`", value))?;
   ensure!(parsed.is_finite(), "decision boundary is not finite: {}", value);
   Ok(parsed)
}

fn metric_report<'a>(cell: &'a ComparisonCellReport, metric_id: &str) -> Option<&'a MetricAnalysisReport>
{
   if cell.primary_metric.metric_id == metric_id
   {
      Some(&cell.primary_metric)
   }
   else
   {
      cell.guardrails.iter().find(|guardrail| guardrail.metric_id == metric_id).map(|guardrail| &guardrail.analysis)
   }
}

fn adjusted_p(families: &[DecisionFamilyReport], family_id: Option<&str>, cell_id: &str, metric_id: &str) -> Option<f64>
{
   find_hypothesis(families, family_id, cell_id, metric_id).and_then(|hypothesis| hypothesis.adjusted_p_value)
}

fn find_hypothesis<'a>(families: &'a [DecisionFamilyReport], family_id: Option<&str>, cell_id: &str, metric_id: &str) -> Option<&'a HypothesisReport>
{
   let family_id = family_id?;
   families.iter().find(|family| family.family_id == family_id)?.members.iter().find(|member| member.comparison_cell_id == cell_id && member.metric_id == metric_id)
}

fn safe_relative_path(value: &str) -> Result<PathBuf>
{
   let path = Path::new(value);
   ensure!(!path.as_os_str().is_empty() && path.components().all(|component| matches!(component, Component::Normal(_))), "comparison artifact path must be a normalized relative path: {}", value);
   Ok(path.to_path_buf())
}

fn decode_json_lines<T>(bytes: &[u8], path: &Path) -> Result<Vec<T>> where T: for<'de> Deserialize<'de>
{
   let text = std::str::from_utf8(bytes).with_context(|| format!("{} is not UTF-8 JSONL", path.display()))?;
   text.lines().enumerate().filter(|(_, line)| !line.trim().is_empty()).map(|(index, line)| {
      serde_json::from_str(line).with_context(|| format!("decoding {} line {}", path.display(), index + 1))
   }).collect()
}

fn sha256(bytes: &[u8]) -> String
{
   format!("{:x}", Sha256::digest(bytes))
}

fn stable_seed(cell_id: &str, metric_id: &str) -> u64
{
   let digest = Sha256::digest(format!("{}\0{}", cell_id, metric_id).as_bytes());
   u64::from_le_bytes(digest[..8].try_into().expect("SHA-256 prefix is eight bytes"))
}

fn metric_subsystem(metric_id: &str) -> &'static str
{
   if metric_id.contains("gpu") { "renderer-gpu" }
   else if metric_id.contains("cpu") || metric_id.contains("main-thread") { "runtime-cpu" }
   else if metric_id.contains("memory") || metric_id.contains("retained") { "memory-lifecycle" }
   else if metric_id.contains("startup") || metric_id.contains("launch") || metric_id.contains("first-complete") { "startup-delivery" }
   else if metric_id.contains("byte") || metric_id.contains("request") { "web-delivery" }
   else if metric_id.contains("energy") { "energy" }
   else { "presentation-pipeline" }
}

fn backlog_priority(classification: ComparisonClassification) -> u8
{
   match classification
   {
      ComparisonClassification::ReferenceFaster => 0,
      ComparisonClassification::HardFailureNoPerformanceClaim => 1,
      ComparisonClassification::Inconclusive => 2,
      ComparisonClassification::EquivalentWithinMaterialityRegion => 3,
      ComparisonClassification::OxideFaster => 4,
   }
}

fn classification_label(classification: ComparisonClassification) -> &'static str
{
   match classification
   {
      ComparisonClassification::OxideFaster => "oxide-faster",
      ComparisonClassification::ReferenceFaster => "reference-faster",
      ComparisonClassification::EquivalentWithinMaterialityRegion => "equivalent-within-materiality-region",
      ComparisonClassification::Inconclusive => "inconclusive",
      ComparisonClassification::HardFailureNoPerformanceClaim => "hard-failure-no-performance-claim",
   }
}

fn yes_no(value: bool) -> &'static str
{
   if value { "yes" } else { "no" }
}
