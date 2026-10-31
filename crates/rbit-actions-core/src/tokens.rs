//! Workflow trust and token permission decisions independent of host account types.

use serde::{Deserialize, Serialize};

/// How a pull request run may run (§11.1 of the design).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunTrust {
    /// Secrets, variables, and the repository's token permission.
    Trusted,
    /// A read-only token, no secrets and no variables — whoever approves it.
    Untrusted {
        /// The run waits for a writer's approval before any job is dispatched.
        awaits_approval: bool,
    },
}

impl RunTrust {
    pub const fn is_trusted(self) -> bool {
        matches!(self, Self::Trusted)
    }
}

/// One coarse permission of a task token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionLevel {
    #[default]
    None,
    Read,
    Write,
}

impl PermissionLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Read => "read",
            Self::Write => "write",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::None, Self::Read, Self::Write].into_iter().find(|level| level.as_str() == value)
    }
}

/// The coarse permissions a workflow declares or a token carries: `contents` (Git, releases),
/// `issues` and `pull-requests` (comments). Every other GitHub permission is ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct TokenPermissions {
    pub contents: PermissionLevel,
    pub issues: PermissionLevel,
    pub pull_requests: PermissionLevel,
}

impl TokenPermissions {
    /// Every permission at `level` (`read-all`, `write-all`, and `{}` for `None`).
    pub const fn uniform(level: PermissionLevel) -> Self {
        Self { contents: level, issues: level, pull_requests: level }
    }

    /// Each permission at most `limit`'s.
    #[must_use]
    pub fn at_most(self, limit: Self) -> Self {
        Self {
            contents: self.contents.min(limit.contents),
            issues: self.issues.min(limit.issues),
            pull_requests: self.pull_requests.min(limit.pull_requests),
        }
    }
}

/// A repository's default workflow token permission (its Actions settings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultTokenPermission {
    #[default]
    Read,
    Write,
}

impl DefaultTokenPermission {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::Read, Self::Write].into_iter().find(|level| level.as_str() == value)
    }

    pub const fn permissions(self) -> TokenPermissions {
        TokenPermissions::uniform(match self {
            Self::Read => PermissionLevel::Read,
            Self::Write => PermissionLevel::Write,
        })
    }
}

/// What a task token may do (§11.4), as on GitHub: the job's `permissions:`, else the
/// workflow's, else the repository default; a declaration replaces the default, above it
/// or below. Untrusted runs read at most.
pub fn token_permissions(
    default: DefaultTokenPermission,
    workflow: Option<TokenPermissions>,
    job: Option<TokenPermissions>,
    trust: RunTrust,
) -> TokenPermissions {
    let mut granted = job.or(workflow).unwrap_or_else(|| default.permissions());
    if !trust.is_trusted() {
        granted = granted.at_most(TokenPermissions::uniform(PermissionLevel::Read));
    }
    granted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_levels() {
        for level in [PermissionLevel::None, PermissionLevel::Read, PermissionLevel::Write] {
            assert_eq!(PermissionLevel::parse(level.as_str()), Some(level));
        }
        assert!(PermissionLevel::None < PermissionLevel::Read && PermissionLevel::Read < PermissionLevel::Write);
        assert_eq!(PermissionLevel::parse("admin"), None);
        for default in [DefaultTokenPermission::Read, DefaultTokenPermission::Write] {
            assert_eq!(DefaultTokenPermission::parse(default.as_str()), Some(default));
        }
        assert_eq!(DefaultTokenPermission::default(), DefaultTokenPermission::Read);
        assert_eq!(DefaultTokenPermission::parse("none"), None);
    }

    #[test]
    fn declarations_replace_the_default_and_untrusted_runs_read() {
        use PermissionLevel::{None as N, Read as R, Write as W};
        let p = |contents, issues, pull_requests| TokenPermissions { contents, issues, pull_requests };
        let all = [N, R, W];
        let mut declared = vec![None];
        for contents in all {
            for issues in all {
                for pull_requests in all {
                    declared.push(Some(p(contents, issues, pull_requests)));
                }
            }
        }
        let trusts = [RunTrust::Trusted, RunTrust::Untrusted { awaits_approval: true }, RunTrust::Untrusted { awaits_approval: false }];
        for default in [DefaultTokenPermission::Read, DefaultTokenPermission::Write] {
            for &workflow in &declared {
                for &job in &declared {
                    for trust in trusts {
                        let granted = token_permissions(default, workflow, job, trust);
                        let ceiling = if trust.is_trusted() { TokenPermissions::uniform(W) } else { TokenPermissions::uniform(R) };
                        assert_eq!(granted.at_most(ceiling), granted, "read at most for untrusted runs");
                        let effective = job.or(workflow).unwrap_or_else(|| default.permissions());
                        assert_eq!(granted, effective.at_most(ceiling), "{default:?} {workflow:?} {job:?} {trust:?}");
                    }
                }
            }
        }
        // Spot checks of the common shapes.
        let trusted = RunTrust::Trusted;
        assert_eq!(token_permissions(DefaultTokenPermission::Read, None, None, trusted), TokenPermissions::uniform(R));
        assert_eq!(token_permissions(DefaultTokenPermission::Write, None, None, trusted), TokenPermissions::uniform(W));
        assert_eq!(
            token_permissions(DefaultTokenPermission::Read, Some(TokenPermissions::uniform(W)), None, trusted),
            TokenPermissions::uniform(W),
            "write-all raises a read default, as on GitHub"
        );
        assert_eq!(token_permissions(DefaultTokenPermission::Read, None, Some(p(W, N, N)), trusted), p(W, N, N));
        assert_eq!(
            token_permissions(
                DefaultTokenPermission::Read,
                Some(TokenPermissions::uniform(W)),
                None,
                RunTrust::Untrusted { awaits_approval: false }
            ),
            TokenPermissions::uniform(R),
            "but never for an untrusted run"
        );
        assert_eq!(
            token_permissions(DefaultTokenPermission::Write, Some(p(R, W, N)), Some(TokenPermissions::uniform(N)), trusted),
            TokenPermissions::uniform(N),
            "the job's declaration replaces the workflow's"
        );
        assert_eq!(token_permissions(DefaultTokenPermission::Write, Some(p(W, N, R)), None, trusted), p(W, N, R));
        assert_eq!(
            token_permissions(DefaultTokenPermission::Write, None, None, RunTrust::Untrusted { awaits_approval: false }),
            TokenPermissions::uniform(R)
        );
    }
}
