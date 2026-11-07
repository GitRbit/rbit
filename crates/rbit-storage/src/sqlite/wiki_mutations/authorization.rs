//! Fresh repository grants/archive and catalog incarnation in the writer snapshot.
use super::WikiMutationError;
use crate::sqlite::{pool::WriteTransaction, repositories::readable_by};
use rbit_access::{OrganizationAccess, RepositoryAccessFacts, ViewerAccessFacts, repo_access};
use rbit_core::{
    AccountId, AccountKind, OrgRole, RepoAccess, RepoId, Visibility,
    membership::{BasePermission, Permission},
};
use rbit_wikis::{
    WikiGeneration, WikiId, WikiStoreId,
    commands::WikiActorIncarnation,
    policy::{WikiAccess, WikiHead, WikiPolicyError},
};

#[derive(sqlx::FromRow)]
pub(super) struct WriteAccessRow {
    wiki_id: i64,
    actor_incarnation: Option<i64>,
    identity: Vec<u8>,
    generation: Option<i64>,
    revision: Option<String>,
    owner_id: i64,
    owner_kind: String,
    visibility: String,
    site_admin: bool,
    role: Option<String>,
    base: Option<String>,
    team: Option<i64>,
    collaborator: Option<String>,
    archived: bool,
}

#[derive(Debug)]
pub(super) struct AuthorizedWiki {
    pub access: WikiAccess,
    pub wiki: WikiId,
    pub store: WikiStoreId,
    pub head: Option<WikiHead>,
    pub actor_incarnation: Option<WikiActorIncarnation>,
}
impl WriteAccessRow {
    pub fn decode(self, repository: RepoId, actor: AccountId) -> Result<AuthorizedWiki, WikiMutationError> {
        let store = WikiStoreId::try_from(<[u8; 16]>::try_from(self.identity).map_err(|_| WikiMutationError::Corrupt)?)
            .map_err(|_| WikiMutationError::Corrupt)?;
        let wiki = WikiId::new(self.wiki_id).ok_or(WikiMutationError::Corrupt)?;
        let role = self.role.as_deref().map(|value| OrgRole::parse(value).ok_or(WikiMutationError::Corrupt)).transpose()?;
        let base = self
            .base
            .as_deref()
            .map(|value| BasePermission::parse(value).ok_or(WikiMutationError::Corrupt))
            .transpose()?
            .unwrap_or_default();
        let collaborator =
            self.collaborator.as_deref().map(|value| Permission::parse(value).ok_or(WikiMutationError::Corrupt)).transpose()?;
        let granted = |permission: Option<Permission>| permission.map_or(RepoAccess::None, Permission::access);
        let team = self.team.map(|rank| match rank {
            3 => Permission::Admin,
            2 => Permission::Write,
            _ => Permission::Read,
        });
        let access = repo_access(
            Some(ViewerAccessFacts {
                site_admin: self.site_admin,
                repository_owner: actor.0 == self.owner_id,
                organization: match role {
                    Some(OrgRole::Owner) => OrganizationAccess::Owner,
                    Some(OrgRole::Member) => OrganizationAccess::Member { base: granted(base.permission()), team: granted(team) },
                    None => OrganizationAccess::None,
                },
                collaborator: granted(collaborator),
            }),
            RepositoryAccessFacts {
                public: Visibility::parse(&self.visibility).ok_or(WikiMutationError::Corrupt)?.is_public(),
                organization: AccountKind::parse(&self.owner_kind).ok_or(WikiMutationError::Corrupt)? == AccountKind::Organization,
            },
        );
        let head = match (self.generation, self.revision) {
            (None, None) => None,
            (Some(generation), Some(revision)) => Some(WikiHead {
                generation: WikiGeneration::new(generation).ok_or(WikiMutationError::Corrupt)?,
                revision: revision.parse().map_err(|_| WikiMutationError::Corrupt)?,
            }),
            _ => return Err(WikiMutationError::Corrupt),
        };
        let actor_incarnation =
            self.actor_incarnation.map(|id| WikiActorIncarnation::new(id).ok_or(WikiMutationError::Corrupt)).transpose()?;
        Ok(AuthorizedWiki {
            access: WikiAccess { repository, actor: Some(actor), access, enabled: true, archived: self.archived },
            wiki,
            store,
            head,
            actor_incarnation,
        })
    }
}

const CURRENT: &str = concat!(
    "SELECT w.id AS wiki_id,w.generation,v.revision, (SELECT identity FROM wiki_store_identity WHERE singleton=1) AS identity, ",
    "(SELECT id FROM wiki_actor_incarnations WHERE account_id=?2) AS actor_incarnation, ",
    "EXISTS(SELECT 1 FROM repo_archives WHERE repo_id=r.id) AS archived, ",
    "r.owner_id,(SELECT kind FROM accounts WHERE id=r.owner_id) AS owner_kind,r.visibility,u.is_admin AS site_admin, ",
    "(SELECT role FROM org_members WHERE org_id=r.owner_id AND user_id=?2) AS role, ",
    "(SELECT member_permission FROM org_profiles WHERE account_id=r.owner_id) AS base, ",
    "(SELECT MAX(CASE tr.permission WHEN 'admin' THEN 3 WHEN 'write' THEN 2 ELSE 1 END) ",
    "FROM team_repositories tr JOIN team_members tm ON tm.team_id=tr.team_id JOIN teams t ON t.id=tr.team_id ",
    "WHERE tr.repo_id=r.id AND t.org_id=r.owner_id AND tm.user_id=?2) AS team, ",
    "(SELECT permission FROM collaborators WHERE repo_id=r.id AND user_id=?2) AS collaborator ",
    "FROM repositories r JOIN wiki_repositories w ON w.repo_id=r.id JOIN user_logins u ON u.account_id=?2 AND u.suspended_at IS NULL ",
    "LEFT JOIN wiki_revisions v ON v.repo_id=r.id AND v.generation=w.generation ",
    "WHERE r.id=?1 AND w.enabled=1 AND ",
    readable_by!("?2", "u.is_admin")
);

pub(super) async fn load(tx: &mut WriteTransaction, repository: RepoId, actor: AccountId) -> Result<WriteAccessRow, WikiMutationError> {
    sqlx::query_as(CURRENT)
        .bind(repository.0)
        .bind(actor.0)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(WikiMutationError::Policy(WikiPolicyError::NotFound))
}
