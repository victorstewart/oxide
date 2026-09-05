use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

pub use crate::paired_statistics::{DecisionAlternative, ExactSignTest, HolmDecision, HolmMember};
use crate::paired_statistics;

pub const COMPARATIVE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MetricDirection
{
   LowerIsBetter,
   HigherIsBetter,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PairEffectKind
{
   StrictlyPositiveRatio,
   ZeroCapableDifference,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WithinSessionEstimator
{
   Median,
   P95,
   P99,
   Maximum,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PairedSessionSamples
{
   pub oxide: Vec<f64>,
   pub reference: Vec<f64>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct ClassificationEvidence
{
   pub oxide_superiority_adjusted_p: Option<f64>,
   pub reference_superiority_adjusted_p: Option<f64>,
   pub equivalence_lower_adjusted_p: Option<f64>,
   pub equivalence_upper_adjusted_p: Option<f64>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ComparisonClassification
{
   OxideFaster,
   ReferenceFaster,
   EquivalentWithinMaterialityRegion,
   Inconclusive,
   HardFailureNoPerformanceClaim,
}

pub fn exact_sign_test(values: &[f64], boundary: f64, alternative: DecisionAlternative) -> Result<ExactSignTest>
{
   paired_statistics::exact_sign_test(values, boundary, alternative)
}

pub fn holm_adjust(members: &[HolmMember]) -> Result<Vec<HolmDecision>>
{
   paired_statistics::holm_adjust(members)
}

pub fn exact_sign_test_resolution_floor(family_size: usize, alpha: f64) -> Result<usize>
{
   paired_statistics::exact_sign_test_resolution_floor(family_size, alpha)
}

pub fn normalized_pair_effect(oxide: f64, reference: f64, direction: MetricDirection, effect_kind: PairEffectKind) -> Result<f64>
{
   ensure!(oxide.is_finite() && reference.is_finite(), "paired values must be finite");
   let effect = match effect_kind
   {
      PairEffectKind::StrictlyPositiveRatio =>
      {
         ensure!(oxide > 0.0 && reference > 0.0, "ratio pair effects require strictly positive values");
         match direction
         {
            MetricDirection::LowerIsBetter => (oxide / reference).ln(),
            MetricDirection::HigherIsBetter => (reference / oxide).ln(),
         }
      }
      PairEffectKind::ZeroCapableDifference => match direction
      {
         MetricDirection::LowerIsBetter => oxide - reference,
         MetricDirection::HigherIsBetter => reference - oxide,
      },
   };
   Ok(effect)
}

pub fn decision_estimand(pair_effects: &[f64]) -> Result<f64>
{
   ensure!(!pair_effects.is_empty(), "comparative decision has no valid pairs");
   ensure!(pair_effects.iter().all(|effect| effect.is_finite()), "comparative pair effect is not finite");
   Ok(paired_statistics::median(pair_effects))
}

pub fn hierarchical_block_bootstrap_ci(pairs: &[PairedSessionSamples], direction: MetricDirection, effect_kind: PairEffectKind, estimator: WithinSessionEstimator, block_len: usize, seed: u64, resamples: usize) -> Result<[f64; 2]>
{
   ensure!(!pairs.is_empty(), "hierarchical bootstrap has no pairs");
   ensure!(block_len > 0, "hierarchical bootstrap block length is zero");
   ensure!(resamples > 0, "hierarchical bootstrap resample count is zero");
   for pair in pairs
   {
      ensure!(!pair.oxide.is_empty() && !pair.reference.is_empty(), "hierarchical bootstrap session is empty");
      ensure!(pair.oxide.iter().chain(pair.reference.iter()).all(|sample| sample.is_finite()), "hierarchical bootstrap sample is not finite");
   }

   let mut state = seed.max(1);
   let mut effects = Vec::with_capacity(resamples);
   let mut pair_effects = Vec::with_capacity(pairs.len());
   for _ in 0..resamples
   {
      pair_effects.clear();
      for _ in 0..pairs.len()
      {
         state = next_random(state);
         let pair = &pairs[(state as usize) % pairs.len()];
         let oxide = resampled_session_estimator(&pair.oxide, estimator, block_len, &mut state);
         let reference = resampled_session_estimator(&pair.reference, estimator, block_len, &mut state);
         pair_effects.push(normalized_pair_effect(oxide, reference, direction, effect_kind)?);
      }
      effects.push(decision_estimand(&pair_effects)?);
   }
   effects.sort_unstable_by(f64::total_cmp);
   Ok([
      paired_statistics::percentile_sorted(&effects, 0.025),
      paired_statistics::percentile_sorted(&effects, 0.975),
   ])
}

pub fn classify_comparison(sufficient: bool, terminal_hard_outcome: bool, alpha: f64, evidence: ClassificationEvidence) -> Result<ComparisonClassification>
{
   ensure!(alpha.is_finite() && alpha > 0.0 && alpha < 1.0, "classification alpha is outside (0, 1)");
   for value in [
      evidence.oxide_superiority_adjusted_p,
      evidence.reference_superiority_adjusted_p,
      evidence.equivalence_lower_adjusted_p,
      evidence.equivalence_upper_adjusted_p,
   ]
   {
      ensure!(value.is_none_or(|value| value.is_finite() && (0.0..=1.0).contains(&value)), "classification p-value is outside [0, 1]");
   }
   if terminal_hard_outcome
   {
      return Ok(ComparisonClassification::HardFailureNoPerformanceClaim);
   }
   if !sufficient
   {
      return Ok(ComparisonClassification::Inconclusive);
   }

   let oxide = evidence.oxide_superiority_adjusted_p.is_some_and(|value| value <= alpha);
   let reference = evidence.reference_superiority_adjusted_p.is_some_and(|value| value <= alpha);
   let equivalent = evidence.equivalence_lower_adjusted_p.is_some_and(|value| value <= alpha)
      && evidence.equivalence_upper_adjusted_p.is_some_and(|value| value <= alpha);
   match (oxide, reference, equivalent)
   {
      (true, false, false) => Ok(ComparisonClassification::OxideFaster),
      (false, true, false) => Ok(ComparisonClassification::ReferenceFaster),
      (false, false, true) => Ok(ComparisonClassification::EquivalentWithinMaterialityRegion),
      _ => Ok(ComparisonClassification::Inconclusive),
   }
}

fn resampled_session_estimator(samples: &[f64], estimator: WithinSessionEstimator, block_len: usize, state: &mut u64) -> f64
{
   let mut resampled = Vec::with_capacity(samples.len());
   while resampled.len() < samples.len()
   {
      *state = next_random(*state);
      let start = (*state as usize) % samples.len();
      for offset in 0..block_len
      {
         if resampled.len() == samples.len()
         {
            break;
         }
         resampled.push(samples[(start + offset) % samples.len()]);
      }
   }
   match estimator
   {
      WithinSessionEstimator::Median => paired_statistics::median(&resampled),
      WithinSessionEstimator::P95 => paired_statistics::percentile(&resampled, 0.95),
      WithinSessionEstimator::P99 => paired_statistics::percentile(&resampled, 0.99),
      WithinSessionEstimator::Maximum => resampled.into_iter().max_by(f64::total_cmp).unwrap_or(0.0),
   }
}

fn next_random(mut value: u64) -> u64
{
   value ^= value << 13;
   value ^= value >> 7;
   value ^= value << 17;
   value
}
