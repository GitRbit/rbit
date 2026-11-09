//! Whole-revision summaries for a repository's front page: how many commits a revision
//! has, and every file path in it (for "Go to file").

use gix::bstr::ByteSlice;

use crate::{GitError, RepositoryReader, reader::resolve_commit};

impl RepositoryReader {
    /// The number of commits reachable from `revision`, counting at most `limit`; the
    /// flag is set when the walk stopped there.
    pub async fn commit_count(&self, revision: String, limit: u64) -> Result<(u64, bool), GitError> {
        self.with_repo(move |repo| {
            let tip = resolve_commit(repo, &revision)?;
            let mut count = 0;
            for info in repo.rev_walk([tip]).all().map_err(GitError::backend)? {
                info.map_err(GitError::backend)?;
                if count == limit {
                    return Ok((count, true));
                }
                count += 1;
            }
            Ok((count, false))
        })
        .await
    }

    /// Paths of every file (blob, symlink or submodule) in `revision`'s tree, sorted, at
    /// most `limit`; the flag is set when the listing stopped there.
    pub async fn file_paths(&self, revision: String, limit: usize) -> Result<(Vec<String>, bool), GitError> {
        self.with_repo(move |repo| {
            let commit = repo.find_commit(resolve_commit(repo, &revision)?).map_err(GitError::backend)?;
            let mut pending = vec![(String::new(), commit.tree_id().map_err(GitError::backend)?.detach())];
            let mut paths = Vec::new();
            let mut truncated = false;
            'walk: while let Some((prefix, id)) = pending.pop() {
                let tree = repo.find_tree(id).map_err(GitError::backend)?;
                let decoded = tree.decode().map_err(GitError::backend)?;
                for entry in &decoded.entries {
                    let path = format!("{prefix}{}", entry.filename.to_str_lossy());
                    if entry.mode.is_tree() {
                        pending.push((format!("{path}/"), entry.oid.to_owned()));
                    } else if paths.len() == limit {
                        truncated = true;
                        break 'walk;
                    } else {
                        paths.push(path);
                    }
                }
            }
            paths.sort_unstable();
            Ok((paths, truncated))
        })
        .await
    }
}
