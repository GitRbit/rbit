//! Stable public model behavior, including the distinction between JSON and query values.

use rbit_contracts::{AccountId, NameError, Timestamp};
use rbit_identity_models::{Account, AccountKind};
use rbit_repositories::{ArchivedFilter, OwnedRepository, RepoId, RepoName, RepoSort, Repository, Visibility};

#[test]
fn enum_json_preserves_variant_names_instead_of_query_values() {
    for (visibility, json) in [(Visibility::Public, "\"Public\""), (Visibility::Private, "\"Private\"")] {
        assert_eq!(serde_json::to_string(&visibility).unwrap(), json);
        assert_eq!(serde_json::from_str::<Visibility>(json).unwrap(), visibility);
        assert_eq!(Visibility::parse(visibility.as_str()), Some(visibility));
    }
    for (sort, json) in RepoSort::ALL.into_iter().zip(["\"RecentlyUpdated\"", "\"Name\"", "\"Newest\""]) {
        assert_eq!(serde_json::to_string(&sort).unwrap(), json);
        assert_eq!(serde_json::from_str::<RepoSort>(json).unwrap(), sort);
        assert_eq!(RepoSort::parse(sort.as_str()), Some(sort));
    }
    for (archived, json) in ArchivedFilter::ALL.into_iter().zip(["\"Include\"", "\"Exclude\"", "\"Only\""]) {
        assert_eq!(serde_json::to_string(&archived).unwrap(), json);
        assert_eq!(serde_json::from_str::<ArchivedFilter>(json).unwrap(), archived);
        assert_eq!(ArchivedFilter::parse(archived.query_value().unwrap_or_default()), Some(archived));
    }
    assert!(serde_json::from_str::<Visibility>("\"public\"").is_err());
}

#[test]
fn name_serde_validates_without_trimming_or_case_normalization() {
    let name: RepoName = "My_repo.rs".parse().unwrap();
    assert_eq!(serde_json::to_string(&name).unwrap(), "\"My_repo.rs\"");
    assert_eq!(serde_json::from_str::<RepoName>("\"My_repo.rs\"").unwrap(), name);
    assert_eq!(name.as_ref(), "My_repo.rs");
    assert_eq!(String::from(name.clone()), name.to_string());
    for invalid in ["", " repo ", "../repo", "x..y", "settings", "People", "REPO.GIT", "x.lock", "é"] {
        assert!(invalid.parse::<RepoName>().is_err(), "{invalid}");
        assert!(serde_json::from_str::<RepoName>(&serde_json::to_string(invalid).unwrap()).is_err(), "{invalid}");
    }
    assert_eq!(RepoName::MAX_LEN, 100);
    assert!("a".repeat(100).parse::<RepoName>().is_ok());
    assert!(matches!("a".repeat(101).parse::<RepoName>(), Err(NameError::TooLong { max: 100, .. })));
    // Length errors precede character errors, matching the original validator.
    assert!(matches!("é".repeat(51).parse::<RepoName>(), Err(NameError::TooLong { .. })));
}

#[test]
fn owner_projection_archive_and_full_name_remain_unchanged() {
    let owner = Account {
        id: AccountId(7),
        kind: AccountKind::Organization,
        name: "Forge".parse().unwrap(),
        display_name: Some("Forge display".into()),
        bio: None,
        created_at: Timestamp(1),
    };
    let mut owned = OwnedRepository {
        owner,
        repo: Repository {
            id: RepoId(9),
            owner_id: AccountId(7),
            name: "MyRepo".parse().unwrap(),
            description: None,
            visibility: Visibility::Private,
            default_branch: "trunk".into(),
            created_at: Timestamp(2),
            updated_at: Timestamp(3),
            pushed_at: Some(Timestamp(4)),
            archived_at: None,
        },
    };
    assert_eq!(owned.full_name(), "Forge/MyRepo");
    assert!(!owned.repo.is_archived());
    owned.repo.archived_at = Some(Timestamp(0));
    assert!(owned.repo.is_archived());
    assert!(ArchivedFilter::Include.shows(true));
    assert!(!ArchivedFilter::Exclude.shows(true));
    assert!(ArchivedFilter::Only.shows(true));
    assert!(!ArchivedFilter::Only.shows(false));
}
