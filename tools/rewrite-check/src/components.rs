//! Exact replacement-component policy inside legacy crates; source errors remain fatal.
use crate::{
    model::Key,
    report::{self, Report},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path},
};
const POLICY: &str = "docs/design/rewrite/component-types.json";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    schema: u32,
    components: Vec<Boundary>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Boundary {
    #[serde(rename = "crate")]
    crate_path: String,
    types: Vec<String>,
}
#[derive(Serialize)]
pub struct ComponentReport {
    pub crates: Vec<Report>,
    pub violations: Vec<String>,
}
pub fn check(root: &Path) -> ComponentReport {
    let mut report = ComponentReport { crates: Vec::new(), violations: Vec::new() };
    match fs::read_to_string(root.join(POLICY))
        .map_err(|error| error.to_string())
        .and_then(|text| serde_json::from_str::<Policy>(&text).map_err(|error| error.to_string()))
    {
        Ok(policy) => apply(root, policy, &mut report),
        Err(error) => report.violations.push(format!("component policy: {error}")),
    }
    report
}
fn apply(root: &Path, policy: Policy, report: &mut ComponentReport) {
    if policy.schema != 1 || policy.components.is_empty() {
        report.violations.push("component policy requires schema 1 and nonempty mappings".into());
        return;
    }
    let mut crates = BTreeSet::new();
    for boundary in policy.components {
        let path = Path::new(&boundary.crate_path);
        if path.is_absolute()
            || path.components().count() < 2
            || path.components().any(|part| !matches!(part, Component::Normal(_)))
            || !crates.insert(boundary.crate_path.clone())
        {
            report.violations.push(format!("invalid or duplicate component crate {}", boundary.crate_path));
            continue;
        }
        let mut required = BTreeSet::<Key>::new();
        let mut valid = !boundary.types.is_empty();
        for name in boundary.types {
            let key: Key = name.split("::").map(str::to_owned).collect();
            if key.len() < 2
                || !matches!(key[0].as_str(), "lib" | "main")
                || key.iter().any(|part| !identifier(part))
                || !required.insert(key)
            {
                report.violations.push(format!("invalid or duplicate component type {name}"));
                valid = false;
            }
        }
        if !valid {
            report.violations.push(format!("nonempty exact component types required for {}", boundary.crate_path));
            continue;
        }
        report.crates.push(report::check_components(&root.join(path), 400, &required));
    }
}
fn identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|value| value.is_ascii_alphabetic() || value == '_')
        && chars.all(|value| value.is_ascii_alphanumeric() || value == '_')
}
#[cfg(test)]
mod tests;
