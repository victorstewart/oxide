use oxide_benchmark_spec::{canonical_macos_comparator_scale_overlay_json, macos_comparator_scale_contract, validate_macos_comparator_scale_overlay, ArtifactIdentity, MacOsComparatorScale, MacOsComparatorScaleDimension, MacOsComparatorScaleOverlay, MacOsComparatorScaleTransform};

const SCENARIOS: [(&str, MacOsComparatorScaleDimension, MacOsComparatorScaleTransform, u64); 13] = [
   ("startup.first-screen", MacOsComparatorScaleDimension::DatasetCardinality, MacOsComparatorScaleTransform::NamespacedDatasetShards, 24 * 1_024),
   ("dashboard.mixed-static", MacOsComparatorScaleDimension::OperationCardinality, MacOsComparatorScaleTransform::IsolatedOperationShadow, 20),
   ("feed.variable-scroll", MacOsComparatorScaleDimension::DatasetCardinality, MacOsComparatorScaleTransform::NamespacedDatasetShards, 2_000),
   ("grid.large-scroll", MacOsComparatorScaleDimension::DatasetCardinality, MacOsComparatorScaleTransform::NamespacedDatasetShards, 10_000),
   ("chat.live-update", MacOsComparatorScaleDimension::DatasetCardinality, MacOsComparatorScaleTransform::NamespacedDatasetShards, 5_000),
   ("navigation.modal", MacOsComparatorScaleDimension::OperationCardinality, MacOsComparatorScaleTransform::IsolatedOperationShadow, 4),
   ("image.decode-zoom", MacOsComparatorScaleDimension::OperationCardinality, MacOsComparatorScaleTransform::IsolatedOperationShadow, 1),
   ("effects.layers", MacOsComparatorScaleDimension::OperationCardinality, MacOsComparatorScaleTransform::IsolatedOperationShadow, 3),
   ("mutation.damage", MacOsComparatorScaleDimension::DatasetCardinality, MacOsComparatorScaleTransform::NamespacedDatasetShards, 10_000),
   ("text.multilingual", MacOsComparatorScaleDimension::DatasetCardinality, MacOsComparatorScaleTransform::NamespacedDatasetShards, 1_000),
   ("resize.theme", MacOsComparatorScaleDimension::OperationCardinality, MacOsComparatorScaleTransform::IsolatedOperationShadow, 10),
   ("idle.steady", MacOsComparatorScaleDimension::OperationCardinality, MacOsComparatorScaleTransform::IsolatedOperationShadow, 1),
   ("endurance.churn", MacOsComparatorScaleDimension::OperationCardinality, MacOsComparatorScaleTransform::IsolatedOperationShadow, 1_300),
];

#[test]
fn all_release_scenarios_have_exact_one_and_two_x_contracts()
{
   for (scenario_id, dimension, transform, base_cardinality) in SCENARIOS
   {
      assert_eq!(macos_comparator_scale_contract(scenario_id).expect("frozen scale contract"), (dimension, transform, base_cardinality));
      let fixture = fixture();
      for (scale, multiplier) in [(MacOsComparatorScale::OneX, 1), (MacOsComparatorScale::TwoX, 2)]
      {
         let overlay = MacOsComparatorScaleOverlay {
            schema_version: 1,
            scenario_id: String::from(scenario_id),
            scale,
            dimension,
            transform,
            base_cardinality,
            effective_cardinality: base_cardinality * multiplier,
            fixture: fixture.clone(),
         };
         validate_macos_comparator_scale_overlay(&overlay, scenario_id, &fixture).expect("valid scale overlay");
         assert_eq!(canonical_macos_comparator_scale_overlay_json(&overlay).expect("canonical overlay"), canonical_macos_comparator_scale_overlay_json(&overlay).expect("canonical overlay again"));
      }
   }
}

#[test]
fn scaling_rejects_wrong_transform_cardinality_fixture_and_unknown_scenario()
{
   let fixture = fixture();
   let mut overlay = MacOsComparatorScaleOverlay {
      schema_version: 1,
      scenario_id: String::from("feed.variable-scroll"),
      scale: MacOsComparatorScale::TwoX,
      dimension: MacOsComparatorScaleDimension::DatasetCardinality,
      transform: MacOsComparatorScaleTransform::IsolatedOperationShadow,
      base_cardinality: 2_000,
      effective_cardinality: 4_000,
      fixture: fixture.clone(),
   };
   assert!(validate_macos_comparator_scale_overlay(&overlay, "feed.variable-scroll", &fixture).expect_err("wrong transform").to_string().contains("transform"));
   overlay.transform = MacOsComparatorScaleTransform::NamespacedDatasetShards;
   overlay.effective_cardinality = 2_000;
   assert!(validate_macos_comparator_scale_overlay(&overlay, "feed.variable-scroll", &fixture).expect_err("wrong cardinality").to_string().contains("effective cardinality"));
   overlay.effective_cardinality = 4_000;
   assert!(validate_macos_comparator_scale_overlay(&overlay, "feed.variable-scroll", &ArtifactIdentity {path: String::from("other"), sha256: String::from("1").repeat(64)}).expect_err("wrong fixture").to_string().contains("fixture identity"));
   assert!(macos_comparator_scale_contract("unknown").is_err());
}

fn fixture() -> ArtifactIdentity
{
   ArtifactIdentity {
      path: String::from("fixtures/frozen.json"),
      sha256: String::from("0").repeat(64),
   }
}
