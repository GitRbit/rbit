//! Merging pull requests (merge commit, squash, rebase) and trial merges for
//! mergeability, on a temporary worktree under `<tmp_dir>/merges`; the result reaches the
//! base repository only through one atomic, lease-protected ref update.
//!
//! Milestone 2 workstream P (pull requests) owns this file.
