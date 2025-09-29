use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use regex::Regex;
use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
struct RunStat
{
   use_thresh: f64,
   prefilter: f64,
   n_fps: usize,
   avg_fps: f64,
   n_ms: usize,
   p95_ms: f64
}

#[derive(Serialize)]
struct Summary
{
   best_use: f64,
   best_prefilter: f64,
   basis: String
}

fn parse_args() -> (PathBuf, Option<PathBuf>, Option<PathBuf>)
{
   let mut input: Option<PathBuf> = None;
   let mut csv: Option<PathBuf> = None;
   let mut json: Option<PathBuf> = None;

   let mut it = std::env::args().skip(1).peekable();
   while let Some(arg) = it.next()
   {
      match arg.as_str()
      {
         "--input" => { input = it.next().map(PathBuf::from); }
         "--csv" => { csv = it.next().map(PathBuf::from); }
         "--json" => { json = it.next().map(PathBuf::from); }
         _ => {}
      }
   }
   let input = input.unwrap_or_else(|| PathBuf::from("sweep.txt"));
   (input, csv, json)
}

fn quantile(vals: &mut [f64], q: f64) -> f64
{
   if vals.is_empty() { return f64::NAN; }
   vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
   let n = vals.len() as f64;
   let idx = ((n - 1.0) * q).clamp(0.0, n - 1.0);
   let lo = idx.floor() as usize;
   let hi = idx.ceil() as usize;
   if lo == hi { return vals[lo]; }
   let w = idx - lo as f64;
   (1.0 - w) * vals[lo] + w * vals[hi]
}

fn main()
{
   let (input, csv_out, json_out) = parse_args();
   let data = fs::read_to_string(&input).expect("read sweep.txt");

   let re_begin = Regex::new(r"^##\s*RUN\s+use=([0-9.]+)\s+prefilter=([0-9.]+)").unwrap();
   let re_end = Regex::new(r"^##\s*END\s+use=([0-9.]+)\s+prefilter=([0-9.]+)").unwrap();

   // Common metric patterns; extend if your runner prints different keys.
   let re_fps = Regex::new(r"(?i)\bfps\s*=\s*([0-9]+(?:\.[0-9]+)?)").unwrap();
   let re_p95 = Regex::new(r"(?i)\bp95(?:_?ms)?\s*=\s*([0-9]+(?:\.[0-9]+)?)").unwrap();
   let re_ms = Regex::new(r"(?i)\b(?:frame[_\s-]?time|encode_ms|enc_ms|ms)\s*=\s*([0-9]+(?:\.[0-9]+)?)").unwrap();

   #[derive(Default)]
   struct Acc { fps_vals: Vec<f64>, ms_vals: Vec<f64> }

   let mut runs: HashMap<(String, String), Acc> = HashMap::new();
   let mut cur: Option<(String, String)> = None;

   for line in data.lines()
   {
      if let Some(cap) = re_begin.captures(line)
      {
         let u = cap[1].to_string();
         let p = cap[2].to_string();
         cur = Some((u.clone(), p.clone()));
         runs.entry((u, p)).or_default();
         continue;
      }
      if re_end.is_match(line) { cur = None; continue; }
      if let Some((u, p)) = cur.clone()
      {
         if let Some(cap) = re_fps.captures(line) { runs.get_mut(&(u.clone(), p.clone())).unwrap().fps_vals.push(cap[1].parse().unwrap_or(f64::NAN)); }
         if let Some(cap) = re_p95.captures(line) { runs.get_mut(&(u.clone(), p.clone())).unwrap().ms_vals.push(cap[1].parse().unwrap_or(f64::NAN)); }
         if let Some(cap) = re_ms.captures(line) { runs.get_mut(&(u, p)).unwrap().ms_vals.push(cap[1].parse().unwrap_or(f64::NAN)); }
      }
   }

   let mut stats: Vec<RunStat> = Vec::new();
   for ((u, p), acc) in runs
   {
      let avg_fps = if acc.fps_vals.is_empty() { f64::NAN } else { acc.fps_vals.iter().copied().sum::<f64>() / (acc.fps_vals.len() as f64) };
      let mut ms = acc.ms_vals.clone();
      let p95_ms = if ms.is_empty() { f64::NAN } else { quantile(&mut ms[..], 0.95) };
      let u_f: f64 = u.parse().unwrap_or(f64::NAN);
      let p_f: f64 = p.parse().unwrap_or(f64::NAN);
      stats.push(RunStat { use_thresh: u_f, prefilter: p_f, n_fps: acc.fps_vals.len(), avg_fps, n_ms: ms.len(), p95_ms });
   }

   stats.sort_by(|a, b|
   {
      // Prefer higher FPS if present; otherwise prefer lower p95_ms.
      let a_key = if a.avg_fps.is_nan() { -1e9 } else { a.avg_fps };
      let b_key = if b.avg_fps.is_nan() { -1e9 } else { b.avg_fps };
      match b_key.partial_cmp(&a_key).unwrap_or(Ordering::Equal)
      {
         Ordering::Equal =>
         {
            let a_p = if a.p95_ms.is_nan() { 1e9 } else { a.p95_ms };
            let b_p = if b.p95_ms.is_nan() { 1e9 } else { b.p95_ms };
            a_p.partial_cmp(&b_p).unwrap_or(Ordering::Equal)
         }
         other => other
      }
   });

   if let Some(csv_path) = csv_out.clone()
   {
      let mut w = String::from("use,prefilter,n_fps,avg_fps,n_ms,p95_ms\n");
      for s in &stats
      {
         w.push_str(&format!("{:.6},{:.6},{},{:.6},{},{:.6}\n", s.use_thresh, s.prefilter, s.n_fps, s.avg_fps, s.n_ms, s.p95_ms));
      }
      fs::write(csv_path, w).expect("write csv");
   }

   if let Some(best) = stats.first()
   {
      let basis = if !best.avg_fps.is_nan() { "max_avg_fps".to_string() } else { "min_p95_ms".to_string() };
      let summary = Summary { best_use: best.use_thresh, best_prefilter: best.prefilter, basis };
      if let Some(json_path) = json_out.clone()
      {
         fs::write(json_path, serde_json::to_string_pretty(&summary).unwrap()).expect("write json");
      }
      println!("best_use={:.6} best_prefilter={:.6} basis={}", summary.best_use, summary.best_prefilter, summary.basis);
   }
   else
   {
      eprintln!("no runs found; check markers in sweep.txt");
      std::process::exit(2);
   }
}
