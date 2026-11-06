//! Development-only cumulative logical-type size checker.
//!
//! Parse authored Rust without expanding macros or selecting cfg branches. Definitions,
//! inherent implementations and trait implementations share a qualified type identity.
//! The Node policy check separately enforces physical file ceilings and dependency edges.

mod components;
mod model;
mod password_boundary;
mod report;
mod resolve;
mod scan;

use std::{
    env,
    path::{Path, PathBuf},
    process::ExitCode,
};

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
    if roots.first().is_some_and(|root| root == Path::new("--components")) {
        if roots.len() > 2 {
            return Err("usage: rbit-rewrite-check --components [repository-root]".into());
        }
        let report = components::check(roots.get(1).map_or(Path::new("."), PathBuf::as_path));
        let passed = report.violations.is_empty() && report.crates.iter().all(|report| report.violations.is_empty());
        println!("{}", serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?);
        return Ok(passed);
    }
    if roots.first().is_some_and(|root| root == Path::new("--password-boundary")) {
        if roots.len() > 2 {
            return Err("usage: rbit-rewrite-check --password-boundary [repository-root]".into());
        }
        let report = password_boundary::check(roots.get(1).map_or(Path::new("."), PathBuf::as_path));
        let passed = report.violations.is_empty();
        println!("{}", serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?);
        return Ok(passed);
    }
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
