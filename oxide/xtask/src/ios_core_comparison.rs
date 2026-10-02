use std::fs;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const SWEEP_CASES: &[&str] = &["shapes", "text", "images", "local", "animation", "scroll", "visual-controls", "visual-editing", "visual-typography", "visual-composition", "visual-layout", "visual-pickers", "visual-opacity", "visual-images", "visual-geometry", "visual-editing-edges"];

const CASES: &[&str] = &["shapes", "text", "images", "local", "animation", "scroll"];

#[derive(Default)]
struct Cli
{
   cases: Vec<String>, device: Option<String>, output: Option<PathBuf>,
   team: Option<String>, apps: Option<PathBuf>, visual: Option<PathBuf>, pilot: bool,
   sweep: bool, sweep_energy_only: bool, sweep_attempt_limit: Option<usize>, sweep_resume: Option<PathBuf>, pilot_renewal: bool, instruments_launch: bool, pilot_resume: Option<PathBuf>, pilot_history: Vec<PathBuf>, pilot_order: Option<String>,
}

pub(super) fn run(args: &[String]) -> Result<()>
{
   if args.len() == 1 && args[0] == "--source-identity"
   {
      println!("{}", source_hash(&super::locate_workspace_root()?)?);
      return Ok(());
   }
   let cli = parse(args)?;
   let root = super::locate_workspace_root()?;
   if cli.sweep {return run_sweep(&root, cli);}
   ensure!(!(cli.pilot && cli.pilot_renewal), "--pilot and --pilot-renewal are mutually exclusive");
   ensure!(cli.pilot_renewal || cli.pilot_resume.is_none(), "--pilot-resume requires --pilot-renewal");
   ensure!(!cli.instruments_launch || cli.pilot_renewal, "--instruments-launch requires --pilot-renewal");
   if cli.pilot {return run_pilot(&root, cli);}
   if cli.pilot_renewal {return run_pilot_renewal(&root, cli);}
   ensure!(cli.pilot_history.is_empty() && cli.pilot_order.is_none(), "pilot history/order require --pilot or --pilot-renewal");
   let cases: Vec<String> = if cli.cases.is_empty() {CASES.iter().map(|v| v.to_string()).collect()} else {cli.cases};
   for case in &cases {ensure!(CASES.contains(&case.as_str()), "unknown case {case}");}
   let visual_path = cli.visual.unwrap_or_else(|| root.join("benchmarks/core-visuals/visual-evidence.json"));
   let visual: Value = serde_json::from_slice(&fs::read(&visual_path)?)?;
   let source_hash = source_hash(&root)?;
   ensure!(visual["source_sha256"].as_str() == Some(&source_hash), "visual evidence source identity does not match current fixtures");
   for case in &cases
   {
      let checks = visual["cases"][case]["checkpoints"].as_array().context("missing visual checkpoints")?;
      ensure!(visual["cases"][case]["status"] == "passed" && checks.len() == 3, "{case}: visual equivalence is blocked: {}", visual["cases"][case]["reason"]);
      for (check, time) in checks.iter().zip([0.0, 10.0, 19.9])
      {
         ensure!(check["time"].as_f64() == Some(time), "invalid checkpoint time");
         for side in ["oxide", "uikit"]
         {
            let path = Path::new(check[side]["path"].as_str().context("missing checkpoint path")?);
            let path = if path.is_absolute() {path.to_path_buf()} else {visual_path.parent().unwrap().join(path)};
            ensure!(check[side]["sha256"].as_str() == Some(&hash_file(&path)?), "checkpoint hash mismatch: {}", path.display());
         }
      }
   }
   let device = super::resolve_uikit_physical_device(&root, cli.device.as_deref())?;
   ensure!(device.product_type.starts_with("iPhone"), "official comparison requires the physical ProMotion iPhone");
   let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
   let out = cli.output.unwrap_or_else(|| std::env::temp_dir().join(format!("oxide-core-{stamp}")));
   ensure!(!out.exists() || fs::read_dir(&out)?.next().is_none(), "output must be new or empty; retained runs are never overwritten");
   fs::create_dir_all(&out)?;
   let apps = if let Some(apps) = cli.apps {apps} else {build_apps(&root, cli.team.as_deref(), &out)?};
   let mut identities = serde_json::Map::new();
   for name in ["OxideBenchIOS", "UIKitBenchIOS"]
   {
      let app = find_app(&apps, name)?;
      identities.insert(name.into(), json!({"app":app,"executable_sha256":hash_file(&app.join(name))?}));
      bounded(&root, "xcrun", &strings(&["devicectl", "device", "install", "app", "--device", &device.udid, &app.to_string_lossy()]), &out.join(format!("install-{name}.log")), 120)?;
   }
   write_json(&out.join("identity.json"), &json!({"schema_version":1,"source_sha256":source_hash,"visual_evidence":visual,
      "device":{"name":device.name,"udid":device.udid,"product_type":device.product_type,"os_version":device.os_version,"os_build":device.os_build},
      "apps":identities,"protocol":{"warmup_seconds":5,"measured_seconds":20,"settled_seconds":5,"pairs":5,"maximum_replacement_pairs":1}}))?;
   let mut results = Vec::new();
   for case in &cases
   {
      let mut replacement_used = false;
      let mut blocked = false;
      for pair in 0..5
      {
         if blocked {break;}
         for attempt in 0..2
         {
            let order = if pair % 2 == 0 {["oxide", "uikit"]} else {["uikit", "oxide"]};
            let mut acquired = Vec::new();
            let mut failure = None;
            for side in order
            {
               let directory = out.join(format!("{case}-pair{pair}-attempt{attempt}-{side}"));
               fs::create_dir(&directory)?;
               println!("Core comparison: {case}, pair {}, attempt {}, {side}", pair + 1, attempt + 1);
               match capture(&root, &device, &directory, case, side)
               {
                  Ok(result) => acquired.push(json!({"side":side,"directory":directory,"metrics":result})),
                  Err(error) =>
                  {
                     write_json(&directory.join("failure.json"), &json!({"status":"acquisition-failed","error":format!("{error:#}")}))?;
                     failure = Some(format!("{error:#}"));
                     break;
                  }
               }
            }
            let accepted = failure.is_none();
            results.push(json!({"case":case,"pair":pair,"attempt":attempt,"accepted":accepted,"error":failure,"runs":acquired}));
            write_json(&out.join("runs.json"), &json!(results))?;
            if accepted {break;}
            if replacement_used {blocked = true; break;}
            replacement_used = true;
         }
      }
   }
   let reducer = root.join("host/apple-comparison/tools/reduce_core_trace.py");
   bounded(&root, "python3", &strings(&[&reducer.to_string_lossy(), "--summarize", &out.to_string_lossy()]), &out.join("summary.log"), 30)?;
   println!("Accounted-for comparison results: {}", out.join("scorecard.md").display());
   Ok(())
}

