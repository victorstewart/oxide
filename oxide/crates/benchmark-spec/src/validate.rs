use anyhow::{ensure, Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::comparison::{CommonIdentity, ComparisonPlan, DecisionClaimKind, ImplementationIdentity};
use crate::migration::COMPARISON_SCENARIO_IDS;
use crate::schema::{BudgetSpec, Platform, Tier, BENCHMARK_SPEC_SCHEMA_VERSION};
use crate::scenario::{AssetManifest, FontPackManifest, ScenarioSpec, TraceEvent, TraceOperation};

struct ExpectedBudget
{
   id: &'static str,
   platform: Platform,
   tier: Tier,
   hard_total_seconds: u64,
   campaign_critical_wall_seconds: u64,
   campaign_aggregate_seconds: u64,
}

const EXPECTED_BUDGETS: &[ExpectedBudget] = &[
   ExpectedBudget {
      id: "apple-pr", platform: Platform::Apple, tier: Tier::Pr, hard_total_seconds: 906,
      campaign_critical_wall_seconds: 906, campaign_aggregate_seconds: 906,
   },
   ExpectedBudget {
      id: "web-pr", platform: Platform::Web, tier: Tier::Pr, hard_total_seconds: 954,
      campaign_critical_wall_seconds: 954, campaign_aggregate_seconds: 954,
   },
   ExpectedBudget {
      id: "nightly-apple", platform: Platform::Apple, tier: Tier::Nightly,
      hard_total_seconds: 5_112, campaign_critical_wall_seconds: 5_112,
      campaign_aggregate_seconds: 5_112,
   },
   ExpectedBudget {
      id: "nightly-web-engine", platform: Platform::Web, tier: Tier::Nightly,
      hard_total_seconds: 4_968, campaign_critical_wall_seconds: 4_968,
      campaign_aggregate_seconds: 6_408,
   },
   ExpectedBudget {
      id: "nightly-web-mobile", platform: Platform::Web, tier: Tier::Nightly,
      hard_total_seconds: 1_440, campaign_critical_wall_seconds: 4_968,
      campaign_aggregate_seconds: 6_408,
   },
   ExpectedBudget {
      id: "apple-release-core", platform: Platform::Apple, tier: Tier::ReleaseCore,
      hard_total_seconds: 20_952, campaign_critical_wall_seconds: 20_952,
      campaign_aggregate_seconds: 20_952,
   },
   ExpectedBudget {
      id: "web-release-core", platform: Platform::Web, tier: Tier::ReleaseCore,
      hard_total_seconds: 20_664, campaign_critical_wall_seconds: 20_664,
      campaign_aggregate_seconds: 20_664,
   },
   ExpectedBudget {
      id: "apple-release-claim-complete", platform: Platform::Apple,
      tier: Tier::ClaimComplete, hard_total_seconds: 33_574,
      campaign_critical_wall_seconds: 33_574, campaign_aggregate_seconds: 33_574,
   },
   ExpectedBudget {
      id: "web-release-claim-complete", platform: Platform::Web,
      tier: Tier::ClaimComplete, hard_total_seconds: 33_430,
      campaign_critical_wall_seconds: 33_430, campaign_aggregate_seconds: 33_430,
   },
];

pub fn validate_budget(budget: &BudgetSpec) -> Result<()>
{
   ensure!(budget.schema_version == BENCHMARK_SPEC_SCHEMA_VERSION, "budget {} has unsupported schema version {}", budget.id, budget.schema_version);
   ensure!(!budget.id.is_empty(), "budget id is empty");
   let computed_pre_reserve = budget.computed_pre_reserve_seconds().context("budget component arithmetic overflows u64")?;
   let computed_reserve = budget.computed_reserve_seconds().context("budget reserve arithmetic overflows u64")?;
   let computed_hard_total = budget.computed_hard_total_seconds().context("budget hard-total arithmetic overflows u64")?;
   ensure!(budget.pre_reserve_seconds == computed_pre_reserve, "budget {} pre-reserve component arithmetic differs: declared {}, computed {}", budget.id, budget.pre_reserve_seconds, computed_pre_reserve);
   ensure!(budget.reserve_seconds == computed_reserve, "budget {} reserve differs: declared {}, computed {}", budget.id, budget.reserve_seconds, computed_reserve);
   ensure!(budget.hard_total_seconds == computed_hard_total, "budget {} hard total differs: declared {}, computed {}", budget.id, budget.hard_total_seconds, computed_hard_total);
   ensure!(budget.hard_total_seconds <= budget.campaign_aggregate_seconds, "budget {} shard exceeds campaign aggregate", budget.id);
   ensure!(budget.campaign_critical_wall_seconds <= budget.campaign_aggregate_seconds, "budget {} critical wall exceeds campaign aggregate", budget.id);
   Ok(())
}

pub fn validate_default_budget_set(budgets: &[(PathBuf, BudgetSpec)]) -> Result<()>
{
   ensure!(budgets.len() == EXPECTED_BUDGETS.len(), "default comparative budget set has {} files, expected {}", budgets.len(), EXPECTED_BUDGETS.len());
   let expected = EXPECTED_BUDGETS.iter().map(|budget| (budget.id, budget)).collect::<BTreeMap<_, _>>();
   let mut seen = BTreeSet::new();
   for (path, budget) in budgets
   {
      validate_budget(budget)?;
      ensure!(seen.insert(budget.id.as_str()), "duplicate comparative budget id {}", budget.id);
      let contract = expected.get(budget.id.as_str()).copied().ok_or_else(|| anyhow::anyhow!("unexpected comparative budget id {}", budget.id))?;
      ensure!(budget.platform == contract.platform, "budget {} platform differs from benchmark-spec v1", budget.id);
      ensure!(budget.tier == contract.tier, "budget {} tier differs from benchmark-spec v1", budget.id);
      ensure!(budget.hard_total_seconds == contract.hard_total_seconds, "budget {} hard total differs from benchmark-spec v1", budget.id);
      ensure!(budget.campaign_critical_wall_seconds == contract.campaign_critical_wall_seconds, "budget {} critical wall differs from benchmark-spec v1", budget.id);
      ensure!(budget.campaign_aggregate_seconds == contract.campaign_aggregate_seconds, "budget {} aggregate differs from benchmark-spec v1", budget.id);
      ensure!(path.file_stem().and_then(|name| name.to_str()) == Some(budget.id.as_str()), "budget {} file name does not match its id", budget.id);
   }
   ensure!(seen.len() == expected.len(), "default comparative budget set is incomplete");
   Ok(())
}

pub fn validate_scenario(scenario: &ScenarioSpec) -> Result<()>
{
   ensure!(scenario.is_v1(), "scenario {} has unsupported schema version {}", scenario.id, scenario.schema_version);
   ensure!(!scenario.id.is_empty(), "scenario id is empty");
   validate_artifact_sha256(&scenario.fixture.sha256, "fixture")?;
   validate_artifact_sha256(&scenario.assets.sha256, "asset manifest")?;
   validate_artifact_sha256(&scenario.font_pack.sha256, "font pack")?;
   ensure!(!scenario.fixture.path.is_empty(), "scenario {} has no fixture path", scenario.id);
   ensure!(!scenario.assets.path.is_empty(), "scenario {} has no asset-manifest path", scenario.id);
   ensure!(scenario.fixture.path.ends_with(".json") && scenario.assets.path.ends_with(".json"), "scenario {} fixture and asset manifest must be JSON", scenario.id);
   ensure!(!scenario.font_pack.id.is_empty(), "scenario {} has no font-pack id", scenario.id);
   ensure!(!scenario.font_pack.manifest.is_empty(), "scenario {} has no font-pack manifest", scenario.id);
   ensure!(scenario.font_pack.manifest.ends_with(".json"), "scenario {} font-pack manifest must be JSON", scenario.id);
   ensure!(!scenario.viewport_class.is_empty(), "scenario {} has no viewport class", scenario.id);
   ensure!(!scenario.scene.roles.is_empty(), "scenario {} has no semantic roles", scenario.id);
   unique_strings("scenario role", &scenario.scene.roles)?;
   ensure!(!scenario.scene.style_tokens.path.is_empty(), "scenario {} has no style-token path", scenario.id);
   ensure!(!scenario.scene.layout_assertions.path.is_empty(), "scenario {} has no layout-assertion path", scenario.id);
   ensure!(scenario.scene.style_tokens.path.ends_with(".json") && scenario.scene.layout_assertions.path.ends_with(".json"), "scenario {} style and layout artifacts must be JSON", scenario.id);
   validate_artifact_sha256(&scenario.scene.style_tokens.sha256, "style tokens")?;
   validate_artifact_sha256(&scenario.scene.layout_assertions.sha256, "layout assertions")?;
   ensure!(!scenario.primary_metric.is_empty(), "scenario {} has no primary metric", scenario.id);
   ensure!(!scenario.required_metrics.is_empty(), "scenario {} has no required metrics", scenario.id);
   ensure!(!scenario.parity_checkpoints.is_empty(), "scenario {} has no parity checkpoints", scenario.id);
   ensure!(!scenario.phases.is_empty(), "scenario {} has no phases", scenario.id);
   ensure!(scenario.phases.iter().any(|phase| phase.measured), "scenario {} has no measured phase", scenario.id);
   ensure!(scenario.fairness_contract.elapsed_time_driven, "scenario {} must be driven by elapsed monotonic time", scenario.id);
   ensure!(scenario.fairness_contract.logical_viewport_width > 0 && scenario.fairness_contract.logical_viewport_height > 0, "scenario {} has an empty logical viewport", scenario.id);
   ensure!(!scenario.fairness_contract.locale.is_empty() && !scenario.fairness_contract.timezone.is_empty() && !scenario.fairness_contract.direction.is_empty(), "scenario {} has incomplete locale, timezone, or direction fairness", scenario.id);
   ensure!(scenario.fairness_contract.schedule_tolerance_us > 0 && scenario.fairness_contract.coordinate_tolerance_microunits > 0, "scenario {} has zero trace tolerance", scenario.id);
   ensure!(!scenario.fairness_contract.expected_visible_role_counts.is_empty(), "scenario {} has no expected visible-role counts", scenario.id);

   let mut phase_ids = BTreeSet::new();
   for phase in &scenario.phases
   {
      ensure!(!phase.id.is_empty(), "scenario {} has an empty phase id", scenario.id);
      ensure!(phase_ids.insert(phase.id.as_str()), "scenario {} has duplicate phase {}", scenario.id, phase.id);
      ensure!(!phase.measured || phase.duration_ms.is_some() || phase.trace.is_some(), "scenario {} measured phase {} has neither duration nor trace", scenario.id, phase.id);
      ensure!(phase.duration_ms.unwrap_or(1) > 0, "scenario {} phase {} has zero duration", scenario.id, phase.id);
      if let Some(trace) = &phase.trace
      {
         ensure!(!trace.path.is_empty(), "scenario {} phase {} has an empty trace path", scenario.id, phase.id);
         ensure!(trace.path.ends_with(".json"), "scenario {} phase {} trace must be JSON", scenario.id, phase.id);
         validate_artifact_sha256(&trace.sha256, "scenario trace")?;
      }
   }

   let mut metrics = BTreeSet::new();
   ensure!(metrics.insert(scenario.primary_metric.as_str()), "scenario {} repeats its primary metric", scenario.id);
   for metric in scenario.required_metrics.iter().chain(scenario.optional_metrics.iter())
   {
      ensure!(!metric.is_empty(), "scenario {} has an empty metric id", scenario.id);
      ensure!(metrics.insert(metric.as_str()), "scenario {} repeats metric {}", scenario.id, metric);
   }

   let mut roles = BTreeSet::new();
   for role in &scenario.fairness_contract.expected_visible_role_counts
   {
      ensure!(!role.role.is_empty(), "scenario {} has an empty visible-role id", scenario.id);
      ensure!(role.count > 0, "scenario {} role {} has zero expected visible instances", scenario.id, role.role);
      ensure!(roles.insert(role.role.as_str()), "scenario {} repeats visible role {}", scenario.id, role.role);
      ensure!(scenario.scene.roles.iter().any(|scene_role| scene_role == &role.role), "scenario {} visible role {} is absent from scene roles", scenario.id, role.role);
   }

   let mut checkpoint_ids = BTreeSet::new();
   for checkpoint in &scenario.parity_checkpoints
   {
      ensure!(!checkpoint.id.is_empty() && checkpoint_ids.insert(checkpoint.id.as_str()), "scenario {} checkpoint ids must be non-empty and unique: {}", scenario.id, checkpoint.id);
      ensure!(phase_ids.contains(checkpoint.phase_id.as_str()), "scenario {} checkpoint {} refers to unknown phase {}", scenario.id, checkpoint.id, checkpoint.phase_id);
      if let Some(at_us) = checkpoint.at_us
      {
         let phase = scenario.phases.iter().find(|phase| phase.id == checkpoint.phase_id).expect("checkpoint phase set and list differ");
         if let Some(duration_ms) = phase.duration_ms
         {
            let phase_us = duration_ms.checked_mul(1_000).context("scenario phase duration overflows microseconds")?;
            ensure!(at_us <= phase_us, "scenario {} checkpoint {} lies beyond phase {}", scenario.id, checkpoint.id, checkpoint.phase_id);
         }
      }
      for (artifact, label) in [
         (&checkpoint.state, "checkpoint state"),
         (&checkpoint.accessibility, "checkpoint accessibility"),
         (&checkpoint.screenshot, "checkpoint screenshot"),
      ]
      {
         ensure!(!artifact.path.is_empty(), "scenario {} checkpoint {} has no {} path", scenario.id, checkpoint.id, label);
         validate_artifact_sha256(&artifact.sha256, label)?;
      }
      if let Some(geometry) = &checkpoint.geometry
      {
         ensure!(!geometry.path.is_empty() && geometry.path.ends_with(".json"), "scenario {} checkpoint {} has a noncanonical geometry artifact", scenario.id, checkpoint.id);
         validate_artifact_sha256(&geometry.sha256, "checkpoint geometry")?;
      }
      if let Some(status) = &checkpoint.recapture_status
      {
         ensure!(!status.is_empty()
            && status.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            && !status.starts_with('-')
            && !status.ends_with('-')
            && !status.contains("--"), "scenario {} checkpoint {} has a noncanonical recapture status", scenario.id, checkpoint.id);
      }
      ensure!(checkpoint.state.path.ends_with(".json") && checkpoint.accessibility.path.ends_with(".json") && checkpoint.screenshot.path.ends_with(".png"), "scenario {} checkpoint {} has a noncanonical artifact type", scenario.id, checkpoint.id);
      ensure!(!checkpoint.expected_visible_role_counts.is_empty(), "scenario {} checkpoint {} has no visible-role contract", scenario.id, checkpoint.id);
      let mut checkpoint_roles = BTreeSet::new();
      for role in &checkpoint.expected_visible_role_counts
      {
         ensure!(!role.role.is_empty() && role.count > 0, "scenario {} checkpoint {} has an empty or zero visible role", scenario.id, checkpoint.id);
         ensure!(checkpoint_roles.insert(role.role.as_str()), "scenario {} checkpoint {} repeats visible role {}", scenario.id, checkpoint.id, role.role);
         ensure!(scenario.scene.roles.iter().any(|scene_role| scene_role == &role.role), "scenario {} checkpoint {} role {} is absent from scene roles", scenario.id, checkpoint.id, role.role);
      }
   }
   Ok(())
}

pub fn validate_scenario_artifacts(spec_root: &Path, scenario: &ScenarioSpec) -> Result<()>
{
   validate_scenario(scenario)?;
   let canonical_root = fs::canonicalize(spec_root).with_context(|| format!("canonicalizing benchmark-spec root {}", spec_root.display()))?;
   validate_artifact_file(&canonical_root, &scenario.fixture, "scenario fixture")?;
   let asset_bytes = validate_artifact_file(&canonical_root, &scenario.assets, "scenario asset manifest")?;
   let asset_manifest = serde_json::from_slice::<AssetManifest>(&asset_bytes).with_context(|| format!("parsing scenario {} asset manifest", scenario.id))?;
   validate_asset_manifest_at_root(&canonical_root, &asset_manifest)?;
   let font_pack = crate::scenario::ArtifactIdentity {
      path: scenario.font_pack.manifest.clone(),
      sha256: scenario.font_pack.sha256.clone(),
   };
   let font_pack_bytes = validate_artifact_file(&canonical_root, &font_pack, "scenario font pack")?;
   let font_pack_manifest = serde_json::from_slice::<FontPackManifest>(&font_pack_bytes).with_context(|| format!("parsing scenario {} font pack", scenario.id))?;
   validate_font_pack_manifest_at_root(&canonical_root, &font_pack_manifest, &scenario.font_pack.id)?;
   validate_artifact_file(&canonical_root, &scenario.scene.style_tokens, "scenario style tokens")?;
   validate_artifact_file(&canonical_root, &scenario.scene.layout_assertions, "scenario layout assertions")?;
   for checkpoint in &scenario.parity_checkpoints
   {
      validate_artifact_file(&canonical_root, &checkpoint.state, "checkpoint state")?;
      validate_artifact_file(&canonical_root, &checkpoint.accessibility, "checkpoint accessibility")?;
      if let Some(geometry) = &checkpoint.geometry
      {
         validate_artifact_file(&canonical_root, geometry, "checkpoint geometry")?;
      }
      validate_artifact_file(&canonical_root, &checkpoint.screenshot, "checkpoint screenshot")?;
   }
   for phase in &scenario.phases
   {
      if let Some(trace) = &phase.trace
      {
         let bytes = validate_artifact_file(&canonical_root, trace, "scenario trace")?;
         let events = serde_json::from_slice::<Vec<TraceEvent>>(&bytes).with_context(|| format!("parsing scenario {} phase {} trace", scenario.id, phase.id))?;
         validate_trace(&events).with_context(|| format!("validating scenario {} phase {} trace", scenario.id, phase.id))?;
      }
   }
   Ok(())
}

pub(crate) fn validate_asset_manifest_at_root(canonical_root: &Path, manifest: &AssetManifest) -> Result<()>
{
   ensure!(manifest.schema_version == BENCHMARK_SPEC_SCHEMA_VERSION, "asset manifest {} has unsupported schema version {}", manifest.id, manifest.schema_version);
   ensure!(!manifest.id.is_empty() && !manifest.artifacts.is_empty(), "asset manifest must have an id and at least one artifact");
   let mut roles = BTreeSet::new();
   for asset in &manifest.artifacts
   {
      ensure!(!asset.role.is_empty() && roles.insert(asset.role.as_str()), "asset manifest {} roles must be non-empty and unique: {}", manifest.id, asset.role);
      ensure!(!asset.media_type.is_empty() && !asset.color_space.is_empty(), "asset manifest {} role {} has incomplete media identity", manifest.id, asset.role);
      validate_artifact_file(canonical_root, &asset.artifact, "asset file")?;
   }
   if let Some(atlas) = &manifest.inline_text_atlas
   {
      ensure!(atlas.columns > 0 && atlas.rows > 0, "asset manifest {} inline-text atlas grid must be positive", manifest.id);
      ensure!(atlas.source_repository.starts_with("https://github.com/"), "asset manifest {} inline-text atlas source repository is not an immutable-capable HTTPS origin", manifest.id);
      ensure!(atlas.source_commit.len() == 40 && atlas.source_commit.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)), "asset manifest {} inline-text atlas source commit must be a full lowercase Git object id", manifest.id);
      validate_artifact_file(canonical_root, &atlas.license, "inline-text atlas license")?;
      ensure!(!atlas.variants.is_empty(), "asset manifest {} inline-text atlas has no raster variants", manifest.id);
      let mut em_sizes = BTreeSet::new();
      let mut variant_roles = BTreeSet::new();
      for variant in &atlas.variants
      {
         ensure!(variant.em_pixels > 0 && em_sizes.insert(variant.em_pixels), "asset manifest {} inline-text raster em sizes must be positive and unique", manifest.id);
         ensure!(variant_roles.insert(variant.artifact_role.as_str()), "asset manifest {} inline-text raster roles must be unique", manifest.id);
         ensure!(variant.pixel_width == atlas.columns.checked_mul(variant.em_pixels).context("inline-text raster width overflow")? && variant.pixel_height == atlas.rows.checked_mul(variant.em_pixels).context("inline-text raster height overflow")?, "asset manifest {} inline-text raster role {} dimensions do not match its grid and em size", manifest.id, variant.artifact_role);
         let artifact = manifest.artifacts.iter().find(|asset| asset.role == variant.artifact_role)
            .with_context(|| format!("asset manifest {} inline-text raster role {} is missing", manifest.id, variant.artifact_role))?;
         ensure!(artifact.media_type == "image/png" && artifact.color_space == "srgb", "asset manifest {} inline-text raster role {} must be an sRGB PNG", manifest.id, variant.artifact_role);
      }
      ensure!(!atlas.entries.is_empty(), "asset manifest {} inline-text atlas has no entries", manifest.id);
      let mut graphemes = BTreeSet::new();
      let mut cells = BTreeSet::new();
      for entry in &atlas.entries
      {
         ensure!(!entry.grapheme.is_empty() && graphemes.insert(entry.grapheme.as_str()), "asset manifest {} inline-text graphemes must be non-empty and unique", manifest.id);
         ensure!(entry.column < atlas.columns && entry.row < atlas.rows, "asset manifest {} inline-text entry {} exceeds the atlas grid", manifest.id, entry.grapheme);
         ensure!(cells.insert((entry.column, entry.row)), "asset manifest {} inline-text entries must occupy unique atlas cells", manifest.id);
         ensure!(entry.advance_millionths > 0 && entry.width_millionths > 0 && entry.height_millionths > 0, "asset manifest {} inline-text entry {} has nonpositive logical metrics", manifest.id, entry.grapheme);
      }
   }
   Ok(())
}

