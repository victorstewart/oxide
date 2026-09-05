use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstrumentationDecisionAlternative
{
   Lower,
   Upper,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct InstrumentationExactSignTest
{
   pub alternative: InstrumentationDecisionAlternative,
   pub less: usize,
   pub greater: usize,
   pub ties: usize,
   pub effective_n: usize,
   pub numerator: String,
   pub denominator: String,
   pub p_value: f64,
}

pub fn instrumentation_exact_sign_test(values: &[f64], boundary: f64, alternative: InstrumentationDecisionAlternative) -> Result<InstrumentationExactSignTest>
{
   ensure!(boundary.is_finite(), "sign-test boundary is not finite");
   let mut less = 0;
   let mut greater = 0;
   let mut ties = 0;
   for value in values
   {
      ensure!(value.is_finite(), "sign-test value is not finite");
      if *value < boundary {less += 1}
      else if *value > boundary {greater += 1}
      else {ties += 1}
   }
   let effective_n = less + greater;
   ensure!(effective_n <= 63, "exact sign test supports at most 63 non-tied pairs");
   let successes = match alternative
   {
      InstrumentationDecisionAlternative::Lower => less,
      InstrumentationDecisionAlternative::Upper => greater,
   };
   let denominator = 1_u128 << effective_n;
   let numerator = (successes..=effective_n).try_fold(0_u128, |sum, count| {
      sum.checked_add(binomial_coefficient(effective_n, count)).ok_or_else(|| anyhow::anyhow!("exact sign-test numerator overflow"))
   })?;
   Ok(InstrumentationExactSignTest {
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

pub fn instrumentation_median(samples: &[f64]) -> f64
{
   let mut sorted = samples.to_vec();
   sorted.sort_unstable_by(f64::total_cmp);
   if sorted.is_empty()
   {
      return 0.0;
   }
   let rank = (sorted.len() - 1) as f64 * 0.5;
   let lower = rank.floor() as usize;
   let upper = rank.ceil() as usize;
   if lower == upper {sorted[lower]}
   else {sorted[lower] + (sorted[upper] - sorted[lower]) * (rank - lower as f64)}
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