fn sweep_order(case_index: usize) -> [&'static str; 4]
{
   if case_index % 2 == 0 {["oxide", "uikit", "uikit", "oxide"]} else {["uikit", "oxide", "oxide", "uikit"]}
}

fn sweep_diagnostic(root: &Path, device: &super::UIKitPhysicalDevice, out: &Path, case: &str, side: &str) -> Result<Value>
{
   let completion = record_capture(root, device, out, case, side, "diagnostic", false, true)?;
   let trace = out.join("capture.trace");
   let toc = out.join("trace-toc.xml");
   let xml = out.join("trace.xml");
   export_trace(root, &strings(&["xctrace", "export", "--input", &trace.to_string_lossy(), "--toc", "--output", &toc.to_string_lossy()]), &out.join("toc.log"), 120)?;
   let xpath = "/trace-toc/run[@number='1']/data/table[@schema='hitches-updates' or @schema='hitches-renders' or @schema='hitches' or @schema='display-surface-swap' or @schema='os-signpost' or @schema='thread-state']";
   export_trace(root, &strings(&["xctrace", "export", "--input", &trace.to_string_lossy(), "--xpath", xpath, "--output", &xml.to_string_lossy()]), &out.join("export.log"), 180)?;
   let reducer = root.join("host/apple-comparison/tools/reduce_core_trace.py");
   bounded(root, "python3", &strings(&[&reducer.to_string_lossy(), "--xml", &xml.to_string_lossy(), "--toc", &toc.to_string_lossy(), "--app", app_name(side), "--case", case, "--completion", &out.join("completion.json").to_string_lossy(), "--output", &out.join("metrics.json").to_string_lossy()]), &out.join("reduce.log"), 60)?;
   let metrics: Value = serde_json::from_slice(&fs::read(out.join("metrics.json"))?)?;
   ensure!(metrics["valid_capture"] == true, "diagnostic capture lacks the completed measurement window");
   Ok(json!({"completion":completion,"metrics":metrics,"energy_recording_delivery_established":false}))
}

fn completed_sweep_group(runs: &[Value], case_index: usize, mode: &str) -> Option<Vec<Value>>
{
   let order = sweep_order(case_index);
   let count = if mode == "energy" {4} else {2};
   for attempt in 0..2
   {
      let mut group: Vec<Value> = runs.iter().filter(|run| run["case"] == SWEEP_CASES[case_index] && run["mode"] == mode && run["attempt"] == attempt).cloned().collect();
      group.sort_by_key(|run| run["index"].as_u64());
      if group.len() == count && group.iter().enumerate().all(|(index, run)| run["status"] == "captured" && run["index"] == index && run["side"] == order[index]) {return Some(group);}
   }
   None
}

fn run_sweep(root: &Path, cli: Cli) -> Result<()>
{
   ensure!(!cli.pilot && !cli.pilot_renewal && cli.pilot_resume.is_none() && cli.pilot_history.is_empty() && cli.pilot_order.is_none() && cli.cases.is_empty(), "--sweep uses the fixed sixteen-case protocol");
   ensure!(!cli.sweep_energy_only || cli.sweep_resume.is_some(), "energy-only continuation requires retained diagnostic failure evidence");
   let diagnostic_blocker = if cli.sweep_energy_only {Some("Repeated Instruments diagnostic recordings lack a complete measurement window; detailed diagnostics remain blocked. Whole-device energy is measured independently.")} else {None};
   let attempt_limit = cli.sweep_attempt_limit.unwrap_or(102);
   ensure!((1..=102).contains(&attempt_limit), "sweep attempt limit must be between 1 and 102");
   let qualification: Value = serde_json::from_slice(&fs::read(root.join("benchmarks/energy-comparison-2026-09-07/completed-qualification/status.json"))?)?;
   ensure!(qualification["normal_and_stall_probes_valid"] == true && qualification["energy_captures_in_quartet"] == 4, "measurement qualification is incomplete");
   let visual_path = cli.visual.context("--sweep requires refreshed --visual-evidence")?;
   let visual: Value = serde_json::from_slice(&fs::read(&visual_path)?)?;
   let source = source_hash(root)?;
   ensure!(visual["source_sha256"] == source, "visual source identity differs from measured fixtures");
   for case in SWEEP_CASES
   {
      let entry = &visual["cases"][*case];
      if entry["status"] == "blocked" {ensure!(entry["reason"].as_str().is_some_and(|v| !v.is_empty()), "blocked case requires exact reason"); continue;}
      ensure!(entry["status"] == "passed" && entry["repeated_cycles_verified"] == true, "{case}: visual/repeated-cycle verification is incomplete");
      let checkpoints = entry["checkpoints"].as_array().context("missing checkpoints")?;
      ensure!(checkpoints.len() == 3, "{case}: three checkpoints required");
      for (check, expected) in checkpoints.iter().zip(if case.starts_with("visual-") {[0.0, 1.0, 2.0]} else {[0.0, 10.0, 19.9]})
      {
         ensure!(check["time"].as_f64() == Some(expected), "checkpoint time mismatch");
         for side in ["oxide", "uikit"]
         {
            let path = PathBuf::from(check[side]["path"].as_str().context("missing screenshot path")?);
            let path = if path.is_absolute() {path} else {visual_path.parent().unwrap().join(path)};
            ensure!(check[side]["sha256"] == hash_file(&path)?, "screenshot identity mismatch");
         }
      }
   }
   let device = super::resolve_uikit_physical_device(root, cli.device.as_deref())?;
   let out = cli.output.context("--sweep requires a new --output directory")?;
   ensure!(!out.exists() || fs::read_dir(&out)?.next().is_none(), "sweep output must be new; attempts are preserved");
   fs::create_dir_all(&out)?;
   bounded(root, "xcrun", &strings(&["devicectl", "device", "info", "details", "--device", &device.udid, "--json-output", &out.join("device.json").to_string_lossy()]), &out.join("device.log"), 30)?;
   let details: Value = serde_json::from_slice(&fs::read(out.join("device.json"))?)?;
   ensure!(details["result"]["connectionProperties"]["transportType"] == "localNetwork", "energy sweep requires wireless recording");
   bounded(root, "xcodebuild", &strings(&["-version"]), &out.join("xcode-version.log"), 15)?;
   let apps = cli.apps.context("--sweep requires the visually verified --apps build")?;
   let mut identities = serde_json::Map::new();
   for side in ["oxide", "uikit"]
   {
      let name = app_name(side);
      let app = find_app(&apps, name)?;
      let hash = hash_file(&app.join(name))?;
      ensure!(visual["device_binaries"][side] == hash, "physical binary differs from reviewed build");
      identities.insert(side.into(), json!({"app":app,"executable_sha256":hash}));
      bounded(root, "xcrun", &strings(&["devicectl", "device", "install", "app", "--device", &device.udid, &app.to_string_lossy()]), &out.join(format!("install-{side}.log")), 120)?;
   }
   write_json(&out.join("identity.json"), &json!({"source_sha256":source,"apps":identities,"visual":visual,"device":details,"protocol":{"cases":SWEEP_CASES,"energy_captures":64,"diagnostic_captures":32,"maximum_energy_replacement_quartets":1,"maximum_diagnostic_replacement_pairs":1,"maximum_sweep_attempts":attempt_limit,"diagnostic_blocker":diagnostic_blocker,"qualification_attempts":18,"maximum_program_attempts":120}}))?;
   let mut runs = Vec::new();
   if let Some(previous) = &cli.sweep_resume
   {
      let old: Value = serde_json::from_slice(&fs::read(previous.join("identity.json"))?)?;
      ensure!(old["device"]["result"]["identifier"].as_str().context("missing resume device identity")? == details["result"]["identifier"].as_str().context("missing current device identity")?, "resume device differs");
      for side in ["oxide", "uikit"] {ensure!(old["apps"][side]["executable_sha256"] == identities[side]["executable_sha256"], "resume app binary differs");}
      let retained: Vec<Value> = serde_json::from_slice(&fs::read(previous.join("runs.json"))?)?;
      if cli.sweep_energy_only
      {
         ensure!(retained.iter().filter(|run| run["mode"] == "diagnostic" && run["status"] == "acquisition-failed" && run["reason"].as_str().is_some_and(|reason| reason.contains("diagnostic capture lacks the completed measurement window"))).count() >= 2, "energy-only continuation requires two retained diagnostic window failures");
      }
      for case_index in 0..SWEEP_CASES.len()
      {
         for mode in ["energy", "diagnostic"]
         {
            if let Some(group) = completed_sweep_group(&retained, case_index, mode)
            {
               for mut run in group
               {
                  let directory = PathBuf::from(run["directory"].as_str().context("missing retained directory")?);
                  let completion: Value = serde_json::from_slice(&fs::read(directory.join("completion.json"))?)?;
                  let recording: Value = serde_json::from_slice(&fs::read(directory.join("recording.json"))?)?;
                  ensure!(completion == run["result"]["completion"] && completion["case"] == SWEEP_CASES[case_index] && capture_completed(&completion, recording["run_id"].as_str().context("missing retained run ID")?, mode), "retained completion differs");
                  let (filename, key, valid) = if mode == "energy" {("power-metrics.json", "power", "valid_energy")} else {("metrics.json", "metrics", "valid_capture")};
                  let metrics: Value = serde_json::from_slice(&fs::read(directory.join(filename))?)?;
                  ensure!(metrics == run["result"][key] && metrics[valid] == true, "retained metrics differ or are invalid");
                  run["reused_without_recording"] = json!(true);
                  runs.push(run);
               }
            }
         }
      }
      if runs.is_empty()
      {
         ensure!(recording_count(previous)? == 1, "resume accepts the single retained first shapes capture only");
         let directory = previous.join("shapes-energy-attempt0-0-oxide");
         let completion: Value = serde_json::from_slice(&fs::read(directory.join("completion.json"))?)?;
         let recording: Value = serde_json::from_slice(&fs::read(directory.join("recording.json"))?)?;
         ensure!(completion["case"] == "shapes" && capture_completed(&completion, recording["run_id"].as_str().context("missing run identity")?, "energy"), "resume capture did not complete shapes energy");
         let power: Value = serde_json::from_slice(&fs::read(directory.join("power-metrics.json"))?)?;
         ensure!(power["valid_energy"] == true && completion["mode"] == "energy" && completion["error"] == "", "retained energy validation failed");
         runs.push(json!({"case":"shapes","mode":"energy","attempt":0,"index":0,"side":"oxide","directory":directory,"status":"captured","reused_without_recording":true,"result":{"completion":completion,"power":power}}));
      }
      write_json(&out.join("resume.json"), &json!({"previous":previous,"previous_identity":old,"reused_captures":runs.len(),"additional_recorder_invocations":0}))?;
      write_json(&out.join("runs.json"), &json!(runs))?;
   }
   let resumed_first_sample = runs.len() == 1 && runs[0]["case"] == "shapes" && runs[0]["mode"] == "energy" && runs[0]["index"] == 0;
   let mut cases = Vec::new();
   let mut energy_replacement_used = false;
   let mut diagnostic_replacement_used = false;
   let mut attempts = 0;
   for (case_index, case) in SWEEP_CASES.iter().enumerate()
   {
      if visual["cases"][*case]["status"] == "blocked"
      {
         cases.push(json!({"case":case,"status":"correctness-blocked","reason":visual["cases"][*case]["reason"]}));
         write_json(&out.join("cases.json"), &json!(cases))?;
         continue;
      }
      let mut completed = true;
      for mode in ["energy", "diagnostic"]
      {
         if !completed {break;}
         if mode == "diagnostic" && cli.sweep_energy_only {continue;}
         if completed_sweep_group(&runs, case_index, mode).is_some() {continue;}
         let order = sweep_order(case_index);
         let sides: &[&str] = if mode == "energy" {&order} else {&order[..2]};
         for attempt in 0..2
         {
            let mut failed = false;
            for (index, side) in sides.iter().enumerate()
            {
               if case_index == 0 && mode == "energy" && attempt == 0 && index == 0 && resumed_first_sample {continue;}
               if attempts >= attempt_limit
               {
                  write_json(&out.join("status.json"), &json!({"status":"attempt-limit-reached","attempts":attempts,"attempt_limit":attempt_limit,"cases":cases,"energy_replacement_used":energy_replacement_used,"diagnostic_replacement_used":diagnostic_replacement_used,"equal_delivery_efficiency_claim":false}))?;
                  bail!("sweep attempt limit reached; retained evidence {}", out.display());
               }
               let directory = out.join(format!("{case}-{mode}-attempt{attempt}-{index}-{side}"));
               fs::create_dir(&directory)?;
               println!("Batch {} / 4: {case} {mode}, attempt {attempt}, {side}", case_index / 4 + 1);
               let result = if mode == "energy"
               {
                  (|| -> Result<Value> {let completion = record_capture(root, &device, &directory, case, side, "energy", false, true)?; let power = reduce_energy_capture(root, &directory, side, &completion)?; Ok(json!({"completion":completion,"power":power}))})()
               }
               else {sweep_diagnostic(root, &device, &directory, case, side)};
               if directory.join("recording.json").is_file() {attempts += 1;}
               match result
               {
                  Ok(value) => runs.push(json!({"case":case,"mode":mode,"attempt":attempt,"index":index,"side":side,"directory":directory,"status":"captured","result":value})),
                  Err(error) =>
                  {
                     let reason = format!("{error:#}");
                     write_json(&directory.join("failure.json"), &json!({"reason":reason}))?;
                     runs.push(json!({"case":case,"mode":mode,"attempt":attempt,"index":index,"side":side,"directory":directory,"status":"acquisition-failed","reason":reason}));
                     failed = true;
                  }
               }
               write_json(&out.join("runs.json"), &json!(runs))?;
               write_json(&out.join("progress.json"), &json!({"case":case,"mode":mode,"attempts":attempts,"energy_replacement_used":energy_replacement_used,"diagnostic_replacement_used":diagnostic_replacement_used}))?;
               if failed {break;}
            }
            if !failed {break;}
            let replacement = if mode == "energy" {&mut energy_replacement_used} else {&mut diagnostic_replacement_used};
            if attempt == 1 || *replacement {completed = false; break;}
            *replacement = true;
         }
      }
      cases.push(json!({"case":case,"status":if completed {"captured-review-pending"} else {"acquisition-blocked"},"diagnostic_blocker":diagnostic_blocker}));
      write_json(&out.join("cases.json"), &json!(cases))?;
      if !completed
      {
         let reason = runs.last().and_then(|run| run.get("reason")).cloned().unwrap_or(json!("acquisition failed"));
         for pending in &SWEEP_CASES[case_index + 1..] {cases.push(json!({"case":pending,"status":"acquisition-blocked","reason":reason}));}
         write_json(&out.join("cases.json"), &json!(cases))?;
         break;
      }
   }
   write_json(&out.join("status.json"), &json!({"status":"sweep-acquisition-complete-review-pending","attempts":attempts,"cases":cases,"energy_replacement_used":energy_replacement_used,"diagnostic_replacement_used":diagnostic_replacement_used,"equal_delivery_efficiency_claim":false}))?;
   Ok(())
}

fn parse(args: &[String]) -> Result<Cli>
{
   let mut cli = Cli::default();
   let mut args = args.iter();
   while let Some(option) = args.next()
   {
      if option == "--sweep" {cli.sweep = true; continue;}
      if option == "--sweep-energy-only" {cli.sweep_energy_only = true; continue;}
      if option == "--pilot" {cli.pilot = true; continue;}
      if option == "--instruments-launch" {cli.instruments_launch = true; continue;}
      if option == "--pilot-renewal" {cli.pilot_renewal = true; continue;}
      let value = args.next().with_context(|| format!("missing value for {option}"))?;
      match option.as_str()
      {
         "--sweep-resume" => cli.sweep_resume = Some(value.into()),
         "--sweep-attempt-limit" => cli.sweep_attempt_limit = Some(value.parse().context("invalid sweep attempt limit")?),
         "--pilot-resume" => cli.pilot_resume = Some(value.into()),
         "--pilot-history" => cli.pilot_history.push(value.into()),
         "--pilot-order" => cli.pilot_order = Some(value.clone()),
         "--case" => cli.cases.push(value.clone()),
         "--device" => cli.device = Some(value.clone()),
         "--output" => cli.output = Some(value.into()),
         "--team" => cli.team = Some(value.clone()),
         "--apps" => cli.apps = Some(value.into()),
         "--visual-evidence" => cli.visual = Some(value.into()),
         _ => bail!("unknown option {option}"),
      }
   }
   Ok(cli)
}

fn strings(values: &[&str]) -> Vec<String> {values.iter().map(|v| v.to_string()).collect()}
fn write_json(path: &Path, value: &Value) -> Result<()> {fs::write(path, serde_json::to_vec_pretty(value)?)?; Ok(())}
fn hash_file(path: &Path) -> Result<String> {Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))}

