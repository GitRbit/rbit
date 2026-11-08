//! Reference names that are one loose file on a case-insensitive filesystem.
//!
//! Windows and macOS repositories normally live on case-insensitive filesystems, where
//! `refs/heads/MAIN` and `refs/heads/main` are the same loose file while `packed-refs`
//! keeps them apart. Creating `MAIN` while `main` is packed therefore writes a loose file
//! that `main` resolves to from then on: a reference expected not to exist replaces an
//! existing one, and that one's compare-and-swap never ran. rbit therefore never writes a
//! reference while another spelling of it exists, as the push gate already refuses case
//! aliases of protected branches (`rbit_core::protection`). Elsewhere every spelling is a
//! distinct reference and nothing is refused.
//!
//! The references are read before the write. A Git client that creates an alias between
//! that read and the write is not excluded: Git's reference store has no lock spanning
//! spellings.

use std::path::Path;

use gix::bstr::ByteSlice;

use crate::GitError;

/// Whether this platform's filesystems are treated as case-insensitive.
const CASE_INSENSITIVE: bool = cfg!(any(windows, target_os = "macos"));

/// Another existing reference of `repo` whose name is `full_name` but for case, where
/// filesystems ignore case.
pub(crate) fn case_alias(repo: &gix::Repository, full_name: &str) -> Result<Option<String>, GitError> {
    if !CASE_INSENSITIVE {
        return Ok(None);
    }
    let platform = repo.references().map_err(GitError::backend)?;
    for reference in platform.all().map_err(GitError::backend)? {
        let reference = reference.map_err(GitError::backend)?;
        let name = reference.name().as_bstr().to_str_lossy();
        if name != full_name && same_but_case(&name, full_name) {
            return Ok(Some(name.into_owned()));
        }
    }
    Ok(None)
}

/// [`case_alias`] for the repository at `repo`, on the blocking pool.
pub(crate) async fn case_alias_at(repo: &Path, full_name: &str) -> Result<Option<String>, GitError> {
    if !CASE_INSENSITIVE {
        return Ok(None);
    }
    let (repo, full_name) = (repo.to_owned(), full_name.to_owned());
    tokio::task::spawn_blocking(move || {
        let repo = gix::open_opts(&repo, gix::open::Options::isolated()).map_err(GitError::backend)?;
        case_alias(&repo, &full_name)
    })
    .await?
}

/// Equal but for case, including Unicode case mappings (as branch protection compares).
fn same_but_case(left: &str, right: &str) -> bool {
    let mut right = right.chars();
    left.chars().all(|left| {
        right.next().is_some_and(|right| {
            left == right || left.to_lowercase().eq(right.to_lowercase()) || left.to_uppercase().eq(right.to_uppercase())
        })
    }) && right.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spellings_compare_without_case() {
        assert!(same_but_case("refs/heads/MAIN", "refs/heads/main"));
        assert!(same_but_case("refs/tags/\u{c9}t\u{e9}", "refs/tags/\u{e9}T\u{c9}"));
        assert!(!same_but_case("refs/heads/main", "refs/heads/mainline"));
        assert!(!same_but_case("refs/heads/main", "refs/heads/maim"));
    }
}
