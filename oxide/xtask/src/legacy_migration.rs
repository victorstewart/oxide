use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use oxide_benchmark_spec::{canonical_legacy_case_disposition_json, load_legacy_case_disposition, validate_legacy_case_disposition};

pub const LEGACY_PERF_SOURCE_FILES: &[&str] = &[
   "host/ios-app/App/OxideHostPerfTests/OxideHostPerfTests.swift",
   "host/ios-app/App/OxideHostUITests/OxideUIKitLaunchPerfTests.swift",
];

pub fn validate_workspace_legacy_disposition(workspace_root: &Path) -> Result<(usize, usize)>
{
   let (manifest_path, manifest) = load_legacy_case_disposition(workspace_root)?;
   let reviewed_files = manifest.source_files.iter().map(String::as_str).collect::<BTreeSet<_>>();
   let expected_files = LEGACY_PERF_SOURCE_FILES.iter().copied().collect::<BTreeSet<_>>();
   if reviewed_files != expected_files
   {
      bail!("legacy disposition source_files do not match the two canonical XCTest sources");
   }
   let mut methods = BTreeSet::new();
   for relative_path in LEGACY_PERF_SOURCE_FILES
   {
      let path = workspace_root.join(relative_path);
      let source = fs::read_to_string(&path).with_context(|| format!("reading legacy XCTest source {}", path.display()))?;
      let suite = path.file_stem().and_then(|value| value.to_str()).context("legacy XCTest source must have a UTF-8 file stem")?;
      for method in extract_swift_test_methods(&source)?
      {
         let qualified = format!("{}.{}", suite, method);
         if !methods.insert(qualified.clone())
         {
            bail!("legacy XCTest method appears more than once in source: `{}`", qualified);
         }
      }
   }
   validate_legacy_case_disposition(&manifest, &methods)?;
   let canonical = canonical_legacy_case_disposition_json(&manifest)?;
   let committed = fs::read(&manifest_path).with_context(|| format!("reading {}", manifest_path.display()))?;
   if committed != canonical
   {
      bail!("legacy disposition is not canonical pretty JSON with one trailing newline: {}", manifest_path.display());
   }
   Ok((methods.len(), manifest.groups.len()))
}

pub fn extract_swift_test_methods(source: &str) -> Result<Vec<String>>
{
   let mut methods = Vec::new();
   for (line_index, line) in source.lines().enumerate()
   {
      let trimmed = line.trim_start();
      let Some(rest) = trimmed.strip_prefix("func test") else
      {
         continue;
      };
      let Some(parenthesis) = rest.find('(') else
      {
         bail!("Swift test declaration on line {} has no argument list", line_index + 1);
      };
      let suffix = &rest[..parenthesis];
      if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
      {
         bail!("Swift test declaration on line {} has an invalid method name", line_index + 1);
      }
      methods.push(format!("test{}", suffix));
   }
   Ok(methods)
}
