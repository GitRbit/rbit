//! Which cache scopes a task may use (§12.5.6), decided from its run's ref and trust. The
//! server puts the result in every [`crate::spec::JobSpec`]; a runner's local cache and the
//! server's shared cache both read and write only these, so an untrusted run poisons
//! nothing another run reads first.

use sha2::{Digest as _, Sha256};

use super::job::CacheScopes;

/// Longest ref-derived scope; a longer ref is named by its digest.
const MAX_REF_SCOPE_BYTES: usize = 200;

/// What the scopes of a task depend on: its run and the repository's default branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunCacheScope<'a> {
    pub repository_id: i64,
    pub run_id: i64,
    /// The run's full ref (`refs/heads/main`, `refs/pull/3/head`).
    pub reference: &'a str,
    pub trusted: bool,
    /// The pull request a `pull_request` run is for.
    pub pull_number: Option<i64>,
    pub default_branch: &'a str,
}

impl CacheScopes {
    /// Read the run's own scope, then the default branch's; write its own — `pull/<n>` for an
    /// untrusted pull request run, `untrusted/<run>` for another untrusted run.
    pub fn for_run(run: &RunCacheScope<'_>) -> Self {
        let write = match (run.trusted, run.pull_number) {
            (true, _) => cache_ref_scope(run.reference),
            (false, Some(number)) => format!("pull/{number}"),
            (false, None) => format!("untrusted/{}", run.run_id),
        };
        let default = cache_ref_scope(&format!("refs/heads/{}", run.default_branch));
        let mut read = vec![write.clone()];
        if default != write {
            read.push(default);
        }
        Self { repository_id: run.repository_id, read, write }
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

    fn run(reference: &str, trusted: bool, pull_number: Option<i64>) -> RunCacheScope<'_> {
        RunCacheScope { repository_id: 4, run_id: 9, reference, trusted, pull_number, default_branch: "main" }
    }

    #[test]
    fn untrusted_runs_never_write_where_others_read() {
        let push = CacheScopes::for_run(&run("refs/heads/feature", true, None));
        assert_eq!(
            (push.read.clone(), push.write.as_str(), push.repository_id),
            (vec!["heads/feature".into(), "heads/main".into()], "heads/feature", 4)
        );
        assert_eq!(CacheScopes::for_run(&run("refs/heads/main", true, None)).read, ["heads/main"]);
        let trusted_pull = CacheScopes::for_run(&run("refs/pull/3/head", true, Some(3)));
        assert_eq!(
            (trusted_pull.read.clone(), trusted_pull.write.as_str()),
            (vec!["pull/3/head".into(), "heads/main".into()], "pull/3/head")
        );
        let untrusted = CacheScopes::for_run(&run("refs/pull/3/head", false, Some(3)));
        assert_eq!((untrusted.read.clone(), untrusted.write.as_str()), (vec!["pull/3".into(), "heads/main".into()], "pull/3"));
        assert_eq!(CacheScopes::for_run(&run("refs/heads/x", false, None)).write, "untrusted/9");
        for scopes in [push, trusted_pull, untrusted] {
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
