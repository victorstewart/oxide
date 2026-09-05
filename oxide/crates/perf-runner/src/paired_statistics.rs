use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PairOrder
{
   Ab,
   Ba,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DecisionAlternative
{
   Lower,
   Upper,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ExactSignTest
{
   pub alternative: DecisionAlternative,
   pub less: usize,
   pub greater: usize,
   pub ties: usize,
   pub effective_n: usize,
   pub numerator: String,
   pub denominator: String,
   pub p_value: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct HolmMember
{
   pub comparison_cell_id: String,
   pub metric_id: String,
   pub boundary_id: String,
   pub p_value: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct HolmDecision
{
   pub comparison_cell_id: String,
   pub metric_id: String,
   pub boundary_id: String,
   pub rank: usize,
   pub p_value: f64,
   pub adjusted_p_value: f64,
}

pub fn balanced_pair_order(seed: u64, pair_count: usize) -> Vec<PairOrder>
{
   let mut state = seed.max(1);
   let mut orders = Vec::with_capacity(pair_count);
   while orders.len() < pair_count
   {
      state = xorshift64(state);
      let block = if state & 1 == 0
      {
         [PairOrder::Ab, PairOrder::Ba, PairOrder::Ba, PairOrder::Ab]
      }
      else
      {
         [PairOrder::Ba, PairOrder::Ab, PairOrder::Ab, PairOrder::Ba]
      };
      let remaining = pair_count - orders.len();
      orders.extend_from_slice(&block[..remaining.min(block.len())]);
   }
   orders
}

pub fn relative_speedup_pct(baseline: f64, candidate: f64, lower_is_better: bool) -> f64
{
   if baseline == 0.0
   {
      if candidate == 0.0
      {
         return 0.0;
      }
      return if lower_is_better { f64::NEG_INFINITY } else { f64::INFINITY };
   }
   if lower_is_better
   {
      (baseline - candidate) / baseline * 100.0
   }
   else
   {
      (candidate - baseline) / baseline * 100.0
   }
}

pub fn regresses_by_ratio(baseline: f64, candidate: f64, lower_is_better: bool, allowed_ratio: f64) -> bool
{
   if lower_is_better
   {
      candidate > baseline * allowed_ratio
   }
   else
   {
      baseline > candidate * allowed_ratio
   }
}

pub fn exact_sign_test(values: &[f64], boundary: f64, alternative: DecisionAlternative) -> Result<ExactSignTest>
{
   ensure!(boundary.is_finite(), "sign-test boundary is not finite");
   let mut less = 0;
   let mut greater = 0;
   let mut ties = 0;
   for value in values
   {
      ensure!(value.is_finite(), "sign-test value is not finite");
      if *value < boundary
      {
         less += 1;
      }
      else if *value > boundary
      {
         greater += 1;
      }
      else
      {
         ties += 1;
      }
   }

   let effective_n = less + greater;
   ensure!(effective_n <= 63, "exact sign test supports at most 63 non-tied pairs");
   let successes = match alternative
   {
      DecisionAlternative::Lower => less,
      DecisionAlternative::Upper => greater,
   };
   let denominator = 1_u128 << effective_n;
   let numerator = (successes..=effective_n).try_fold(0_u128, |sum, count| {
      sum.checked_add(binomial_coefficient(effective_n, count)).ok_or_else(|| anyhow::anyhow!("exact sign-test numerator overflow"))
   })?;

   Ok(ExactSignTest {
      alternative,
      less,
      greater,
      ties,
      effective_n,
      numerator: numerator.to_string(),
      denominator: denominator.to_string(),
      p_value: numerator as f64 / denominator as f64,
   })
}

pub fn holm_adjust(members: &[HolmMember]) -> Result<Vec<HolmDecision>>
{
   ensure!(!members.is_empty(), "Holm decision family is empty");
   for member in members
   {
      ensure!(member.p_value.is_finite() && (0.0..=1.0).contains(&member.p_value), "Holm member p-value is outside [0, 1]");
      ensure!(!member.comparison_cell_id.is_empty(), "Holm member comparison cell id is empty");
      ensure!(!member.metric_id.is_empty(), "Holm member metric id is empty");
      ensure!(!member.boundary_id.is_empty(), "Holm member boundary id is empty");
   }

   let mut ordered = members.to_vec();
   ordered.sort_by(|left, right| {
      left.p_value
         .total_cmp(&right.p_value)
         .then_with(|| left.comparison_cell_id.cmp(&right.comparison_cell_id))
         .then_with(|| left.metric_id.cmp(&right.metric_id))
         .then_with(|| left.boundary_id.cmp(&right.boundary_id))
   });

   let count = ordered.len();
   let mut running = 0.0_f64;
   Ok(ordered
      .into_iter()
      .enumerate()
      .map(|(index, member)| {
         running = running.max(((count - index) as f64 * member.p_value).min(1.0));
         HolmDecision {
            comparison_cell_id: member.comparison_cell_id,
            metric_id: member.metric_id,
            boundary_id: member.boundary_id,
            rank: index + 1,
            p_value: member.p_value,
            adjusted_p_value: running,
         }
      })
      .collect())
}

pub fn exact_sign_test_resolution_floor(family_size: usize, alpha: f64) -> Result<usize>
{
   ensure!(family_size > 0, "decision family is empty");
   ensure!(alpha.is_finite() && alpha > 0.0 && alpha < 1.0, "decision alpha is outside (0, 1)");
   Ok(((family_size as f64 / alpha).log2().ceil() as usize).max(1))
}

pub fn paired_bootstrap_ci(values: &[f64], seed: u64, resamples: usize) -> [f64; 2]
{
   let mut state = seed.max(1);
   let mut medians = Vec::with_capacity(resamples);
   let mut resample = vec![0.0; values.len()];
   for _ in 0..resamples
   {
      for value in &mut resample
      {
         state = xorshift64(state);
         *value = values[(state as usize) % values.len()];
      }
      medians.push(median(&resample));
   }
   medians.sort_unstable_by(f64::total_cmp);
   [percentile_sorted(&medians, 0.025), percentile_sorted(&medians, 0.975)]
}

pub fn median(samples: &[f64]) -> f64
{
   let mut sorted = samples.to_vec();
   sorted.sort_unstable_by(f64::total_cmp);
   percentile_sorted(&sorted, 0.50)
}

pub fn percentile(samples: &[f64], quantile: f64) -> f64
{
   let mut sorted = samples.to_vec();
   sorted.sort_unstable_by(f64::total_cmp);
   percentile_sorted(&sorted, quantile)
}

pub fn percentile_sorted(sorted: &[f64], quantile: f64) -> f64
{
   if sorted.is_empty()
   {
      return 0.0;
   }
   let rank = quantile.clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
   let lower = rank.floor() as usize;
   let upper = rank.ceil() as usize;
   if lower == upper
   {
      sorted[lower]
   }
   else
   {
      sorted[lower] + (sorted[upper] - sorted[lower]) * (rank - lower as f64)
   }
}

fn xorshift64(mut value: u64) -> u64
{
   value ^= value << 13;
   value ^= value >> 7;
   value ^= value << 17;
   value
}

fn binomial_coefficient(n: usize, k: usize) -> u128
{
   let k = k.min(n - k);
   let mut result = 1_u128;
   for index in 0..k
   {
      result = result * (n - index) as u128 / (index + 1) as u128;
   }
   result
}
