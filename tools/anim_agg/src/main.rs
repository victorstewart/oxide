use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;

#[derive(Serialize, Clone, Debug)]
struct FrameRow {
    component: String,
    variant: String,
    state: String,
    time_ms: u32,
    pixdiff: u64,
    max_err: u32,
    mse: f64,
}

#[derive(Serialize, Clone, Debug)]
struct Summary {
    frames: usize,
    failures: usize,
    by_component: HashMap<String, usize>,
}

fn parse_args() -> (PathBuf, Option<PathBuf>, Option<PathBuf>) {
    let mut input: Option<PathBuf> = None;
    let mut csv: Option<PathBuf> = None;
    let mut json: Option<PathBuf> = None;
    let mut it = std::env::args().skip(1).peekable();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--input" => input = it.next().map(PathBuf::from),
            "--csv" => csv = it.next().map(PathBuf::from),
            "--json" => json = it.next().map(PathBuf::from),
            _ => {}
        }
    }
    (
        input.unwrap_or_else(|| PathBuf::from("artifacts/anim/sweep.txt")),
        csv,
        json,
    )
}

fn run() -> Result<(), Box<dyn Error>> {
    let (input, csv_out, json_out) = parse_args();
    let data = fs::read_to_string(&input)?;
    let re = Regex::new(
        r"(?m)^summary\s+suite=anim\s+component=([^\s]+)\s+variant=([^\s]+)\s+state=([^\s]+)\s+time_ms=([0-9]+)\s+pixdiff=([0-9]+)\s+max_err=([0-9]+)\s+mse=([0-9.]+)",
    )?;
    let mut rows: Vec<FrameRow> = Vec::new();
    for cap in re.captures_iter(&data) {
        rows.push(FrameRow {
            component: cap[1].to_string(),
            variant: cap[2].to_string(),
            state: cap[3].to_string(),
            time_ms: cap[4].parse().unwrap_or(0),
            pixdiff: cap[5].parse().unwrap_or(0),
            max_err: cap[6].parse().unwrap_or(0),
            mse: cap[7].parse().unwrap_or(0.0),
        });
    }
    if rows.is_empty() {
        return Err(Box::new(IoError::new(
            ErrorKind::InvalidData,
            format!("no summary lines found in {}", input.display()),
        )));
    }
    rows.sort_by_key(|r| (r.component.clone(), r.variant.clone(), r.state.clone(), r.time_ms));
    if let Some(csv_path) = csv_out.clone() {
        let mut w = String::from("component,variant,state,time_ms,pixdiff,max_err,mse\n");
        for r in &rows {
            w.push_str(&format!("{},{},{},{},{},{},{}\n", r.component, r.variant, r.state, r.time_ms, r.pixdiff, r.max_err, r.mse));
        }
        fs::write(csv_path, w)?;
    }
    let mut by_component: HashMap<String, usize> = HashMap::new();
    let failures = rows.iter().filter(|r| r.pixdiff > 0).count();
    for r in &rows {
        *by_component.entry(r.component.clone()).or_default() += 1;
    }
    let summary = Summary { frames: rows.len(), failures, by_component };
    if let Some(json_path) = json_out.clone() {
        let json = serde_json::to_string_pretty(&summary)?;
        fs::write(json_path, json)?;
    }
    println!("frames={} failures={}", summary.frames, summary.failures);
    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{}", err);
        std::process::exit(2);
    }
}
