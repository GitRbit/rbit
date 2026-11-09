//! Editing an open pull request: who may, draft state and moving it to another base.

use std::future::Future;

use super::branch_ref;

/// The author or a writer may change a pull request's draft state or base.
pub const fn may_edit_pull(is_author: bool, writer: bool) -> bool {
    is_author || writer
}

/// The refusal for editing a pull request that is no longer open.
pub const CLOSED: &str = "the pull request is closed";

/// A pull request as a base change sees it.
#[derive(Debug, Clone, Copy)]
pub struct BaseChangeFacts<'a> {
    pub is_open: bool,
    pub is_merged: bool,
    pub cross_repository: bool,
    pub head_branch: &'a str,
    pub base_branch: &'a str,
}

/// What a base change does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BaseChange {
    /// The pull request already targets that branch.
    Unchanged,
    /// Move to `branch`, dismissing approvals with `message`.
    To { branch: String, message: String },
}

/// Plans moving a pull request to the trimmed `branch`: refused when it is closed or
/// merged, the name is implausible, `exists` cannot read the branch, or (in one
/// repository) it is the head branch.
pub async fn plan_base_change<Fut: Future<Output = bool>>(
    facts: BaseChangeFacts<'_>,
    branch: &str,
    exists: impl FnOnce(String) -> Fut,
) -> Result<BaseChange, &'static str> {
    let branch = branch.trim();
    if !facts.is_open || facts.is_merged {
        return Err(CLOSED);
    }
    let full = branch_ref(branch).ok_or("choose an existing branch")?;
    if !exists(full).await {
        return Err("the base branch does not exist");
    }
    if !facts.cross_repository && branch == facts.head_branch {
        return Err("the base and the head branch must differ");
    }
    if branch == facts.base_branch {
        return Ok(BaseChange::Unchanged);
    }
    let message = format!("The base branch changed from {} to {branch}.", facts.base_branch);
    Ok(BaseChange::To { branch: branch.to_owned(), message })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPEN: BaseChangeFacts<'static> =
        BaseChangeFacts { is_open: true, is_merged: false, cross_repository: false, head_branch: "fix", base_branch: "main" };

    #[tokio::test]
    async fn base_changes_keep_the_legacy_order() {
        let yes = |_| async { true };
        let closed = BaseChangeFacts { is_open: false, ..OPEN };
        assert_eq!(plan_base_change(closed, "a..b", yes).await, Err("the pull request is closed"));
        assert_eq!(plan_base_change(OPEN, "a..b", yes).await, Err("choose an existing branch"));
        assert_eq!(plan_base_change(OPEN, "dev", |_| async { false }).await, Err("the base branch does not exist"));
        assert_eq!(plan_base_change(OPEN, "fix", yes).await, Err("the base and the head branch must differ"));
        let fork = BaseChangeFacts { cross_repository: true, ..OPEN };
        assert!(matches!(plan_base_change(fork, "fix", yes).await, Ok(BaseChange::To { .. })), "a fork's head may share the name");
        assert_eq!(plan_base_change(OPEN, " main ", yes).await, Ok(BaseChange::Unchanged));
        assert_eq!(
            plan_base_change(OPEN, " dev ", yes).await,
            Ok(BaseChange::To { branch: "dev".into(), message: "The base branch changed from main to dev.".into() })
        );
        assert!(may_edit_pull(true, false) && may_edit_pull(false, true) && !may_edit_pull(false, false));
    }
}