pub fn validate_font_pack_manifest(spec_root: &Path, manifest: &FontPackManifest, expected_id: &str) -> Result<()>
{
   let canonical_root = fs::canonicalize(spec_root).with_context(|| format!("canonicalizing benchmark-spec root {}", spec_root.display()))?;
   validate_font_pack_manifest_at_root(&canonical_root, manifest, expected_id)
}

pub(crate) fn validate_font_pack_manifest_at_root(canonical_root: &Path, manifest: &FontPackManifest, expected_id: &str) -> Result<()>
{
   ensure!(manifest.schema_version == BENCHMARK_SPEC_SCHEMA_VERSION, "font pack {} has unsupported schema version {}", manifest.id, manifest.schema_version);
   ensure!(manifest.id == expected_id, "font pack id {} differs from scenario identity {}", manifest.id, expected_id);
   ensure!(!manifest.fonts.is_empty(), "font pack {} has no fonts", manifest.id);
   let mut roles = BTreeSet::new();
   for font in &manifest.fonts
   {
      ensure!(!font.role.is_empty() && roles.insert(font.role.as_str()), "font pack {} roles must be non-empty and unique: {}", manifest.id, font.role);
      ensure!(!font.source_repository.is_empty() && !font.source_commit.is_empty() && !font.source_path.is_empty(), "font pack {} role {} has incomplete source provenance", manifest.id, font.role);
      ensure!(font.source_repository.starts_with("https://github.com/"), "font pack {} role {} source repository is not an immutable-capable HTTPS origin", manifest.id, font.role);
      ensure!(font.source_commit.len() == 40 && font.source_commit.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)), "font pack {} role {} source commit must be a full lowercase Git object id", manifest.id, font.role);
      let source_path = Path::new(&font.source_path);
      ensure!(!source_path.is_absolute() && source_path.components().all(|component| matches!(component, Component::Normal(_))), "font pack {} role {} source path is not repository-relative", manifest.id, font.role);
      let font_bytes = validate_artifact_file(canonical_root, &font.artifact, "font file")?;
      validate_font_variation_axes(&manifest.id, &font.role, &font_bytes, &font.variation_axes)?;
      validate_artifact_file(canonical_root, &font.license, "font license")?;
   }
   Ok(())
}

