//! Conformance cases with unchanged verdict and fixture semantics.
use super::observation::string_list;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Every `kind` a case may use, and what it drives. See the README for each input and output.
const KINDS: &[&str] = &[
    "expr",
    "condition",
    "render",
    "needs_status",
    "parse",
    "plan",
    "run_name",
    "concurrency",
    "trigger",
    "dispatch",
    "pattern",
    "skip_marker",
    "cron",
];

/// One conformance case, validated.
#[derive(Debug)]
pub(super) struct Case {
    pub(super) id: String,
    pub(super) file: String,
    pub(super) area: String,
    pub(super) tags: Vec<String>,
    pub(super) why_hard: Option<String>,
    pub(super) source: String,
    pub(super) split: String,
    pub(super) kind: String,
    pub(super) input: Value,
    pub(super) expect: Value,
}

pub(super) fn load_cases(dir: &Path, problems: &mut Vec<String>) -> Vec<Case> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("{}: {error}", dir.display()))
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "json"))
        .collect();
    files.sort();

    let mut cases = Vec::new();
    let mut seen = BTreeSet::new();
    for path in files {
        let file = path.file_name().expect("a file name").to_string_lossy().into_owned();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                problems.push(format!("{file}: unreadable: {error}"));
                continue;
            }
        };
        let document: Value = match serde_json::from_str(&text) {
            Ok(document) => document,
            Err(error) => {
                problems.push(format!("{file}: not valid JSON: {error}"));
                continue;
            }
        };
        let Some(raw_cases) = document.get("cases").and_then(Value::as_array) else {
            problems.push(format!("{file}: no `cases` array"));
            continue;
        };
        for raw in raw_cases {
            if let Some(case) = parse_case(raw, &file, problems) {
                if !seen.insert(case.id.clone()) {
                    problems.push(format!("{file}: duplicate case id `{}`", case.id));
                }
                cases.push(case);
            }
        }
    }
    cases
}

/// Reads one case and checks the schema: every field present, the tags known, a `why_hard`
/// on every hard case, and a split of `train` or `test`.
fn parse_case(raw: &Value, file: &str, problems: &mut Vec<String>) -> Option<Case> {
    let id = raw.get("id").and_then(Value::as_str).unwrap_or("<no id>").to_string();
    let mut fail = |why: &str| problems.push(format!("{file} {id}: {why}"));
    let text = |key: &str| raw.get(key).and_then(Value::as_str).map(str::to_string);

    let mut ok = true;
    if raw.get("id").and_then(Value::as_str).is_none() {
        fail("missing `id`");
        ok = false;
    }
    let tags = string_list(raw.get("tags"));
    if tags.is_empty() || tags.iter().any(|tag| !["easy", "hard", "edge"].contains(&tag.as_str())) {
        fail("`tags` must hold easy, hard or edge");
        ok = false;
    }
    let why_hard = text("why_hard").filter(|why| !why.trim().is_empty());
    if tags.iter().any(|tag| tag == "hard") && why_hard.is_none() {
        fail("a hard case needs `why_hard`");
        ok = false;
    }
    let source = text("source").filter(|source| !source.trim().is_empty());
    if source.is_none() {
        fail("missing `source` (the GitHub docs behaviour it relies on)");
        ok = false;
    }
    let split = text("split").unwrap_or_default();
    if split != "train" && split != "test" {
        fail("`split` must be train or test");
        ok = false;
    }
    let kind = text("kind").unwrap_or_default();
    if !KINDS.contains(&kind.as_str()) {
        fail(&format!("unknown kind `{kind}`"));
        ok = false;
    }
    let (Some(input), Some(expect)) = (raw.get("input").cloned(), raw.get("expect").cloned()) else {
        fail("missing `input` or `expect`");
        return None;
    };
    if !ok {
        return None;
    }
    Some(Case {
        id,
        file: file.to_string(),
        area: text("area").unwrap_or_default(),
        tags,
        why_hard,
        source: source.unwrap_or_default(),
        split,
        kind,
        input,
        expect,
    })
}

/// The ids in `known_failures.txt`: the first word of each line that is not blank or a
/// comment.
pub(super) fn load_known(dir: &Path, problems: &mut Vec<String>) -> BTreeSet<String> {
    let path = dir.join("known_failures.txt");
    let Ok(text) = std::fs::read_to_string(&path) else {
        problems.push("known_failures.txt is missing".into());
        return BTreeSet::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_whitespace().next().map(str::to_string))
        .collect()
}
