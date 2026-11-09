use crate::{OrganizationAccess, RepoAccess, RepositoryAccessFacts, ViewerAccessFacts, repo_access};

const LEVELS: [RepoAccess; 4] = [RepoAccess::None, RepoAccess::Read, RepoAccess::Write, RepoAccess::Admin];

fn rank(access: RepoAccess) -> usize {
    match access {
        RepoAccess::None => 0,
        RepoAccess::Read => 1,
        RepoAccess::Write => 2,
        RepoAccess::Admin => 3,
    }
}

#[test]
fn lattice_denies_by_default_and_orders_operation_thresholds() {
    assert_eq!(RepoAccess::default(), RepoAccess::None);
    for (index, access) in LEVELS.into_iter().enumerate() {
        assert_eq!(access.can_read(), index >= 1);
        assert_eq!(access.can_write(), index >= 2);
        assert_eq!(access.can_admin(), index >= 3);
        for (other_index, other) in LEVELS.into_iter().enumerate() {
            assert_eq!(access.cmp(&other), index.cmp(&other_index));
        }
    }
}

#[test]
fn resolved_policy_matches_independent_grant_table_exhaustively() {
    let mut cases = 0;
    for public in [false, true] {
        for organization in [false, true] {
            let repo = RepositoryAccessFacts { public, organization };
            assert_eq!(repo_access(None, repo), if public { RepoAccess::Read } else { RepoAccess::None });
            for base in LEVELS {
                for team in LEVELS {
                    for collaborator in LEVELS {
                        for membership in 0..3 {
                            for actor in 0..4 {
                                let viewer = ViewerAccessFacts {
                                    site_admin: actor == 1 || actor == 3,
                                    repository_owner: actor == 2 || actor == 3,
                                    organization: match membership {
                                        0 => OrganizationAccess::None,
                                        1 => OrganizationAccess::Member { base, team },
                                        _ => OrganizationAccess::Owner,
                                    },
                                    collaborator,
                                };
                                // Enumerate independent rule grants numerically. This does not
                                // use the policy lattice's ordering or combination operations.
                                let grants = [
                                    usize::from(public),
                                    if actor != 0 { 3 } else { 0 },
                                    if organization && membership == 2 { 3 } else { 0 },
                                    if organization && membership == 1 { rank(base) } else { 0 },
                                    if organization && membership == 1 { rank(team) } else { 0 },
                                    rank(collaborator),
                                ];
                                let expected = LEVELS[grants.into_iter().max().unwrap()];
                                assert_eq!(repo_access(Some(viewer), repo), expected, "{repo:?} {viewer:?}");
                                cases += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 2 * 2 * 4 * 4 * 4 * 3 * 4);
}

#[test]
fn absent_membership_and_personal_repositories_ignore_organization_grants() {
    let private = RepositoryAccessFacts { public: false, organization: true };
    let outside = ViewerAccessFacts::default();
    assert_eq!(repo_access(Some(outside), private), RepoAccess::None);
    // Outside collaborators do not require membership in the owning organisation.
    let outside = ViewerAccessFacts { collaborator: RepoAccess::Write, ..outside };
    assert_eq!(repo_access(Some(outside), private), RepoAccess::Write);
    // Even a fully trusted org-owner fact has no effect on a personal repository.
    let personal = RepositoryAccessFacts { organization: false, ..private };
    let org_owner = ViewerAccessFacts { organization: OrganizationAccess::Owner, ..ViewerAccessFacts::default() };
    assert_eq!(repo_access(Some(org_owner), personal), RepoAccess::None);
}

#[test]
fn stronger_collaborator_and_member_grants_never_reduce_effective_access() {
    for public in [false, true] {
        for organization in [false, true] {
            let repo = RepositoryAccessFacts { public, organization };
            for base in LEVELS {
                for team in LEVELS {
                    for collaborator in LEVELS {
                        let viewer = ViewerAccessFacts {
                            organization: OrganizationAccess::Member { base, team },
                            collaborator,
                            ..ViewerAccessFacts::default()
                        };
                        let original = repo_access(Some(viewer), repo);
                        for stronger in LEVELS.into_iter().filter(|level| *level >= collaborator) {
                            assert!(repo_access(Some(ViewerAccessFacts { collaborator: stronger, ..viewer }), repo) >= original);
                        }
                        for stronger in LEVELS.into_iter().filter(|level| *level >= base) {
                            let organization = OrganizationAccess::Member { base: stronger, team };
                            assert!(repo_access(Some(ViewerAccessFacts { organization, ..viewer }), repo) >= original);
                        }
                        for stronger in LEVELS.into_iter().filter(|level| *level >= team) {
                            let organization = OrganizationAccess::Member { base, team: stronger };
                            assert!(repo_access(Some(ViewerAccessFacts { organization, ..viewer }), repo) >= original);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn stored_grants_count_only_for_viewers_they_can_change() {
    use crate::grants::{GrantRepository, Repository, RepositoryGrants, Viewer, access, viewer_grants};
    use crate::{OrgRole, organizations::tests::ready};

    /// Every viewer is a member with a write team grant; loads are counted.
    struct Store(std::sync::atomic::AtomicUsize);
    impl GrantRepository for Store {
        type Error = ();
        type Id = u32;
        type Repo = ();
        type Permission = RepoAccess;
        fn grants(&self, (): (), _: u32, _: u32) -> impl std::future::Future<Output = Result<RepositoryGrants<RepoAccess>, ()>> + Send {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            std::future::ready(Ok(RepositoryGrants {
                org_role: Some(OrgRole::Member),
                team: Some(RepoAccess::Write),
                ..RepositoryGrants::default()
            }))
        }
    }
    let store = Store(std::sync::atomic::AtomicUsize::new(0));
    let viewer = |id, site_admin| Some(Viewer { id, site_admin });
    for skipped in [None, viewer(1, false), viewer(2, true)] {
        assert_eq!(ready(viewer_grants(&store, skipped, (), 1)).unwrap(), RepositoryGrants::default());
    }
    assert_eq!(store.0.load(std::sync::atomic::Ordering::Relaxed), 0, "owner, admins and anonymous visitors load nothing");
    let member = ready(viewer_grants(&store, viewer(3, false), (), 1)).unwrap();
    let org = Repository { owner: 1, public: false, organization: true };
    assert_eq!(access(viewer(3, false).as_ref(), &org, &member), RepoAccess::Write);
    let personal = Repository { organization: false, ..org };
    assert_eq!(access(viewer(3, false).as_ref(), &personal, &member), RepoAccess::None, "organisation grants need an organisation");
    assert_eq!(access(viewer(1, false).as_ref(), &personal, &RepositoryGrants::<RepoAccess>::default()), RepoAccess::Admin);
    assert_eq!(access::<u32, RepoAccess>(None, &Repository { public: true, ..org }, &member), RepoAccess::Read);
}