fn source_hash(root: &Path) -> Result<String>
{
   fn visit(root: &Path, files: &mut Vec<PathBuf>) -> Result<()>
   {
      for entry in fs::read_dir(root)?
      {
         let path = entry?.path();
         if path.is_dir() {visit(&path, files)?;}
         else if ["swift", "m", "h", "metal", "rs", "png", "json", "toml", "plist", "pbxproj", "tracetemplate"].contains(&path.extension().and_then(|x| x.to_str()).unwrap_or("")) {files.push(path);}
      }
      Ok(())
   }
   let base = root.join("host/apple-comparison");
   let mut files = Vec::new();
   for name in ["Shared", "Oxide-iOS", "UIKit-iOS", "oxide-comparison-runtime", "fixtures"] {visit(&base.join(name), &mut files)?;}
   // Production rendering changes invalidate fixture equivalence too.
   for name in ["renderer-metal", "renderer-api", "text", "ui-core", "platform-ios", "platform-apple", "input", "timing", "utils", "image-store"]
   {
      visit(&root.join("crates").join(name), &mut files)?;
   }
   files.push(root.join("Cargo.lock"));
   files.push(root.join("rust-toolchain.toml"));
   files.push(base.join("AppleComparison.xcodeproj/project.pbxproj"));
   files.push(base.join("tools/reduce_core_trace.py"));
   files.push(base.join("tools/reduce_power_trace.py"));
   files.push(base.join("CoreEnergy.tracetemplate"));
   files.push(root.join("xtask/src/ios_core_comparison.rs"));
   files.push(root.join("xtask/src/lib.rs"));
   files.push(root.join("xtask/src/xctrace_record.rs"));
   files.push(root.join("crates/text/tests/fixtures/NotoSans-VF.ttf"));
   files.sort();
   let mut hash = Sha256::new();
   for file in files {hash.update(file.strip_prefix(root)?.to_string_lossy().as_bytes()); hash.update([0]); hash.update(fs::read(file)?);}
   Ok(format!("{:x}", hash.finalize()))
}

#[derive(Debug)]
struct ToolExit
{
   program: String, status: std::process::ExitStatus, log: PathBuf,
}

impl std::fmt::Display for ToolExit
{
   fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
   {
      write!(output, "{} exited {}; {}", self.program, self.status, self.log.display())
   }
}

impl std::error::Error for ToolExit {}

fn bounded(root: &Path, program: &str, args: &[String], log: &Path, seconds: u64) -> Result<()>
{
   let output = fs::File::create(log)?;
   let mut child = Command::new(program).args(args).current_dir(root).stdout(Stdio::from(output.try_clone()?)).stderr(Stdio::from(output)).spawn()?;
   let deadline = Instant::now() + Duration::from_secs(seconds);
   loop
   {
      if let Some(status) = child.try_wait()?
      {
         if !status.success() {return Err(ToolExit {program: program.into(), status, log: log.into()}.into());}
         return Ok(());
      }
      if Instant::now() >= deadline {let _ = child.kill(); let _ = child.wait(); bail!("{program} timed out after {seconds}s; {}", log.display());}
      thread::sleep(Duration::from_millis(200));
   }
}

fn export_crashed(status: std::process::ExitStatus) -> bool
{
   // Do not retry normal command failures, user interrupts, or termination.
   matches!(status.signal(), Some(6 | 11))
}

fn export_trace(root: &Path, args: &[String], log: &Path, seconds: u64) -> Result<()>
{
   match bounded(root, "xcrun", args, log, seconds)
   {
      Ok(()) => Ok(()),
      Err(error) =>
      {
         // The exporter has crashed in objc_release on a valid retained trace.
         // Retry only a process signal, once, without another device recording.
         if !error.downcast_ref::<ToolExit>().is_some_and(|exit| export_crashed(exit.status)) {return Err(error);}
         let output_index = args.iter().position(|arg| arg == "--output").context("export output missing")? + 1;
         let output = Path::new(args.get(output_index).context("export output value missing")?);
         if output.exists() {fs::rename(output, output.with_extension("failed.xml"))?;}
         write_json(&log.with_extension("retry.json"), &json!({"reason":error.to_string(),"maximum_export_retries":1,"new_device_capture":false}))?;
         bounded(root, "xcrun", args, &log.with_extension("retry.log"), seconds)
      }
   }
}

fn build_apps(root: &Path, team: Option<&str>, out: &Path) -> Result<PathBuf>
{
   let products = out.join("Products");
   let team = super::resolve_uikit_development_team(root, team, None)?;
   let mut args = strings(&["-project", "host/apple-comparison/AppleComparison.xcodeproj", "-scheme", "CoreComparison", "-configuration", "Release", "-sdk", "iphoneos", "-destination", "generic/platform=iOS", &format!("SYMROOT={}", products.display()), &format!("OBJROOT={}", out.join("Objects").display()), "build"]);
   super::append_uikit_device_signing_args(&mut args, &team);
   bounded(root, "xcodebuild", &args, &out.join("build.log"), 900)?;
   Ok(products.join("Release-iphoneos"))
}

fn find_app(root: &Path, name: &str) -> Result<PathBuf>
{
   let path = root.join(format!("{name}.app"));
   ensure!(path.join(name).is_file(), "--apps must contain Release iphoneos app bundles: {}", path.display());
   Ok(path)
}

fn app_name(side: &str) -> &'static str
{
   if side == "oxide" {"OxideBenchIOS"} else {"UIKitBenchIOS"}
}

struct HostChild(Child);
impl Drop for HostChild
{
   fn drop(&mut self)
   {
      let _ = self.0.kill();
      let _ = self.0.wait();
   }
}

fn expected_console_exit(status: std::process::ExitStatus, output: &str) -> bool
{
   // devicectl process terminate defaults to SIGTERM. The console reports the
   // requested device signal with local status 1 after the PID is confirmed gone.
   status.success() || (status.code() == Some(1) && output.lines().last().is_some_and(|line| line.trim() == "App terminated due to signal 15."))
}

impl HostChild
{
   fn wait_for_console_exit(&mut self, log: &Path, timeout: Duration) -> Result<()>
   {
      let deadline = Instant::now() + timeout;
      loop
      {
         if let Some(status) = self.0.try_wait()?
         {
            let output = fs::read_to_string(log).unwrap_or_default();
            ensure!(expected_console_exit(status, &output), "device launch console exited unexpectedly with {status}: {}", output.trim());
            return Ok(());
         }
         if Instant::now() >= deadline {bail!("device launch console did not exit within {} seconds; {}", timeout.as_secs(), log.display());}
         thread::sleep(Duration::from_millis(150));
      }
   }
}

fn host_child(root: &Path, program: &str, args: &[String], out: &Path, label: &str) -> Result<HostChild>
{
   Ok(HostChild(super::spawn_command_owned_with_output_paths(root, program, args, &out.join(format!("{label}.log")), &out.join(format!("{label}.stderr.log")))?))
}

