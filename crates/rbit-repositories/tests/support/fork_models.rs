use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_identity_models::{Account, AccountKind, User};
use rbit_repositories::{OwnedRepository, Repository, Visibility};
pub fn user() -> User {
    User {
        account: Account {
            id: AccountId(1),
            kind: AccountKind::User,
            name: "alice".parse().unwrap(),
            display_name: None,
            bio: None,
            created_at: Timestamp(1),
        },
        email: "alice@example.com".parse().unwrap(),
        is_admin: false,
    }
}
pub fn repository(id: i64) -> OwnedRepository {
    OwnedRepository {
        owner: user().account,
        repo: Repository {
            id: RepoId(id),
            owner_id: AccountId(1),
            name: "source".parse().unwrap(),
            description: Some("description".into()),
            visibility: Visibility::Private,
            default_branch: "trunk".into(),
            created_at: Timestamp(1),
            updated_at: Timestamp(1),
            pushed_at: Some(Timestamp(2)),
            archived_at: Some(Timestamp(3)),
        },
    }
}
