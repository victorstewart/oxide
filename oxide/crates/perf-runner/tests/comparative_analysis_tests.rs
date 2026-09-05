use oxide_perf_runner::comparative::{
   classify_comparison, decision_estimand, exact_sign_test, exact_sign_test_resolution_floor,
   hierarchical_block_bootstrap_ci, holm_adjust, normalized_pair_effect, ClassificationEvidence,
   ComparisonClassification, DecisionAlternative, HolmMember, MetricDirection, PairEffectKind,
   PairedSessionSamples, WithinSessionEstimator,
};

#[test]
fn exact_sign_test_covers_both_directions_and_removes_ties()
{
   let values = [-2.0, -1.0, 0.0, 1.0];
   let lower = exact_sign_test(&values, 0.0, DecisionAlternative::Lower).expect("lower exact sign test");
   assert_eq!(lower.less, 2);
   assert_eq!(lower.greater, 1);
   assert_eq!(lower.ties, 1);
   assert_eq!(lower.effective_n, 3);
   assert_eq!(lower.numerator, "4");
   assert_eq!(lower.denominator, "8");
   assert_eq!(lower.p_value, 0.5);

   let upper = exact_sign_test(&values, 0.0, DecisionAlternative::Upper).expect("upper exact sign test");
   assert_eq!(upper.numerator, "7");
   assert_eq!(upper.denominator, "8");
   assert_eq!(upper.p_value, 0.875);
}

#[test]
fn exact_sign_test_matches_unanimous_release_boundary_vector()
{
   let values = vec![-0.10; 12];
   let test = exact_sign_test(&values, -0.05, DecisionAlternative::Lower).expect("unanimous lower sign test");
   assert_eq!(test.effective_n, 12);
   assert_eq!(test.numerator, "1");
   assert_eq!(test.denominator, "4096");
   assert_eq!(test.p_value, 1.0 / 4096.0);
}

#[test]
fn holm_adjustment_uses_lexical_tie_order_and_monotonic_values()
{
   let adjusted = holm_adjust(&[
      member("cell-b", "metric", "boundary", 0.01),
      member("cell-a", "metric", "boundary", 0.01),
      member("cell-c", "metric", "boundary", 0.04),
   ])
   .expect("Holm adjustment");
   assert_eq!(adjusted[0].comparison_cell_id, "cell-a");
   assert_eq!(adjusted[1].comparison_cell_id, "cell-b");
   assert_eq!(adjusted[2].comparison_cell_id, "cell-c");
   assert!((adjusted[0].adjusted_p_value - 0.03).abs() < f64::EPSILON);
   assert!((adjusted[1].adjusted_p_value - 0.03).abs() < f64::EPSILON);
   assert!((adjusted[2].adjusted_p_value - 0.04).abs() < f64::EPSILON);
}

#[test]
fn exact_resolution_floor_matches_default_holm_family_formula()
{
   assert_eq!(exact_sign_test_resolution_floor(1, 0.05).expect("single-member floor"), 5);
   assert_eq!(exact_sign_test_resolution_floor(10, 0.05).expect("ten-member floor"), 8);
   assert!(exact_sign_test_resolution_floor(0, 0.05).is_err());
   assert!(exact_sign_test_resolution_floor(1, 1.0).is_err());
}

#[test]
fn decision_helpers_reject_nonfinite_or_invalid_inputs()
{
   assert!(exact_sign_test(&[f64::NAN], 0.0, DecisionAlternative::Lower).is_err());
   assert!(exact_sign_test(&[0.0], f64::INFINITY, DecisionAlternative::Lower).is_err());
   assert!(holm_adjust(&[]).is_err());
   assert!(holm_adjust(&[member("cell", "metric", "boundary", f64::NAN)]).is_err());
   assert!(holm_adjust(&[member("cell", "metric", "boundary", 1.01)]).is_err());
}

