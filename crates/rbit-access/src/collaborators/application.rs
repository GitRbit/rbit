//! Collaborator management rules over one request's repository port.

use super::CollaboratorRepository;
use crate::{
    organizations::{AccessError, UserAccount},
    teams::Unique,
};

type Outcome<T, P> = Result<T, AccessError<<P as CollaboratorRepository>::Error>>;

/// The repository a change applies to, as the caller resolved it for the actor.
#[derive(Debug, Clone, Copy)]
pub struct CollaboratorScope<R, I> {
    pub repo: R,
    /// The repository's owner account (a person who owns it cannot also collaborate).
    pub owner: I,
    /// The actor administers the repository.
    pub can_admin: bool,
}

/// What [`Collaborators::set`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetOutcome<I, P> {
    Added(UserAccount<I>),
    Changed {
        user: UserAccount<I>,
        from: P,
    },
    /// They already had this permission: nothing changed.
    Unchanged,
}

/// Collaborator rules; refused actors never reach a write.
pub struct Collaborators<'a, P> {
    port: &'a P,
}

impl<P> std::fmt::Debug for Collaborators<'_, P> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Collaborators").finish_non_exhaustive()
    }
}

impl<'a, P: CollaboratorRepository> Collaborators<'a, P> {
    pub fn new(port: &'a P) -> Self {
        Self { port }
    }

    /// Repository administrators only; everyone else is `Forbidden`.
    pub fn require_admin(scope: CollaboratorScope<P::Repo, P::Id>) -> Outcome<(), P> {
        if scope.can_admin { Ok(()) } else { Err(AccessError::Forbidden) }
    }

    /// The repository's collaborators. Administrators only.
    pub async fn list(&self, scope: CollaboratorScope<P::Repo, P::Id>) -> Outcome<Vec<P::Collaborator>, P> {
        Self::require_admin(scope)?;
        Ok(self.port.list(scope.repo).await?)
    }

    /// Adds the user called `username` with `permission`; returns them.
    pub async fn add(
        &self,
        scope: CollaboratorScope<P::Repo, P::Id>,
        actor: P::Id,
        username: &str,
        permission: P::Permission,
    ) -> Outcome<UserAccount<P::Id>, P> {
        Self::require_admin(scope)?;
        let user = self.find_user(username).await?;
        if user.id == scope.owner {
            return Err(invalid(format!("{} owns this repository", user.name)));
        }
        if self.port.is_suspended(user.id).await? {
            return Err(invalid(format!("{} is suspended and cannot be added", user.name)));
        }
        match self.port.add(scope.repo, user.id, permission, actor).await? {
            Unique::Done(()) => Ok(user),
            Unique::Taken => Err(invalid(format!("{} is already a collaborator", user.name))),
        }
    }

    /// Gives the collaborator called `username` a new permission; returns them and the
    /// permission they had.
    pub async fn change(
        &self,
        scope: CollaboratorScope<P::Repo, P::Id>,
        username: &str,
        permission: P::Permission,
    ) -> Outcome<(UserAccount<P::Id>, P::Permission), P> {
        Self::require_admin(scope)?;
        let user = self.find_user(username).await?;
        let not_collaborator = |user: &UserAccount<P::Id>| invalid(format!("{} is not a collaborator", user.name));
        let Some(from) = self.port.permission(scope.repo, user.id).await? else { return Err(not_collaborator(&user)) };
        if !self.port.set_permission(scope.repo, user.id, permission).await? {
            return Err(not_collaborator(&user));
        }
        Ok((user, from))
    }

    /// Removes the collaborator called `username`: administrators may remove anyone,
    /// collaborators themselves.
    pub async fn remove(&self, scope: CollaboratorScope<P::Repo, P::Id>, actor: P::Id, username: &str) -> Outcome<UserAccount<P::Id>, P> {
        let user = self.find_user(username).await?;
        if user.id != actor {
            Self::require_admin(scope)?;
        }
        if !self.port.remove(scope.repo, user.id).await? {
            return Err(invalid(format!("{} is not a collaborator", user.name)));
        }
        Ok(user)
    }

    /// Adds `username` with `permission`, or gives them `permission` if they already
    /// collaborate; nothing changes when they have it already.
    pub async fn set(
        &self,
        scope: CollaboratorScope<P::Repo, P::Id>,
        actor: P::Id,
        username: &str,
        permission: P::Permission,
    ) -> Outcome<SetOutcome<P::Id, P::Permission>, P> {
        Self::require_admin(scope)?;
        let user = self.find_user(username).await?;
        match self.port.permission(scope.repo, user.id).await? {
            None => self.add(scope, actor, &user.name, permission).await.map(SetOutcome::Added),
            Some(current) if current == permission => Ok(SetOutcome::Unchanged),
            Some(_) => self.change(scope, &user.name, permission).await.map(|(user, from)| SetOutcome::Changed { user, from }),
        }
    }

    /// The user account called `username`; organisations cannot be collaborators.
    pub async fn find_user(&self, username: &str) -> Outcome<UserAccount<P::Id>, P> {
        let username = username.trim();
        if username.is_empty() {
            return Err(invalid("enter a username".to_owned()));
        }
        self.port.find_user(username).await?.ok_or_else(|| invalid(format!("there is no user called {username}")))
    }
}

fn invalid<E>(message: String) -> AccessError<E> {
    AccessError::Invalid(message)
}
