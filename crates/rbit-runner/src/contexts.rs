//! The expression contexts a runner supplies at step time (§12.4, last row): the server's
//! `github`, `inputs`, `vars`, `matrix`, `strategy` and `needs`, plus the runner's own
//! `env`, `secrets`, `steps`, `job` and `runner`, the status functions, and `hashFiles`
//! over the task's workspace.

use std::{
    io::Read as _,
    path::{Path, PathBuf},
};

use rbit_workflow::{
    expr::{ContextMap, Contexts, Status, Value},
    pattern::Patterns,
    spec::{JobResult, JobSpec, Strategy},
};
use serde_json::{Map, json};
use sha2::{Digest as _, Sha256};

/// Most files `hashFiles` reads: files its patterns match.
const MAX_HASHED_FILES: usize = 100_000;
/// Most files `hashFiles` looks at to find them; a warm build directory or `node_modules`
/// holds far more than it hashes.
const MAX_VISITED_FILES: usize = 5_000_000;

/// The contexts of one evaluation.
#[derive(Debug)]
pub struct Eval<'a> {
    pub contexts: &'a ContextMap,
    /// For `if:` conditions only.
    pub status: Option<Status>,
    /// The workspace `hashFiles` reads.
    pub workspace: Option<&'a Path>,
}

impl Contexts for Eval<'_> {
    fn context(&self, name: &str) -> Option<&Value> {
        self.contexts.get(name)
    }

    fn status(&self) -> Option<Status> {
        self.status
    }

    fn hash_files(&self, patterns: &[String]) -> Option<Result<String, String>> {
        self.workspace.map(|workspace| hash_files(workspace, patterns))
    }
}

/// The contexts that hold for the whole job: everything but `env`, `steps` and `job`,
/// which change as the job runs.
pub fn job_contexts(spec: &JobSpec, github: Map<String, Value>, runner: Value) -> ContextMap {
    let vars: Map<String, Value> = spec.contexts.vars.iter().map(|(name, value)| (name.clone(), Value::String(value.clone()))).collect();
    let needs: Map<String, Value> = spec
        .contexts
        .needs
        .iter()
        .map(|(id, needed)| (id.clone(), json!({"result": result_name(needed.result), "outputs": needed.outputs})))
        .collect();
    let mut secrets: Map<String, Value> =
        spec.secrets.iter().map(|(name, value)| (name.clone(), Value::String(value.expose().to_owned()))).collect();
    // The task token is these two secrets and `github.token`; it reaches a step's
    // environment only where the step maps it.
    for name in ["GITHUB_TOKEN", "RBIT_TOKEN"] {
        secrets.insert(name.to_owned(), Value::String(spec.token.expose().to_owned()));
    }
    ContextMap::new()
        .with("github", github)
        .with("inputs", spec.contexts.inputs.clone())
        .with("vars", vars)
        .with("matrix", spec.template.matrix.clone())
        .with("strategy", strategy(&spec.template.strategy))
        .with("needs", needs)
        .with("secrets", secrets)
        .with("runner", runner)
}

/// The `strategy` context, as the server builds it at planning.
fn strategy(strategy: &Strategy) -> Value {
    json!({
        "fail-fast": strategy.fail_fast,
        "job-index": strategy.job_index,
        "job-total": strategy.job_total,
        "max-parallel": strategy.max_parallel.unwrap_or(strategy.job_total),
    })
}

pub const fn result_name(result: JobResult) -> &'static str {
    match result {
        JobResult::Success => "success",
        JobResult::Failure => "failure",
        JobResult::Cancelled => "cancelled",
        JobResult::Skipped => "skipped",
    }
}

/// `hashFiles(patterns…)`: the SHA-256 over the SHA-256 of every matching file of the
/// workspace, in path order, as hex; empty when nothing matches. Patterns are GitHub
/// filter patterns relative to the workspace, `!` excluding; `.git` is never read.
pub fn hash_files(workspace: &Path, patterns: &[String]) -> Result<String, String> {
    hash_files_within(workspace, patterns, Limits { hashed: MAX_HASHED_FILES, visited: MAX_VISITED_FILES })
}

/// How many files `hashFiles` may hash, and look at.
#[derive(Debug, Clone, Copy)]
struct Limits {
    hashed: usize,
    visited: usize,
}