fn validate_font_variation_axes(pack_id: &str, role: &str, font_bytes: &[u8], configured: &[crate::scenario::FontVariationAxis]) -> Result<()>
{
   let face = ttf_parser::Face::parse(font_bytes, 0).map_err(|error| anyhow::anyhow!("font pack {} role {} cannot parse its hashed font bytes: {:?}", pack_id, role, error))?;
   let font_axes = face.variation_axes();
   ensure!(!font_axes.is_empty(), "font pack {} role {} hashed font has no fvar axes", pack_id, role);
   ensure!(configured.len() == usize::from(font_axes.len()), "font pack {} role {} variation axis count {} differs from fvar count {}", pack_id, role, configured.len(), font_axes.len());
   let mut tags = BTreeSet::new();
   for (index, (axis, font_axis)) in configured.iter().zip(font_axes.into_iter()).enumerate()
   {
      let tag = axis.tag.as_bytes();
      ensure!(tag.len() == 4 && tag.iter().all(|byte| (0x20..=0x7e).contains(byte)), "font pack {} role {} variation axis {} tag must be exactly four printable ASCII bytes", pack_id, role, index);
      ensure!(tags.insert(axis.tag.as_str()), "font pack {} role {} variation axis tag {} is duplicated", pack_id, role, axis.tag);
      let tag_bytes = [tag[0], tag[1], tag[2], tag[3]];
      ensure!(ttf_parser::Tag::from_bytes(&tag_bytes) == font_axis.tag, "font pack {} role {} variation axis {} tag {} differs from fvar order", pack_id, role, index, axis.tag);
      let value = axis.value_millionths as f64 / 1_000_000.0;
      ensure!(value >= f64::from(font_axis.min_value) && value <= f64::from(font_axis.max_value), "font pack {} role {} variation axis {} value {} is outside fvar range {}..={}", pack_id, role, axis.tag, axis.value_millionths, font_axis.min_value, font_axis.max_value);
   }
   Ok(())
}

