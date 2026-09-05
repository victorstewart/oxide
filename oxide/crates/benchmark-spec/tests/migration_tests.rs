use std::collections::BTreeSet;

use oxide_benchmark_spec::{validate_legacy_case_disposition, LegacyCaseDisposition, LegacyDisposition, LegacyDispositionGroup};

#[test]
fn legacy_disposition_requires_exactly_one_mapping_per_source_method()
{
   let expected = methods(&["Suite.testA", "Suite.testB"]);
   assert!(validate_legacy_case_disposition(&manifest(vec![
      group("comparative", LegacyDisposition::ComparativePhase, "dashboard.mixed-static", &["latency"], &["Suite.testA"]),
      group("correctness", LegacyDisposition::FastCorrectness, "harness-unit", &["semantics"], &["Suite.testB"]),
   ]), &expected).is_ok());

   let duplicate = manifest(vec![
      group("comparative", LegacyDisposition::ComparativePhase, "dashboard.mixed-static", &["latency"], &["Suite.testA"]),
      group("duplicate", LegacyDisposition::RemoveDuplicate, "target.duplicate", &["semantics"], &["Suite.testA", "Suite.testB"]),
   ]);
   assert!(validate_legacy_case_disposition(&duplicate, &expected).is_err());

   let missing = manifest(vec![group("comparative", LegacyDisposition::ComparativePhase, "dashboard.mixed-static", &["latency", "semantics"], &["Suite.testA"])]);
   assert!(validate_legacy_case_disposition(&missing, &expected).is_err());
}

#[test]
fn removed_duplicates_cannot_preserve_required_risk_coverage()
{
   let expected = methods(&["Suite.testA", "Suite.testB"]);
   let manifest = manifest(vec![
      group("comparative", LegacyDisposition::ComparativePhase, "dashboard.mixed-static", &["latency"], &["Suite.testA"]),
      group("duplicate", LegacyDisposition::RemoveDuplicate, "target.duplicate", &["semantics"], &["Suite.testB"]),
   ]);
   assert!(validate_legacy_case_disposition(&manifest, &expected).is_err());
}

fn manifest(groups: Vec<LegacyDispositionGroup>) -> LegacyCaseDisposition
{
   LegacyCaseDisposition {
      schema_version: 1,
      source_revision: String::from("reviewed-revision"),
      source_files: vec![String::from("A.swift"), String::from("B.swift")],
      required_risk_dimensions: vec![String::from("latency"), String::from("semantics")],
      known_coverage_gaps: Vec::new(),
      groups,
   }
}

fn group(id: &str, disposition: LegacyDisposition, target_id: &str, risks: &[&str], methods: &[&str]) -> LegacyDispositionGroup
{
   LegacyDispositionGroup {
      id: String::from(id),
      disposition,
      target_id: String::from(target_id),
      covered_risk_dimensions: risks.iter().map(|value| String::from(*value)).collect(),
      methods: methods.iter().map(|value| String::from(*value)).collect(),
   }
}

fn methods(values: &[&str]) -> BTreeSet<String>
{
   values.iter().map(|value| String::from(*value)).collect()
}