fn hash_files_within(workspace: &Path, patterns: &[String], limits: Limits) -> Result<String, String> {
    if patterns.is_empty() {
        return Err("hashFiles needs at least one pattern".to_owned());
    }
    let patterns = Patterns::compile(patterns.iter().map(|pattern| pattern.trim_start_matches("./"))).map_err(|error| error.to_string())?;
    let mut files = Vec::new();
    let mut visited = 0;
    collect_files(workspace, workspace, &mut |relative, path| {
        visited += 1;
        if visited > limits.visited {
            return Err(format!("the workspace has more than {} files", limits.visited));
        }
        if !patterns.skip([relative.as_str()]) {
            if files.len() == limits.hashed {
                return Err(format!("the patterns match more than {} files", limits.hashed));
            }
            files.push((relative, path));
        }
        Ok(())
    })?;
    files.sort();
    let mut outer = Sha256::new();
    let mut matched = 0usize;
    for (relative, path) in &files {
        let mut file = crate::untrusted::open(path).map_err(|error| format!("cannot read {relative}: {error}"))?;
        let mut inner = Sha256::new();
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let read = file.read(&mut buffer).map_err(|error| format!("cannot read {relative}: {error}"))?;
            if read == 0 {
                break;
            }
            inner.update(&buffer[..read]);
        }
        outer.update(inner.finalize());
        matched += 1;
    }
    Ok(if matched == 0 { String::new() } else { hex(&outer.finalize()) })
}

/// Hands every file under `dir` (but `.git`) to `found`, with its path relative to `root`.
fn collect_files(root: &Path, dir: &Path, found: &mut impl FnMut(String, PathBuf) -> Result<(), String>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|error| format!("cannot list {}: {error}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            if entry.file_name() != ".git" {
                collect_files(root, &path, found)?;
            }
        } else if kind.is_file() {
            let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            found(relative, path)?;
        }
    }
    Ok(())
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::job_spec;

    #[test]
    fn hash_files_hashes_matching_files_in_order() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("crates/a")).unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join("Cargo.lock"), "root").unwrap();
        std::fs::write(dir.path().join("crates/a/Cargo.lock"), "a").unwrap();
        std::fs::write(dir.path().join(".git/Cargo.lock"), "ignored").unwrap();
        let hash = |patterns: &[&str]| hash_files(dir.path(), &patterns.iter().map(|p| (*p).to_owned()).collect::<Vec<_>>()).unwrap();
        let all = hash(&["**/Cargo.lock"]);
        assert_eq!(all.len(), 64);
        assert_eq!(all, hash(&["./**/Cargo.lock"]));
        assert_ne!(all, hash(&["Cargo.lock"]));
        assert_eq!(hash(&["**/Cargo.lock", "!crates/**"]), hash(&["Cargo.lock"]));
        assert_eq!(hash(&["*.nothing"]), "");
        // The digest is over the files' digests, so it changes with any file.
        std::fs::write(dir.path().join("crates/a/Cargo.lock"), "b").unwrap();
        assert_ne!(all, hash(&["**/Cargo.lock"]));
        assert!(hash_files(dir.path(), &[]).is_err());
    }

    #[test]
    fn hash_files_is_bounded_by_the_files_it_matches_not_the_workspace() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("target")).unwrap();
        for index in 0..20 {
            std::fs::write(dir.path().join("target").join(index.to_string()), "build output").unwrap();
        }
        std::fs::write(dir.path().join("Cargo.lock"), "lock").unwrap();
        let small = Limits { hashed: 5, visited: 100 };
        let lock = hash_files_within(dir.path(), &["Cargo.lock".to_owned()], small).unwrap();
        assert!(!lock.is_empty(), "20 unmatched files are no reason to fail");
        let every = hash_files_within(dir.path(), &["**".to_owned()], small).unwrap_err();
        assert!(every.contains("match more than 5 files"), "{every}");
        let tiny = Limits { hashed: 5, visited: 10 };
        assert!(hash_files_within(dir.path(), &["Cargo.lock".to_owned()], tiny).unwrap_err().contains("more than 10 files"));
    }

    #[test]
    fn the_token_is_a_secret_but_not_in_github() {
        let spec = job_spec(1, 1);
        let contexts = job_contexts(&spec, Map::new(), json!({"os": "Linux"}));
        let secrets = contexts.get("secrets").unwrap();
        assert_eq!(secrets["GITHUB_TOKEN"], spec.token.expose());
        assert_eq!(secrets["RBIT_TOKEN"], spec.token.expose());
        assert_eq!(contexts.get("strategy").unwrap()["job-total"], 1);
        assert_eq!(contexts.get("runner").unwrap()["os"], "Linux");
        assert!(contexts.get("env").is_none());
    }
}
