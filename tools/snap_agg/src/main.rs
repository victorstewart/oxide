use regex::Regex;
use serde::Serialize;
use std::error::Error;
use std::fs;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;

#[derive(Serialize, Clone, Debug)]
struct SnapRow {
    component: String,
    variant: String,
    state: String,
    pixdiff: u64,
    max_err: u32,
    mse: f64,
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
    (input.unwrap_or_else(|| PathBuf::from("artifacts/static/sweep.txt")), csv, json)
}

fn run() -> Result<(), Box<dyn Error>> {
    let (input, csv_out, json_out) = parse_args();
    let data = fs::read_to_string(&input)?;
    let re = Regex::new(
        r"(?m)^summary\s+suite=static\s+component=([^\s]+)\s+variant=([^\s]+)(?:\s+state=([^\s]+))?\s+pixdiff=([0-9]+)\s+max_err=([0-9]+)\s+mse=([0-9.]+)",
    )?;
    let mut rows: Vec<SnapRow> = Vec::new();
    for cap in re.captures_iter(&data) {
        rows.push(SnapRow {
            component: cap[1].to_string(),
            variant: cap[2].to_string(),
            state: cap.get(3).map(|m| m.as_str().to_string()).unwrap_or_else(|| "default".to_string()),
            pixdiff: cap[4].parse().unwrap_or(0),
            max_err: cap[5].parse().unwrap_or(0),
            mse: cap[6].parse().unwrap_or(0.0),
        });
    }
    if rows.is_empty() {
        return Err(Box::new(IoError::new(
            ErrorKind::InvalidData,
            format!("no summary lines found in {}", input.display()),
        )));
    }
    if let Some(csv_path) = csv_out.clone() {
        let mut w = String::from("component,variant,state,pixdiff,max_err,mse\n");
        for r in &rows {
            w.push_str(&format!("{},{},{},{},{},{}\n", r.component, r.variant, r.state, r.pixdiff, r.max_err, r.mse));
        }
        fs::write(csv_path, w)?;
    }
    if let Some(json_path) = json_out.clone() {
        let json = serde_json::to_string_pretty(&rows)?;
        fs::write(json_path, json)?;
    }
    let fails = rows.iter().filter(|r| r.pixdiff > 0).count();
    println!("failures={} total={}", fails, rows.len());
    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{}", err);
        std::process::exit(2);
    }
}
