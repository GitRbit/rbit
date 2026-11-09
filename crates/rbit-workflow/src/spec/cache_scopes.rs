//! Which cache scopes a task may use (§12.5.6), from the scope its run writes. The
//! server puts the result in every [`crate::spec::JobSpec`]; a runner's local cache and the
//! server's shared cache both read and write only these, so an untrusted run poisons
//! nothing another run reads first.

use sha2::{Digest as _, Sha256};

use super::job::CacheScopes;

/// Longest ref-derived scope; a longer ref is named by its digest.
const MAX_REF_SCOPE_BYTES: usize = 200;

/// What the scopes of a task depend on: the scope its run writes and the repository's
/// default branch. The server decides `write` from the run's trust (`rbit_actions::trust`):
/// the run's own [`cache_ref_scope`] when trusted, else one no other run reads first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunCacheScope<'a> {
    pub repository_id: i64,
    pub write: String,
    pub default_branch: &'a str,
}

impl CacheScopes {
    /// Write the run's scope; read it, then the default branch's.
    pub fn for_run(run: &RunCacheScope<'_>) -> Self {
        let default = cache_ref_scope(&format!("refs/heads/{}", run.default_branch));
        let mut read = vec![run.write.clone()];
        if default != run.write {
            read.push(default);
        }
        Self { repository_id: run.repository_id, read, write: run.write.clone() }
    }
}

/// A ref's cache scope: the ref without `refs/` (`heads/main`, `pull/3/head`), or
/// `ref/` and the first 16 bytes of its SHA-256 in hex when that would not make a safe path.
pub fn cache_ref_scope(reference: &str) -> String {
    let scope = reference.strip_prefix("refs/").unwrap_or(reference);
    if safe_scope(scope, MAX_REF_SCOPE_BYTES) {
        return scope.to_owned();
    }
    let digest = Sha256::digest(reference.as_bytes());
    digest.iter().take(16).fold(String::from("ref/"), |mut hex, byte| {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

/// Whether `scope` is a relative path a cache can live under: 1 to `max_bytes` bytes of
/// non-empty `/`-separated parts, none `.` or `..`, and no `\` or NUL.
pub(super) fn safe_scope(scope: &str, max_bytes: usize) -> bool {
    !scope.is_empty()
        && scope.len() <= max_bytes
        && !scope.split('/').any(|part| part.is_empty() || part == "." || part == "..")
        && !scope.contains(['\\', '\0'])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(write: &str) -> RunCacheScope<'static> {
        RunCacheScope { repository_id: 4, write: write.to_owned(), default_branch: "main" }
    }

    #[test]
    fn a_task_reads_its_own_scope_then_the_default_branch() {
        let feature = CacheScopes::for_run(&run(&cache_ref_scope("refs/heads/feature")));
        assert_eq!(
            (feature.read.clone(), feature.write.as_str(), feature.repository_id),
            (vec!["heads/feature".into(), "heads/main".into()], "heads/feature", 4)
        );
        assert_eq!(CacheScopes::for_run(&run("heads/main")).read, ["heads/main"]);
        let pull = CacheScopes::for_run(&run("pull/3"));
        assert_eq!((pull.read.clone(), pull.write.as_str()), (vec!["pull/3".into(), "heads/main".into()], "pull/3"));
        for scopes in [feature, pull] {
            scopes.validate().unwrap();
        }
    }

    #[test]
    fn unusual_refs_are_named_by_their_digest() {
        let long = cache_ref_scope(&format!("refs/heads/{}", "x".repeat(300)));
        assert!(long.starts_with("ref/") && long.len() == 4 + 32, "{long}");
        for reference in ["refs/heads/a//b", "refs/heads/../x", "refs/heads/a\\b", "refs/"] {
            assert!(cache_ref_scope(reference).starts_with("ref/"), "{reference}");
        }
        assert_eq!(cache_ref_scope("refs/tags/v1.0"), "tags/v1.0");
        assert_eq!(cache_ref_scope("refs/heads/x"), cache_ref_scope("refs/heads/x"));
        assert_ne!(cache_ref_scope("refs/heads/a//b"), cache_ref_scope("refs/heads/a//c"));
    }
}
