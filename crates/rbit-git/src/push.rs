//! What a push changed: ref snapshots taken before and after it, and the commits a ref
//! update introduced.
//!
//! The transports run `git receive-pack` without hooks, so Rbit learns about ref updates
//! by comparing a [`RefSnapshot`] taken just before a push with one taken just after it.
//! Two pushes racing on the same repository may each report the other's updates too;
//! consumers (webhooks, caches) treat updates as facts about the repository, not about
//! one client.

use std::collections::BTreeMap;

use gix::{ObjectId, bstr::ByteSlice, revision::walk::Sorting, traverse::commit::simple::CommitTimeOrder};

use crate::{CommitSummary, GitError, RepositoryReader, reader::summarize};

/// Every ref of a repository: full name (`refs/heads/main`) → the object id it points
/// at directly (annotated tags are not peeled). Symbolic refs such as `HEAD` are left out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefSnapshot {
    targets: BTreeMap<String, String>,
}

/// Defined in `rbit-core` so the push gate in `rbit-transport` can describe the
/// updates a client requests with the same type.
pub use rbit_core::ref_update::RefUpdate;

impl RefSnapshot {
    /// A snapshot from `(full name, object id)` pairs.
    pub fn from_targets<N: Into<String>, I: Into<String>>(targets: impl IntoIterator<Item = (N, I)>) -> Self {
        Self { targets: targets.into_iter().map(|(name, id)| (name.into(), id.into())).collect() }
    }

    /// The object id `name` (a full ref name) points at.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.targets.get(name).map(String::as_str)
    }

    /// The object ids of every ref, e.g. to exclude already known history.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.targets.values().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.targets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }

    /// The updates that turn `self` (taken before a push) into `after`, ordered by ref name.
    pub fn updates_to(&self, after: &Self) -> Vec<RefUpdate> {
        let mut updates: Vec<RefUpdate> = after
            .targets
            .iter()
            .filter(|(name, id)| self.targets.get(*name) != Some(*id))
            .map(|(name, id)| RefUpdate { name: name.clone(), before: self.targets.get(name).cloned(), after: Some(id.clone()) })
            .collect();
        updates.extend(self.targets.iter().filter(|(name, _)| !after.targets.contains_key(*name)).map(|(name, id)| RefUpdate {
            name: name.clone(),
            before: Some(id.clone()),
            after: None,
        }));
        updates.sort_by(|a, b| a.name.cmp(&b.name));
        updates
    }
}

impl RepositoryReader {
    /// The current target of every ref (see [`RefSnapshot`]).
    pub async fn ref_snapshot(&self) -> Result<RefSnapshot, GitError> {
        self.with_repo(|repo| {
            let platform = repo.references().map_err(GitError::backend)?;
            let mut targets = BTreeMap::new();
            for reference in platform.all().map_err(GitError::backend)?.flatten() {
                let Some(id) = reference.try_id() else { continue };
                let name = reference.name().as_bstr().to_str_lossy().into_owned();
                if name.starts_with("refs/") {
                    targets.insert(name, id.to_hex().to_string());
                }
            }
            Ok(RefSnapshot { targets })
        })
        .await
    }

    /// Commits reachable from `head` but from none of `excluding`, newest first, at most
    /// `limit`. This is what a ref update from `excluding` to `head` introduced; for a new
    /// branch pass every id of the previous [`RefSnapshot`] so only new work is listed.
    ///
    /// Ids that do not name a commit in `excluding` are ignored (a deleted tag, say).
    pub async fn new_commits(&self, head: String, excluding: Vec<String>, limit: usize) -> Result<Vec<CommitSummary>, GitError> {
        self.with_repo(move |repo| {
            let tip = parse_id(&head)?;
            let hidden: Vec<ObjectId> = excluding
                .iter()
                .filter_map(|id| parse_id(id).ok())
                .filter_map(|id| repo.find_object(id).ok()?.peel_to_commit().ok().map(|commit| commit.id))
                .collect();
            repo.rev_walk([tip])
                .with_hidden(hidden)
                .sorting(Sorting::ByCommitTime(CommitTimeOrder::NewestFirst))
                .all()
                .map_err(GitError::backend)?
                .take(limit)
                .map(|info| {
                    let info = info.map_err(GitError::backend)?;
                    summarize(&info.object().map_err(GitError::backend)?)
                })
                .collect()
        })
        .await
    }
}

fn parse_id(hex: &str) -> Result<ObjectId, GitError> {
    ObjectId::from_hex(hex.as_bytes()).map_err(|_| GitError::NotFound("commit"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update(name: &str, before: Option<&str>, after: Option<&str>) -> RefUpdate {
        RefUpdate { name: name.to_owned(), before: before.map(str::to_owned), after: after.map(str::to_owned) }
    }

    #[test]
    fn updates_list_created_moved_and_deleted_refs_by_name() {
        let before = RefSnapshot::from_targets([("refs/heads/main", "a1"), ("refs/heads/old", "b1"), ("refs/tags/v1", "c1")]);
        let after = RefSnapshot::from_targets([("refs/heads/main", "a2"), ("refs/heads/new", "d1"), ("refs/tags/v1", "c1")]);

        assert_eq!(
            before.updates_to(&after),
            vec![
                update("refs/heads/main", Some("a1"), Some("a2")),
                update("refs/heads/new", None, Some("d1")),
                update("refs/heads/old", Some("b1"), None),
            ]
        );
        assert_eq!(after.updates_to(&after), Vec::new());
    }

    #[test]
    fn update_helpers_classify_refs() {
        let created = update("refs/heads/feature/x", None, Some("a"));
        assert!(created.is_created() && !created.is_deleted());
        assert_eq!(created.branch(), Some("feature/x"));
        assert_eq!(created.tag(), None);

        let deleted = update("refs/tags/v1", Some("a"), None);
        assert!(deleted.is_deleted() && !deleted.is_created());
        assert_eq!(deleted.tag(), Some("v1"));
    }
}