fn record_capture(root: &Path, device: &super::UIKitPhysicalDevice, out: &Path, case: &str, side: &str, mode: &str, attach_by_name: bool, managed_launch: bool) -> Result<Value>
{
   // Prime Instruments' device discovery for each launch, rather than trusting
   // CoreDevice's independent availability view or a previous run's connection.
   let discovery = out.join("instruments-devices.log");
   bounded(root, "xcrun", &strings(&["xctrace", "list", "devices"]), &discovery, 30)?;
   let discovered = fs::read_to_string(&discovery)?;
   ensure!(instruments_online(&discovered, &device.udid), "Instruments does not list the requested iPhone online; see instruments-devices.log");
   let name = app_name(side);
   let bundle = format!("com.oxide.comparison.{side}benchios");
   let run_id = format!("{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos());
   let ready_name = format!("com.oxide.compare-core.ready.{run_id}");
   let started_name = format!("com.oxide.compare-core.started.{run_id}");
   let start_name = format!("com.oxide.compare-core.start.{run_id}");
   let trace_name = format!("com.oxide.compare-core.trace.{run_id}");
   let ready_args = super::uikit_device_notification_observe_args(device, &ready_name, 150);
   let ack_args = super::uikit_device_notification_observe_args(device, &started_name, 150);
   let mut ready = host_child(root, "xcrun", &ready_args, out, "ready")?;
   let mut ack = host_child(root, "xcrun", &ack_args, out, "ack")?;
   let mut app_args = strings(&["-oxide-core-run-id", &run_id, "-oxide-core-wait-for-trace"]);
   if mode.starts_with("probe")
   {
      if mode == "probe-delay" {app_args.push("-oxide-core-probe-delay".into());}
   }
   else {app_args.extend(strings(&["-oxide-core-case", case, "-oxide-core-mode", mode]));}
   let trace = out.join("capture.trace");
   let energy_template = root.join("host/apple-comparison/CoreEnergy.tracetemplate");
   let template = if mode == "energy" {energy_template.to_str().context("invalid energy template path")?} else {"Animation Hitches"};
   let seconds = match mode {"energy" => 240, "diagnostic" => 90, _ => 45};
   let mut trace_args = strings(&["xctrace", "record", "--template", template, "--instrument", "Points of Interest", "--device", &device.udid,
      "--time-limit", &format!("{seconds}s"), "--output", &trace.to_string_lossy(), "--no-prompt", "--notify-tracing-started", &trace_name]);
   let mut trace_ready = host_child(root, "notifyutil", &strings(&["-1", &trace_name]), out, "trace-ready")?;
   let mut recorder = None;
   let mut launched = None;
   let pid;
   if managed_launch
   {
      bounded(root, "xcrun", &strings(&["devicectl", "device", "info", "apps", "--device", &device.udid, "--json-output", &out.join("installed-apps.json").to_string_lossy()]), &out.join("installed-apps.log"), 30)?;
      let apps: Value = serde_json::from_slice(&fs::read(out.join("installed-apps.json"))?)?;
      let path = installed_launch_path(&apps, &bundle, name)?;
      bounded(root, "xcrun", &strings(&["devicectl", "device", "info", "processes", "--device", &device.udid, "--json-output", &out.join("prelaunch-processes.json").to_string_lossy()]), &out.join("prelaunch-processes.log"), 30)?;
      let processes: Value = serde_json::from_slice(&fs::read(out.join("prelaunch-processes.json"))?)?;
      ensure!(processes["result"]["runningProcesses"].as_array().context("missing prelaunch processes")?.iter().all(|entry| !entry["executable"].as_str().is_some_and(|value| super::device_process_name(value) == name)), "managed launch requires the selected benchmark app to be stopped");
      trace_args.extend(strings(&["--target-stdout", &out.join("launch.log").to_string_lossy(), "--launch", "--", &path]));
      trace_args.extend(app_args.clone());
      write_json(&out.join("request.json"), &json!({"schema_version":3,"run_id":run_id,"case":case,"side":side,"mode":mode,"launch_owner":"Instruments","arguments":trace_args}))?;
      write_json(&out.join("recording.json"), &json!({"pid":null,"launch":path,"template":template,"arguments":trace_args,"run_id":run_id}))?;
      let mut owned = super::xctrace_record::XctraceRecordProcess::spawn(root, "xcrun", &trace_args, &trace, &out.join("trace.log"), &out.join("trace.stderr.log"), 4 * 1024 * 1024 * 1024)?;
      owned.preserve_partial_trace();
      let resolved = super::wait_for_uikit_process_start_or_launch_failure(root, device, name, &mut owned, "xcrun", &trace_args,
         &out.join("trace.log"), &out.join("trace.stderr.log"), Duration::from_secs(30));
      match resolved
      {
         Ok(value) => pid = value,
         Err(error) =>
         {
            let cancel = format!("com.oxide.compare-core.cancel.{run_id}");
            let _ = bounded(root, "xcrun", &strings(&["devicectl", "device", "notification", "post", "--device", &device.udid, "--name", &cancel]), &out.join("cancel.log"), 10);
            return Err(error);
         }
      }
      write_json(&out.join("recording.json"), &json!({"pid":pid,"launch":path,"template":template,"arguments":trace_args,"run_id":run_id}))?;
      recorder = Some(owned);
   }
   else
   {
      let mut args = strings(&["devicectl", "device", "process", "launch", "--device", &device.udid, "--terminate-existing", "--console", "--", &bundle]);
      args.extend(app_args);
      write_json(&out.join("request.json"), &json!({"schema_version":2,"run_id":run_id,"case":case,"side":side,"mode":mode,"arguments":args}))?;
      let mut owned = host_child(root, "xcrun", &args, out, "launch")?;
      pid = super::wait_for_uikit_process_start_or_launch_failure(root, device, name, &mut owned.0, "xcrun", &args,
         &out.join("launch.log"), &out.join("launch.stderr.log"), Duration::from_secs(30))?;
      launched = Some(owned);
   }
   let result = (|| -> Result<Value>
   {
      super::wait_for_device_notification_or_console_marker("xcrun", &ready_args, &mut ready.0, &out.join("ready.log"), &out.join("ready.stderr.log"),
         &ready_name, &out.join("launch.log"), &format!("CORE_READY {run_id}"), Duration::from_secs(30))?;
      let ready_receipt = out.join("ready.json");
      let receipt_name = if mode.starts_with("probe") {"core-probe.json"} else {"core-result.json"};
      copy_receipt(root, device, &bundle, receipt_name, &ready_receipt, &out.join("ready-copy.log"))?;
      let receipt: Value = serde_json::from_slice(&fs::read(&ready_receipt)?)?;
      ensure!(receipt["run_id"] == run_id && receipt["status"] == "waiting", "app was not ready for requested run: {receipt}");
      if mode == "energy" || mode.starts_with("probe")
      {
         ensure!(receipt["battery_state"] == 1 && receipt["low_power_mode"] == false && receipt["thermal_state_at_end"] == 0,
            "qualification requires unplugged battery operation, low power off and nominal starting thermal state: {receipt}");
      }
      // Refresh discovery after app readiness and retain an exact PID witness.
      // Never retry a failed recorder silently or fall back to another process.
      bounded(root, "xcrun", &strings(&["xctrace", "list", "devices"]), &out.join("instruments-ready-devices.log"), 30)?;
      ensure!(instruments_online(&fs::read_to_string(out.join("instruments-ready-devices.log"))?, &device.udid), "Instruments lost the iPhone during app preparation");
      bounded(root, "xcrun", &strings(&["devicectl", "device", "info", "processes", "--device", &device.udid, "--json-output", &out.join("ready-processes.json").to_string_lossy()]), &out.join("ready-processes.log"), 30)?;
      let processes: Value = serde_json::from_slice(&fs::read(out.join("ready-processes.json"))?)?;
      let matching = ready_process(&processes, name, pid)?;
      write_json(&out.join("ready-process.json"), &matching)?;
      if recorder.is_none()
      {
         let attach = if attach_by_name {name.to_string()} else {pid.to_string()};
         trace_args.extend(strings(&["--attach", &attach]));
         write_json(&out.join("recording.json"), &json!({"pid":pid,"attach":attach,"template":template,"arguments":trace_args,"run_id":run_id}))?;
         let mut owned = super::xctrace_record::XctraceRecordProcess::spawn(root, "xcrun", &trace_args, &trace, &out.join("trace.log"), &out.join("trace.stderr.log"), 4 * 1024 * 1024 * 1024)?;
         owned.preserve_partial_trace();
         recorder = Some(owned);
      }
      let recorder = recorder.as_mut().context("recorder not started")?;
      super::wait_for_named_trace_started_or_trace_exit("xcrun", &trace_args, recorder, &out.join("trace.log"), &out.join("trace.stderr.log"),
         &mut trace_ready.0, &out.join("trace-ready.log"), &out.join("trace-ready.stderr.log"), &trace_name, Duration::from_secs(120))?;
      bounded(root, "xcrun", &strings(&["devicectl", "device", "notification", "post", "--device", &device.udid, "--name", &start_name]), &out.join("start.log"), 10)?;
      super::wait_for_device_notification_or_console_marker("xcrun", &ack_args, &mut ack.0, &out.join("ack.log"), &out.join("ack.stderr.log"),
         &started_name, &out.join("launch.log"), &format!("CORE_STARTED {run_id}"), Duration::from_secs(10))?;
      super::wait_for_xctrace_record_with_timeout("xcrun", &trace_args, recorder, &out.join("trace.log"), &out.join("trace.stderr.log"), Duration::from_secs(seconds + 180))?;
      recorder.commit()?;
      let completion = out.join("completion.json");
      copy_receipt(root, device, &bundle, receipt_name, &completion, &out.join("completion-copy.log"))?;
      let completed: Value = serde_json::from_slice(&fs::read(&completion)?)?;
      ensure!(capture_completed(&completed, &run_id, mode), "requested workload did not complete: {completed}");
      Ok(completed)
   })();
   if result.is_err()
   {
      let _ = bounded(root, "xcrun", &strings(&["devicectl", "device", "info", "processes", "--device", &device.udid, "--json-output", &out.join("failure-processes.json").to_string_lossy()]), &out.join("failure-processes.log"), 30);
      let cancel_name = format!("com.oxide.compare-core.cancel.{run_id}");
      let _ = bounded(root, "xcrun", &strings(&["devicectl", "device", "notification", "post", "--device", &device.udid, "--name", &cancel_name]), &out.join("cancel.log"), 10);
   }
   // Always collect the final receipt, including failures before/after startup.
   let receipt_name = if mode.starts_with("probe") {"core-probe.json"} else {"core-result.json"};
   let _ = copy_receipt(root, device, &bundle, receipt_name, &out.join("final-receipt.json"), &out.join("final-receipt-copy.log"));
   let cleanup = (|| -> Result<()>
   {
      let mut needs_termination = true;
      if managed_launch
      {
         bounded(root, "xcrun", &strings(&["devicectl", "device", "info", "processes", "--device", &device.udid, "--json-output", &out.join("recorder-exit-processes.json").to_string_lossy()]), &out.join("recorder-exit-processes.log"), 30)?;
         let processes: Value = serde_json::from_slice(&fs::read(out.join("recorder-exit-processes.json"))?)?;
         needs_termination = processes["result"]["runningProcesses"].as_array().context("missing recorder-exit processes")?.iter().any(|entry| entry["processIdentifier"].as_u64() == Some(pid));
      }
      if needs_termination {bounded(root, "xcrun", &strings(&["devicectl", "device", "process", "terminate", "--device", &device.udid, "--pid", &pid.to_string()]), &out.join("terminate.log"), 15)?;}
      wait_for_pid_absence(root, device, pid, &out.join("cleanup-processes.json"), Duration::from_secs(15))?;
      if let Some(launched) = launched.as_mut() {launched.wait_for_console_exit(&out.join("launch.log"), Duration::from_secs(15))?;}
      Ok(())
   })();
   if let Err(error) = &cleanup {write_json(&out.join("cleanup-failure.json"), &json!({"error":format!("{error:#}")}))?;}
   match result
   {
      Ok(value) => {cleanup?; Ok(value)}
      Err(error) => Err(error),
   }
}

fn capture_completed(receipt: &Value, run_id: &str, mode: &str) -> bool
{
   receipt["run_id"] == run_id && receipt["status"] == if mode == "energy" {"energy-complete"} else {"diagnostic-complete"}
}

fn installed_launch_path(apps: &Value, bundle: &str, name: &str) -> Result<String>
{
   let matches: Vec<&Value> = apps["result"]["apps"].as_array().context("missing installed applications")?.iter().filter(|app| app["bundleIdentifier"] == bundle).collect();
   ensure!(matches.len() == 1, "installed launch target is missing or ambiguous");
   let path = matches[0]["url"].as_str().and_then(|url| url.strip_prefix("file://")).context("installed app has no file URL")?.trim_end_matches('/');
   ensure!(path.ends_with(&format!("/{name}.app")), "installed launch path does not match expected app");
   Ok(path.into())
}

fn ready_process(processes: &Value, name: &str, pid: u64) -> Result<Value>
{
   let same_name: Vec<&Value> = processes["result"]["runningProcesses"].as_array().context("missing process list")?.iter()
      .filter(|entry| entry["executable"].as_str().is_some_and(|executable| super::device_process_name(executable) == name)).collect();
   ensure!(same_name.len() == 1, "ready executable name is not unique for selected app");
   let entry = same_name[0];
   ensure!(entry["processIdentifier"].as_u64() == Some(pid) && entry["executable"].as_str().unwrap_or("").ends_with(&format!("/{name}.app/{name}")), "ready PID no longer identifies the selected app");
   Ok(entry.clone())
}

fn recording_count(directory: &Path) -> Result<usize>
{
   let mut count = 0;
   for entry in fs::read_dir(directory)?
   {
      if entry?.path().join("recording.json").is_file() {count += 1;}
   }
   Ok(count)
}

fn wait_for_pid_absence(root: &Path, device: &super::UIKitPhysicalDevice, pid: u64, snapshot: &Path, timeout: Duration) -> Result<()>
{
   let deadline = Instant::now() + timeout;
   loop
   {
      ensure!(Instant::now() < deadline, "PID {pid} absence check timed out");
      let remaining = deadline.saturating_duration_since(Instant::now()).as_secs().max(1);
      bounded(root, "xcrun", &strings(&["devicectl", "device", "info", "processes", "--device", &device.udid, "--json-output", &snapshot.to_string_lossy()]), &snapshot.with_extension("log"), remaining)?;
      let processes: Value = serde_json::from_slice(&fs::read(snapshot)?)?;
      let absent = processes["result"]["runningProcesses"].as_array().context("missing cleanup process list")?.iter()
         .all(|entry| entry["processIdentifier"].as_u64() != Some(pid));
      if absent {return Ok(());}
      if Instant::now() >= deadline {bail!("PID {pid} remained present after termination; {}", snapshot.display());}
      thread::sleep(Duration::from_millis(150));
   }
}

fn copy_receipt(root: &Path, device: &super::UIKitPhysicalDevice, bundle: &str, receipt: &str, destination: &Path, log: &Path) -> Result<()>
{
   bounded(root, "xcrun", &strings(&["devicectl", "device", "copy", "from", "--device", &device.udid, "--domain-type", "appDataContainer",
      "--domain-identifier", bundle, "--source", &format!("Documents/{receipt}"), "--destination", &destination.to_string_lossy()]), log, 20)
}

fn run_pilot(root: &Path, cli: Cli) -> Result<()>
{
   ensure!(cli.cases.is_empty() || cli.cases == ["images"], "the qualification pilot is images-only");
   let order: Vec<&str> = cli.pilot_order.as_deref().unwrap_or("oxide,uikit,uikit,oxide").split(',').collect();
   ensure!(!order.is_empty() && order.iter().all(|side| ["oxide", "uikit"].contains(side)), "pilot order must contain only oxide/uikit");
   // History is explicit and retained. Every recorder invocation consumes a slot,
   // including failures before recording starts; a resumed run gets no new budget.
   let mut prior_attempts = 0;
   let mut histories = std::collections::BTreeSet::new();
   for history in &cli.pilot_history
   {
      let canonical = fs::canonicalize(history)?;
      ensure!(histories.insert(canonical.clone()), "duplicate pilot history");
      for entry in fs::read_dir(&canonical)?
      {
         if entry?.path().join("recording.json").is_file() {prior_attempts += 1;}
      }
   }
   ensure!(prior_attempts + order.len() <= 8, "requested pilot recordings exceed the remaining eight-attempt budget");
   let device = super::resolve_uikit_physical_device(root, cli.device.as_deref())?;
   let out = cli.output.context("--pilot requires --output pointing to a new evidence directory")?;
   ensure!(!out.exists() || fs::read_dir(&out)?.next().is_none(), "pilot output must be new/empty; attempts are never overwritten or silently retried");
   fs::create_dir_all(&out)?;
   bounded(root, "xcodebuild", &strings(&["-version"]), &out.join("xcode-version.log"), 15)?;
   bounded(root, "rustc", &strings(&["--version", "--verbose"]), &out.join("rust-version.log"), 15)?;
   bounded(root, "xcrun", &strings(&["devicectl", "device", "info", "details", "--device", &device.udid, "--json-output", &out.join("device.json").to_string_lossy()]), &out.join("device.log"), 30)?;
   let details: Value = serde_json::from_slice(&fs::read(out.join("device.json"))?)?;
   ensure!(details["result"]["connectionProperties"]["transportType"] == "localNetwork", "power pilot requires wireless transport; see device.json");
   let apps = if let Some(apps) = cli.apps {apps} else {build_apps(root, cli.team.as_deref(), &out)?};
   let mut identities = serde_json::Map::new();
   for side in ["oxide", "uikit"]
   {
      let name = app_name(side);
      let app = find_app(&apps, name)?;
      identities.insert(side.into(), json!({"path":app,"executable_sha256":hash_file(&app.join(name))?}));
      bounded(root, "xcrun", &strings(&["devicectl", "device", "install", "app", "--device", &device.udid, &app.to_string_lossy()]), &out.join(format!("install-{side}.log")), 120)?;
   }
   write_json(&out.join("identity.json"), &json!({"schema_version":2,"source_sha256":source_hash(root)?,"apps":identities,
      "device":{"udid":device.udid,"product_type":device.product_type,"os_version":device.os_version,"os_build":device.os_build},
      "protocol":{"phase":"qualification","maximum_captures":8,"energy_order":order,"prior_attempts":prior_attempts,"history":cli.pilot_history,"warmup_seconds":30,"control_before_seconds":30,"active_seconds":120,"drain_seconds":10,"control_after_seconds":30}}))?;
   let mut runs = Vec::new();
   for (index, side) in order.iter().copied().enumerate()
   {
      let directory = out.join(format!("energy-{index}-{side}"));
      fs::create_dir(&directory)?;
      println!("Power qualification {}/{}: {side}", index + 1, order.len());
      let result = (|| -> Result<Value>
      {
         let completion = record_capture(root, &device, &directory, "images", side, "energy", false, false)?;
         let power = reduce_energy_capture(root, &directory, side, &completion)?;
         Ok(json!({"completion":completion,"power":power,"comparative_energy":null}))
      })();
      match result
      {
         Ok(value) => runs.push(json!({"index":index,"side":side,"directory":directory,"status":"captured","result":value})),
         Err(error) =>
         {
            let message = format!("{error:#}");
            write_json(&directory.join("failure.json"), &json!({"status":"qualification-blocked","error":message}))?;
            runs.push(json!({"index":index,"side":side,"directory":directory,"status":"failed","error":message}));
            write_json(&out.join("runs.json"), &json!(runs))?;
            write_json(&out.join("status.json"), &json!({"status":"qualification-blocked","captures_attempted":recording_count(&out)?,"total_attempts_including_history":prior_attempts+recording_count(&out)?,"reason":message,"sweep_started":false}))?;
            bail!("power qualification stopped: {message}; retained evidence {}", out.display());
         }
      }
      write_json(&out.join("runs.json"), &json!(runs))?;
   }
   write_json(&out.join("status.json"), &json!({"status":"repeatability-and-presentation-verification-pending","captures_attempted":order.len(),"total_attempts_including_history":prior_attempts+order.len(),"sweep_started":false,
      "reason":"Per-run power validation passed; repeatability and presentation qualification remain required before the all-case sweep."}))?;
   println!("Validated power windows retained at {}. No comparative efficiency is claimed.", out.display());
   Ok(())
}

fn reduce_energy_capture(root: &Path, directory: &Path, side: &str, completion: &Value) -> Result<Value>
{
   let trace = directory.join("capture.trace");
   let toc = directory.join("trace-toc.xml");
   export_trace(root, &strings(&["xctrace", "export", "--input", &trace.to_string_lossy(), "--toc", "--output", &toc.to_string_lossy()]), &directory.join("toc.log"), 120)?;
   let tables = super::parse_xctrace_toc_tables(&fs::read_to_string(&toc)?)?;
   ensure!(tables.iter().any(|table| table.schema == "SystemPowerLevel"), "Power Profiler exported no whole-device power table");
   ensure!(!tables.iter().any(|table| table.schema == "time-profile" || table.schema.starts_with("metal-perf-overview") || table.schema == "network-connection-update"), "energy template contains detailed diagnostic instruments");
   for schema in ["SystemPowerLevel", "DeviceChargingState", "os-signpost"]
   {
      let xpath = format!("/trace-toc/run[@number='1']/data/table[@schema='{schema}']");
      export_trace(root, &strings(&["xctrace", "export", "--input", &trace.to_string_lossy(), "--xpath", &xpath, "--output", &directory.join(format!("{schema}.xml")).to_string_lossy()]), &directory.join(format!("{schema}-export.log")), 120)?;
   }
   export_trace(root, &strings(&["xctrace", "export", "--input", &trace.to_string_lossy(), "--xpath", "/trace-toc/run[@number='1']/data/table", "--output", &directory.join("trace.xml").to_string_lossy()]), &directory.join("export.log"), 180)?;
   let recording: Value = serde_json::from_slice(&fs::read(directory.join("recording.json"))?)?;
   let reducer = root.join("host/apple-comparison/tools/reduce_power_trace.py");
   bounded(root, "python3", &strings(&[&reducer.to_string_lossy(), "--xml", &directory.join("SystemPowerLevel.xml").to_string_lossy(),
      "--signposts", &directory.join("os-signpost.xml").to_string_lossy(), "--toc", &toc.to_string_lossy(),
      "--receipt", &directory.join("completion.json").to_string_lossy(), "--app", app_name(side),
      "--pid", &recording["pid"].to_string(), "--run-id", completion["run_id"].as_str().context("missing completed run ID")?,
      "--output", &directory.join("power-metrics.json").to_string_lossy()]), &directory.join("power-reduction.log"), 30)?;
   let power: Value = serde_json::from_slice(&fs::read(directory.join("power-metrics.json"))?)?;
   ensure!(power["valid_energy"] == true, "power sample/receipt/window qualification failed; see power-metrics.json");
   Ok(power)
}

fn run_pilot_renewal(root: &Path, cli: Cli) -> Result<()>
{
   ensure!(!cli.pilot && cli.pilot_order.is_none() && cli.cases.is_empty(), "--pilot-renewal has a fixed protocol and does not accept --pilot, --pilot-order or --case");
   ensure!(!cli.pilot_history.is_empty(), "--pilot-renewal requires --pilot-history entries containing the original eight attempts");
   let mut histories = std::collections::BTreeSet::new();
   let mut run_ids = std::collections::BTreeSet::new();
   let mut prior_attempts = 0;
   for history in &cli.pilot_history
   {
      let history = fs::canonicalize(history)?;
      ensure!(histories.insert(history.clone()), "duplicate pilot history");
      for entry in fs::read_dir(&history)?
      {
         let recording = entry?.path().join("recording.json");
         if !recording.is_file() {continue;}
         let value: Value = serde_json::from_slice(&fs::read(&recording)?)?;
         let run_id = value["run_id"].as_str().context("historical recording missing run_id")?;
         ensure!(run_ids.insert(run_id.to_string()), "duplicate historical recorder run ID: {run_id}");
         prior_attempts += 1;
      }
   }
   let finish_managed = cli.instruments_launch && cli.pilot_resume.is_some();
   let reviewed_resume = finish_managed && cli.pilot_resume.as_ref().is_some_and(|path| path.join("validated-resume.json").is_file());
   let expected_history = if reviewed_resume {13} else if finish_managed {12} else if cli.instruments_launch {10} else {8};
   let requested = if reviewed_resume {5} else if finish_managed {6} else if cli.instruments_launch {2} else {8};
   let maximum_attempts = if finish_managed {18} else {16};
   ensure!(prior_attempts == expected_history, "qualification history must contain {expected_history} recorder attempts, found {prior_attempts}");
   ensure!(prior_attempts + requested <= maximum_attempts, "renewal would exceed the approved cumulative budget");
   let mut resumed = Vec::new();
   let mut resumed_identity = None;
   if let Some(previous) = &cli.pilot_resume
   {
      let previous = fs::canonicalize(previous)?;
      let identity: Value = serde_json::from_slice(&fs::read(previous.join("identity.json"))?)?;
      ensure!(identity["protocol"]["phase"] == "renewal", "resume protocol differs from this renewal");
      if finish_managed
      {
         ensure!(histories.contains(&previous) && identity["protocol"]["attach"] == "Instruments-managed-launch", "finish requires managed-launch evidence included in history");
      }
      else {ensure!(identity["protocol"]["original_history"] == json!(histories), "resume history differs from this renewal");}
      let reviewed: Vec<Value> = if reviewed_resume {serde_json::from_slice(&fs::read(previous.join("validated-resume.json"))?)?} else {Vec::new()};
      let count = if reviewed_resume {ensure!(recording_count(&previous)? == 1 && reviewed.len() == 3, "reviewed continuation requires one retained stall and two normal probes"); 3} else {recording_count(&previous)?};
      ensure!((1..=4).contains(&count), "resume requires a completed prefix of the four probes, with no energy attempts");
      if finish_managed && !reviewed_resume {ensure!(count == 2, "managed continuation requires exactly two normal probes");}
      let previous_sequence = if finish_managed {[(0, "uikit", false), (1, "oxide", false), (2, "uikit", true), (3, "oxide", true)]} else {[(0, "oxide", false), (1, "uikit", false), (2, "uikit", true), (3, "oxide", true)]};
      for (index, side, delayed) in previous_sequence.into_iter().take(count)
      {
         let directory = if reviewed_resume
         {
            let entry = &reviewed[index];
            ensure!(entry["index"] == index && entry["side"] == side && entry["delayed"] == delayed, "reviewed probe order differs");
            let path = fs::canonicalize(entry["directory"].as_str().context("missing reviewed directory")?)?;
            ensure!(path.parent().is_some_and(|parent| histories.contains(parent)), "reviewed evidence must belong to counted history");
            path
         }
         else {previous.join(format!("probe-{index}-{side}-{}", if delayed {"delay"} else {"normal"}))};
         let metrics_file = if reviewed_resume {"reclassified-probe-metrics.json"} else {"probe-metrics.json"};
         let recording: Value = serde_json::from_slice(&fs::read(directory.join("recording.json"))?)?;
         let completion: Value = serde_json::from_slice(&fs::read(directory.join("completion.json"))?)?;
         let metrics: Value = serde_json::from_slice(&fs::read(directory.join(metrics_file))?)?;
         ensure!((finish_managed || recording["attach"] == app_name(side)) && completion["run_id"] == recording["run_id"] && completion["status"] == "diagnostic-complete", "resume probe did not complete the owned run");
         ensure!(completion["injected_delay"] == delayed && completion["battery_state"] == 1 && completion["low_power_mode"] == false, "resume probe state is invalid");
         ensure!(metrics["process"] == format!("{} ({})", app_name(side), recording["pid"]) && metrics["valid_capture"] == true && metrics["validity"]["presentation_cadence"]["valid"] == true && metrics["validity"]["stall_detection"]["valid"] == true, "resume probe has not passed presentation qualification");
         let cleanup: Value = serde_json::from_slice(&fs::read(directory.join("cleanup-processes.json"))?)?;
         ensure!(cleanup["result"]["runningProcesses"].as_array().context("missing cleanup process snapshot")?.iter().all(|entry| entry["processIdentifier"] != recording["pid"]), "resume target has no confirmed cleanup");
         if !finish_managed {ensure!(fs::read_to_string(directory.join("launch.log"))?.lines().last().is_some_and(|line| line.trim() == "App terminated due to signal 15."), "resume requires the confirmed requested SIGTERM console exit");}
         let mut hashes = serde_json::Map::new();
         for file in ["recording.json", "completion.json", metrics_file, "trace.xml", "trace-toc.xml", "cleanup-processes.json", "launch.log"]
         {
            hashes.insert(file.into(), json!(hash_file(&directory.join(file))?));
         }
         resumed.push(json!({"index":index,"kind":"probe","side":side,"delayed":delayed,"directory":directory,"status":"passed","metrics":metrics,"reused_without_recording":true,"metrics_file":metrics_file,"evidence_sha256":hashes}));
      }
      resumed_identity = Some(identity);
   }
   let resume_count = resumed.len();
   let device = super::resolve_uikit_physical_device(root, cli.device.as_deref())?;
   if let Some(identity) = &resumed_identity {ensure!(identity["device"]["udid"] == device.udid, "resume device differs");}
   let out = cli.output.context("--pilot-renewal requires --output pointing to a new evidence directory")?;
   ensure!(!out.exists() || fs::read_dir(&out)?.next().is_none(), "renewal output must be new/empty; attempts are never overwritten or retried");
   fs::create_dir_all(&out)?;
   bounded(root, "xcodebuild", &strings(&["-version"]), &out.join("xcode-version.log"), 15)?;
   bounded(root, "rustc", &strings(&["--version", "--verbose"]), &out.join("rust-version.log"), 15)?;
   bounded(root, "xcrun", &strings(&["devicectl", "device", "info", "details", "--device", &device.udid, "--json-output", &out.join("device.json").to_string_lossy()]), &out.join("device.log"), 30)?;
   let details: Value = serde_json::from_slice(&fs::read(out.join("device.json"))?)?;
   ensure!(details["result"]["connectionProperties"]["transportType"] == "localNetwork", "renewal qualification requires wireless transport; see device.json");
   let apps = if let Some(apps) = cli.apps {apps} else {build_apps(root, cli.team.as_deref(), &out)?};
   let mut identities = serde_json::Map::new();
   for side in ["oxide", "uikit"]
   {
      let name = app_name(side);
      let app = find_app(&apps, name)?;
      let binary_hash = hash_file(&app.join(name))?;
      if let Some(identity) = &resumed_identity {ensure!(identity["apps"][side]["executable_sha256"] == binary_hash, "resume requires identical app binaries");}
      identities.insert(side.into(), json!({"path":app,"executable_sha256":binary_hash}));
      bounded(root, "xcrun", &strings(&["devicectl", "device", "install", "app", "--device", &device.udid, &app.to_string_lossy()]), &out.join(format!("install-{side}.log")), 120)?;
   }
   write_json(&out.join("identity.json"), &json!({"schema_version":3,"source_sha256":source_hash(root)?,"apps":identities,
      "device":{"udid":device.udid,"product_type":device.product_type,"os_version":device.os_version,"os_build":device.os_build},
      "protocol":{"phase":"renewal","original_history":histories,"prior_attempts":prior_attempts,"maximum_cumulative_attempts":maximum_attempts,
      "probe_order":if finish_managed {json!(["uikit-normal","oxide-normal","uikit-delay","oxide-delay"])} else if cli.instruments_launch {json!(["uikit-normal","oxide-normal"])} else {json!(["oxide-normal","uikit-normal","uikit-delay","oxide-delay"])},"energy_order":if cli.instruments_launch && !finish_managed {json!([])} else {json!(["oxide","uikit","uikit","oxide"])},"attach":if cli.instruments_launch {"Instruments-managed-launch"} else {"exact-executable-name"},"resumed_probe_count":resume_count,"resumed_identity":resumed_identity}}))?;
   let mut runs = resumed;
   let mut normal_metrics = std::collections::BTreeMap::new();
   for run in &runs
   {
      if run["delayed"] == false
      {
         let side = if run["side"] == "oxide" {"oxide"} else {"uikit"};
         normal_metrics.insert(side, PathBuf::from(run["directory"].as_str().context("missing resumed directory")?).join(run["metrics_file"].as_str().unwrap_or("probe-metrics.json")));
      }
   }
   write_json(&out.join("runs.json"), &json!(runs))?;
   let sequence: &[(usize, &str, bool)] = if finish_managed {&[(0, "uikit", false), (1, "oxide", false), (2, "uikit", true), (3, "oxide", true)]} else if cli.instruments_launch {&[(0, "uikit", false), (1, "oxide", false)]} else {&[(0, "oxide", false), (1, "uikit", false), (2, "uikit", true), (3, "oxide", true)]};
   for &(index, side, delayed) in sequence
   {
      if index < resume_count {continue;}
      let directory = out.join(format!("probe-{index}-{side}-{}", if delayed {"delay"} else {"normal"}));
      fs::create_dir(&directory)?;
      let baseline = delayed.then(|| normal_metrics.get(side).cloned().context("delayed probe missing same-framework normal metrics")).transpose()?;
      let result = probe_capture(root, &device, &directory, side, delayed, baseline.as_deref(), cli.instruments_launch);
      match result
      {
         Ok(metrics) =>
         {
            if !delayed {normal_metrics.insert(side, directory.join("probe-metrics.json"));}
            runs.push(json!({"index":index,"kind":"probe","side":side,"delayed":delayed,"directory":directory,"status":"passed","metrics":metrics}));
            write_json(&out.join("runs.json"), &json!(runs))?;
         }
         Err(error) =>
         {
            let message = format!("{error:#}");
            write_json(&directory.join("failure.json"), &json!({"status":"qualification-blocked","error":message}))?;
            runs.push(json!({"index":index,"kind":"probe","side":side,"delayed":delayed,"directory":directory,"status":"failed","error":message}));
            write_json(&out.join("runs.json"), &json!(runs))?;
            write_json(&out.join("status.json"), &json!({"status":"qualification-blocked","stage":"probe","captures_attempted":resume_count+recording_count(&out)?,"new_recorder_attempts":recording_count(&out)?,"total_attempts_including_history":prior_attempts+if finish_managed {0} else {resume_count}+recording_count(&out)?,"reason":message,"energy_started":false}))?;
            bail!("renewal probe qualification stopped: {message}; retained evidence {}", out.display());
         }
      }
   }
   if cli.instruments_launch && !finish_managed
   {
      write_json(&out.join("status.json"), &json!({"status":"managed-launch-pair-qualified","captures_attempted":2,"total_attempts_including_history":prior_attempts+2,"energy_started":false,"sweep_started":false,"reason":"UIKit and Oxide managed-launch normal probes passed; stall and energy qualification remain pending"}))?;
      return Ok(());
   }
   for (offset, side) in ["oxide", "uikit", "uikit", "oxide"].iter().enumerate()
   {
      let index = offset + 4;
      let directory = out.join(format!("energy-{index}-{side}"));
      fs::create_dir(&directory)?;
      let result = (|| -> Result<Value>
      {
         let completion = record_capture(root, &device, &directory, "images", side, "energy", true, cli.instruments_launch)?;
         let power = reduce_energy_capture(root, &directory, side, &completion)?;
         Ok(json!({"completion":completion,"power":power,"comparative_energy":null}))
      })();
      match result
      {
         Ok(result) =>
         {
            runs.push(json!({"index":index,"kind":"energy","side":side,"directory":directory,"status":"captured","result":result}));
            write_json(&out.join("runs.json"), &json!(runs))?;
         }
         Err(error) =>
         {
            let message = format!("{error:#}");
            write_json(&directory.join("failure.json"), &json!({"status":"qualification-blocked","error":message}))?;
            runs.push(json!({"index":index,"kind":"energy","side":side,"directory":directory,"status":"failed","error":message}));
            write_json(&out.join("runs.json"), &json!(runs))?;
            write_json(&out.join("status.json"), &json!({"status":"qualification-blocked","stage":"energy","captures_attempted":resume_count+recording_count(&out)?,"new_recorder_attempts":recording_count(&out)?,"total_attempts_including_history":prior_attempts+if finish_managed {0} else {resume_count}+recording_count(&out)?,"reason":message,"qualified":false}))?;
            bail!("renewal energy qualification stopped: {message}; retained evidence {}", out.display());
         }
      }
   }
   write_json(&out.join("status.json"), &json!({"status":"repeatability-review-pending","captures_attempted":recording_count(&out)?,"total_attempts_including_history":prior_attempts+if finish_managed {0} else {resume_count}+recording_count(&out)?,"qualified":false,"reason":"presentation probes and energy recordings completed; quartet repeatability and control variation require review"}))?;
   println!("Renewal qualification retained at {}.", out.display());
   Ok(())
}

fn probe_capture(root: &Path, device: &super::UIKitPhysicalDevice, out: &Path, side: &str, delayed: bool, normal_metrics: Option<&Path>, managed_launch: bool) -> Result<Value>
{
   let mode = if delayed {"probe-delay"} else {"probe"};
   let completion = record_capture(root, device, out, "images", side, mode, true, managed_launch)?;
   ensure!(completion["battery_state"] == 1 && completion["low_power_mode"] == false, "probe ended charging or in low power mode: {completion}");
   let trace = out.join("capture.trace");
   let toc = out.join("trace-toc.xml");
   let xml = out.join("trace.xml");
   export_trace(root, &strings(&["xctrace", "export", "--input", &trace.to_string_lossy(), "--toc", "--output", &toc.to_string_lossy()]), &out.join("toc.log"), 120)?;
   let xpath = "/trace-toc/run[@number='1']/data/table[@schema='hitches-updates' or @schema='hitches-renders' or @schema='hitches' or @schema='display-surface-swap' or @schema='os-signpost' or @schema='thread-state']";
   export_trace(root, &strings(&["xctrace", "export", "--input", &trace.to_string_lossy(), "--xpath", xpath, "--output", &xml.to_string_lossy()]), &out.join("export.log"), 120)?;
   let recording: Value = serde_json::from_slice(&fs::read(out.join("recording.json"))?)?;
   let metrics = out.join("probe-metrics.json");
   let reducer = root.join("host/apple-comparison/tools/reduce_core_trace.py");
   let mut args = strings(&[&reducer.to_string_lossy(), "--probe", "--xml", &xml.to_string_lossy(), "--toc", &toc.to_string_lossy(), "--app", app_name(side),
      "--completion", &out.join("completion.json").to_string_lossy(), "--pid", &recording["pid"].to_string(), "--run-id", completion["run_id"].as_str().context("missing probe run ID")?, "--output", &metrics.to_string_lossy()]);
   if let Some(normal_metrics) = normal_metrics {args.extend(strings(&["--normal-metrics", &normal_metrics.to_string_lossy()]));}
   bounded(root, "python3", &args, &out.join("probe-reduce.log"), 30)?;
   let reduced: Value = serde_json::from_slice(&fs::read(&metrics)?)?;
   ensure!(reduced["valid_capture"] == true, "probe trace is invalid: {reduced}");
   ensure!(reduced["validity"]["presentation_cadence"]["valid"] == true, "probe presentation cadence is invalid: {reduced}");
   ensure!(reduced["validity"]["stall_detection"]["valid"] == true, "probe did not establish its declared normal/stall behavior: {reduced}");
   Ok(reduced)
}

fn capture(root: &Path, device: &super::UIKitPhysicalDevice, out: &Path, case: &str, side: &str) -> Result<Value>
{
   record_capture(root, device, out, case, side, "legacy", false, false)?;
   let name = app_name(side);
   let trace = out.join("capture.trace");
   let completion = out.join("completion.json");
   let xml = out.join("trace.xml");
   let toc = out.join("trace-toc.xml");
   export_trace(root, &strings(&["xctrace", "export", "--input", &trace.to_string_lossy(), "--toc", "--output", &toc.to_string_lossy()]), &out.join("toc.log"), 120)?;
   let xpath = "/trace-toc/run[@number='1']/data/table[@schema='hitches-updates' or @schema='hitches-renders' or @schema='hitches' or @schema='display-surface-swap' or @schema='os-signpost' or @schema='thread-state']";
   export_trace(root, &strings(&["xctrace", "export", "--input", &trace.to_string_lossy(), "--xpath", xpath, "--output", &xml.to_string_lossy()]), &out.join("export.log"), 120)?;
   let metrics = out.join("metrics.json");
   let reducer = root.join("host/apple-comparison/tools/reduce_core_trace.py");
   bounded(root, "python3", &strings(&[&reducer.to_string_lossy(), "--xml", &xml.to_string_lossy(), "--toc", &toc.to_string_lossy(), "--app", name, "--case", case, "--completion", &completion.to_string_lossy(), "--output", &metrics.to_string_lossy()]), &out.join("reduce.log"), 30)?;
   let reduced: Value = serde_json::from_slice(&fs::read(&metrics)?)?;
   ensure!(reduced["valid_capture"] == true, "trace does not contain the completed measured window: {reduced}");
   let metric = if case == "text" || case == "local" {"scheduled_update_to_present_ms"} else {"presentation_interval_ms"};
   ensure!(reduced[metric].is_object(), "app-attributed {metric} unavailable; no comparative result can be accepted: {reduced}");
   // Keep the raw Instruments package losslessly, without growing 60 uncompressed copies.
   let archive = out.join("capture.trace.zip");
   bounded(root, "ditto", &strings(&["-c", "-k", "--keepParent", &trace.to_string_lossy(), &archive.to_string_lossy()]), &out.join("archive.log"), 120)?;
   bounded(root, "unzip", &strings(&["-tq", &archive.to_string_lossy()]), &out.join("archive-check.log"), 120)?;
   write_json(&out.join("archive.json"), &json!({"path":archive,"sha256":hash_file(&archive)?}))?;
   fs::remove_dir_all(trace)?;
   Ok(reduced)
}

fn instruments_online(list: &str, udid: &str) -> bool
{
   let mut online = false;
   for line in list.lines()
   {
      if line.starts_with("==") {online = line == "== Devices ==";}
      else if online && line.ends_with(&format!("({udid})")) {return true;}
   }
   false
}

#[cfg(test)]
mod qualification_tests
{
   use super::*;

   #[test]
   fn resume_requires_a_complete_ordered_group_without_mixing_attempts()
   {
      let mut runs: Vec<Value> = sweep_order(0).iter().enumerate().map(|(index, side)| json!({"case":"shapes", "mode":"energy", "attempt":1, "index":index, "side":side, "status":"captured"})).collect();
      assert_eq!(completed_sweep_group(&runs, 0, "energy").unwrap().len(), 4);
      runs[3]["attempt"] = json!(0);
      assert!(completed_sweep_group(&runs, 0, "energy").is_none());
      runs[3]["attempt"] = json!(1);
      runs[3]["side"] = json!("uikit");
      assert!(completed_sweep_group(&runs, 0, "energy").is_none());
   }

   #[test]
   fn completion_status_matches_recording_mode_and_identity()
   {
      let energy = json!({"run_id":"test", "status":"energy-complete"});
      let diagnostic = json!({"run_id":"test", "status":"diagnostic-complete"});
      assert!(capture_completed(&energy, "test", "energy"));
      assert!(capture_completed(&diagnostic, "test", "diagnostic"));
      assert!(capture_completed(&diagnostic, "test", "probe-normal"));
      assert!(!capture_completed(&energy, "test", "diagnostic"));
      assert!(!capture_completed(&diagnostic, "test", "energy"));
      assert!(!capture_completed(&energy, "another-run", "energy"));
      assert!(!capture_completed(&json!({"run_id":"test", "status":"failed"}), "test", "energy"));
   }

   #[test]
   fn sweep_contract_has_sixteen_cases_and_alternating_quartets()
   {
      assert_eq!(SWEEP_CASES.len(), 16);
      assert_eq!(SWEEP_CASES.iter().collect::<std::collections::BTreeSet<_>>().len(), 16);
      for index in 0..16
      {
         let order = sweep_order(index);
         assert_eq!(order[0], order[3]);
         assert_eq!(order[1], order[2]);
         assert_ne!(order[0], order[1]);
         assert_ne!(order[0], sweep_order(index + 1)[0]);
      }
      assert!(parse(&strings(&["--sweep"])).unwrap().sweep);
      assert!(parse(&strings(&["--sweep", "--sweep-energy-only"])).unwrap().sweep_energy_only);
      assert_eq!(parse(&strings(&["--sweep", "--sweep-attempt-limit", "1"])).unwrap().sweep_attempt_limit, Some(1));
      assert_eq!(parse(&strings(&["--sweep", "--sweep-resume", "/retained"])).unwrap().sweep_resume, Some(PathBuf::from("/retained")));
   }

   #[test]
   fn export_retry_excludes_interrupts_and_normal_failures()
   {
      assert!(export_crashed(std::process::ExitStatus::from_raw(11)));
      assert!(export_crashed(std::process::ExitStatus::from_raw(6)));
      assert!(!export_crashed(std::process::ExitStatus::from_raw(2)));
      assert!(!export_crashed(std::process::ExitStatus::from_raw(15)));
      assert!(!export_crashed(std::process::ExitStatus::from_raw(256)));
   }

   #[test]
   fn pilot_options_preserve_history_and_order()
   {
      let cli = parse(&strings(&["--pilot", "--pilot-order", "uikit,oxide", "--pilot-history", "/first", "--pilot-history", "/second"])).unwrap();
      assert!(cli.pilot);
      assert_eq!(cli.pilot_order.as_deref(), Some("uikit,oxide"));
      assert_eq!(cli.pilot_history, [PathBuf::from("/first"), PathBuf::from("/second")]);
   }

   #[test]
   fn named_attachment_rejects_ambiguous_wrong_and_stale_processes()
   {
      let entry = json!({"processIdentifier":42,"executable":"file:///app/OxideBenchIOS.app/OxideBenchIOS"});
      let snapshot = json!({"result":{"runningProcesses":[entry.clone()]}});
      assert_eq!(ready_process(&snapshot, "OxideBenchIOS", 42).unwrap(), entry);
      assert!(ready_process(&snapshot, "OxideBenchIOS", 43).is_err());
      assert!(ready_process(&snapshot, "UIKitBenchIOS", 42).is_err());
      assert!(ready_process(&json!({"result":{"runningProcesses":[entry.clone(), entry]}}), "OxideBenchIOS", 42).is_err());
      assert!(ready_process(&json!({"result":{"runningProcesses":[{"processIdentifier":42,"executable":"file:///Other.app/OxideBenchIOS"}]}}), "OxideBenchIOS", 42).is_err());
   }

   #[test]
   fn ready_and_ack_console_markers_reject_stale_or_prefixed_run_ids()
   {
      assert!(super::super::console_output_contains_marker("CORE_READY current\n", "CORE_READY current"));
      assert!(!super::super::console_output_contains_marker("CORE_READY old\n", "CORE_READY current"));
      assert!(!super::super::console_output_contains_marker("CORE_STARTED current-extra\n", "CORE_STARTED current"));
   }

   #[test]
   fn console_cleanup_accepts_requested_sigterm_but_not_a_crash()
   {
      let failure = std::process::ExitStatus::from_raw(256);
      assert!(expected_console_exit(failure, "CORE_COMPLETE current\nApp terminated due to signal 15.\n"));
      assert!(!expected_console_exit(failure, "App terminated due to signal 11.\n"));
      assert!(!expected_console_exit(failure, "unexpected console error"));
   }

   #[test]
   fn console_cleanup_is_bounded()
   {
      let child = Command::new("sleep").arg("5").spawn().unwrap();
      let mut owned = HostChild(child);
      assert!(owned.wait_for_console_exit(Path::new("/unused"), Duration::ZERO).is_err());
   }

   #[test]
   fn resume_is_explicit_and_requires_renewal_mode()
   {
      assert!(run(&strings(&["--pilot-resume", "/unused"])).unwrap_err().to_string().contains("requires --pilot-renewal"));
      let cli = parse(&strings(&["--pilot-renewal", "--pilot-resume", "/retained"])).unwrap();
      assert_eq!(cli.pilot_resume, Some(PathBuf::from("/retained")));
   }

   #[test]
   fn managed_launch_resolves_only_the_exact_installed_bundle()
   {
      let app = json!({"bundleIdentifier":"com.oxide.comparison.uikitbenchios","url":"file:///private/app/UIKitBenchIOS.app/"});
      let apps = json!({"result":{"apps":[app.clone()]}});
      assert_eq!(installed_launch_path(&apps, "com.oxide.comparison.uikitbenchios", "UIKitBenchIOS").unwrap(), "/private/app/UIKitBenchIOS.app");
      assert!(installed_launch_path(&apps, "wrong", "UIKitBenchIOS").is_err());
      assert!(installed_launch_path(&apps, "com.oxide.comparison.uikitbenchios", "OxideBenchIOS").is_err());
      assert!(installed_launch_path(&json!({"result":{"apps":[app.clone(),app]}}), "com.oxide.comparison.uikitbenchios", "UIKitBenchIOS").is_err());
   }

   #[test]
   fn managed_launch_requires_explicit_qualification_mode()
   {
      assert!(run(&strings(&["--instruments-launch"])).unwrap_err().to_string().contains("requires --pilot-renewal"));
      assert!(parse(&strings(&["--pilot-renewal", "--instruments-launch"])).unwrap().instruments_launch);
   }

   #[test]
   fn renewal_option_parses_without_pilot_order()
   {
      let cli = parse(&strings(&["--pilot-renewal", "--pilot-history", "/original-eight"])).unwrap();
      assert!(cli.pilot_renewal);
      assert!(!cli.pilot);
      assert_eq!(cli.pilot_history, [PathBuf::from("/original-eight")]);
      assert!(cli.pilot_order.is_none());
   }

   #[test]
   fn renewal_requires_history_before_device_access()
   {
      let cli = Cli {pilot_renewal: true, ..Cli::default()};
      assert!(run_pilot_renewal(Path::new("/unused"), cli).unwrap_err().to_string().contains("requires --pilot-history"));
   }

   #[test]
   fn pilot_and_renewal_are_mutually_exclusive()
   {
      let cli = parse(&strings(&["--pilot", "--pilot-renewal"])).unwrap();
      assert!(cli.pilot && cli.pilot_renewal);
   }

   #[test]
   fn excess_pilot_budget_rejected_before_device_access()
   {
      let cli = Cli {pilot: true, pilot_order: Some(vec!["oxide"; 9].join(",")), ..Cli::default()};
      assert!(run_pilot(Path::new("/unused"), cli).unwrap_err().to_string().contains("eight-attempt budget"));
   }

   #[test]
   fn discovery_requires_online_exact_device()
   {
      assert!(instruments_online("== Devices ==\nPhone (123)\n== Devices Offline ==\n", "123"));
      assert!(!instruments_online("== Devices ==\nMac (999)\n== Devices Offline ==\nPhone (123)\n", "123"));
      assert!(!instruments_online("== Simulators ==\nPhone (123)\n", "123"));
      assert!(!instruments_online("== Devices ==\nPhone (0123)\n", "123"));
   }
}