pub(crate) fn validate_artifact_file(canonical_root: &Path, artifact: &crate::scenario::ArtifactIdentity, label: &str) -> Result<Vec<u8>>
{
   let relative = Path::new(&artifact.path);
   ensure!(!relative.is_absolute(), "{} path must be relative to the benchmark-spec root", label);
   ensure!(relative.components().all(|component| matches!(component, Component::Normal(_))), "{} path contains traversal or non-normal components", label);
   let path = canonical_root.join(relative);
   let canonical_path = fs::canonicalize(&path).with_context(|| format!("resolving {} artifact {}", label, path.display()))?;
   ensure!(canonical_path.starts_with(canonical_root), "{} path escapes the benchmark-spec root", label);
   let bytes = fs::read(&canonical_path).with_context(|| format!("reading {} artifact {}", label, canonical_path.display()))?;
   let actual = format!("{:x}", Sha256::digest(&bytes));
   ensure!(actual == artifact.sha256, "{} artifact {} SHA-256 mismatch: expected {}, observed {}", label, artifact.path, artifact.sha256, actual);
   Ok(bytes)
}

pub fn validate_trace(events: &[TraceEvent]) -> Result<()>
{
   ensure!(!events.is_empty(), "trace has no events");
   for (index, event) in events.iter().enumerate()
   {
      if index > 0
      {
         ensure!(events[index - 1].at_us <= event.at_us, "trace event {} is earlier than its predecessor", index);
      }
      for coordinate in [event.x_millionths, event.y_millionths]
      {
         if let Some(coordinate) = coordinate
         {
            ensure!((0..=1_000_000).contains(&coordinate), "trace event {} has an out-of-range normalized coordinate", index);
         }
      }
      match event.op
      {
         TraceOperation::PointerDown | TraceOperation::PointerMove | TraceOperation::PointerUp =>
         {
            ensure!(event.pointer.is_some() && event.x_millionths.is_some() && event.y_millionths.is_some(), "pointer trace event {} lacks identity or coordinates", index);
         }
         TraceOperation::Mutate =>
         {
            ensure!(event.target.as_deref().is_some_and(|target| !target.is_empty()), "mutation trace event {} lacks a target", index);
            ensure!(event.value.is_some(), "mutation trace event {} lacks a value", index);
         }
         _ => {}
      }
   }
   Ok(())
}

