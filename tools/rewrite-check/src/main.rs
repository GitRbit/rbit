//! Development-only cumulative logical-type size checker.
//!
//! Parse authored Rust without expanding macros or selecting cfg branches. Definitions,
//! inherent implementations and trait implementations share a qualified type identity.
//! The Node policy check separately enforces physical file ceilings and dependency edges.

mod model;
mod report;
mod resolve;
mod scan;

use std::{env, path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(passed) => {
            if passed {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("rewrite-check: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<bool, String> {
    let roots: Vec<PathBuf> = env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        return Err("usage: rbit-rewrite-check <crate-directory> [crate-directory ...]".into());
    }
    let reports: Vec<_> = roots.iter().map(|root| report::check(root, 400)).collect();
    let passed = reports.iter().all(|report| report.violations.is_empty());
    println!("{}", serde_json::to_string_pretty(&reports).map_err(|error| error.to_string())?);
    Ok(passed)
}

#[cfg(test)]
mod tests;
