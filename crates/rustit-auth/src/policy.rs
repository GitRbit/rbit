//! Repository access rules.
//!
//! Kept as a pure function of (viewer, repository, grants) so it is exhaustively testable
//! and shared by the web UI and the Git transports. Looking the grants up is the caller's
//! job; this module never does I/O.
//!
//! Effective access is the **maximum** of every rule that applies:
//!
//! | Actor                    | Personal repo  | Org repo                      |
//! |--------------------------|----------------|-------------------------------|
//! | Anyone (incl. anonymous) | Read if public | Read if public                |
//! | Site admin               | Admin          | Admin                         |
//! | Owner (user)             | Admin          | —                             |
//! | Org owner                | —              | Admin                         |
//! | Org member               | —              | base permission, team grants  |
//! | Collaborator             | its permission | its permission                |
//!
//! Organisation fields are ignored for personal repositories, team grants count only for
//! members of the owning organisation, and nothing lowers access below the public baseline.

use rustit_core::{OrgRole, RepoAccess, User, membership::Permission, repository::OwnedRepository};

/// What the caller looked up about one viewer and one repository.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Grants {
    /// The viewer's role in the owning organisation.
    pub org_role: Option<OrgRole>,
    /// That organisation's base permission (`None` = no permission).
    pub org_base: Option<Permission>,
    /// The best grant among the viewer's teams for this repository.
    pub team: Option<Permission>,
    /// The viewer's collaborator permission.
    pub collaborator: Option<Permission>,
}

