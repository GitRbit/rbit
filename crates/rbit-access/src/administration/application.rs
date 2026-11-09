//! Site administration rules: who may administer, self-action guards, deletion
//! confirmations and the refusals moderation gets.

use super::{AdministrationRepository, ListQuery, Moderation, Paged};
use crate::organizations::{AccessError, Actor};

type Outcome<T, P> = Result<T, AccessError<<P as AdministrationRepository>::Error>>;

/// A user deletion the caller must finish: remove these repositories from disk and
/// announce them.
#[derive(Debug)]
pub struct DeletedUser<U, R> {
    pub user: U,
    /// The deleted user's repositories, with their names.
    pub repositories: Vec<(R, String)>,
}

/// Only site administrators administer; everyone else is `Forbidden`.
pub fn require_site_admin<E, I>(actor: &Actor<I>) -> Result<(), AccessError<E>> {
    if actor.site_admin { Ok(()) } else { Err(AccessError::Forbidden) }
}

/// Site administration rules over one request's repository port.
pub struct Administration<'a, P> {
    port: &'a P,
}

impl<P> std::fmt::Debug for Administration<'_, P> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Administration").finish_non_exhaustive()
    }
}

impl<'a, P: AdministrationRepository> Administration<'a, P> {
    pub fn new(port: &'a P) -> Self {
        Self { port }
    }

    pub async fn users(&self, actor: &Actor<P::Id>, query: &ListQuery) -> Outcome<Paged<P::User>, P> {
        require_site_admin::<P::Error, _>(actor)?;
        Ok(Paged::new(self.port.list_users(query.filter()).await?, query))
    }

    pub async fn user(&self, actor: &Actor<P::Id>, id: P::Id) -> Outcome<P::User, P> {
        require_site_admin::<P::Error, _>(actor)?;
        self.port.find_user(id).await?.ok_or(AccessError::NotFound)
    }

    /// Grants or revokes site administrator rights; administrators cannot demote
    /// themselves and the last active administrator cannot be demoted.
    pub async fn set_admin(&self, actor: &Actor<P::Id>, id: P::Id, is_admin: bool) -> Outcome<P::User, P> {
        let target = self.user(actor, id).await?;
        if !is_admin {
            refuse_self::<P>(actor, &target, "remove your own administrator rights")?;
        }
        explain::<P>(self.port.set_admin(id, is_admin).await?, &target, "demoted")?;
        Ok(target)
    }

    /// Suspends a user or lifts it; administrators cannot suspend themselves and the last
    /// active administrator cannot be suspended.
    pub async fn set_suspended(&self, actor: &Actor<P::Id>, id: P::Id, suspended: bool) -> Outcome<P::User, P> {
        let target = self.user(actor, id).await?;
        if suspended {
            refuse_self::<P>(actor, &target, "suspend yourself")?;
        }
        explain::<P>(self.port.set_suspended(id, suspended).await?, &target, "suspended")?;
        Ok(target)
    }

    /// Deletes a user; `confirmation` must repeat the username exactly, and no
    /// organisation may be left without an owner.
    pub async fn delete_user(&self, actor: &Actor<P::Id>, id: P::Id, confirmation: &str) -> Outcome<DeletedUser<P::User, P::RepoId>, P> {
        let target = self.user(actor, id).await?;
        if confirmation.trim() != P::user_name(&target) {
            return Err(invalid("type the username exactly to confirm".to_owned()));
        }
        refuse_self::<P>(actor, &target, "delete your own account here")?;
        let orphaned = self.port.organizations_solely_owned_by(id).await?;
        if !orphaned.is_empty() {
            return Err(invalid(format!(
                "{} is the only owner of {}; add another owner or delete them first",
                P::user_name(&target),
                orphaned.join(", ")
            )));
        }
        let repositories = self.port.repositories_owned_by(id).await?;
        explain::<P>(self.port.delete_user(id).await?, &target, "deleted")?;
        Ok(DeletedUser { user: target, repositories })
    }

    pub async fn organizations(&self, actor: &Actor<P::Id>, query: &ListQuery) -> Outcome<Paged<P::Organization>, P> {
        require_site_admin::<P::Error, _>(actor)?;
        Ok(Paged::new(self.port.list_organizations(query.filter()).await?, query))
    }

    pub async fn repositories(&self, actor: &Actor<P::Id>, query: &ListQuery) -> Outcome<Paged<P::Repository>, P> {
        require_site_admin::<P::Error, _>(actor)?;
        Ok(Paged::new(self.port.list_repositories(query.filter()).await?, query))
    }

    pub async fn repository(&self, actor: &Actor<P::Id>, id: P::RepoId) -> Outcome<P::Repository, P> {
        require_site_admin::<P::Error, _>(actor)?;
        self.port.find_repository(id).await?.ok_or(AccessError::NotFound)
    }

    /// Deletes a repository's metadata; `confirmation` must repeat `owner/name` exactly.
    /// The caller removes it from disk.
    pub async fn delete_repository(&self, actor: &Actor<P::Id>, id: P::RepoId, confirmation: &str) -> Outcome<P::Repository, P> {
        let repo = self.repository(actor, id).await?;
        if confirmation.trim() != P::repository_name(&repo) {
            return Err(invalid("type the repository's full name exactly to confirm".to_owned()));
        }
        if !self.port.delete_repository(id).await? {
            return Err(AccessError::NotFound);
        }
        Ok(repo)
    }
}

fn invalid<E>(message: String) -> AccessError<E> {
    AccessError::Invalid(message)
}

fn refuse_self<P: AdministrationRepository>(actor: &Actor<P::Id>, target: &P::User, action: &str) -> Outcome<(), P> {
    if actor.id == P::user_id(target) { Err(invalid(format!("you can't {action}"))) } else { Ok(()) }
}

fn explain<P: AdministrationRepository>(outcome: Moderation, target: &P::User, verb: &str) -> Outcome<(), P> {
    match outcome {
        Moderation::Applied => Ok(()),
        Moderation::NotFound => Err(AccessError::NotFound),
        Moderation::LastAdmin => Err(invalid(format!(
            "{} is the last active administrator and can't be {verb}; promote someone else first",
            P::user_name(target)
        ))),
    }
}

/// The audit description of changed settings, e.g.
/// `registration: open → closed; site name: rbit → Forge`. `fields` are
/// `(name, before, after)`; unchanged ones are left out.
pub fn describe_changes(branding_changed: bool, fields: &[(&str, &str, &str)]) -> String {
    let mut description = if branding_changed { "branding changed; ".to_owned() } else { String::new() };
    description.push_str(
        &fields
            .iter()
            .filter(|(_, old, new)| old != new)
            .map(|(name, old, new)| format!("{name}: {old} → {new}"))
            .collect::<Vec<_>>()
            .join("; "),
    );
    description
}