#[test]
fn pair_effects_are_worse_is_positive_for_ratio_and_zero_capable_metrics()
{
   let lower_ratio = normalized_pair_effect(8.0, 10.0, MetricDirection::LowerIsBetter, PairEffectKind::StrictlyPositiveRatio).expect("lower-is-better ratio");
   let higher_ratio = normalized_pair_effect(120.0, 100.0, MetricDirection::HigherIsBetter, PairEffectKind::StrictlyPositiveRatio).expect("higher-is-better ratio");
   let lower_difference = normalized_pair_effect(0.0, 2.0, MetricDirection::LowerIsBetter, PairEffectKind::ZeroCapableDifference).expect("lower-is-better difference");
   let higher_difference = normalized_pair_effect(12.0, 10.0, MetricDirection::HigherIsBetter, PairEffectKind::ZeroCapableDifference).expect("higher-is-better difference");
   assert!((lower_ratio.exp() - 0.8).abs() < f64::EPSILON);
   assert!((higher_ratio.exp() - (100.0 / 120.0)).abs() < f64::EPSILON);
   assert_eq!(lower_difference, -2.0);
   assert_eq!(higher_difference, -2.0);
   assert!(normalized_pair_effect(0.0, 1.0, MetricDirection::LowerIsBetter, PairEffectKind::StrictlyPositiveRatio).is_err());
}

#[test]
fn decision_estimand_uses_arithmetic_midpoint_for_even_pair_count()
{
   assert_eq!(decision_estimand(&[-4.0, -1.0, 3.0, 10.0]).expect("decision estimand"), 1.0);
   assert!(decision_estimand(&[]).is_err());
}

#[test]
fn hierarchical_bootstrap_is_deterministic_and_resamples_whole_pairs_and_time_blocks()
{
   let pairs = vec![
      samples(&[8.0, 8.1, 7.9, 8.0], &[10.0, 10.1, 9.9, 10.0]),
      samples(&[7.8, 8.0, 8.2, 8.0], &[9.8, 10.0, 10.2, 10.0]),
      samples(&[8.1, 8.0, 7.9, 8.0], &[10.1, 10.0, 9.9, 10.0]),
      samples(&[7.9, 8.0, 8.1, 8.0], &[9.9, 10.0, 10.1, 10.0]),
   ];
   let first = hierarchical_block_bootstrap_ci(&pairs, MetricDirection::LowerIsBetter, PairEffectKind::StrictlyPositiveRatio, WithinSessionEstimator::P95, 2, 0x1234, 2_000).expect("hierarchical bootstrap");
   let second = hierarchical_block_bootstrap_ci(&pairs, MetricDirection::LowerIsBetter, PairEffectKind::StrictlyPositiveRatio, WithinSessionEstimator::P95, 2, 0x1234, 2_000).expect("repeat hierarchical bootstrap");
   assert_eq!(first, second);
   assert!(first[0] < first[1]);
   assert!(first[1] < 0.0);
}

#[test]
fn classification_requires_sufficiency_and_unambiguous_adjusted_decisions()
{
   let oxide = ClassificationEvidence {
      oxide_superiority_adjusted_p: Some(0.01),
      reference_superiority_adjusted_p: Some(0.9),
      equivalence_lower_adjusted_p: Some(0.9),
      equivalence_upper_adjusted_p: Some(0.01),
   };
   assert_eq!(classify_comparison(true, false, 0.05, oxide).expect("Oxide classification"), ComparisonClassification::OxideFaster);
   assert_eq!(classify_comparison(false, false, 0.05, oxide).expect("insufficient classification"), ComparisonClassification::Inconclusive);
   assert_eq!(classify_comparison(true, true, 0.05, oxide).expect("hard outcome classification"), ComparisonClassification::HardFailureNoPerformanceClaim);

   let equivalent = ClassificationEvidence {
      oxide_superiority_adjusted_p: None,
      reference_superiority_adjusted_p: None,
      equivalence_lower_adjusted_p: Some(0.02),
      equivalence_upper_adjusted_p: Some(0.03),
   };
   assert_eq!(classify_comparison(true, false, 0.05, equivalent).expect("equivalence classification"), ComparisonClassification::EquivalentWithinMaterialityRegion);

   let ambiguous = ClassificationEvidence {
      oxide_superiority_adjusted_p: Some(0.01),
      reference_superiority_adjusted_p: Some(0.01),
      equivalence_lower_adjusted_p: None,
      equivalence_upper_adjusted_p: None,
   };
   assert_eq!(classify_comparison(true, false, 0.05, ambiguous).expect("ambiguous classification"), ComparisonClassification::Inconclusive);
}

fn member(cell: &str, metric: &str, boundary: &str, p_value: f64) -> HolmMember
{
   HolmMember {
      comparison_cell_id: String::from(cell),
      metric_id: String::from(metric),
      boundary_id: String::from(boundary),
      p_value,
   }
}

fn samples(oxide: &[f64], reference: &[f64]) -> PairedSessionSamples
{
   PairedSessionSamples { oxide: oxide.to_vec(), reference: reference.to_vec() }
}