/// Computes what `viewer` (or an anonymous visitor) may do with `repo`.
pub fn repo_access(viewer: Option<&User>, repo: &OwnedRepository, grants: &Grants) -> RepoAccess {
    let baseline = if repo.repo.visibility.is_public() { RepoAccess::Read } else { RepoAccess::None };

    let Some(user) = viewer else {
        return baseline;
    };
    if user.is_admin || user.id() == repo.repo.owner_id {
        return RepoAccess::Admin;
    }

    let granted = |permission: Option<Permission>| permission.map_or(RepoAccess::None, Permission::access);
    let organisation = match grants.org_role {
        _ if !repo.owner.is_organization() => RepoAccess::None,
        Some(OrgRole::Owner) => RepoAccess::Admin,
        Some(OrgRole::Member) => granted(grants.org_base).max(granted(grants.team)),
        None => RepoAccess::None,
    };
    baseline.max(organisation).max(granted(grants.collaborator))
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

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Actor {
        Anonymous,
        SiteAdmin,
        /// The account that owns the repository (only a user can sign in as it).
        Owner,
        Other,
    }

    const ACTORS: [Actor; 4] = [Actor::Anonymous, Actor::SiteAdmin, Actor::Owner, Actor::Other];
    const VISIBILITIES: [Visibility; 2] = [Visibility::Public, Visibility::Private];
    const OWNER_KINDS: [AccountKind; 2] = [AccountKind::User, AccountKind::Organization];
    const ROLES: [Option<OrgRole>; 3] = [None, Some(OrgRole::Member), Some(OrgRole::Owner)];
    const PERMISSIONS: [Option<Permission>; 4] = [None, Some(Permission::Read), Some(Permission::Write), Some(Permission::Admin)];

    fn viewer(actor: Actor) -> Option<User> {
        match actor {
            Actor::Anonymous => None,
            Actor::SiteAdmin => Some(user(9, "root", true)),
            Actor::Owner => Some(user(OWNER, "owner", false)),
            Actor::Other => Some(user(2, "eve", false)),
        }
    }

    /// Every combination the policy can be asked about.
    fn every_case() -> impl Iterator<Item = (Actor, Visibility, AccountKind, Grants)> {
        let mut cases = Vec::new();
        for actor in ACTORS {
            for visibility in VISIBILITIES {
                for kind in OWNER_KINDS {
                    for org_role in ROLES {
                        for org_base in PERMISSIONS {
                            for team in PERMISSIONS {
                                for collaborator in PERMISSIONS {
                                    cases.push((actor, visibility, kind, Grants { org_role, org_base, team, collaborator }));
                                }
                            }
                        }
                    }
                }
            }
        }
        cases.into_iter()
    }

    /// The access level as a number: 0 none, 1 read, 2 write, 3 admin.
    fn rank(permission: Option<Permission>) -> u8 {
        match permission {
            None => 0,
            Some(Permission::Read) => 1,
            Some(Permission::Write) => 2,
            Some(Permission::Admin) => 3,
        }
    }

    /// An independent statement of the rules: collect every level a rule grants, take the largest.
    fn reference(actor: Actor, visibility: Visibility, kind: AccountKind, grants: Grants) -> RepoAccess {
        let mut levels = vec![u8::from(visibility == Visibility::Public)];
        let signed_in = actor != Actor::Anonymous;
        if actor == Actor::SiteAdmin {
            levels.push(3);
        }
        // An organisation cannot sign in, so "owner" only means something for personal repositories.
        if actor == Actor::Owner && kind == AccountKind::User {
            levels.push(3);
        }
        if signed_in && kind == AccountKind::Organization {
            match grants.org_role {
                Some(OrgRole::Owner) => levels.push(3),
                Some(OrgRole::Member) => levels.extend([rank(grants.org_base), rank(grants.team)]),
                None => {}
            }
        }
        if signed_in {
            levels.push(rank(grants.collaborator));
        }
        match levels.into_iter().max().unwrap_or(0) {
            0 => RepoAccess::None,
            1 => RepoAccess::Read,
            2 => RepoAccess::Write,
            _ => RepoAccess::Admin,
        }
    }

    #[test]
    fn agrees_with_the_reference_table_for_every_combination() {
        let mut checked = 0;
        for (actor, visibility, kind, grants) in every_case() {
            let repo = repo(kind, visibility);
            let viewer = viewer(actor);
            // The owner of an organisation repository is the organisation, which never signs in;
            // a user whose id matches is the personal owner only when the owner is a user.
            if actor == Actor::Owner && kind == AccountKind::Organization {
                continue;
            }
            let expected = reference(actor, visibility, kind, grants);
            assert_eq!(repo_access(viewer.as_ref(), &repo, &grants), expected, "{actor:?} {visibility:?} {kind:?} {grants:?}");
            checked += 1;
        }
        assert_eq!(checked, 4 * 2 * 2 * 3 * 4 * 4 * 4 - 2 * 3 * 4 * 4 * 4);
    }

    #[test]
    fn nothing_lowers_access_below_the_public_baseline() {
        for (actor, visibility, kind, grants) in every_case() {
            let access = repo_access(viewer(actor).as_ref(), &repo(kind, visibility), &grants);
            if visibility.is_public() {
                assert!(access.can_read(), "{actor:?} {kind:?} {grants:?}");
            }
        }
    }

    #[test]
    fn anonymous_visitors_ignore_every_grant() {
        for (_, visibility, kind, grants) in every_case().filter(|(actor, ..)| *actor == Actor::Anonymous) {
            let expected = if visibility.is_public() { RepoAccess::Read } else { RepoAccess::None };
            assert_eq!(repo_access(None, &repo(kind, visibility), &grants), expected);
        }
    }

    #[test]
    fn organisation_grants_never_apply_to_personal_repositories() {
        let eve = user(2, "eve", false);
        for org_role in ROLES {
            for permission in PERMISSIONS {
                let grants = Grants { org_role, org_base: permission, team: permission, collaborator: None };
                assert_eq!(repo_access(Some(&eve), &repo(AccountKind::User, Visibility::Private), &grants), RepoAccess::None);
            }
        }
    }

    #[test]
    fn team_grants_need_organisation_membership() {
        let eve = user(2, "eve", false);
        let org = repo(AccountKind::Organization, Visibility::Private);
        let stale = Grants { team: Some(Permission::Admin), ..Grants::default() };
        assert_eq!(repo_access(Some(&eve), &org, &stale), RepoAccess::None);
        let member = Grants { org_role: Some(OrgRole::Member), ..stale };
        assert_eq!(repo_access(Some(&eve), &org, &member), RepoAccess::Admin);
    }

    #[test]
    fn base_permission_and_collaborators_combine_by_maximum() {
        let eve = user(2, "eve", false);
        let org = repo(AccountKind::Organization, Visibility::Private);
        let member = |org_base, collaborator| Grants { org_role: Some(OrgRole::Member), org_base, team: None, collaborator };
        assert_eq!(repo_access(Some(&eve), &org, &member(None, None)), RepoAccess::None);
        assert_eq!(repo_access(Some(&eve), &org, &member(Some(Permission::Read), None)), RepoAccess::Read);
        assert_eq!(repo_access(Some(&eve), &org, &member(Some(Permission::Write), Some(Permission::Read))), RepoAccess::Write);
        assert_eq!(repo_access(Some(&eve), &org, &member(Some(Permission::Read), Some(Permission::Admin))), RepoAccess::Admin);
        let outside = Grants { collaborator: Some(Permission::Write), ..Grants::default() };
        assert_eq!(repo_access(Some(&eve), &org, &outside), RepoAccess::Write, "outside collaborators");
    }
}