pub fn validate_comparison_plan(plan: &ComparisonPlan) -> Result<()>
{
   ensure!(plan.schema_version == BENCHMARK_SPEC_SCHEMA_VERSION, "comparison plan has unsupported schema version {}", plan.schema_version);
   ensure!(!plan.suite_id.is_empty() && !plan.plan_id.is_empty(), "comparison plan suite_id and plan_id must not be empty");
   validate_artifact_sha256(&plan.plan_sha256, "comparison plan")?;
   validate_implementation_identity(&plan.reference, "reference")?;
   validate_implementation_identity(&plan.contender, "contender")?;
   ensure!(plan.reference.id != plan.contender.id, "comparison plan reference and contender identities must differ");
   validate_common_identity(&plan.common)?;
   ensure!(!plan.measurement_pass_id.is_empty() && !plan.instrumentation_profile.is_empty(), "comparison plan pass and instrumentation ids must not be empty");
   ensure!(plan.pass_pair_count.0 > 0, "comparison plan must preregister at least one pair");
   ensure!(!plan.process_boundary_plan.is_empty(), "comparison plan process boundary must not be empty");

   let scenario_ids = unique_strings("comparison scenario", &plan.scenario_ids)?;
   ensure!(!scenario_ids.is_empty(), "comparison plan must select at least one scenario");
   for scenario_id in &scenario_ids
   {
      ensure!(COMPARISON_SCENARIO_IDS.contains(&scenario_id.as_str()), "comparison plan names unknown scenario {}", scenario_id);
   }

   let mut pack_ids = BTreeSet::new();
   let mut pack_scenarios_by_id = BTreeMap::new();
   for pack in &plan.scenario_packs
   {
      ensure!(!pack.id.is_empty() && pack_ids.insert(pack.id.clone()), "comparison pack ids must be non-empty and unique: {}", pack.id);
      ensure!(!pack.ordered_scenario_ids.is_empty(), "comparison pack {} is empty", pack.id);
      let pack_scenarios = unique_strings("pack scenario", &pack.ordered_scenario_ids)?;
      ensure!(pack_scenarios.iter().all(|id| scenario_ids.contains(id)), "comparison pack {} contains a scenario outside the plan", pack.id);
      ensure!(pack_scenarios.contains(&pack.sentinel_scenario_id), "comparison pack {} sentinel is not in the pack", pack.id);
      ensure!(!pack.isolation_class.is_empty() && !pack.reset_contract.is_empty() && !pack.common_ready_predicate.is_empty(), "comparison pack {} has an incomplete execution contract", pack.id);
      ensure!(pack.measured_duration_ns.0 > 0 && pack.max_process_wall_ns.0 > 0 && pack.trace_capacity_limit.0 > 0, "comparison pack {} has zero duration, wall limit, or trace capacity", pack.id);
      let bounded_work = pack.fixed_warmup_ns.0.checked_add(pack.measured_duration_ns.0).context("comparison pack warmup/duration overflows u64")?;
      ensure!(bounded_work <= pack.max_process_wall_ns.0, "comparison pack {} warmup and measurement exceed process wall limit", pack.id);
      validate_artifact_sha256(&pack.calibration_evidence_sha256, "comparison pack calibration")?;
      pack_scenarios_by_id.insert(pack.id.clone(), pack_scenarios);
   }
   ensure!(!pack_ids.is_empty(), "comparison plan has no scenario packs");

   let mut chunk_ids = BTreeSet::new();
   for chunk in &plan.controller_chunks
   {
      ensure!(!chunk.id.is_empty() && chunk_ids.insert(chunk.id.clone()), "controller chunk ids must be non-empty and unique: {}", chunk.id);
      ensure!(!chunk.pass_id.is_empty() && !chunk.pack_ids.is_empty(), "controller chunk {} has no pass or packs", chunk.id);
      ensure!(chunk.pack_ids.iter().all(|id| pack_ids.contains(id)), "controller chunk {} refers to an unknown pack", chunk.id);
      ensure!(chunk.max_occupied_ns.0 > 0 && chunk.max_occupied_ns.0 <= 300_000_000_000, "controller chunk {} exceeds the five-minute default boundary", chunk.id);
      ensure!(chunk.expected_heartbeat_count.0 > 0, "controller chunk {} has no heartbeat contract", chunk.id);
      validate_artifact_sha256(&chunk.bundled_plan_resource_sha256, "controller bundled plan")?;
      let mut pair_indices = BTreeSet::new();
      for pair_index in &chunk.ordered_pair_indices
      {
         ensure!(pair_index.0 < plan.pass_pair_count.0, "controller chunk {} pair {} exceeds preregistered pair count", chunk.id, pair_index.0);
         ensure!(pair_indices.insert(pair_index.0), "controller chunk {} repeats pair {}", chunk.id, pair_index.0);
      }
   }
   ensure!(!chunk_ids.is_empty(), "comparison plan has no controller chunks");

   let mut metric_ids = BTreeSet::new();
   for metric in &plan.metric_definitions
   {
      ensure!(!metric.id.is_empty() && metric_ids.insert(metric.id.clone()), "metric ids must be non-empty and unique: {}", metric.id);
      ensure!(!metric.unit.is_empty() && !metric.scope.is_empty() && !metric.comparability.is_empty() && !metric.source.is_empty(), "metric {} has incomplete identity", metric.id);
      ensure!(!metric.owning_pass_id.is_empty() && !metric.sample_unit.is_empty() && !metric.within_session_estimator.is_empty() && !metric.across_session_estimator.is_empty(), "metric {} has an incomplete estimator contract", metric.id);
      ensure!(metric.decision_alpha.is_finite() && metric.decision_alpha > 0.0 && metric.decision_alpha < 1.0, "metric {} has invalid decision alpha", metric.id);
      ensure!(metric.exact_test_resolution_floor.0 > 0, "metric {} has no exact-test resolution floor", metric.id);
   }
   ensure!(!metric_ids.is_empty(), "comparison plan has no metric definitions");

   let mut cell_ids = BTreeSet::new();
   for cell in &plan.comparison_cells
   {
      ensure!(!cell.id.is_empty() && cell_ids.insert(cell.id.clone()), "comparison cell ids must be non-empty and unique: {}", cell.id);
      ensure!(cell.platform == plan.platform, "comparison cell {} platform differs from the plan", cell.id);
      ensure!(cell.reference_id == plan.reference.id && cell.contender_id == plan.contender.id, "comparison cell {} implementation identity differs from the plan", cell.id);
      ensure!(scenario_ids.contains(&cell.scenario_id) && pack_ids.contains(&cell.pack_id), "comparison cell {} refers to an unknown scenario or pack", cell.id);
      ensure!(pack_scenarios_by_id.get(&cell.pack_id).is_some_and(|pack_scenarios| pack_scenarios.contains(&cell.scenario_id)), "comparison cell {} scenario is not present in its selected pack", cell.id);
      ensure!(metric_ids.contains(&cell.primary_metric_id), "comparison cell {} has an unknown primary metric", cell.id);
      ensure!(cell.required_guardrail_metric_ids.iter().all(|id| metric_ids.contains(id)), "comparison cell {} has an unknown guardrail metric", cell.id);
      ensure!(!cell.owning_pass_id.is_empty() && !cell.within_session_estimator.is_empty() && !cell.materiality_boundary.is_empty() && !cell.sufficiency_rule.is_empty(), "comparison cell {} has an incomplete decision contract", cell.id);
   }
   ensure!(!cell_ids.is_empty(), "comparison plan has no comparison cells");

   let mut family_ids = BTreeSet::new();
   for family in &plan.decision_families
   {
      ensure!(!family.id.is_empty() && family_ids.insert(family.id.clone()), "decision family ids must be non-empty and unique: {}", family.id);
      ensure!(family.alpha.is_finite() && family.alpha > 0.0 && family.alpha < 1.0, "decision family {} has invalid alpha", family.id);
      ensure!(!family.ordered_members.is_empty(), "decision family {} has no members", family.id);
      ensure!(family.exact_test_resolution_floor.0 > 0 && family.exact_test_resolution_floor.0 <= family.maximum_pair_count.0, "decision family {} has an impossible exact-test floor", family.id);
      let mut members = BTreeSet::new();
      for member in &family.ordered_members
      {
         ensure!(cell_ids.contains(&member.comparison_cell_id) && metric_ids.contains(&member.metric_id), "decision family {} refers to an unknown cell or metric", family.id);
         ensure!(!member.boundary_id.is_empty(), "decision family {} has an empty boundary id", family.id);
         ensure!(members.insert((member.comparison_cell_id.as_str(), member.metric_id.as_str(), member.boundary_id.as_str())), "decision family {} repeats a member", family.id);
      }
   }
   ensure!(!family_ids.is_empty(), "comparison plan has no decision families");
   for cell in &plan.comparison_cells
   {
      for (family_id, claim_kind) in [
         (&cell.oxide_superiority_family_id, DecisionClaimKind::OxideSuperiority),
         (&cell.reference_superiority_family_id, DecisionClaimKind::ReferenceSuperiority),
         (&cell.equivalence_lower_family_id, DecisionClaimKind::EquivalenceLower),
         (&cell.equivalence_upper_family_id, DecisionClaimKind::EquivalenceUpper),
         (&cell.required_guardrail_family_id, DecisionClaimKind::RequiredGuardrailNoninferiority),
      ]
      {
         if let Some(family_id) = family_id
         {
            let family = plan.decision_families.iter().find(|family| family.id == *family_id).expect("family id set and list differ");
            ensure!(family.claim_kind == claim_kind, "comparison cell {} refers to decision family {} with the wrong claim kind", cell.id, family_id);
            ensure!(family.ordered_members.iter().any(|member| member.comparison_cell_id == cell.id), "comparison cell {} decision family {} does not contain the cell", cell.id, family_id);
         }
      }
   }
   Ok(())
}

