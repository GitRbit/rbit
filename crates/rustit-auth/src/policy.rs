//! Repository access rules.
//!
//! Kept as a pure function of (viewer, repository, membership) so it is exhaustively
//! testable and shared by the web UI and the Git transport. Looking the membership up is
//! the caller's job; this module never does I/O.
//!
//! | Actor        | Personal repo | Org repo |
//! |--------------|---------------|----------|
//! | Site admin   | Admin         | Admin    |
//! | Owner (user) | Admin         | —        |
//! | Org owner    | —             | Admin    |
//! | Org member   | —             | Write    |
//! | Anyone else  | Read if public| Read if public |

use rustit_core::{OrgRole, RepoAccess, User, repository::OwnedRepository};

/// Computes what `viewer` (or an anonymous visitor) may do with `repo`.
///
/// `viewer_org_role` is the viewer's role in the organisation that owns `repo`, if the
/// owner is an organisation and the viewer belongs to it. It is ignored for repositories
/// owned by a user, so a stale or mistaken role can never widen access to personal repos.
pub fn repo_access(viewer: Option<&User>, repo: &OwnedRepository, viewer_org_role: Option<OrgRole>) -> RepoAccess {
    let baseline = if repo.repo.visibility.is_public() { RepoAccess::Read } else { RepoAccess::None };

    let Some(user) = viewer else {
        return baseline;
    };

    if user.is_admin || user.id() == repo.repo.owner_id {
        return RepoAccess::Admin;
    }

    let membership = if repo.owner.is_organization() { viewer_org_role } else { None };
    match membership {
        Some(OrgRole::Owner) => RepoAccess::Admin,
        Some(OrgRole::Member) => RepoAccess::Write,
        None => baseline,
    }
}

#[cfg(test)]
mod tests {
    use rustit_core::{Account, AccountId, AccountKind, RepoId, Repository, Timestamp, Visibility};

    use super::*;

    const OWNER: i64 = 1;

    fn account(id: i64, name: &str, kind: AccountKind) -> Account {
        Account { id: AccountId(id), kind, name: name.parse().unwrap(), display_name: None, bio: None, created_at: Timestamp(0) }
    }

    fn user(id: i64, name: &str, is_admin: bool) -> User {
        User { account: account(id, name, AccountKind::User), email: format!("{name}@example.com").parse().unwrap(), is_admin }
    }

    fn repo(owner_kind: AccountKind, visibility: Visibility) -> OwnedRepository {
        OwnedRepository {
            owner: account(OWNER, "owner", owner_kind),
            repo: Repository {
                id: RepoId(1),
                owner_id: AccountId(OWNER),
                name: "project".parse().unwrap(),
                description: None,
                visibility,
                default_branch: "main".into(),
                created_at: Timestamp(0),
                updated_at: Timestamp(0),
                pushed_at: None,
            },
        }
    }

    fn personal(visibility: Visibility) -> OwnedRepository {
        repo(AccountKind::User, visibility)
    }

    fn org(visibility: Visibility) -> OwnedRepository {
        repo(AccountKind::Organization, visibility)
    }

    const VISIBILITIES: [Visibility; 2] = [Visibility::Public, Visibility::Private];
    const ROLES: [Option<OrgRole>; 3] = [None, Some(OrgRole::Member), Some(OrgRole::Owner)];

    #[test]
    fn anonymous_reads_only_public() {
        for role in ROLES {
            assert_eq!(repo_access(None, &personal(Visibility::Public), role), RepoAccess::Read);
            assert_eq!(repo_access(None, &personal(Visibility::Private), role), RepoAccess::None);
            assert_eq!(repo_access(None, &org(Visibility::Public), role), RepoAccess::Read);
            assert_eq!(repo_access(None, &org(Visibility::Private), role), RepoAccess::None);
        }
    }

    #[test]
    fn personal_owner_administers() {
        for visibility in VISIBILITIES {
            assert_eq!(repo_access(Some(&user(OWNER, "owner", false)), &personal(visibility), None), RepoAccess::Admin);
        }
    }

    #[test]
    fn site_admin_administers_everything() {
        let root = user(9, "root", true);
        for visibility in VISIBILITIES {
            for role in ROLES {
                assert_eq!(repo_access(Some(&root), &personal(visibility), role), RepoAccess::Admin);
                assert_eq!(repo_access(Some(&root), &org(visibility), role), RepoAccess::Admin);
            }
        }
    }

    #[test]
    fn org_owner_administers_and_member_writes() {
        let alice = user(2, "alice", false);
        for visibility in VISIBILITIES {
            assert_eq!(repo_access(Some(&alice), &org(visibility), Some(OrgRole::Owner)), RepoAccess::Admin);
            assert_eq!(repo_access(Some(&alice), &org(visibility), Some(OrgRole::Member)), RepoAccess::Write);
        }
    }

    #[test]
    fn non_members_get_the_public_baseline() {
        let eve = user(2, "eve", false);
        assert_eq!(repo_access(Some(&eve), &org(Visibility::Public), None), RepoAccess::Read);
        assert_eq!(repo_access(Some(&eve), &org(Visibility::Private), None), RepoAccess::None);
        assert_eq!(repo_access(Some(&eve), &personal(Visibility::Public), None), RepoAccess::Read);
        assert_eq!(repo_access(Some(&eve), &personal(Visibility::Private), None), RepoAccess::None);
    }

    #[test]
    fn org_roles_never_apply_to_personal_repositories() {
        let eve = user(2, "eve", false);
        for role in ROLES {
            assert_eq!(repo_access(Some(&eve), &personal(Visibility::Private), role), RepoAccess::None);
            assert_eq!(repo_access(Some(&eve), &personal(Visibility::Public), role), RepoAccess::Read);
        }
    }
}
