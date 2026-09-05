use anyhow::{bail, Context, Result};
use oxide_benchmark_spec::ApplePrAcquisitionSpec;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedAppleTransportPair
{
   pub run_id: String,
   pub plan_sha256: String,
   pub pass_id: String,
   pub pair_index: u64,
   pub generation: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProbeEnvelope
{
   schema_version: u64,
   #[serde(rename = "runID")]
   run_id: String,
   #[serde(rename = "planSHA256")]
   plan_sha256: String,
   #[serde(rename = "passID")]
   pass_id: String,
   pair_index: u64,
   side: String,
   generation: String,
   #[serde(rename = "predecessorSHA256")]
   predecessor_sha256: Option<String>,
   #[serde(rename = "payloadSHA256")]
   payload_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProbeAcknowledgement
{
   schema_version: u64,
   generation: String,
   #[serde(rename = "artifactSHA256")]
   artifact_sha256: String,
   durable: bool,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppleTransportPairCheckpoint
{
   pub schema_version: u64,
   pub transport_mode: String,
   pub run_id: String,
   pub plan_sha256: String,
   pub pass_id: String,
   pub pair_index: u64,
   pub generation: String,
   pub oxide_artifact_sha256: String,
   pub oxide_ack_sha256: String,
   pub native_artifact_sha256: String,
   pub native_ack_sha256: String,
   pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedAppleCampaignPair
{
   pub run_id: String,
   pub plan_sha256: String,
   pub chunk_id: String,
   pub pass_id: String,
   pub pair_index: u64,
   pub oxide_generation: String,
   pub native_generation: String,
   pub predecessor_pair_sha256: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppleCampaignPairCheckpoint
{
   pub schema_version: u64,
   pub run_id: String,
   pub plan_sha256: String,
   pub chunk_id: String,
   pub pass_id: String,
   pub pair_index: u64,
   pub predecessor_pair_sha256: Option<String>,
   pub oxide_artifact_sha256: String,
   pub oxide_ack_sha256: String,
   pub oxide_telemetry_sha256: String,
   pub native_artifact_sha256: String,
   pub native_ack_sha256: String,
   pub native_telemetry_sha256: String,
   pub complete: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CampaignEnvelope
{
   schema_version: u64,
   #[serde(rename = "runID")]
   run_id: String,
   #[serde(rename = "planSHA256")]
   plan_sha256: String,
   #[serde(rename = "chunkID")]
   chunk_id: String,
   #[serde(rename = "passID")]
   pass_id: String,
   pair_index: u64,
   side: String,
   generation: String,
   #[serde(rename = "telemetrySHA256")]
   telemetry_sha256: String,
   telemetry_byte_count: u64,
   timebase_numerator: u32,
   timebase_denominator: u32,
   injection_scope: String,
   validation: String,
}

pub fn finalize_apple_campaign_pair(
   expected: &ExpectedAppleCampaignPair,
   oxide_artifact_path: &Path,
   oxide_ack_path: &Path,
   oxide_telemetry_path: &Path,
   native_artifact_path: &Path,
   native_ack_path: &Path,
   native_telemetry_path: &Path,
   output_path: &Path,
) -> Result<AppleCampaignPairCheckpoint>
{
   validate_sha256(&expected.plan_sha256)?;
   if let Some(predecessor) = expected.predecessor_pair_sha256.as_deref()
   {
      validate_sha256(predecessor).context("validating predecessor pair SHA-256")?;
   }
   let oxide = validate_campaign_side(
      expected,
      "oxide",
      &expected.oxide_generation,
      oxide_artifact_path,
      oxide_ack_path,
      oxide_telemetry_path,
   )?;
   let native = validate_campaign_side(
      expected,
      "native",
      &expected.native_generation,
      native_artifact_path,
      native_ack_path,
      native_telemetry_path,
   )?;
   let checkpoint = AppleCampaignPairCheckpoint {
      schema_version: 1,
      run_id: expected.run_id.clone(),
      plan_sha256: expected.plan_sha256.clone(),
      chunk_id: expected.chunk_id.clone(),
      pass_id: expected.pass_id.clone(),
      pair_index: expected.pair_index,
      predecessor_pair_sha256: expected.predecessor_pair_sha256.clone(),
      oxide_artifact_sha256: oxide.artifact_sha256,
      oxide_ack_sha256: oxide.ack_sha256,
      oxide_telemetry_sha256: oxide.telemetry_sha256,
      native_artifact_sha256: native.artifact_sha256,
      native_ack_sha256: native.ack_sha256,
      native_telemetry_sha256: native.telemetry_sha256,
      complete: true,
   };
   durable_json(&checkpoint, output_path)?;
   Ok(checkpoint)
}

struct ValidatedCampaignSide
{
   artifact_sha256: String,
   ack_sha256: String,
   telemetry_sha256: String,
}

fn validate_campaign_side(
   expected: &ExpectedAppleCampaignPair,
   side: &str,
   generation: &str,
   artifact_path: &Path,
   ack_path: &Path,
   telemetry_path: &Path,
) -> Result<ValidatedCampaignSide>
{
   let artifact = fs::read(artifact_path).with_context(|| format!("reading {} campaign artifact", side))?;
   let ack = fs::read(ack_path).with_context(|| format!("reading {} campaign acknowledgement", side))?;
   let telemetry = fs::read(telemetry_path).with_context(|| format!("reading {} campaign telemetry", side))?;
   let envelope: CampaignEnvelope = serde_json::from_slice(&artifact).with_context(|| format!("decoding {} campaign artifact", side))?;
   let acknowledgement: ProbeAcknowledgement = serde_json::from_slice(&ack).with_context(|| format!("decoding {} campaign acknowledgement", side))?;
   if envelope.schema_version != 1
      || envelope.run_id != expected.run_id
      || envelope.plan_sha256 != expected.plan_sha256
      || envelope.chunk_id != expected.chunk_id
      || envelope.pass_id != expected.pass_id
      || envelope.pair_index != expected.pair_index
      || envelope.side != side
      || envelope.generation != generation
      || envelope.telemetry_byte_count != telemetry.len() as u64
      || envelope.timebase_numerator == 0
      || envelope.timebase_denominator == 0
      || envelope.injection_scope != "direct-callback-diagnostic"
      || envelope.validation != "complete-diagnostic-not-claim-bearing"
   {
      bail!("{} campaign artifact does not match its frozen identity", side);
   }
   let artifact_sha256 = sha256(&artifact);
   let telemetry_sha256 = sha256(&telemetry);
   if envelope.telemetry_sha256 != telemetry_sha256
   {
      bail!("{} campaign telemetry hash differs from the complete artifact", side);
   }
   if acknowledgement.schema_version != 1
      || acknowledgement.generation != generation
      || acknowledgement.artifact_sha256 != artifact_sha256
      || !acknowledgement.durable
   {
      bail!("{} campaign acknowledgement does not commit its complete artifact", side);
   }
   let session_id = format!("{}:{}:{}:{}", expected.run_id, expected.chunk_id, expected.pair_index, side);
   validate_benchmark_telemetry(
      &telemetry,
      &expected.plan_sha256,
      generation,
      &expected.chunk_id,
      &expected.pass_id,
      &session_id,
   ).with_context(|| format!("validating {} campaign telemetry", side))?;
   Ok(ValidatedCampaignSide {
      artifact_sha256,
      ack_sha256: sha256(&ack),
      telemetry_sha256,
   })
}

pub fn validate_benchmark_telemetry(bytes: &[u8], plan_sha256: &str, generation: &str, chunk_id: &str, pass_id: &str, session_id: &str) -> Result<u64>
{
   const HEADER_BYTES: usize = 136;
   const RECORD_BYTES: usize = 44;
   const FOOTER_BYTES: usize = 32;
   if bytes.len() < HEADER_BYTES + FOOTER_BYTES || &bytes[..8] != b"OXBTEL02"
   {
      bail!("telemetry header is missing or truncated");
   }
   let schema = read_u32(bytes, 8)?;
   let header_bytes = read_u32(bytes, 12)? as usize;
   let record_bytes = read_u32(bytes, 16)? as usize;
   let flags = read_u32(bytes, 20)?;
   let count = read_u64(bytes, 24)?;
   let capacity = read_u64(bytes, 32)?;
   if schema != 2 || header_bytes != HEADER_BYTES || record_bytes != RECORD_BYTES || flags != 1 || count > capacity
   {
      bail!("telemetry header contract is invalid");
   }
   let count_usize = usize::try_from(count).context("telemetry record count exceeds usize")?;
   let expected_len = HEADER_BYTES.checked_add(count_usize.checked_mul(RECORD_BYTES).context("telemetry record bytes overflow")?)
      .and_then(|value| value.checked_add(FOOTER_BYTES)).context("telemetry total bytes overflow")?;
   if bytes.len() != expected_len
   {
      bail!("telemetry byte count differs from its header");
   }
   let plan = decode_sha256(plan_sha256)?;
   if bytes[40..72] != plan || bytes[72..104] != Sha256::digest(generation.as_bytes())[..]
   {
      bail!("telemetry plan or generation identity differs");
   }
   if read_u64(bytes, 104)? != stable_id(chunk_id)
      || read_u64(bytes, 112)? != stable_id(pass_id)
      || read_u64(bytes, 120)? != stable_id(session_id)
      || read_u32(bytes, 128)? == 0
      || read_u32(bytes, 132)? == 0
   {
      bail!("telemetry session identity or timebase differs");
   }
   if Sha256::digest(&bytes[..bytes.len() - FOOTER_BYTES])[..] != bytes[bytes.len() - FOOTER_BYTES..]
   {
      bail!("telemetry integrity footer differs");
   }
   let mut previous_timestamp = 0;
   let mut scopes = Vec::<(u16, u64)>::new();
   for index in 0..count_usize
   {
      let offset = HEADER_BYTES + index * RECORD_BYTES;
      if read_u64(bytes, offset)? != index as u64
      {
         bail!("telemetry sequence is not contiguous");
      }
      let timestamp = read_u64(bytes, offset + 8)?;
      if index > 0 && timestamp < previous_timestamp
      {
         bail!("telemetry timestamp regressed");
      }
      previous_timestamp = timestamp;
      let kind = read_u16(bytes, offset + 16)?;
      let identifier = read_u64(bytes, offset + 20)?;
      if !(1..=33).contains(&kind)
      {
         bail!("telemetry event kind is unknown");
      }
      match kind
      {
         1 | 3 | 5 => scopes.push((kind, identifier)),
         2 => pop_telemetry_scope(&mut scopes, 1, identifier)?,
         4 => pop_telemetry_scope(&mut scopes, 3, identifier)?,
         6 => pop_telemetry_scope(&mut scopes, 5, identifier)?,
         _ => {}
      }
   }
   if !scopes.is_empty()
   {
      bail!("telemetry has unterminated scopes");
   }
   Ok(count)
}

fn pop_telemetry_scope(scopes: &mut Vec<(u16, u64)>, begin_kind: u16, identifier: u64) -> Result<()>
{
   if scopes.pop() != Some((begin_kind, identifier))
   {
      bail!("telemetry scope nesting is invalid");
   }
   Ok(())
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16>
{
   Ok(u16::from_le_bytes(bytes.get(offset..offset + 2).context("truncated u16")?.try_into().expect("two-byte slice")))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32>
{
   Ok(u32::from_le_bytes(bytes.get(offset..offset + 4).context("truncated u32")?.try_into().expect("four-byte slice")))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64>
{
   Ok(u64::from_le_bytes(bytes.get(offset..offset + 8).context("truncated u64")?.try_into().expect("eight-byte slice")))
}

fn decode_sha256(value: &str) -> Result<[u8; 32]>
{
   validate_sha256(value)?;
   let mut decoded = [0_u8; 32];
   for (index, byte) in decoded.iter_mut().enumerate()
   {
      *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).context("decoding SHA-256")?;
   }
   Ok(decoded)
}

fn stable_id(value: &str) -> u64
{
   value.as_bytes().iter().fold(0xcbf29ce484222325, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3))
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplePrDryRun
{
   pub schema_version: u64,
   pub build_for_testing_count: u32,
   pub hard_total_seconds: u64,
   pub acquisitions: Vec<ApplePrDryRunAcquisition>,
   pub session_slots: Vec<ApplePrDryRunSessionSlot>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplePrDryRunAcquisition
{
   pub chunk_id: String,
   pub xctest_method: String,
   pub pass_id: String,
   pub pair_indices: Vec<u32>,
   pub max_occupied_seconds: u64,
   pub artifact_pull_after: bool,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplePrDryRunSessionSlot
{
   pub chunk_id: String,
   pub pass_id: String,
   pub pair_index: u32,
   pub side: String,
   pub scenario_id: String,
}

pub fn apple_pr_dry_run(spec: &ApplePrAcquisitionSpec) -> Result<ApplePrDryRun>
{
   if spec.build_for_testing_count != 1 || spec.controller_chunks.len() != 4
   {
      bail!("Apple PR dry run requires one build-for-testing and four controller acquisitions");
   }
   if spec.hard_total_seconds > 1_200
   {
      bail!("Apple PR dry run exceeds the twenty-minute hard ceiling");
   }
   let dynamic = spec.packs.iter().find(|pack| pack.id == "pr-non-launch").context("Apple PR dry run has no non-launch pack")?;
   let launch = spec.packs.iter().find(|pack| pack.id == "pr-launch").context("Apple PR dry run has no launch pack")?;
   let mut acquisitions = Vec::with_capacity(spec.controller_chunks.len());
   let mut session_slots = Vec::new();
   for chunk in &spec.controller_chunks
   {
      if chunk.pass_id == "lean"
      {
         bail!("Apple PR dry run must not contain a separate lean replay");
      }
      acquisitions.push(ApplePrDryRunAcquisition {
         chunk_id: chunk.id.clone(),
         xctest_method: chunk.xctest_method.clone(),
         pass_id: chunk.pass_id.clone(),
         pair_indices: chunk.ordered_pair_indices.clone(),
         max_occupied_seconds: chunk.max_occupied_seconds,
         artifact_pull_after: chunk.artifact_pull_after,
      });
      let scenarios = if chunk.pack_ids == ["pr-non-launch"]
      {
         &dynamic.ordered_scenario_ids
      }
      else if chunk.pack_ids == ["pr-launch"]
      {
         &launch.ordered_scenario_ids
      }
      else
      {
         continue;
      };
      for &pair_index in &chunk.ordered_pair_indices
      {
         let sides = if pair_index % 2 == 0 { ["oxide", "native"] } else { ["native", "oxide"] };
         for side in sides
         {
            for scenario_id in scenarios
            {
               session_slots.push(ApplePrDryRunSessionSlot {
                  chunk_id: chunk.id.clone(),
                  pass_id: chunk.pass_id.clone(),
                  pair_index,
                  side: String::from(side),
                  scenario_id: scenario_id.clone(),
               });
            }
         }
      }
   }
   let dry_run = ApplePrDryRun {
      schema_version: 1,
      build_for_testing_count: spec.build_for_testing_count,
      hard_total_seconds: spec.hard_total_seconds,
      acquisitions,
      session_slots,
   };
   validate_apple_pr_dry_run(spec, &dry_run)?;
   Ok(dry_run)
}

pub fn write_apple_pr_dry_run(spec: &ApplePrAcquisitionSpec, output: &Path) -> Result<ApplePrDryRun>
{
   let dry_run = apple_pr_dry_run(spec)?;
   durable_json(&dry_run, output)?;
   Ok(dry_run)
}

fn validate_apple_pr_dry_run(spec: &ApplePrAcquisitionSpec, dry_run: &ApplePrDryRun) -> Result<()>
{
   let expected_chunks = ["correctness", "presentation-pairs-0-1", "presentation-pairs-2-3", "launch-pairs-0-3"];
   if !dry_run.acquisitions.iter().map(|chunk| chunk.chunk_id.as_str()).eq(expected_chunks)
   {
      bail!("Apple PR dry run controller acquisition order is not canonical");
   }
   let dynamic = spec.packs.iter().find(|pack| pack.id == "pr-non-launch").context("Apple PR dry run has no non-launch pack")?;
   for scenario_id in &dynamic.ordered_scenario_ids
   {
      for side in ["oxide", "native"]
      {
         let mut pairs = dry_run.session_slots.iter()
            .filter(|slot| slot.pass_id == "minimal-presentation" && slot.scenario_id == *scenario_id && slot.side == side)
            .map(|slot| slot.pair_index)
            .collect::<Vec<_>>();
         pairs.sort_unstable();
         if pairs != [0, 1, 2, 3]
         {
            bail!("Apple PR dry run scenario {} side {} does not have four independent presentation pairs", scenario_id, side);
         }
      }
   }
   let launch_slots = dry_run.session_slots.iter().filter(|slot| slot.pass_id == "canonical-launch").count();
   if launch_slots != 8
   {
      bail!("Apple PR dry run requires exactly eight launch side samples");
   }
   Ok(())
}

pub fn finalize_per_app_transport_pair(
   expected: &ExpectedAppleTransportPair,
   oxide_artifact_path: &Path,
   oxide_ack_path: &Path,
   native_artifact_path: &Path,
   native_ack_path: &Path,
   output_path: &Path,
) -> Result<AppleTransportPairCheckpoint>
{
   validate_sha256(&expected.plan_sha256).context("validating expected plan SHA-256")?;
   let oxide_artifact = fs::read(oxide_artifact_path).with_context(|| format!("reading {}", oxide_artifact_path.display()))?;
   let oxide_ack = fs::read(oxide_ack_path).with_context(|| format!("reading {}", oxide_ack_path.display()))?;
   let native_artifact = fs::read(native_artifact_path).with_context(|| format!("reading {}", native_artifact_path.display()))?;
   let native_ack = fs::read(native_ack_path).with_context(|| format!("reading {}", native_ack_path.display()))?;
   let oxide: ProbeEnvelope = serde_json::from_slice(&oxide_artifact).context("decoding pulled Oxide artifact")?;
   let oxide_acknowledgement: ProbeAcknowledgement = serde_json::from_slice(&oxide_ack).context("decoding pulled Oxide acknowledgement")?;
   let native: ProbeEnvelope = serde_json::from_slice(&native_artifact).context("decoding pulled native artifact")?;
   let native_acknowledgement: ProbeAcknowledgement = serde_json::from_slice(&native_ack).context("decoding pulled native acknowledgement")?;

   validate_envelope(&oxide, expected, "oxide")?;
   validate_envelope(&native, expected, "native")?;
   let oxide_sha256 = sha256(&oxide_artifact);
   let native_sha256 = sha256(&native_artifact);
   validate_acknowledgement(&oxide_acknowledgement, expected, &oxide_sha256, "oxide")?;
   validate_acknowledgement(&native_acknowledgement, expected, &native_sha256, "native")?;
   if oxide.predecessor_sha256.is_some()
   {
      bail!("Oxide artifact unexpectedly declares a predecessor");
   }
   if native.predecessor_sha256.as_deref() != Some(&oxide_sha256)
   {
      bail!("native artifact predecessor does not match the pulled Oxide artifact");
   }

   let checkpoint = AppleTransportPairCheckpoint {
      schema_version: 1,
      transport_mode: String::from("per-app-container"),
      run_id: expected.run_id.clone(),
      plan_sha256: expected.plan_sha256.clone(),
      pass_id: expected.pass_id.clone(),
      pair_index: expected.pair_index,
      generation: expected.generation.clone(),
      oxide_artifact_sha256: oxide_sha256,
      oxide_ack_sha256: sha256(&oxide_ack),
      native_artifact_sha256: native_sha256,
      native_ack_sha256: sha256(&native_ack),
      complete: true,
   };
   durable_json(&checkpoint, output_path)?;
   Ok(checkpoint)
}

pub fn finalize_apple_transport_cli(args: &[String]) -> Result<()>
{
   let mut oxide_artifact = None;
   let mut oxide_ack = None;
   let mut native_artifact = None;
   let mut native_ack = None;
   let mut output = None;
   let mut run_id = None;
   let mut plan_sha256 = None;
   let mut pass_id = None;
   let mut pair_index = None;
   let mut generation = None;
   let mut index = 0;
   while index < args.len()
   {
      let flag = &args[index];
      index += 1;
      let value = args.get(index).with_context(|| format!("{} requires a value", flag))?;
      match flag.as_str()
      {
         "--oxide-artifact" => oxide_artifact = Some(PathBuf::from(value)),
         "--oxide-ack" => oxide_ack = Some(PathBuf::from(value)),
         "--native-artifact" => native_artifact = Some(PathBuf::from(value)),
         "--native-ack" => native_ack = Some(PathBuf::from(value)),
         "--output" => output = Some(PathBuf::from(value)),
         "--run-id" => run_id = Some(value.clone()),
         "--plan-sha" => plan_sha256 = Some(value.clone()),
         "--pass-id" => pass_id = Some(value.clone()),
         "--pair-index" => pair_index = Some(value.parse::<u64>().context("--pair-index must be an unsigned integer")?),
         "--generation" => generation = Some(value.clone()),
         _ => bail!("unknown finalize-apple-transport argument `{}`", flag),
      }
      index += 1;
   }

   let expected = ExpectedAppleTransportPair {
      run_id: run_id.context("finalize-apple-transport requires --run-id")?,
      plan_sha256: plan_sha256.context("finalize-apple-transport requires --plan-sha")?,
      pass_id: pass_id.context("finalize-apple-transport requires --pass-id")?,
      pair_index: pair_index.context("finalize-apple-transport requires --pair-index")?,
      generation: generation.context("finalize-apple-transport requires --generation")?,
   };
   let checkpoint = finalize_per_app_transport_pair(
      &expected,
      &oxide_artifact.context("finalize-apple-transport requires --oxide-artifact")?,
      &oxide_ack.context("finalize-apple-transport requires --oxide-ack")?,
      &native_artifact.context("finalize-apple-transport requires --native-artifact")?,
      &native_ack.context("finalize-apple-transport requires --native-ack")?,
      &output.context("finalize-apple-transport requires --output")?,
   )?;
   println!("finalized Apple transport pair {} generation {}", checkpoint.pair_index, checkpoint.generation);
   Ok(())
}

fn validate_envelope(envelope: &ProbeEnvelope, expected: &ExpectedAppleTransportPair, side: &str) -> Result<()>
{
   if envelope.schema_version != 1
      || envelope.run_id != expected.run_id
      || envelope.plan_sha256 != expected.plan_sha256
      || envelope.pass_id != expected.pass_id
      || envelope.pair_index != expected.pair_index
      || envelope.generation != expected.generation
      || envelope.side != side
   {
      bail!("{} artifact does not match the frozen pair identity or generation", side);
   }
   validate_sha256(&envelope.payload_sha256).with_context(|| format!("validating {} payload SHA-256", side))
}

fn validate_acknowledgement(acknowledgement: &ProbeAcknowledgement, expected: &ExpectedAppleTransportPair, artifact_sha256: &str, side: &str) -> Result<()>
{
   if acknowledgement.schema_version != 1
      || acknowledgement.generation != expected.generation
      || !acknowledgement.durable
      || acknowledgement.artifact_sha256 != artifact_sha256
   {
      bail!("{} acknowledgement does not match its durable artifact and generation", side);
   }
   Ok(())
}

fn validate_sha256(value: &str) -> Result<()>
{
   if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
   {
      bail!("expected canonical lowercase SHA-256");
   }
   Ok(())
}

fn sha256(bytes: &[u8]) -> String
{
   format!("{:x}", Sha256::digest(bytes))
}

fn durable_json<T: Serialize>(value: &T, destination: &Path) -> Result<()>
{
   let directory = destination.parent().context("checkpoint output must have a parent directory")?;
   fs::create_dir_all(directory).with_context(|| format!("creating {}", directory.display()))?;
   let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).context("system clock precedes Unix epoch")?.as_nanos();
   let file_name = destination.file_name().and_then(|value| value.to_str()).context("checkpoint output must have a UTF-8 file name")?;
   let temporary = directory.join(format!(".{}.{}.{}.tmp", file_name, std::process::id(), timestamp));
   let result = (|| -> Result<()> {
      let mut file = OpenOptions::new().write(true).create_new(true).open(&temporary).with_context(|| format!("creating {}", temporary.display()))?;
      let data = serde_json::to_vec_pretty(value).context("encoding pair checkpoint")?;
      file.write_all(&data).with_context(|| format!("writing {}", temporary.display()))?;
      file.write_all(b"\n").with_context(|| format!("terminating {}", temporary.display()))?;
      file.sync_all().with_context(|| format!("synchronizing {}", temporary.display()))?;
      fs::rename(&temporary, destination).with_context(|| format!("renaming {} to {}", temporary.display(), destination.display()))?;
      File::open(directory).with_context(|| format!("opening {}", directory.display()))?.sync_all().with_context(|| format!("synchronizing {}", directory.display()))?;
      Ok(())
   })();
   if result.is_err()
   {
      let _ = fs::remove_file(&temporary);
   }
   result
}