fn validate_implementation_identity(identity: &ImplementationIdentity, role: &str) -> Result<()>
{
   ensure!(!identity.id.is_empty() && !identity.variant.is_empty() && !identity.source_commit.is_empty() && !identity.source_tree.is_empty(), "{} implementation identity is incomplete", role);
   ensure!(!identity.comparator_acceptance_status.is_empty(), "{} implementation has no comparator acceptance status", role);
   for (hash, label) in [
      (&identity.build_command_hash, "build command"),
      (&identity.executable_or_bundle_sha256, "executable or bundle"),
      (&identity.shipping_payload_manifest_sha256, "shipping payload manifest"),
      (&identity.reference_audit_sha256, "reference audit"),
   ]
   {
      validate_artifact_sha256(hash, &format!("{} {}", role, label))?;
   }
   Ok(())
}

fn validate_common_identity(identity: &CommonIdentity) -> Result<()>
{
   for (hash, label) in [
      (&identity.harness_sha256, "harness"),
      (&identity.pass_instrumentation_sha256, "pass instrumentation"),
      (&identity.scenario_manifest_sha256, "scenario manifest"),
      (&identity.trace_sha256, "trace"),
      (&identity.fixture_sha256, "fixture"),
      (&identity.asset_manifest_sha256, "asset manifest"),
      (&identity.font_pack_sha256, "font pack"),
   ]
   {
      validate_artifact_sha256(hash, label)?;
   }
   Ok(())
}

fn unique_strings(label: &str, values: &[String]) -> Result<BTreeSet<String>>
{
   let mut unique = BTreeSet::new();
   for value in values
   {
      ensure!(!value.is_empty() && unique.insert(value.clone()), "{} ids must be non-empty and unique: {}", label, value);
   }
   Ok(unique)
}

fn validate_artifact_sha256(sha256: &str, label: &str) -> Result<()>
{
   ensure!(sha256.len() == 64 && sha256.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)), "{} SHA-256 must be 64 lowercase hexadecimal characters", label);
   Ok(())
}
