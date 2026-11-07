//! Aggregate unique authored line spans and produce machine-readable diagnostics.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::{
    model::{Key, Lines},
    scan,
};

#[derive(Debug, Serialize)]
pub struct TypeSize {
    pub name: String,
    pub lines: usize,
    pub files: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub crate_path: String,
    pub limit: usize,
    pub types: Vec<TypeSize>,
    pub violations: Vec<String>,
}

pub fn check(root: &Path, limit: usize) -> Report {
    check_selected(root, limit, None)
}

pub fn check_components(root: &Path, limit: usize, required: &BTreeSet<Key>) -> Report {
    check_selected(root, limit, Some(required))
}

fn check_selected(root: &Path, limit: usize, required: Option<&BTreeSet<Key>>) -> Report {
    let sources = scan::read_crate(root);
    let mut report = Report { crate_path: root.display().to_string(), limit, types: Vec::new(), violations: sources.errors.clone() };
    let mut groups: BTreeMap<Key, Vec<Lines>> = BTreeMap::new();
    for (key, definition) in &sources.definitions {
        let target = if let Some((scope, ty)) = &definition.alias {
            match sources.resolve_type(scope, ty, &mut BTreeSet::new()) {
                Ok(Some(key)) => key,
                Ok(None) => key.clone(),
                Err(error) => {
                    report.violations.push(error);
                    key.clone()
                }
            }
        } else {
            key.clone()
        };
        groups.entry(target).or_default().extend(definition.lines.iter().cloned());
    }
    for implementation in &sources.implementations {
        match sources.implementation_key(implementation) {
            Ok(Some(key)) => groups.entry(key).or_default().push(implementation.lines.clone()),
            Ok(None) => {
                // Foreign/blanket self types have no local class definition. Each
                // implementation still has an independent logical-size ceiling.
                let mut key = implementation.scope.clone();
                key.push(format!("@foreign-impl:{}:{}", implementation.lines.file.display(), implementation.lines.first));
                groups.entry(key).or_default().push(implementation.lines.clone());
            }
            Err(error) => {
                report.violations.push(format!("{}:{}: {error}", implementation.lines.file.display(), implementation.lines.first));
            }
        }
    }
    if let Some(required) = required {
        for key in required {
            match sources.definitions.get(key) {
                None => report.violations.push(format!("missing required component {}", key.join("::"))),
                Some(definition) if definition.alias.is_some() => {
                    report.violations.push(format!("required component {} must own its definition, not an alias", key.join("::")));
                }
                Some(_) => {}
            }
        }
    }
    for (key, spans) in groups {
        if required.is_some_and(|required| !required.contains(&key)) {
            continue;
        }
        let (lines, files) = count_lines(&spans);
        let name = key.join("::");
        if lines > limit {
            report.violations.push(format!("{name}: {lines} cumulative authored lines exceed {limit}"));
        }
        report.types.push(TypeSize { name, lines, files });
    }
    report
}

fn count_lines(spans: &[Lines]) -> (usize, Vec<String>) {
    let mut files: BTreeMap<PathBuf, BTreeSet<usize>> = BTreeMap::new();
    for span in spans {
        files.entry(span.file.clone()).or_default().extend(span.first..=span.last);
    }
    let count = files.values().map(BTreeSet::len).sum();
    (count, files.keys().map(|path| path.display().to_string()).collect())
}
